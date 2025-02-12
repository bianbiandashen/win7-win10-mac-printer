use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::WebSocketStream;
use futures_util::{stream::StreamExt, SinkExt};
use tauri::State;
use tauri::AppHandle;
use tauri::Manager; 

// 定义类型
pub type SharedWebSocket = Arc<Mutex<Option<WebSocketStream<TcpStream>>>>;

// 启动 WebSocket 服务器
pub async fn start_websocket_server(app_handle: AppHandle, ws_conn: SharedWebSocket) {
    let addr = "127.0.0.1:14528";
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");

    println!("WebSocket server listening on {}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let ws_stream = accept_async(stream).await.expect("Error during WebSocket handshake");
        {
            let mut guard = ws_conn.lock().await;
            *guard = Some(ws_stream);
            println!("WebSocket 连接已存储");
        }
        // 这里会阻塞
        tokio::spawn(handle_connection(ws_conn.clone(), app_handle.clone()));
    }
}

#[tauri::command]
pub async fn check_websocket_connection(ws_conn: State<'_, SharedWebSocket>) -> Result<bool, String> {
    let guard = ws_conn.lock().await;
    Ok(guard.is_some())
}

#[tauri::command]
pub async fn send_message_to_websocket(message: String, ws_conn: State<'_, SharedWebSocket>) -> Result<(), String> {
    let mut ws_conn_guard: tokio::sync::MutexGuard<Option<WebSocketStream<TcpStream>>> = ws_conn.lock().await;
    if let Some(ws_stream) = ws_conn_guard.as_mut() {
        ws_stream.send(Message::Text(message)).await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("WebSocket connection not established".to_string())
    }
}

fn send_message(app_handle: AppHandle, message: String) {
    app_handle.emit_all("websocket-message", message).unwrap();
}

async fn handle_connection(ws_conn: SharedWebSocket, app_handle: AppHandle) {
    loop {
        let message = {
            let mut ws_stream_guard = ws_conn.lock().await;
            if let Some(ws_stream) = ws_stream_guard.as_mut() {
                let (_write, mut read) = ws_stream.split();

                tokio::select! {
                    message = read.next() => message,
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                        println!("等待消息超时，释放锁");
                        None
                    }
                }
            } else {
                // 没有活跃的连接，等待一段时间后重试
                drop(ws_stream_guard);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        match message {
            Some(Ok(msg)) => {
                // println!("收到消息: {:?}", msg);
                if let Ok(message_str) = msg.to_text() {
                    send_message(app_handle.clone(), message_str.to_string());

                    let mut ws_stream_guard = ws_conn.lock().await;
                    if let Some(ws_stream) = ws_stream_guard.as_mut() {
                        if let Err(e) = ws_stream.send(Message::Text("回声: ".to_string() + message_str)).await {
                            eprintln!("发送消息错误: {:?}", e);
                            break;
                        }
                    }
                }
            }
            Some(Err(e)) => {
                eprintln!("接收消息错误: {:?}", e);
                break;
            }
            None => {
                println!("连接已关闭或等待消息超时");
                continue;
            }
        }
    }
    
    // 连接已关闭，清理连接池
    let mut ws_stream_guard = ws_conn.lock().await;
    *ws_stream_guard = None;
}
