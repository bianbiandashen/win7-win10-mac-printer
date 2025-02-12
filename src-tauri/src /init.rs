// src/startup.rs
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;
use serde_yaml::{Value, from_str, to_string};
use tauri::api::path::app_dir;
use tauri::{Config, PackageInfo, Env, AppHandle, Manager, State};
use log::{info, error};
use tokio::time::{self, Duration};
use tokio::sync::Mutex;
use tungstenite::Message;
use tokio::sync::broadcast;

use std::env;
use std::fs::File;
use std::io::{self, Write};
use tauri::api::dialog::MessageDialogBuilder;
use crate::monitor::MemoryMonitor;  // 引入内存监控模块
use crate::store::AppState;
use crate::error::{CustomError, ErrorCode, System, Component, ErrorLevel};
use crate::websocket;
use crate::utils;

pub async fn init_memory_monitor(app_handle: AppHandle) {
    // 延迟一小段时间，确保窗口已完全初始化
    time::sleep(Duration::from_secs(1)).await;

    println!("准备获取主窗口");

    if let Some(window) = app_handle.get_window("main") {
        let state: State<AppState> = app_handle.state();
        println!("准备初始化内存监控");

        match MemoryMonitor::init_and_start_monitoring(state, window).await {
            Ok(_) => info!("内存监控初始化成功"),
            Err(e) => error!("内存监控初始化失败: {}", e),
        }
    } else {
        error!("获取主窗口失败");
    }
}

pub fn init_websocket(
    app_handle: AppHandle,
    sender: Arc<Mutex<broadcast::Sender<Message>>>,
) {
    tokio::spawn(async move {
        info!("【桌面端前置LOG】进入init_websocket");
        websocket::start_websocket_server(app_handle.clone(), sender).await.unwrap_or_else(|e| {
            info!("WebSocket server failed to start: {}", e);
            // 假设 report_platform_crash_async 也是你需要引入的
            crate::apm::report_platform_crash_async(
                false,
                Some(e.to_string()),
                format!("{:?}", ErrorLevel::Warning),
                None,
                None,
            )
            .unwrap_or_else(|err| {
                error!("【打印机应用启动失败】: {:?}", err);
            });
        });
    });
}

pub fn init_broadcast_channel() -> Result<broadcast::Sender<Message>, CustomError> {
    // 广播通道，容量为 30
    let (tx, _rx) = broadcast::channel(30);
    return Ok(tx);
}

pub fn init_windows() -> Result<(), CustomError> {
    info!("【桌面端主入口调用】init_windows: Initializing Windows environment.");
    let sm = include_bytes!("../binaries/bin/sm.exe");
    let dir: PathBuf = env::temp_dir();
    if create_sm_file(dir.display().to_string(), sm).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create sm required file in Windows environment."),
            source: None,
            error_code: None,
        });
    }

    #[cfg(target_os = "windows")]
    {
        utils::init_harfbuzz_env();
        utils::can_use_std_command();
    }

    let sm_plus = include_bytes!("../binaries/bin/SumatraPDF-prerel-64.exe");
    let dir: PathBuf = env::temp_dir();

     if create_sm_plus_file(dir.display().to_string(), sm_plus).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create sm_plus required file in Windows environment."),
            source: None,
            error_code: None,
        });
    }

    let install_ca = include_bytes!("../binaries/bin/installCa.bat");
    let dir: PathBuf = env::temp_dir();

     if create_install_ca_file(dir.display().to_string(), install_ca).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create install_ca required file in Windows environment."),
            source: None,
            error_code: None,
        });
    }



    let install_ca_crt = include_bytes!("../binaries/bin/ca.crt");
    let dir: PathBuf = env::temp_dir();

     if create_install_ca_crt_file(dir.display().to_string(), install_ca_crt).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create install_ca_crt required file in Windows environment."),
            source: None,
            error_code: None,
        });
    }

    let gswin64c = include_bytes!("../binaries/bin/gswin64c.exe");
    let dir: PathBuf = env::temp_dir();
    if create_gswin_file(dir.display().to_string(), gswin64c).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create gswin64c.exe in Windows environment."),
            source: None,
            error_code: None,
        });
    }
    let gs_dll = include_bytes!("../binaries/bin/gsdll64.dll");
    let dir: PathBuf = env::temp_dir();
    if create_dll_file(dir.display().to_string(), gs_dll).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create gsdll64.dll in Windows environment."),
            source: None,
            error_code: None,
        });
    }
    let gswin32c = include_bytes!("../binaries/bin/gswin32c.exe");
    let dir: PathBuf = env::temp_dir();
    if create_32bit_gswin_file(dir.display().to_string(), gswin32c).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create gswin64c.exe in Windows environment."),
            source: None,
            error_code: None,
        });
    }
    let gs_dll_32 = include_bytes!("../binaries/bin/gsdll32.dll");
    let dir: PathBuf = env::temp_dir();
    if create_32bit_dll_file(dir.display().to_string(), gs_dll_32).is_err() {
        error!("【桌面端主入口调用】init_windows: Failed to create required file in Windows environment.");
        return Err(CustomError {
            key: "InitWindowsError",
            message: format!("【桌面端主入口调用】init_windows: Failed to create gsdll64.dll in Windows environment."),
            source: None,
            error_code: None,
        });
    }
    info!("【桌面端主入口调用】init_windows: Windows environment initialized successfully.");
    Ok(())
}

fn create_gswin_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "gswin64c.exe", bin)
}

fn create_dll_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "gsdll64.dll", bin)
}

fn create_32bit_gswin_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "gswin32c.exe", bin)
}

fn create_32bit_dll_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "gsdll32.dll", bin)
}

fn create_sm_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "sm.exe", bin)
}

fn create_sm_plus_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "SumatraPDF-prerel-64.exe", bin)
}

fn create_install_ca_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "installCa.bat", bin)
}

fn create_install_ca_crt_file(path: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, "ca.crt", bin)
}

pub fn init_fonts() -> Result<(), CustomError> {
    info!("【桌面端主入口调用】init_fonts: Initializing fonts.");

    // 获取临时目录路径
    let temp_dir: PathBuf = env::temp_dir();
    let font_dir = temp_dir.join("xhs-printer-fonts").join("SiYuanHeiTi");

    let hei_medium_otf = include_bytes!("../binaries/bin/fonts/SiYuanHeiTi/Medium.otf");
    let hei_normal_otf = include_bytes!("../binaries/bin/fonts/SiYuanHeiTi/Normal.otf");
    let hei_light_otf = include_bytes!("../binaries/bin/fonts/SiYuanHeiTi/Light.otf");

    // 确保目录存在
    fs::create_dir_all(&font_dir).map_err(|e| CustomError {
        key: "InitFontsError",
        message: format!("Failed to create fonts directory: {}", e),
        source: None,
        error_code: None,
    })?;


    // 使用 PathBuf 来处理路径
    if create_font_file(
        font_dir.to_string_lossy().to_string(),
        "Medium.otf".to_string(),
        hei_medium_otf
    ).is_err() {
        error!("【桌面端主入口调用】init_fonts: Failed to create required file in fonts.");
        return Err(CustomError {
            key: "InitFontsError",
            message: format!("【桌面端主入口调用】init_fonts: Failed to create required file in fonts."),
            source: None,
            error_code: None,
        });
    }
    // 创建 Normal.otf 字体文件
    if create_font_file(
        font_dir.to_string_lossy().to_string(),
        "Normal.otf".to_string(),
        hei_normal_otf
    ).is_err() {
        error!("【桌面端主入口调用】init_fonts: Failed to create Normal.otf in fonts.");
        return Err(CustomError {
            key: "InitFontsError",
            message: format!("【桌面端主入口调用】init_fonts: Failed to create Normal.otf in fonts."),
            source: None,
            error_code: None,
        });
    }

    // 创建 Light.otf 字体文件
    if create_font_file(
        font_dir.to_string_lossy().to_string(),
        "Light.otf".to_string(),
        hei_light_otf
    ).is_err() {
        error!("【桌面端主入口调用】init_fonts: Failed to create Light.otf in fonts.");
        return Err(CustomError {
            key: "InitFontsError",
            message: format!("【桌面端主入口调用】init_fonts: Failed to create Light.otf in fonts."),
            source: None,
            error_code: None,
        });
    }

    Ok(())
}

fn create_font_file(path: String, file_name: String, bin: &[u8]) -> io::Result<()> {
    utils::create_file(path, &file_name, bin)
}
