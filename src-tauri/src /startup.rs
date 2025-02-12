// src/startup.rs
use crate::error::{Component, CustomError, ErrorCode, ErrorLevel, System};
use crate::monitor::MemoryMonitor; // 引入内存监控模块
use crate::store::AppState;
use crate::websocket;
use auto_launch::AutoLaunch;
use log::{error, info};
use serde_yaml::{from_str, to_string, Value};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::api::path::app_dir;
use tauri::{AppHandle, Config, Env, Manager, PackageInfo, State};
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use tungstenite::Message;

pub fn prepare_config(config: &Config, source: &str) -> Result<(), CustomError> {
    println!("开始准备配置...");

    // 获取应用程序的配置目录
    let base_path = app_dir(config).ok_or(CustomError {
        key: "ConfigDirError",
        message: "无法获取配置目录".to_string(),
        source: None,
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x008,
        )),
    })?;
    println!("应用程序的配置目录: {}", base_path.display());

    // 定义源文件和目标文件路径
    let destination = base_path.join("log4rs.yaml"); // 目标路径
    println!("源文件路径: {}", source);
    println!("目标文件路径: {}", destination.display());

    // 检查目标文件是否已存在
    if PathBuf::from(&destination).exists() {
        println!(
            "配置文件 \"{}\" 已经存在，跳过复制。",
            destination.display()
        );
        return Ok(());
    }

    // 创建目标目录（如果不存在）
    println!("创建目标目录: {}", base_path.display());
    fs::create_dir_all(&base_path).map_err(|e| CustomError {
        key: "CreateDirError",
        message: format!("无法创建{}目录: {}", base_path.display(), e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x009,
        )),
    })?;

    // 复制 log4rs 文件到目标目录
    println!("复制源文件到目标目录...");
    fs::copy(&source, &destination).map_err(|e| CustomError {
        key: "CopyFileError",
        message: format!("无法复制 log4rs 文件: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x010,
        )),
    })?;

    // 读取 log4rs 配置文件
    println!("读取 log4rs 配置文件...");
    let log4rs_content = fs::read_to_string(&destination).map_err(|e| CustomError {
        key: "ReadFileError",
        message: format!("无法读取 log4rs 配置文件: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x011,
        )),
    })?;
    println!("读取的配置内容:\n{}", log4rs_content);

    // 解析 YAML 内容
    println!("解析 YAML 配置内容...");
    let mut yaml_value: Value = from_str(&log4rs_content).map_err(|e| CustomError {
        key: "ParseYamlError",
        message: format!("解析 log4rs 配置失败: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x012,
        )),
    })?;

    // 修改 appenders 中的 path
    println!("修改 appenders 的路径...");
    if let Some(appenders_value) = yaml_value.get_mut("appenders") {
        if let Some(appenders) = appenders_value.as_mapping_mut() {
            for (_, appender_value) in appenders {
                if let Some(appender_map) = appender_value.as_mapping_mut() {
                    if let Some(path) = appender_map.get_mut("path").and_then(|p| p.as_str()) {
                        let absolute_path = format!("{}/{}", base_path.display(), path);
                        println!("更新 appender 路径为: {}", absolute_path);
                        *appender_map.get_mut("path").unwrap() = absolute_path.into();
                    }
                }
            }
        }
    }

    // 将修改后的内容写回到文件
    println!("序列化修改后的配置...");
    let updated_log4rs_content = to_string(&yaml_value).map_err(|e| CustomError {
        key: "SerializeYamlError",
        message: format!("将修改后的配置序列化失败: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x013,
        )),
    })?;
    println!("写入更新后的配置文件...");
    fs::write(&destination, updated_log4rs_content).map_err(|e| CustomError {
        key: "WriteFileError",
        message: format!("无法写入更新后的 log4rs 配置文件: {}", e),
        source: Some(Box::new(e)),
        error_code: Some(ErrorCode::new(
            System::GeneralReport,
            Component::StartingModule,
            0x014,
        )),
    })?;
    println!("配置文件已更新: \"{}\"", destination.display());

    println!("配置准备完成.");
    Ok(())
}

pub fn start_prepare_config(
    package_info: &PackageInfo,
    config: &Config,
) -> Result<(), CustomError> {
    let env = Env::default();
    // 获取资源目录路径，根据不同的操作系统动态获取资源路径
    let log4rs_path = tauri::api::path::resource_dir(package_info, &env)
        .map(|mut path| {
            path.push("binaries/bin/log4rs.yaml");
            path
        })
        .ok_or_else(|| CustomError {
            key: "GetLog4rsPathError",
            message: "【桌面端前置ERROR】获取log4rs资源目录失败".to_string(),
            source: None,
            error_code: None,
        })?;

    info!(
        "【桌面端前置LOG】log4rs 配置文件路径: {}",
        log4rs_path.display()
    );

    // 调用配置准备函数
    prepare_config(
        config,
        log4rs_path
            .to_str()
            .expect("【桌面端前置ERROR】配置准备失败"),
    )?;

    Ok(())
}

#[tauri::command]
pub fn enable_auto_launch(enable: bool) -> Result<String, String> {
    let app_path = std::env::current_exe()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let app_name = Path::new(&app_path)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    info!("【桌面端 自动启动功能】开始配置自动启动功能");
    info!("【桌面端 自动启动功能】应用名称: {}", app_name);
    info!("【桌面端 自动启动功能】应用路径: {}", app_path);

    // 修复：将 app_name 和 app_path 转换为 &str
    #[cfg(target_os = "windows")]
    let auto_launch = AutoLaunch::new(&app_name, &app_path); // Windows 平台使用 2 个参数

    #[cfg(target_os = "macos")]
    let auto_launch = AutoLaunch::new(&app_name, &app_path, true, false); // macOS 平台使用 4 个参数

    if enable {
        log::info!("【桌面端 自动启动功能】尝试启用自动启动功能");
        match auto_launch.enable() {
            Ok(_) => {
                info!("【桌面端 自动启动功能】自动启动功能已成功启用");
                Ok("桌面端 自动启动已启用".to_string())
            }
            Err(e) => {
                error!("【桌面端 自动启动功能】启用自动启动功能失败: {}", e);
                Err(format!("桌面端 启用自动启动失败: {}", e))
            }
        }
    } else {
        log::info!("【桌面端 自动启动功能】尝试禁用自动启动功能");
        match auto_launch.disable() {
            Ok(_) => {
                log::info!("【桌面端 自动启动功能】自动启动功能已成功禁用");
                Ok("桌面端 自动启动已禁用".to_string())
            }
            Err(e) => {
                log::error!("【桌面端 自动启动功能】禁用自动启动功能失败: {}", e);
                Err(format!("桌面端 禁用自动启动失败: {}", e))
            }
        }
    }
}
