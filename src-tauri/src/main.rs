#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use std::fs::File;
use std::process::Command;
use std::io::prelude::*;
use std::env;
use tauri::api::http::{ClientBuilder, HttpRequestBuilder};

use futures_util::{stream::StreamExt, SinkExt};
use tauri::{Manager, State, WindowEvent};
use reqwest::Client;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;


mod windows;
mod windows7;
mod macos;
mod declare;
mod fsys;
mod utils;
mod apikit;
mod websocket;
mod apm;
use apm::is_windows_7_or_newer;

// Define AppState 主要是要要给apm.rs 共享使用
#[derive(Clone)]
pub struct AppState {
    pub user_agent: Arc<Mutex<Option<String>>>,
    pub start_time: u128, // Store the timestamp in milliseconds
}

#[tauri::command]
fn remove_job(printername: String, jobid: String) -> String {
    println!("main  remove_job");
    if cfg!(windows) {
        return windows::remove_job(printername, jobid);
    }
    "Unsupported OS".to_string()
}

#[tauri::command(rename_all = "snake_case")]
fn create_temp_file(buffer_data: String, filename: String) -> String {
    println!("main create_temp_file");
    let dir = env::temp_dir();
    let result = fsys::create_file_from_base64(buffer_data.as_str(), &format!("{}{}", dir.display(), filename));
    if result.is_ok() {
        return format!("{}{}", dir.display(), filename);
    }
    "".to_owned()
}

#[tauri::command(rename_all = "snake_case")]
fn remove_temp_file(filename: String) -> bool {
    println!("main remove_temp_file");
    let dir = env::temp_dir();
    let result = fsys::remove_file(&format!("{}{}", dir.display(), filename));
    result.is_ok()
}



#[tauri::command(rename_all = "snake_case")]
fn get_printers() -> String {
    println!("Initializing get_printers function.");

    if cfg!(windows) {
        println!("Running on Windows.");
        unsafe {
            if is_windows_7_or_newer() {
                println!("Detected Windows 7 or newer.");
                let result = windows::get_printers();
                println!("Result from get_printers: {}", result); // Print result
                return result;
            } else {
                println!("检测到 Win7 系统。");
                let result = windows7::get_printers_win7();
                println!("Result from get_printers_win7: {}", result); // Print result
                return result;
            }
        }
    } else {
        println!("Running on macOS.");
        let result = macos::get_printers_macos();
        println!("Result from get_printers_macos: {}", result); // Print result
        return result;
    }
}


#[tauri::command]
fn get_printers_by_name(printername: String) -> String {
    println!("main get_printers_by_name");
    if cfg!(windows) {
        unsafe {
            if is_windows_7_or_newer() {
                println!("Detected Windows 7 or newer. get_printers_by_name ");
                let result = windows::get_printers_by_name(printername);
                println!("Result from get_printers_by_name above win7: {}", result); // Print result
                return result;
            } else {
                println!("检测到 Win7 系统。 get_printers_by_name");
                let result = windows7::get_printers_by_name_win7(printername);
                println!("Result from get_printers_by_name win7: {}", result); // Print result
                return result;
            }
        }
    }
    "Unsupported OS".to_string()
}




#[tauri::command]
fn print_pdf(id: String, path: String, printer_setting: String, remove_after_print: bool) -> String {
    println!("main print_pdf");
    
    let options = declare::PrintOptions {
        id: id.clone(),
        path: path.clone(),
        print_setting: printer_setting.clone(),
        remove_after_print,
    };
    
    if cfg!(windows) {
        unsafe {
            if is_windows_7_or_newer() {
            match windows::print_pdf(options) {
                Ok(_) => "Windows-打印成功".to_string(),
                Err(err) => format!("Windows-打印失败: {}", err),
            }} else {
            match windows7::print_pdf_win7(options) {
                Ok(_) => "Windows7-打印成功".to_string(),
                Err(err) => format!("Windows7-打印失败: {}", err),
            }
            } 
        }

        // return windows::print_pdf(options);
    } else if cfg!(target_os = "macos") {
        // macOS 处理逻辑
        match macos::print_pdf_macos(id.clone(), path.clone(), printer_setting.clone(), remove_after_print) {
            Ok(_) => "MacOS-打印成功".to_string(),
            Err(err) => format!("MacOS-打印失败: {}", err),
        }
    } else {
        "Unsupported OS".to_string()
    }
}

#[tauri::command(rename_all = "snake_case")]
fn get_jobs(printer_name: String) -> String {
    println!("main get_jobs");
    if cfg!(windows) {
        unsafe {
            if is_windows_7_or_newer() {
                println!("Detected Windows 7 or newer. get_printers_by_name ");
                let result = windows::get_jobs(printer_name);
                println!("Result from get_jobs above win7: {}", result); // Print result
                return result;
            } else {
                println!("检测到 Win7 系统。 get_printers_by_name");
                let result = windows7::get_jobs_win7(printer_name);
                println!("Result from get_jobs win7: {}", result); // Print result
                return result;
            }
        }
        
    } else if cfg!(target_os = "macos") {
        return macos::get_jobs_macos(&printer_name);
    }
    "Unsupported OS".to_string()
}

#[tauri::command(rename_all = "snake_case")]
fn get_jobs_by_id(printername: String, jobid: String) -> String {
    println!("main get_jobs_by_id");
    if cfg!(windows) {
        return windows::get_jobs_by_id(printername, jobid);
    }
    "Unsupported OS".to_string()
}

#[tauri::command(rename_all = "snake_case")]
fn resume_job(printername: String, jobid: String) -> String {
    println!("main resume_job");
    if cfg!(windows) {
        return windows::resume_job(printername, jobid);
    }
    "Unsupported OS".to_string()
}

#[tauri::command]
fn restart_job(printername: String, jobid: String) -> String {
    println!("main restart_job");
    if cfg!(target_os = "windows") {
        return windows::windows_restart_job(printername, jobid);
    }
    "Unsupported OS".to_string()
}

#[tauri::command]
fn pause_job(printername: String, jobid: String) -> String {
    println!("main pause_job");
    if cfg!(windows) {
        return windows::pause_job(printername, jobid);
    }
    "Unsupported OS".to_string()
}

#[tauri::command]
async fn fetch_image(url: String) -> Result<Vec<u8>, String> {
    let client = ClientBuilder::new().build().map_err(|e| e.to_string())?;
    let request = HttpRequestBuilder::new("GET", &url).map_err(|e| e.to_string())?;
    let response = client.send(request).await.map_err(|e| e.to_string())?;
    let data = response.bytes().await.map_err(|e| e.to_string())?.data;
    Ok(data.to_vec())
}

fn create_file(path: String, bin: &[u8]) -> std::io::Result<()> {
    let mut f = File::create(format!("{}sm.exe", path))?;
    f.write_all(bin)?;
    f.sync_all()?;
    Ok(())
}

pub fn init_windows() {
    let sm = include_bytes!("../binaries/bin/sm");
    let dir: std::path::PathBuf = env::temp_dir();
    if create_file(dir.display().to_string(), sm).is_err() {
        panic!("Failed to create file");
    }
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    open::that(&path).map_err(|e| e.to_string())
}
// 使用 Tokio 的多线程运行时来启动异步程序
#[tokio::main]
async fn main() {
   // 创建一个共享的 WebSocket 连接对象，用于在多个地方使用
   let ws_conn: websocket::SharedWebSocket = Arc::new(Mutex::new(None));

    // 如果是在 Windows 操作系统上，初始化 Windows 环境
    #[cfg(target_os = "windows")]
    {
        init_windows();
    }

    // 为 WebSocket 管理克隆一个连接对象
    let ws_conn_for_manage = ws_conn.clone();

    // 创建应用程序状态，获取应用启动时间（以毫秒为单位）
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH)
                     .expect("Time went backwards")
                     .as_millis();

    // 初始化应用程序状态
    let app_state = AppState {
        user_agent: Arc::new(Mutex::new(None)), // 存储用户代理信息
        start_time, // 存储应用启动时间
    };

    // 使用 Tauri 框架创建应用程序
    tauri::Builder::default()
        .setup({
            // 应用程序的初始化设置
            move |app| {
                // 获取应用程序的 handle，用于后续操作
                let app_handle = app.handle();

                // 异步启动 WebSocket 服务器
                tokio::spawn({
                    let app_handle_clone = app_handle.clone();
                    async move {
                        websocket::start_websocket_server(app_handle_clone, ws_conn.clone()).await;
                    }
                });

                Ok(())
            }
        })
        // 管理 WebSocket 连接的状态
        .manage(ws_conn_for_manage)
        // 管理刚初始化的 AppState 实例
        .manage(app_state.clone())
        // 处理用户操作调用的命令
        .invoke_handler(tauri::generate_handler![
            apm::report_custom_measurement,
            apm::set_user_agent,
            create_temp_file,
            remove_temp_file,
            get_printers,
            get_printers_by_name,
            print_pdf,
            macos::print_pdf_macos,
            get_jobs,
            get_jobs_by_id,
            resume_job,
            pause_job,
            remove_job,
            fetch_image,
            restart_job,
            websocket::check_websocket_connection,
            websocket::send_message_to_websocket,
            open_file,
            macos::get_jobs_macos,
            utils::get_version_from_config,
            apikit::send_request_command,
        ])
        // 处理窗口事件，如窗口关闭请求
        .on_window_event(|event| {
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                // 阻止窗口关闭
                api.prevent_close();
                // 获取窗口的标签，并发送自定义关闭请求事件到前端
                let window_label = event.window().label().to_string();
                event.window().emit("close-requested", Some(window_label)).unwrap();
            }
        })
        // 运行 Tauri 应用程序
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
