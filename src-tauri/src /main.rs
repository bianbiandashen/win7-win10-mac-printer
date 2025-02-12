// main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use crate::error::{Component, CustomError, ErrorCode, ErrorLevel, System};
use crate::log::{get_log_directory, get_log_snapshot_id, AsyncLogger, LogOptions};
use ::log::{error, info};
use apm::{report_platform_crash, report_platform_crash_async};
use base64;
use event::init_event_app_handle;
use lazy_static::lazy_static;
use monitor::MemoryMonitor; // 引入内存监控模块
use serde_json::json;
use serde_json::Value;
use std::env;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, Write};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use store::{AppState, PreSpawnedProcess}; // 引入上面定义的 AppState 和 PreSpawnedProcess
use tauri::api::dialog::MessageDialogBuilder;
use tauri::api::http::{ClientBuilder, HttpRequestBuilder};
use tauri::State; // 用于获取应用状态
use tauri::{
    AppHandle, Config, CustomMenuItem, Env, Manager, PackageInfo, SystemTray, SystemTrayEvent,
    SystemTrayMenu, WindowEvent,
};
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tungstenite::Message; // 导入 CommandExt 特性

use once_cell::sync::Lazy;
use single_instance::SingleInstance;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::command;
// 引入模块
mod aes;
mod apikit;
mod apm;
mod asyncfunc;
mod declare;
mod error;
mod event;
mod fileserver;
mod fsys;
mod globalcache;
mod harfbuzz;
mod init;
mod log;
mod macos;
mod monitor;
mod onixcomponents;
mod previewpdf;
mod printer;
mod printpdf;
mod scripts;
mod startup;
mod store;
mod utils;
mod websocket;
mod windows10;
mod windows7;
mod windows_version;
mod wss;

use warp::filters;
use warp::reply::Html;
use warp::Filter;

lazy_static! {
    pub static ref APP_HANDLE: Arc<Mutex<Option<AppHandle>>> = Arc::new(Mutex::new(None));
    pub static ref ASYNC_LOGGER: Arc<Mutex<Option<AsyncLogger>>> = Arc::new(Mutex::new(None));
    pub static ref POWERSHELL_PATH: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
}

/// 定义全局标记，标记当前实例是否为重复实例
static IS_DUPLICATE: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

// 使用 Arc 和 tokio 的异步 Mutex 定义 SharedSender 类型 共享的 WebSocket 发送者
type SharedSender = Arc<Mutex<broadcast::Sender<tungstenite::Message>>>;

/// 主入口异步运行
#[tokio::main]
async fn main() {
    // 设置 Tokio 线程池大小为 500
    std::env::set_var("TOKIO_THREAD_POOL_SIZE", "500");
    // 通过 single_instance 检测当前实例是否为主实例
    let instance =
        SingleInstance::new("my_unique_app_identifier").expect("创建 SingleInstance 失败");
    if !instance.is_single() {
        println!("检测到重复应用实例，激活已有窗口，退出当前进程");
        // 如果不是主实例，则将标记设为 true
        IS_DUPLICATE.store(true, Ordering::SeqCst);
        std::process::exit(0)
    }

    // 重要提示：确保 instance 变量在整个应用生命周期内都不被释放，
    // 否则锁会被释放，允许其他实例启动。这里我们直接在 main 函数中保持它的作用域。

    // 输出当前标记，用于调试
    println!(
        "当前 IS_DUPLICATE 值：{}",
        IS_DUPLICATE.load(Ordering::SeqCst)
    );
    match run().await {
        Ok(_) => {
            info!("【Main执行成功】");

            // 在服务器启动后进行连接测试
            fileserver::check_server_connection().await;
            // 上报启动成功
            if let Err(report_err) =
                report_platform_crash(false, None, format!("{:?}", ErrorLevel::Info), None, None)
                    .await
            {
                error!("【成功状态上报失败】: {:?}", report_err);
            }
        }
        Err(e) => {
            info!("【Main执行失败】: {:?}, {:?}", e.key, e.message);
            // 上报Crash日志
            if let Err(report_err) = report_platform_crash(
                true,
                Some(e.message.clone()),
                format!("{:?}", ErrorLevel::Error),
                None,
                Some(format!("{:?}", e.key)),
            )
            .await
            {
                error!("【错误报告失败】: {:?}", report_err);
            }
            let app_handle = APP_HANDLE.lock().await;
            if let Some(handle) = &*app_handle {
                let log_options = LogOptions::new(Some(3), None);
                info!("【桌面端主入口调用】log_options: ");
                if let Err(err) = get_log_snapshot_id(Some(log_options), handle.clone()).await {
                    info!("【日志ID获取失败】: {:?}", err);
                } else {
                    info!("【桌面端主入口调用】日志ID获取成功");
                }
            } else {
                info!("【桌面端主入口调用】app_handle 为空");
            }
        }
    }
}

/// 执行行函数
async fn run() -> Result<(), CustomError> {
    let context: tauri::Context<tauri::utils::assets::EmbeddedAssets> = tauri::generate_context!();

    info!("【桌面端前置LOG】开始配置初始化");
    startup::start_prepare_config(context.package_info(), context.config())?;

    info!("【桌面端前置LOG】开始日志初始化");
    let _logger = log::init_logger(context.config()).map_err(|e| CustomError {
        key: "LoggerInitError",
        message: format!("【桌面端前置ERROR】日志初始化失败: {}", e),
        source: Some(e),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x015,
        )),
    })?;

    info!("【桌面端前置LOG】日志初始化成功");
    utils::calculate_optimal_workers();
    let mut async_logger_lock = ASYNC_LOGGER.lock().await;
    *async_logger_lock = Some(_logger.clone());

    let broadcast = init::init_broadcast_channel().map_err(|e| CustomError {
        key: "BroadcastChannelError",
        message: format!("【桌面端前置ERROR】广播通道初始化失败: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x016,
        )),
    })?;

    #[cfg(target_os = "windows")]
    get_c_drive_usage();
    {
        if let Err(e) = init::init_windows() {
            return Err(CustomError {
                key: "InitWindowsError",
                message: format!("【桌面端前置ERROR】Windows 环境初始化失败: {}", e),
                source: Some(Box::new(e)),
                error_code: Some(ErrorCode::new(
                    System::GeneralReport,
                    Component::StartingModule,
                    0x017,
                )),
            });
        }

        // PowerShell 路径检查并赋值
        #[cfg(target_os = "windows")]
        check_powershell_path_and_store();
        // 注意：现在不再处理返回的 Result
    }

    if let Err(e) = init::init_fonts() {
        return Err(CustomError {
            key: "InitFontsError",
            message: format!("【桌面端前置ERROR】字体初始化失败: {}", e),
            source: Some(Box::new(e)),
            error_code: Some(ErrorCode::new(
                System::GeneralReport,
                Component::StartingModule,
                0x018,
            )),
        });
    }

    info!("【桌面端前置LOG】开始初始化打印任务队列和监控");
    // 初始化任务队列和监控
    printpdf::initialize_task_queue().await;
    info!("【桌面端前置LOG】打印任务队列和监控初始化成功");

    info!("【桌面端前置LOG】开始打印机应用");
    start_printer_app(context, _logger, broadcast)?;
    info!("【桌面端前置LOG】打印机应用启动成功");
    Ok(())
}

/// 在 Windows 上预先启动 N 个 PowerShell 子进程的函数
#[cfg(target_os = "windows")]
fn spawn_powershell_processes(n: usize) -> Result<Vec<PreSpawnedProcess>, CustomError> {
    use std::os::windows::process::CommandExt; // 引入 Windows 特定的 CommandExt trait

    let mut processes = Vec::new();

    // 使用绝对路径确保 PowerShell 可执行文件被正确找到
    // 根据不同的 Windows 版本，路径可能有所不同，这里假设使用 Windows PowerShell
    let powershell_path = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";

    for i in 0..n {
        let id = format!("powershell_child_{}", i);

        // 记录创建 Command 的开始时间
        let command_start = Instant::now();

        // 创建并启动 PowerShell 子进程，添加 creation_flags
        let child = std::process::Command::new(powershell_path)
            .args(&["-NoProfile", "-WindowStyle", "Hidden", "-Command", "-"])
            .creation_flags(0x08000000 | 0x04000000) // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| CustomError {
                key: "PowerShellSpawnError",
                message: format!("【桌面端前置ERROR】预启动 PowerShell 进程失败: {}", e),
                source: Some(Box::new(e)),
                error_code: Some(ErrorCode::new(
                    System::GeneralReport,
                    Component::StartingModule,
                    0x019,
                )),
            })?;

        let command_duration = command_start.elapsed();
        info!(
            "【桌面端前置LOG】预启动子进程: {} 花费时间: {:?}",
            id, command_duration
        );

        processes.push(PreSpawnedProcess { id, child });
    }

    Ok(processes)
}

#[tauri::command]
async fn initialize_websocket(app_handle: AppHandle) {
    // 通过解引用获取实际的 Arc<Mutex<_>>
    let sender_arc = app_handle.state::<SharedSender>();
    let sender_arc = sender_arc.inner().clone();
    let mut sender = sender_arc.lock().await;

    // 调用 WebSocket 初始化函数
    init::init_websocket(app_handle, sender_arc.clone());
}

#[cfg(target_os = "windows")]
fn get_disk_space_info(drive_path: &str) -> Result<(u64, u64, u64), windows::core::Error> {
    use windows::{
        core::{Error as WinError, PCWSTR},
        Win32::Storage::FileSystem::GetDiskFreeSpaceExW,
    };

    // Convert the drive path to a wide string (UTF-16) and null-terminate it
    let mut wide_path: Vec<u16> = drive_path.encode_utf16().collect();
    wide_path.push(0);

    let mut free_bytes_available: u64 = 0;
    let mut total_number_of_bytes: u64 = 0;
    let mut total_number_of_free_bytes: u64 = 0;

    unsafe {
        // Call the Win32 API: GetDiskFreeSpaceExW
        let ret = GetDiskFreeSpaceExW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut free_bytes_available),
            Some(&mut total_number_of_bytes),
            Some(&mut total_number_of_free_bytes),
        );

        // Check if the result is Ok
        if ret.is_ok() {
            let used_bytes = total_number_of_bytes - total_number_of_free_bytes;
            Ok((
                total_number_of_bytes,
                total_number_of_free_bytes,
                used_bytes,
            ))
        } else {
            // If the call fails, return Windows error information
            Err(WinError::from_win32())
        }
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn get_c_drive_usage() -> Result<Value, String> {
    match get_disk_space_info("C:\\") {
        Ok((total, free, used)) => {
            let gb = 1024 * 1024 * 1024;
            let result = json!({
                "total_gb": total / gb,
                "free_gb": free / gb,
                "used_gb": used / gb,
            });
            Ok(result)
        }
        Err(e) => {
            // 将 windows::core::Error 转成字符串
            Err(format!("Windows Error: {:?}", e))
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn get_c_drive_usage() -> Result<(), String> {
    Err("此命令仅在 Windows 平台可用".into())
}

#[cfg(target_os = "macos")]
#[command]
fn open_temp_directory() -> Result<(), String> {
    // 空函数，不执行任何操作
    Ok(())
}

#[cfg(target_os = "windows")]
#[command]
fn open_temp_directory() -> Result<(), String> {
    let temp_path = "C:\\";
    Command::new("explorer")
        .arg(temp_path)
        .spawn()
        .map_err(|e| format!("Failed to open temp directory: {}", e))?;
    Ok(())
}

/// 启动打印机应用
fn start_printer_app(
    context: tauri::Context<tauri::utils::assets::EmbeddedAssets>,
    _logger: log::AsyncLogger,
    tx: broadcast::Sender<Message>,
) -> Result<(), CustomError> {
    let sender: SharedSender = Arc::new(Mutex::new(tx));
    let sender_clone = sender.clone();

    let start_time = utils::get_start_time()?;
    let temp_dir = env::temp_dir();
    tokio::spawn(fileserver::start_server(Arc::new(Mutex::new(
        temp_dir.clone(),
    ))));

    let app_state = AppState::new(start_time);

    tauri::Builder::default()
        .setup({
            // 如果是主实例，则继续执行初始化代码
            println!("show111333");
            println!(
                "IS_DUPLICATE 初始值setup: {}",
                IS_DUPLICATE.load(Ordering::SeqCst)
            );
            move |app| {
                let event_handle = app.handle();
                init_event_app_handle(event_handle);

                let app_handle = app.handle();
                {
                    let mut app_handle_lock = futures::executor::block_on(APP_HANDLE.lock());
                    *app_handle_lock = Some(app_handle.clone());
                }

                let monitor_handle = app_handle.clone();
                tokio::spawn(async move {
                    init::init_memory_monitor(monitor_handle).await;
                });

                if let Some(existing_window) = app.get_window("main") {
                    existing_window.set_focus().unwrap();
                    return Ok(());
                }
                let main_window = app.get_window("main").unwrap();
                main_window
                    .emit("show-window", None::<()>)
                    .expect("failed to emit event");

                Ok(())
            }
        })
        .system_tray(utils::setup_system_tray())
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::LeftClick {
                position: _,
                size: _,
                ..
            } => {
                println!("system tray received a left click");
            }
            SystemTrayEvent::DoubleClick {
                position: _,
                size: _,
                ..
            } => {
                let handle = app.get_window("main").unwrap();
                if handle.is_minimized().unwrap() {
                    handle.unminimize().unwrap();
                }
                handle.show().unwrap();
                handle.set_focus().unwrap();
                info!("【桌面端系统托盘LOG】Tauri 双击");
            }
            SystemTrayEvent::MenuItemClick { id, .. } => {
                let handle = app.get_window("main").unwrap();
                match id.as_str() {
                    "show" => {
                        if handle.is_minimized().unwrap() {
                            handle.unminimize().unwrap();
                        }
                        handle.show().unwrap();
                        handle.set_focus().unwrap();
                        info!("【桌面端系统托盘LOG】Tauri 点击打开");
                    }
                    "quit" => {
                        // 检查并清理 Y:
                        #[cfg(target_os = "windows")]
                        {
                            use std::os::windows::process::CommandExt;
                            use std::process::Command;
                            // 检查 Y: 是否已映射
                            if let Ok(output) = Command::new("cmd")
                                .args(&["/C", "subst"])
                                .creation_flags(0x08000000)
                                .output()
                            {
                                let subst_list = String::from_utf8_lossy(&output.stdout);
                                if subst_list.contains("X:") {
                                    // 清理 X: 驱动器映射
                                    if let Err(e) = Command::new("cmd")
                                        .args(&["/C", "subst X: /D"])
                                        .creation_flags(0x08000000)
                                        .output()
                                    {
                                        error!("【桌面端】清理驱动器映射X失败: {}", e);
                                    }
                                }
                            }
                        }

                        info!("【桌面端系统托盘LOG】Tauri 点击退出");
                        std::process::exit(0);
                    }
                    _ => {}
                }
            }
            _ => {}
        })
        .manage(_logger)
        .manage(sender_clone)
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            apm::report_custom_measurement,
            apm::get_last_build_version,
            apm::get_last_build_version_macos,
            globalcache::set_user_agent,
            macos::print_pdf_macos,
            macos::get_jobs_macos,
            log::log_frontend,
            log::get_log_snapshot_id,
            log::get_log_directory,
            log::submit_feedback,
            printer::get_default_printer,
            printer::create_temp_file,
            printer::create_temp_image_file,
            printer::remove_temp_file,
            printer::get_printers,
            printer::get_jobs,
            printer::open_system_jobs,
            websocket::check_websocket_connection,
            websocket::send_message_to_websocket,
            utils::open_file,
            utils::get_version_from_config,
            utils::get_system_info,
            utils::get_build_version,
            utils::is_window_visible,
            utils::install_ca_fun,
            utils::fetch_image,
            utils::get_os_type,
            apikit::send_request_command,
            printpdf::print_pdf,
            printpdf::start_print_pdf, // 确保这里使用优化后的命令函数
            previewpdf::start_preview_print_pdf,
            previewpdf::check_preview_status,
            printpdf::add_batch_print_tasks,
            printpdf::update_printer_sequence,
            aes::decrypt_aes_encrypted_string,
            startup::enable_auto_launch,
            initialize_websocket,
            get_c_drive_usage,
            open_temp_directory
        ])
        .on_window_event(|event| {
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                api.prevent_close();
                let window_label = event.window().label().to_string();
                event
                    .window()
                    .emit("close-requested", Some(window_label))
                    .unwrap();
            }
        })
        .run(context)
        .map_err(|e| CustomError {
            key: "StartPrinterAppError",
            message: format!("【桌面端前置ERROR】打印机应用启动失败: {}", e),
            source: Some(Box::new(e)),
            error_code: Some(ErrorCode::new(
                System::GeneralReport,
                Component::StartingModule,
                0x007,
            )),
        })?;

    info!("【桌面端前置LOG】Tauri 应用程序运行成功");
    Ok(())
}

#[cfg(target_os = "windows")]
fn check_powershell_path_and_store() {
    use std::path::Path;

    // 执行 where powershell 命令
    let output = std::process::Command::new("cmd")
        .args(&["/C", "where", "powershell"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();

    // 定义默认的 PowerShell 路径
    let default_powershell_path =
        r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string();

    // 根据 where 命令的执行结果决定使用哪一个路径
    let path_to_set = match output {
        Ok(output) if output.status.success() => {
            let path_str = String::from_utf8_lossy(&output.stdout);
            match path_str.lines().next() {
                Some(path) => path.trim().to_string(),
                None => {
                    // 如果 where 命令成功但未找到路径，使用默认路径
                    error!(
                        "【桌面端】where powershell 命令成功但未找到路径，使用默认路径: {}",
                        default_powershell_path
                    );
                    default_powershell_path.clone()
                }
            }
        }
        _ => {
            // 如果 where 命令失败，使用默认路径
            error!(
                "【桌面端】where powershell 命令失败，使用默认路径: {}",
                default_powershell_path
            );
            default_powershell_path.clone()
        }
    };

    // 检查路径是否存在，如果不存在，记录错误但继续执行
    if !Path::new(&path_to_set).exists() {
        error!("【桌面端】指定的 PowerShell 路径不存在: {}", path_to_set);
        // 你可以选择在这里进一步处理，比如提示用户或尝试其他路径
        // 这里不返回错误，继续执行
    }

    // 尝试获取锁并赋值路径
    match POWERSHELL_PATH.try_lock() {
        Ok(mut path_lock) => {
            *path_lock = path_to_set.clone();
            info!("【桌面端】PowerShell 路径已设置: {}", path_to_set);
        }
        Err(_) => {
            // 如果无法获取锁，记录错误但不阻断程序
            error!(
                "【桌面端】无法获取 PowerShell 路径锁，无法设置路径: {}",
                path_to_set
            );
            // 这里选择不做进一步处理，让 main 函数继续执行
        }
    }

    // 不返回任何结果，确保函数不会中断程序执行
}
