use crate::globalcache::{DEVICE_ID_CACHE, SYSTEM_INFO_CACHE, OS_NAME_CACHE, USER_AGENT_CACHE, APP_VERSION, get_app_version};
use crate::utils::{self, get_system_info};
use chrono::{DateTime, Local, NaiveDate, Utc};
use crossbeam_channel::{unbounded, Sender};
use log::{error, info, warn};
use log4rs;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::sync::Once;
use std::thread;
use tauri::api::path::app_dir;
use tauri::{Config, State};
use tauri::AppHandle;
use std::path::PathBuf;

// 定义一个 Once 结构体，用来确保日志系统只初始化一次
static INIT: Once = Once::new();

// 前端日志结构体，包含日志级别和消息
#[derive(Deserialize, Clone)]
pub struct FrontendLog {
    level: String,
    message: String,
}

// 异步日志结构体，包含一个跨线程的消息发送者
#[derive(Clone)]
pub struct AsyncLogger {
    sender: Sender<(String, String, String)>, // (level, message, target)
}

impl LogOptions {
    // 为 LogOptions 实现一个构造函数
    pub fn new(days: Option<i64>, seller_id: Option<String>) -> Self {
        LogOptions { days, seller_id }
    }
}

impl AsyncLogger {
    // 创建一个新的 AsyncLogger 实例，并启动一个线程来处理日志
    fn new() -> Self {
        let (sender, receiver) = unbounded::<(String, String, String)>();
        thread::spawn(move || {
            // 不断接收日志消息并根据日志级别打印
            for (level, message, target) in receiver.iter() {
                match level.as_str() {
                    "ERROR" => error!(target: &target, "{}", message),
                    "WARN" => warn!(target: &target, "{}", message),
                    _ => info!(target: &target, "{}", message),
                }
            }
        });
        AsyncLogger { sender }
    }

    // 日志记录函数，将日志发送到异步线程
    pub fn log(&self, level: &str, message: &str, target: &str) {
        let _ = self
            .sender
            .send((level.to_string(), message.to_string(), target.to_string()));
    }
}

// 获取日志文件目录
#[tauri::command(rename_all = "snake_case")]
pub async fn get_log_directory(app_handle: AppHandle) -> Result<String, String> {
    println!("INFO: 开始执行 get_log_directory 函数");

    // 获取并检查配置
    let config = app_handle.config();
    let app_base_dir: PathBuf = match app_dir(&config) {
        Some(dir) => dir,
        None => return Err("无法获取 app_dir 路径".into()),
    };

    // 构建日志目录路径
    let logs_dir: PathBuf = app_base_dir.join("logs");
    println!("INFO: logs_dir: {}", logs_dir.display());

    // 检查日志目录是否存在，返回适当的结果
    if logs_dir.exists() {
        Ok(logs_dir.to_string_lossy().to_string())
    } else {
        Err("日志目录不存在".into())
    }
}

// 初始化日志系统，并接收 Tauri 的 `Config` 作为参数
pub fn init_logger(config: &Config) -> Result<AsyncLogger, Box<dyn std::error::Error>> {
    let async_logger = AsyncLogger::new();
    let app_version = APP_VERSION.try_lock().unwrap();
    log_mdc::insert("app_version", format!("[APP版本:{}] ", app_version));

    INIT.call_once(|| {
        // 使用 Tauri API 获取 app_dir 作为日志目录的基础路径，传递 &Config
        let app_base_dir = app_dir(config).expect("无法获取 app_dir 路径");

        // 确保日志目录存在
        let log_dir = app_base_dir.join("logs");
        if let Err(e) = fs::create_dir_all(&log_dir) {
            error!("无法创建日志目录: {}", e);
        }

        // 打印日志目录路径
        println!("日志目录路径: {}", log_dir.display());

        // 初始化 log4rs 配置文件
        let config_path = app_base_dir.join("log4rs.yaml");
        println!("log4rs 配置文件路径: {}", config_path.display());

        if let Err(e) = log4rs::init_file(&config_path, Default::default()) {
            error!("无法初始化 log4rs: {}", e);
        }
    });

    Ok(async_logger)
}

// 用于接收前端日志的 Tauri 命令
#[tauri::command()]
pub fn log_frontend(log: FrontendLog, async_logger: State<'_, AsyncLogger>) {
    // 根据前端传递的日志级别选择适当的 logger
    let logger = if log.level == "ERROR" {
        "frontend_error"
    } else {
        "frontend"
    };

    // 构建带有时间戳的日志消息
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let message = format!("{} [Frontend] {}", timestamp, log.message);

    // 记录日志
    async_logger.log(&log.level, &message, "frontend");

    // 打印日志消息
    println!("前端日志: [{}] {}", log.level, log.message);
}

#[derive(Deserialize, Clone)]
pub struct LogOptions {
    days: Option<i64>,
    seller_id: Option<String>,
}

async fn upload_log(
    client: &Client,
    system_type: &str,
    environment: &str,
    device_id: &str,
    seller_id: &str,
    log_body_str: &str,
    software_version: &str,
) -> Result<String, String> {
    println!("INFO: 开始上传日志...");
    println!("INFO: environment: {:?}", environment);
    println!("INFO: device_id: {:?}", device_id);
    println!("INFO: seller_id: {:?}", seller_id);
    println!("INFO: report_time: {:?}", Utc::now().timestamp_millis());
    println!("INFO: platform: {:?}", system_type);
    println!("INFO: software_version: {:?}", software_version);

    let request_body = json!({
        "meta": {
            "systemType": format!("{:?}", system_type),
            "environment": environment
        },
        "device_id": device_id,
        "seller_id": seller_id,
        "platform": system_type,
        "log_body": log_body_str,
        "report_time": Utc::now().timestamp_millis(),
        "software_version": software_version
    });

    // println!("INFO: 请求体: {:?}", request_body);

    let response = client
        .post("https://cloudprint.xiaohongshu.com/api/edith/v1/cloudprint/client/uploadlog")
        .json(&request_body)
        .send()
        .await;

    match response {
        Ok(resp) => {
            println!("INFO: 请求成功发出，状态码: {:?}", resp.status());

            let response_json: serde_json::Value = resp.json().await.map_err(|e| {
                let error_message = format!("ERROR: 解析响应失败: {:?}", e);
                println!("{}", error_message);
                error_message
            })?;

            println!("INFO: 响应体: {:?}", response_json);

            let log_id = response_json["data"]["id"]
                .as_i64()
                .ok_or_else(|| {
                    let error_message = "ERROR: 响应中未找到有效的日志ID".to_string();
                    println!("{}", error_message);
                    error_message
                })?
                .to_string(); // Convert the number to a string

            println!("INFO: 成功获取日志ID: {}", log_id);
            Ok(log_id)
        }
        Err(e) => {
            let error_message = format!("ERROR: 请求发送失败: {:?}", e);
            println!("{}", error_message);
            Err(error_message)
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_log_snapshot_id(
    options: Option<LogOptions>,
    app_handle: AppHandle, // 修改这里
) -> Result<String, String> {
    // Log function call
    println!("INFO: 开始执行 get_log_snapshot_id 函数");
    let config = app_handle.config();
    let app_base_dir = app_dir(&config).expect("无法获取 app_dir 路径");
    // 打印 app_base_dir
    // 确保日志目录存在
    let logs_dir = app_base_dir.join("logs");
    println!("INFO: logs_dir: {}", logs_dir.display());
    let seller_id = options
        .clone()
        .and_then(|opt| opt.seller_id)
        .unwrap_or("".to_string());
    println!("INFO: 卖家ID: {}", seller_id);

    println!("INFO: 日志目录: {}", logs_dir.display());

    let mut log_body = json!({ "rust": "", "frontend": "" });
    let days = options.clone().and_then(|opt| opt.days).unwrap_or(1);
    let days_ago = Utc::now() - chrono::Duration::days(days);

    println!("INFO: 扫描最近 {} 天的日志", days);

    for entry in fs::read_dir(&logs_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let log_type = path.file_name().unwrap().to_str().unwrap();
            let mut combined_content = String::new();
            for file in fs::read_dir(&path).map_err(|e| e.to_string())? {
                let file = file.map_err(|e| e.to_string())?;
                let file_path = file.path();
                if file_path.is_file() {
                    let file = File::open(&file_path).map_err(|e| e.to_string())?;
                    let reader = BufReader::new(file);
                    for line in reader.lines() {
                        let line = line.map_err(|e| e.to_string())?;
                        if let Some(date_str) = line.split_whitespace().next() {
                            if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                                let datetime = date.and_hms_opt(0, 0, 0).unwrap(); // 日期转换
                                let date =
                                    DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc);
                                if date >= days_ago {
                                    combined_content.push_str(&line);
                                    combined_content.push('\n');
                                }
                            }
                        }
                    }
                }
            }
            if !combined_content.is_empty() {
                log_body[log_type] = json!(combined_content);
            }
        }
    }

    println!(
        "INFO: 日志扫描完成,共收集 {} 个日志类型",
        log_body.as_object().unwrap().len()
    );

    let log_body_str = serde_json::to_string(&log_body).map_err(|e| e.to_string())?;
    println!("INFO: 日志长度: {}", log_body_str.len());

    if log_body_str.is_empty() {
        println!("WARN: 收集到的日志内容为空");
    }

    // HTTP 请求调用 upload_log
    let client = Client::new();
    let system_type = get_system_info();
    let device_id = DEVICE_ID_CACHE.clone().unwrap_or_else(|| "unknown".to_string());
    let software_version = APP_VERSION.lock().await;
    let environment = utils::get_environment();

    println!("INFO: 准备上传日志到服务器");
    println!("INFO: 环境: {}", environment);

    let log_id = upload_log(
        &client,                   // HTTP client
        system_type.as_str(),      // Convert to &str if needed
        environment.as_str(),      // Convert to &str if needed
        device_id.as_str(),        // Convert to &str if needed
        seller_id.as_str(),        // Convert to &str if needed
        &log_body_str,             // Already a reference
        software_version.as_str(), // Convert to &str if needed
    )
    .await?;

    Ok(log_id)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn submit_feedback(
    id: String,
    feedback: String,
    async_logger: tauri::State<'_, AsyncLogger>,
) -> Result<String, String> {
    // 打印日志以记录函数调用
    println!("INFO: 开始执行 submit_feedback 函数");

    // 构建请求体
    let request_body = json!({
        "id": id,
        "device_id": DEVICE_ID_CACHE.clone().unwrap_or_else(|| "unknown".to_string()),
        "feedback": feedback,
    });

    println!("INFO: 提交反馈的请求体: {:?}", request_body);

    // 创建 HTTP 客户端
    let client = Client::new();

    // 执行 POST 请求
    let response = client
        .post("https://cloudprint.xiaohongshu.com/api/edith/v1/cloudprint/client/submitfeedback")
        .json(&request_body) // 发送JSON请求体
        .send()
        .await
        .map_err(|e| {
            let error_message = format!("ERROR: 反馈提交失败: {:?}", e);
            println!("{}", error_message);
            error_message
        })?;

    // 检查响应状态码
    if !response.status().is_success() {
        let error_message = format!("ERROR: 反馈提交失败, 状态码: {:?}", response.status());
        println!("{}", error_message);
        return Err(error_message);
    }

    // 解析响应
    let response_json: serde_json::Value = response.json().await.map_err(|e| {
        let error_message = format!("ERROR: 解析反馈响应失败: {:?}", e);
        println!("{}", error_message);
        error_message
    })?;

    // 确认反馈是否成功提交
    println!("INFO: 反馈提交成功, 响应体: {:?}", response_json);

    // 返回结果
    Ok("反馈成功提交".to_string())
}
