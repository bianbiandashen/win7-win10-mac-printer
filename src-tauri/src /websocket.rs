// websocket.rs
use crate::error::{CustomError, ErrorCode, System, Component, ErrorLevel};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use log::{error, info};
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Manager;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::WebSocketStream;
use std::collections::HashMap;
use uuid::Uuid;
use serde_json;

// macos==================================================================

#[cfg(target_os = "macos")]
use tokio_rustls::TlsAcceptor;
#[cfg(target_os = "macos")]
use tokio_rustls::rustls::ServerConfig;
use std::fs::File;
use std::io::BufReader;

#[cfg(target_os = "macos")]
use rustls_pemfile::{certs, rsa_private_keys};
use std::sync::Arc as StdArc;
use std::io::Write;
use std::path::PathBuf;
use std::env;
use std::io;
// #[cfg(target_os = "macos")]
// use openssl::{symm::Cipher, pkey::Private};
// #[cfg(target_os = "macos")]
// use std::process::Command;
// #[cfg(target_os = "macos")]
// use openssl::rsa::Rsa;
// #[cfg(target_os = "macos")]
// use openssl::pkey::PKey;

// 使用共享广播协议
pub type SharedSender = Arc<Mutex<broadcast::Sender<Message>>>;

// 添加 clientId 和 clientMap
type ClientId = String;
type ClientMap = Arc<Mutex<HashMap<ClientId, WebSocketSender>>>;
type WebSocketSender = Arc<Mutex<SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>>;

// 启动 WebSocket 服务器
// #[cfg(target_os = "windows")]
pub async fn start_websocket_server(
    app_handle: AppHandle,
    sender: SharedSender,
) -> Result<(), CustomError> {
    let clients: ClientMap = Arc::new(Mutex::new(HashMap::new()));
    let addr = "0.0.0.0:10818";
    let listener = TcpListener::bind(addr).await.map_err(|e| CustomError {
        key: "BindError",
        message: format!("【桌面端前置ERROR】WebSocket 绑定失败: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x019)),
    })?;

    info!("WebSocket server listening on {}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        let sender_clone = sender.clone();
        let app_handle_clone = app_handle.clone();
        let clients_clone = clients.clone();

        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => {
                    info!("WebSocket connection established: {}", addr);
                    handle_connection(ws_stream, sender_clone, app_handle_clone, clients_clone).await;
                }
                Err(e) => {
                    info!("WebSocket 握手错误: {}", e);
                    let custom_error = CustomError {
                        key: "HandshakeError",
                        message: format!("【桌面端前置ERROR】WebSocket 握手错误: {}", e),
                        source: Some(Box::new(e)),
                        error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x021)),
                    };
                    // 发送错误事件到主线程
                    if let Err(emit_err) = app_handle_clone.emit_all("websocket-error", custom_error.message.clone()) {
                        error!("Failed to emit error event: {}", emit_err);
                        crate::apm::report_platform_crash_async(
                            false,  
                            Some(custom_error.message.to_string()),
                            format!("{:?}", ErrorLevel::Warning),
                            None,
                            None,
                        )
                        .unwrap_or_else(|err| {
                            error!("【WebSocket启动失败】: {:?}", err);
                        });
                    }
                }
            }
        });
    }

    Ok(())
}

// #[cfg(target_os = "windows")]
#[tauri::command]
pub async fn send_message_to_websocket(
    message: String,
    sender: tauri::State<'_, SharedSender>,
) -> Result<(), String> {
    let sender_guard = sender.lock().await;
    sender_guard
        .send(Message::Text(message))
        .map(|_| ())
        .map_err(|e| format!("发送消息失败: {}", e))
}

// #[cfg(target_os = "windows")]
fn send_message(app_handle: AppHandle, message: String) {
    if let Err(e) = app_handle.emit_all("websocket-message", message) {
        error!("发送消息到前端失败: {}", e);
    }
}

// #[cfg(target_os = "windows")]
async fn handle_connection(
    ws_stream: WebSocketStream<tokio::net::TcpStream>,
    sender: SharedSender,
    app_handle: AppHandle,
    clients: ClientMap,
) {
    let client_id = Uuid::new_v4().to_string();
    let client_id_for_send = client_id.clone();
    let client_id_for_recv = client_id.clone();
    let client_id_for_log = client_id.clone();
    
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut receiver = {
        let sender_guard = sender.lock().await;
        sender_guard.subscribe()
    };

    // 处理接收消息
    let recv_task = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message {
                Ok(msg) => {
                    if let Ok(text) = msg.to_text() {
                        // 尝试解析 JSON
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(mut json_msg) => {
                                // 成功解析 JSON，注入 client_id
                                if let serde_json::Value::Object(ref mut map) = json_msg {
                                    map.insert("clientID".to_string(), serde_json::Value::String(client_id_for_recv.clone()));
                                    // 发送修改后的消息
                                    send_message(app_handle.clone(), json_msg.to_string());
                                }
                            },
                            Err(e) => {
                                // JSON 解析失败，直接转发原始消息
                                error!("JSON 解析错误: {}, 原始消息: {}", e, text);
                                send_message(app_handle.clone(), text.to_string());
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("接收消息错误: {:?}", e);
                    break;
                }
            }
        }
    });

    // 处理发送消息给客户端
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
            if let Message::Text(text) = &msg {
                // 检查消息是否包含对应的 client_id，如果是的就发给对应端
                if text.contains(&format!("\"clientID\":\"{}\"", client_id_for_send)) {
                    if let Err(e) = ws_sender.send(msg).await {
                        error!("发送消息失败: {}", e);
                        break;
                    }
                }
                else if text.contains("\"clientID\":\"error_event\"") {
                    if text.contains("0x00420009") {
                        // 忽略特定的错误消息
                        continue;
                    }
                    if let Err(e) = ws_sender.send(msg).await {
                        error!("发送错误消息失败: {}", e);
                        break;
                    }
                }
            }
        }
    });

    // 等待接收和发送任务完成
    tokio::select! {
        result = recv_task => {
            if let Err(e) = result {
                error!("接收任务错误: {:?}", e);
            }
        },
        result = send_task => {
            if let Err(e) = result {
                error!("发送任务错误: {:?}", e);
            }
        },
    }

    info!("WebSocket 连接关闭 (Client ID: {})", client_id_for_log);
}

// #[cfg(target_os = "windows")]
#[tauri::command]
pub async fn check_websocket_connection() -> Result<String, String> {
    Ok("WebSocket 服务正在运行".to_string())
}




// ========================================================================================================

// #[cfg(target_os = "macos")]
// fn load_tls_config(cert_path: &str, key_path: &str) -> Result<StdArc<ServerConfig>, CustomError> {
//     // 读取证文件
//     let cert_file = File::open(cert_path).map_err(|e| CustomError {
//         key: "CertFileError",
//         message: format!("无法打开证书文件 {}: {}", cert_path, e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x080)),
//     })?; 
//     let mut cert_reader = BufReader::new(cert_file);
//     let certs: Vec<_> = certs(&mut cert_reader)
//         .collect::<Result<_, _>>()
//         .map_err(|e| CustomError {
//             key: "CertParseError",
//             message: format!("解析证书失败: {}", e),
//             source: Some(Box::new(e)),
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x081)),

//         })?;

//     // 取加密的私钥件
//     let encrypted_key = std::fs::read_to_string(key_path).map_err(|e| CustomError {
//         key: "KeyFileError",
//         message: format!("无法读取私钥文件 {}: {}", key_path, e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x082)),
//     })?;

//     // 解密私钥
//     let password = "xiaohongshu";
//     let decrypted_key = decrypt_private_key(&encrypted_key, password).map_err(|e| CustomError {
//         key: "KeyDecryptError",
//         message: format!("解密私钥失败: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x083)),
//     })?;
//     // 打印解密后的私钥内容
//     println!("解密后的私钥: {:?}", String::from_utf8_lossy(&decrypted_key));
//     let keys: Vec<_> = Rsa::private_key_from_pem(&decrypted_key)
//         .map(|key| vec![key])
//         .map_err(|e| CustomError {
//             key: "KeyParseError",
//             message: format!("解析私钥失败: {}", e),
//             source: Some(Box::new(e)),
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x084)),
//         })?;

//     if keys.is_empty() {
//         return Err(CustomError {
//             key: "NoKeyError",
//             message: "未找到私钥".to_string(),
//             source: None,
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x085)),
//         });
//     }

//     // 配置 TLS
//     let config = ServerConfig::builder()
//         .with_safe_defaults()
//         .with_no_client_auth()
//         .with_single_cert(
//             certs.into_iter().map(|cert| tokio_rustls::rustls::Certificate(cert.to_vec())).collect(),
//             tokio_rustls::rustls::PrivateKey(keys[0].private_key_to_der().unwrap())
//         )
//         .map_err(|e| CustomError {
//             key: "TLSConfigError",
//             message: format!("设置 TLS 配置失败: {}", e),
//             source: Some(Box::new(e)),
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x086)),
//         })?;

//     Ok(StdArc::new(config))
// }

// /// 解密私钥的辅助函数
// #[cfg(target_os = "macos")]
// fn decrypt_private_key(encrypted_pem: &str, password: &str) -> Result<Vec<u8>, CustomError> {
//     Rsa::private_key_from_pem_passphrase(
//         encrypted_pem.as_bytes(),
//         password.as_bytes()
//     )
//     .map_err(|e| CustomError {
//         key: "DecryptError",
//         message: "私钥解密失败".to_string(),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x087)),
//     })
//     .and_then(|rsa| {
//         PKey::from_rsa(rsa).map_err(|e| CustomError {
//             key: "KeyConversionError",
//             message: "私钥格式转换失败".to_string(),
//             source: Some(Box::new(e)),
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x088)),
//         })
//     })
//     .and_then(|pkey| {
//         pkey.private_key_to_pem_pkcs8().map_err(|e| CustomError {
//             key: "PemEncodingError",
//             message: "私钥PEM编码失败".to_string(),
//             source: Some(Box::new(e)),
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x089)),
//         })
//     })
// }

// #[cfg(target_os = "macos")]
// fn create_cert_file(path: &PathBuf, filename: &str, bin: &[u8]) -> io::Result<PathBuf> {
//     let file_path = path.join(filename);
//     info!("【桌面端主入口调用】create_cert_file: Creating file at path: {}", file_path.display());

//     let mut f = File::create(&file_path).map_err(|e| {
//         error!("【桌面端主入口调用】create_cert_file: Failed to create file: {}", e);
//         e
//     })?;
    
//     f.write_all(bin)?;
//     f.sync_all()?;
//     info!("【桌面端主入口调用】create_cert_file: File created successfully at path: {}", file_path.display());
//     Ok(file_path)
// }

// /// 启动 WebSocket 服务器，包括 WS 和 WSS
// #[cfg(target_os = "macos")]
// pub async fn start_websocket_server(
//     app_handle: AppHandle,
//     sender: SharedSender,
// ) -> Result<(), CustomError> {
//     println!("【桌面端主入口调用】start_websocket_server");
//     // 获取临时目录路径
//     let temp_dir: PathBuf = env::temp_dir();

//     // 嵌入证书和私钥文件
//     let cert_data = include_bytes!("../binaries/cert/server.crt");
//     let key_data = include_bytes!("../binaries/cert/server.key");

//     // 创建证书和私钥文件
//     let cert_path = create_cert_file(&temp_dir, "server.crt", cert_data).map_err(|e| CustomError {
//         key: "CertFileCreationError",
//         message: format!("无法创建证书文件: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x090)),
//     })?;

//     let key_path = create_cert_file(&temp_dir, "server.key", key_data).map_err(|e| CustomError {
//         key: "KeyFileCreationError",
//         message: format!("无法创建私钥文件: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x091)),
//     })?;

//     // 使用创建的文件路径加载 TLS 配置
//     let tls_config = load_tls_config(
//         cert_path.to_str().ok_or(CustomError {
//             key: "PathError",
//             message: "证书路径转换失败".to_string(),
//             source: None,
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x092)),
//         })?,
//         key_path.to_str().ok_or(CustomError {
//             key: "PathError",
//             message: "私钥路径转换失败".to_string(),
//             source: None,
//             error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x093)),
//         })?,
//     )?;

//     // 加载 TLS 配置
//     let tls_acceptor = TlsAcceptor::from(tls_config);

//     // 启动 WS 服务器
//     let ws_addr = "0.0.0.0:10818";
//     let ws_listener = TcpListener::bind(ws_addr).await.map_err(|e| CustomError {
//         key: "BindError",
//         message: format!("【桌面端前置ERROR】WebSocket 绑定失败: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x019)),
//     })?;
//     info!("WebSocket 服务器正在监听 {}", ws_addr);

//     // 启动 WSS 服务器
//     let wss_addr = "0.0.0.0:10820";
//     let wss_listener = TcpListener::bind(wss_addr).await.map_err(|e| CustomError {
//         key: "BindError",
//         message: format!("【桌面端前置ERROR】WebSocket Secure 绑定失败: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x094)),
//     })?;
//     info!("WebSocket Secure 服务器正在监听 {}", wss_addr);

//     // WS 服务器任务
//     let sender_clone_ws = sender.clone();
//     let app_handle_clone_ws = app_handle.clone();
//     let ws_task = tokio::spawn(async move {
//         loop {
//             match ws_listener.accept().await {
//                 Ok((stream, _)) => {
//                     let sender_clone = sender_clone_ws.clone();
//                     let app_handle_clone = app_handle_clone_ws.clone();
//                     tokio::spawn(async move {
//                         match accept_async(stream).await {
//                             Ok(ws_stream) => {
//                                 handle_connection(ws_stream, sender_clone, app_handle_clone).await;
//                             }
//                             Err(e) => {
//                                 error!("WebSocket 握手错误: {}", e);
//                             }
//                         }
//                     });
//                 }
//                 Err(e) => {
//                     error!("WebSocket 接受连接错误: {}", e);
//                 }
//             }
//         }
//     });

//     // WSS 服务器任务
//     let sender_clone_wss = sender.clone();
//     let app_handle_clone_wss = app_handle.clone();
//     let tls_acceptor_clone = tls_acceptor.clone();
//     let wss_task = tokio::spawn(async move {
//         loop {
//             match wss_listener.accept().await {
//                 Ok((stream, _)) => {
//                     let acceptor = tls_acceptor_clone.clone();
//                     let sender_clone = sender_clone_wss.clone();
//                     let app_handle_clone = app_handle_clone_wss.clone();
//                     tokio::spawn(async move {
//                         match acceptor.accept(stream).await {
//                             Ok(tls_stream) => {
//                                 match accept_async(tls_stream).await {
//                                     Ok(ws_stream) => {
//                                         handle_connection(ws_stream, sender_clone, app_handle_clone).await;
//                                     }
//                                     Err(e) => {
//                                         error!("WSS WebSocket 握手错误: {}", e);
//                                     }
//                                 }
//                             }
//                             Err(e) => {
//                                 error!("WebSocket Secure 接受连接错误: {}", e);
//                             }
//                         }
//                     });
//                 }
//                 Err(e) => {
//                     error!("WebSocket Secure 接受连接错误: {}", e);
//                 }
//             }
//         }
//     });

//     // 等待两个任务
//     tokio::try_join!(ws_task, wss_task).map_err(|e| CustomError {
//         key: "JoinError",
//         message: format!("【桌面端前置ERROR】任务执行错误: {}", e),
//         source: Some(Box::new(e)),
//         error_code: Some(ErrorCode::new(System::GeneralReport, Component::StartingModule, 0x021)),
//     })?;

//     Ok(())
// }

// #[cfg(target_os = "macos")]
// #[tauri::command]
// pub async fn send_message_to_websocket(
//     message: String,
//     sender: tauri::State<'_, SharedSender>,
// ) -> Result<(), String> {
//     let sender_guard = sender.lock().await;
//     sender_guard
//         .send(Message::Text(message))
//         .map(|_| ())
//         .map_err(|e| format!("发送消息失败: {}", e))
// }

// /// 向前端发送消息
// #[cfg(target_os = "macos")]
// fn send_message(app_handle: AppHandle, message: String) {
//     if let Err(e) = app_handle.emit_all("websocket-message", message) {
//         error!("发送消息到前端失败: {}", e);
//     }
// }

// /// 处理 WebSocket 连接
// #[cfg(target_os = "macos")]
// async fn handle_connection<S>(
//     ws_stream: WebSocketStream<S>,
//     sender: SharedSender,
//     app_handle: AppHandle,
// ) where
//     S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
// {
//     let (mut ws_sender, mut ws_receiver) = ws_stream.split();
//     let mut receiver = {
//         let sender_guard = sender.lock().await;
//         sender_guard.subscribe()
//     };

//     // 处理接收来自客户端的消息
//     let recv_task = tokio::spawn(async move {
//         while let Some(message) = ws_receiver.next().await {
//             match message {
//                 Ok(msg) => {
//                     if let Ok(text) = msg.to_text() {
//                         send_message(app_handle.clone(), text.to_string());

//                         // 可选：将消息广播给其他客户端
//                         // let sender_guard = sender.lock().await;
//                         // if let Err(e) = sender_guard.send(Message::Text(text.to_string())) {
//                         //     error!("广播消息失败: {}", e);
//                         // }
//                     }
//                 }
//                 Err(e) => {
//                     error!("接收消息错误: {:?}", e);
//                     break;
//                 }
//             }
//         }
//     });

//     // 处理发送消息给客户端
//     let send_task = tokio::spawn(async move {
//         while let Ok(msg) = receiver.recv().await {
//             if let Err(e) = ws_sender.send(msg).await {
//                 error!("发送消息失败: {}", e);
//                 break;
//             }
//         }
//     });

//     // 等待接收和发送任务完成
//     tokio::select! {
//         result = recv_task => {
//             if let Err(e) = result {
//                 error!("接收任务错误: {:?}", e);
//             }
//         },
//         result = send_task => {
//             if let Err(e) = result {
//                 error!("发送任务错误: {:?}", e);
//             }
//         },
//     }

//     info!("WebSocket 连接关闭");
// }

// #[cfg(target_os = "macos")]
// #[tauri::command]
// pub async fn check_websocket_connection() -> Result<String, String> {
//     Ok("WebSocket 服务正在运行".to_string())
// }
