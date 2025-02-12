use tauri::command;
use log::{info, error};
use serde_json::json;
use std::{
    env,  // 导入整个 std::env 模块
    time::{SystemTime, UNIX_EPOCH},
    fs::{self, File},
    path::PathBuf,
    io::Write, // 导入 Write trait 用于扩展 std::io 中的额外方法
};

use crate::macos;
use crate::windows10;
use crate::windows7;
use crate::windows_version;

use tauri::async_runtime;

use std::process::Command as StdCommand;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
use crate::utils::can_use_std_command;

#[tauri::command(rename_all = "snake_case")]
pub fn get_default_printer() -> String {
    info!("[桌面端主入口调用] get_default_printer - 获取打印机列表");

    #[cfg(target_os = "windows")]
    {
        info!("[桌面端主入口调用] get_default_printer - 当前操作系统为 Windows");
        unsafe {
          info!("[桌面端主入口调用] get_default_printer - 不区分windows版本");
          match windows10::get_default_printer_new() {
              Ok(printer) => {
                  info!("[桌面端主入口调用] get_default_printer - 默认打印机: {}", printer);
                  return json!({"default_printer": printer, "error": null}).to_string();
              }
              Err(e) => {
                  error!("[桌面端主入口调用] get_default_printer - 错误: {}", e);
                  return json!({"default_printer": null, "error": e}).to_string();
              }
          }
        }
    }


    #[cfg(target_os = "macos")]
    {
        info!("[桌面端主入口调用] get_default_printer - 当前操作系统为 macOS");
        match macos::get_default_printer_macos() {
            Ok(printer) => {
                info!("[桌面端主入口调用] get_default_printer - 打印机列表: {}", printer);
                return json!({"default_printer": printer, "error": null}).to_string();
            }
            Err(e) => {
                error!("[桌面端主入口调用] get_default_printer - 错误: {}", e);
                return json!({"default_printer": null, "error": e}).to_string();
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        error!("[桌面端主入口调用] get_default_printer - 当前操作系统不支持");
        return json!({"default_printer": null, "error": "当前操作系统不支持"}).to_string();
    }
}


#[cfg(target_os = "windows")]
pub fn open_system_jobs_std_command(printer_name: &str) -> Result<String, String> {
    // 检查是否可以使用 std::process::Command
    let use_std_command = can_use_std_command();

    if use_std_command {
        log::info!(
            "【桌面端 + 执行方法: open_system_jobs_std_command】尝试直接使用 std::process::Command"
        );

        match std::process::Command::new("rundll32.exe")
            .arg("printui.dll,PrintUIEntry")
            .arg("/o")
            .arg("/n")
            .arg(printer_name) // 分开传递
            .creation_flags(0x08000000)
            .spawn()
        {
            Ok(_) => {
                log::info!("桌面端 Windows 执行命令成功");
                Ok("成功打开打印机属性窗口".to_string())
            }
            Err(e) => {
                log::info!("桌面端 Windows 执行命令失败: {}", e);
                Err(format!("命令执行出错: {}", e))
            }
        }
    } else {
        log::info!("【桌面端 + 执行方法: open_system_jobs_std_command】直接使用 tauri::api::process::Command");
        open_system_jobs_tauri_command(printer_name)
    }
}


#[cfg(target_os = "windows")]
fn open_system_jobs_tauri_command(printer_name: &str) -> Result<String, String> {
  use tauri::api::process::Command as TauriCommand;

  // 构建并执行命令
  match TauriCommand::new("rundll32.exe")
      .args(vec!["printui.dll,PrintUIEntry", "/o", "/n", printer_name]) // 使用 Vec 分开传递参数
      .spawn()
  {
      Ok(_) => {
          info!("桌面端 Windows 执行命令成功");
          Ok("成功打开打印机属性窗口".to_string())
      }
      Err(e) => {
          info!("桌面端 Windows 执行命令失败: {}", e);
          Err(format!("命令执行出错: {}", e))
      }
  }
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_temp_file(buffer_data: String, filename: String, start_time: String) -> String {
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let transfer_time = current_time - start_time.parse::<u128>().unwrap();
    info!("[桌面端主入口调用] create_temp_file - 正在生成临时 PDF 文件...请求传输耗时: {}ms", transfer_time);

    let dir = env::temp_dir();

    // 检查文件名后缀是否包含 ".pdf"，没有则添加
    let pdf_filename = if filename.ends_with(".pdf") {
        filename.clone()
    } else {
        format!("{}.pdf", filename)
    };
    let file_path = dir.join(&pdf_filename);

    match base64::decode(&buffer_data) {
        Ok(decoded_data) => {
            let mut file = match File::create(&file_path) {
                Ok(f) => f,
                Err(e) => {
                    error!("[桌面端主入口调用] create_temp_file - 创建文件失败: {:?}", e);
                    return "".to_owned();
                }
            };
            if file.write_all(&decoded_data).is_ok() {

                // 将 file_path 转换为 String 并返回
                return file_path.to_string_lossy().into_owned();
            } else {
                error!("[桌面端主入口调用] create_temp_file - 写入数据失败");
            }
        }
        Err(e) => error!("[桌面端主入口调用] create_temp_file - base64 解码失败: {:?}", e),
    }
    "".to_owned()
}

#[tauri::command(rename_all = "snake_case")]
pub fn create_temp_image_file(base64_data: String, filename: String) -> String {
    info!("[桌面端主入口调用] create_temp_image_file - 正在生成临时图片文件...");

    // Decoding base64 data
    let decoded_data = match base64::decode(&base64_data) {
        Ok(data) => data,
        Err(err) => {
            error!("[桌面端主入口调用] create_temp_image_file - Base64 解码失败: {:?}", err);
            return "".to_owned();
        }
    };

    let dir = env::temp_dir();
    let file_path = format!("{}{}", dir.display(), filename);

    info!("[桌面端主入口调用] file_path - file_path生成成功: {}", file_path);
    // Writing decoded data to file
    let result = std::fs::write(&file_path, decoded_data);
    if result.is_ok() {
        info!("[桌面端主入口调用] create_temp_image_file - 临时图片文件已生成: {}", file_path);
        return file_path;
    }

    error!("[桌面端主入口调用] create_temp_image_file - 创建临时文件失败");
    "".to_owned()
}

#[tauri::command(rename_all = "snake_case")]
pub fn open_system_jobs(printername: String) {
    info!("[桌面端主入口调用] open_system_jobs - 打开打印机任务队列");

    #[cfg(target_os = "windows")]
    {
        info!("[桌面端主入口调用] open_system_jobs - 当前操作系统为 Windows");
        unsafe {
            info!("[桌面端主入口调用] open_system_jobs - 检测到 Windows 7 以上的版本");
            if let Err(e) = open_system_jobs_std_command(&printername) {
                error!("[桌面端主入口调用] open_system_jobs - windows打开打印机任务队列失败: {}", e);
            } else {
                info!("[桌面端主入口调用] open_system_jobs - windows打开打印机任务队列成功");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        info!("[桌面端主入口调用] get_default_printer - 当前操作系统为 macOS");
        // if let Err(e) = macos::open_system_jobs_macos() {
        //     error!("[桌面端主入口调用] open_system_jobs - mac打开打印机任务队列失败: {}", e);
        // } else {
        //     info!("[桌面端主入口调用] open_system_jobs - mac打开打印机任务队列成功");
        // }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        error!("[桌面端主入口调用] open_system_jobs - 当前操作系统不支持");
        panic!("当前操作系统不支持");
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn remove_temp_file(filename: String) -> bool {
    info!("[面端主入口调用] remove_temp_file - 移除临时文件: {}", filename);
    let mut path: PathBuf = env::temp_dir();
    path.push(filename);

    match fs::remove_file(&path) {
        Ok(_) => {
            info!("Successfully removed file: {}", path.display());
            true
        }
        Err(e) => {
            error!("Failed to remove file '{}': {}", path.display(), e);
            false
        }
    }
}

#[cfg(target_os = "windows")]
#[tauri::command(rename_all = "snake_case")]
pub async fn get_printers() -> String {
    info!("[桌面端主入口调用] get_printers - 获取打印机列表");
    info!("[桌面端主入口调用] get_printers - 当前操作系统为 Windows");

    let result = tokio::task::spawn_blocking(|| {
        unsafe {
          info!("[桌面端主入口调用] get_printers - 不区分window版本");
          let result = windows10::get_printers_new();
          info!("[桌面端主入口调用] get_printers - 打印机列表: {}", result);
          result
        }
    })
    .await
    .unwrap_or_else(|_| "获取打印机列表失败".to_string());

    result
}

#[cfg(target_os = "macos")]
#[tauri::command(rename_all = "snake_case")]
pub async fn get_printers() -> String {
    info!("[桌面端主入口调用] get_printers - 当前操作系统为 macOS");

    let result = tokio::task::spawn_blocking(|| {
        let result = macos::get_printers_macos();
        info!("[桌面端主入口调用] get_printers - macOS 打印机列表: {}", result);
        result
    })
    .await
    .unwrap_or_else(|_| "获取打印机列表失败".to_string());

    result
}


#[cfg(target_os = "macos")]
#[tauri::command(rename_all = "snake_case")]
pub async fn get_jobs(printer_name: String) -> Result<String, String> {
    info!("[桌面端主入口调用] get_jobs - 获取打印任务: {}", printer_name);
    // 将同步操作放在另一个线程中执行
    let result = async_runtime::spawn_blocking(move || {
        macos::get_jobs_macos(&printer_name)
    }).await.unwrap_or_else(|e| {
        error!("获取打印任务失败: {}", e);
        String::from("[]")
    });

    Ok(result)
}

#[cfg(target_os = "windows")]
#[tauri::command(rename_all = "snake_case")]
pub async fn get_jobs(printer_name: String) -> Result<String, String> {
    info!("[桌面端主入口调用] get_jobs - 获取打印任务: {}", printer_name);

    // 将同步操作放在另一个线程中执行
    let result = async_runtime::spawn_blocking(move || {
        unsafe {
            if windows_version::check_windows_version() {
                let result = windows10::get_jobs(printer_name);
                info!("[桌面端主入口调用] get_jobs - 打印任务列表: {}", result);
                Ok(result)
            } else {
                info!("[桌面端主入口调用] get_jobs - 检测到 Windows 7 以下版本");
                let result = windows7::get_jobs_win7(printer_name);
                match result {
                    Ok(jobs) => {
                        info!("[桌面端主入口调用] get_jobs - Win7 打印机任务列表: {}", jobs);
                        Ok(jobs)
                    },
                    Err(e) => Ok(e)
                }
            }
        }
    }).await.unwrap_or_else(|e| {
        error!("获取打印任务失败: {}", e);
        Ok(String::from("[]"))
    });

    result
}
