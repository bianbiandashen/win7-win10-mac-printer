use log::{info, error}; // 引入日志宏，用于记录信息和错误
use serde::{Serialize, Deserialize}; // 引入序列化和反序列化宏
use std::collections::HashMap; // 引入 HashMap，用于存储请求头
use reqwest::{Client, ClientBuilder, StatusCode}; // 引入 reqwest 库的客户端和状态码
use std::time::Duration; // 引入 Duration，用于设置超时时间和延迟
use tokio::time::sleep; // 引入异步睡眠函数，用于实现重试延迟
use std::error::Error; // 引入错误处理特征，用于错误链处理
use crate::error::{CustomError, ErrorCode, System, Component};
use cfg_if::cfg_if; // 引入条件编译宏

// 新增的use行，用于本地文件读取
use std::fs;
use std::path::Path;

/// 定义支持的 HTTP 方法枚举
#[derive(Serialize, Deserialize, Debug)]
pub enum HttpMethod {
    GET,
    POST,
}

/// 定义 API 请求的结构体
#[derive(Serialize, Deserialize, Debug)]
pub struct ApiRequest {
    pub url: String,                         // 请求的目标 URL
    pub headers: HashMap<String, String>,    // 请求头键值对，键为头部名称，值为头部内容
    pub body: Option<String>,                // 请求体，仅适用于 POST 请求，可选
    pub method: HttpMethod,                  // 请求方法，支持 GET 和 POST
}

impl ApiRequest {
    /// 创建一个新的 ApiRequest 实例，默认使用 POST 方法
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            method: HttpMethod::POST,
        }
    }

    /// 设置请求头
    pub fn set_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// 设置请求体（仅适用于 POST 请求）
    pub fn set_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self
    }

    /// 设置请求方法（GET 或 POST）
    pub fn set_method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }
}

/// 发送 API 请求的 Tauri 命令
#[tauri::command]
pub async fn send_request_command(request: ApiRequest) -> Result<String, String> {
    // 首先判断是否为 file:// 协议请求
    info!("检测到有新的请求:  {}", request.url);
    if request.url.to_lowercase().starts_with("file://") {
        info!("检测到 file:// 请求，尝试读取本地文件内容: {}", request.url);
        let local_path = request.url.trim_start_matches("file://");
        let local_path = Path::new(local_path);

        if !local_path.exists() {
            let error_msg = format!("文件不存在: {:?}", local_path);
            error!("{}", error_msg);
            return Err(error_msg);
        }

        // 尝试读取文件内容
        match fs::read_to_string(local_path) {
            Ok(content) => {
                info!("成功读取本地文件内容: {:?}", local_path);
                return Ok(content);
            }
            Err(e) => {
                let error_msg = format!("读取本地文件失败: {:?}, 错误原因: {}", local_path, e);
                error!("{}", error_msg);
                return Err(error_msg);
            }
        }
    }

    // 定义最大重试次数和初始重试间隔（秒）
    const MAX_RETRIES: usize = 3;
    const INITIAL_RETRY_DELAY_SECS: u64 = 2;

    // 构建 HTTP 客户端构建器
    let client_builder = ClientBuilder::new()
        .timeout(Duration::from_secs(200)); // 设置请求超时时间为 10 秒

    // 根据平台选择 TLS 配置
    cfg_if! {
        if #[cfg(target_os = "macos")] {
            // 在 macOS 上使用系统默认的 TLS 配置
            let client_builder = client_builder.use_native_tls();
        } else {
            // 在其他平台上，使用 rustls，不需要额外配置
            let client_builder = client_builder;
        }
    }


    // 构建 HTTP 客户端
    let client = match client_builder.build() {
        Ok(c) => {
            // info!("HTTP 客户端创建成功");
            c
        },
        Err(e) => {
            let error_msg = format!("{}创建 HTTP 客户端失败: {:?}", ErrorCode::new(System::GeneralReport, Component::NetworkModule, 0x075), e);
            error!("{}", error_msg);
            return Err(error_msg);
        }
    };

    // 判断是否需要在日志中打印 URL（过滤特定 URL）
    let log_url = if request.url != "https://apm-fe.xiaohongshu.com/api/data" {
        format!("URL: {}, ", request.url)
    } else {
        String::new()
    };

    // 根据请求方法（GET 或 POST）构建 HTTP 请求构建器
    let mut request_builder = match request.method {
        HttpMethod::GET => {
            // info!("构建 GET 请求");
            client.get(&request.url)
        },
        HttpMethod::POST => {
            // info!("构建 POST 请求");
            client.post(&request.url)
        },
    };

    // 设置请求头
    if !request.headers.is_empty() {
        // info!("设置请求头: {:?}", request.headers);
        for (key, value) in &request.headers {
            request_builder = request_builder.header(key.as_str(), value.as_str());
        }
    } else {
        info!("没有设置任何请求头");
    }

    // 设置请求体（仅适用于 POST 请求）
    if let Some(body) = &request.body {
        // info!("设置请求体: {}", body);
        request_builder = request_builder.body(body.clone());
    }

    // 初始化尝试次数
    let mut attempt = 0;

    // 进入重试循环
    loop {
        attempt += 1;
        info!("发送请求尝试 {}: {}", attempt, request.url);

        // 发送 HTTP 请求
        // 使用 `try_clone` 以确保请求构建器可以在循环中被多次使用
        let response = if let Some(builder) = request_builder.try_clone() {
            // 如果成功克隆构建器，发送请求
            builder.send().await
        } else {
            // 如果构建器克隆失败，记录错误并返回
            let error_msg = format!("{}请求构建器克隆失败，无法发送请求，URL: {}", ErrorCode::new(System::GeneralReport, Component::NetworkModule, 0x076), request.url);
            error!("{}", error_msg);
            return Err(format!("请求构建器克隆失败，URL: {}", request.url).into());
        };

        // 处理请求结果
        match response {
            Ok(res) => {
                let status = res.status(); // 获取响应状态码
                if status.is_success() {
                    // 如果状态码表示请求成功（2xx）
                    match res.text().await {
                        Ok(text) => {
                            // 成功读取响应体文本
                            info!(
                                "[桌面端apikitrs] {}响应信息: 状态: {:?}, 内容: {}",
                                log_url, status, text
                            );
                            return Ok(text); // 返回响应体文本
                        },
                        Err(e) => {
                            // 读取响应体文本失败
                            error!("读取响应内容失败: {:?}", e);
                            return Err(format!("读取响应内容失败: {:?}", e));
                        }
                    }
                } else {
                    // 如果状态码表示请求失败（非 2xx）
                    let error_text = res.text().await.unwrap_or_default(); // 尝试读取错误内容
                    info!(
                        "[桌面端apikitrs] {}响应状态异常: 状态: {:?}, 内容: {}",
                        log_url, status, error_text
                    );

                    // 判断是否需要重试
                    if attempt < MAX_RETRIES && status.is_server_error() {
                        let delay = INITIAL_RETRY_DELAY_SECS * 2_u64.pow((attempt - 1) as u32); // 指数退避
                        info!(
                            "服务器错误（状态码: {:?}），准备进行第 {} 次重试，延迟: {} 秒",
                            status, attempt + 1, delay
                        );
                        sleep(Duration::from_secs(delay)).await; // 异步延迟
                        continue; // 继续下一次重试
                    }

                    // 如果不需要重试，则返回错误
                    return Err(format!("请求失败，状态码: {:?}", status));
                }
            },
            Err(e) => {
                // 请求发送失败，可能是网络错误、超时等
                // 需要根据错误类型决定是否重试
                if e.is_timeout() {
                    info!("请求超时: {:?}", e);
                    if attempt < MAX_RETRIES {
                        let delay = INITIAL_RETRY_DELAY_SECS * 2_u64.pow((attempt - 1) as u32); // 指数退避
                        info!(
                            "请求超时，准备进行第 {} 次重试，延迟: {} 秒",
                            attempt + 1, delay
                        );
                        sleep(Duration::from_secs(delay)).await; // 异步延迟
                        continue; // 继续下一次重试
                    }
                    let root_cause: String = get_root_cause(&e);
                    let error_msg = format!("{}请求超时的根本原因: {}", ErrorCode::new(System::GeneralReport, Component::NetworkModule, 0x077), root_cause);
                    error!("{}", error_msg);
                    return Err(format!(
                        "请求超时，请检查网络连接或稍后重试。根本原因: {}",
                        root_cause
                    ));
                } else if e.is_connect() {
                    info!("网络连接失败: {:?}", e);
                    if attempt < MAX_RETRIES {
                        let delay = INITIAL_RETRY_DELAY_SECS * 2_u64.pow((attempt - 1) as u32); // 指数退避
                        info!(
                            "网络连接失败，准备进行第 {} 次重试，延迟: {} 秒",
                            attempt + 1, delay
                        );
                        sleep(Duration::from_secs(delay)).await;
                        continue;
                    }
                    let root_cause = get_root_cause(&e);
                    let error_msg = format!("{}网络连接失败的根本原因: {}", ErrorCode::new(System::GeneralReport, Component::NetworkModule, 0x078), root_cause);
                    error!("{}", error_msg);
                    return Err(format!(
                        "网络连接失败，请检查网络连接或服务器地址。根本原因: {}",
                        root_cause
                    ));
                } else {
                    error!("请求错误: {:?}", e);
                    if attempt < MAX_RETRIES {
                        let delay = INITIAL_RETRY_DELAY_SECS * 2_u64.pow((attempt - 1) as u32); // 指数退避
                        info!(
                            "请求错误，准备进行第 {} 次重试，延迟: {} 秒",
                            attempt + 1, delay
                        );
                        sleep(Duration::from_secs(delay)).await;
                        continue;
                    }
                    let root_cause = get_root_cause(&e);
                    let error_msg = format!("{}请求错误，请求发送失败的根本原因: {}", ErrorCode::new(System::GeneralReport, Component::NetworkModule, 0x079), root_cause);
                    error!("{}", error_msg);
                    return Err(format!(
                        "请求发送失败: {}",
                        root_cause
                    ));
                }
            }
        }
    }

    /// 返回根本原因的字符串描述
    fn get_root_cause(err: &reqwest::Error) -> String {
        // 初始化错误链的源
        let mut source = err.source();
        // 初始化根本原因为当前错误的描述
        let mut root = err.to_string();

        // 遍历错误链，直到没有源错误
        while let Some(e) = source {
            root = e.to_string(); // 更新根本原因为当前源错误的描述
            source = e.source(); // 移动到下一个源错误
        }

        root // 返回最深层的错误描述
    }
}
