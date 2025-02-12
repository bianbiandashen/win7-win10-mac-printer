use crate::declare;
use crate::declare::PrintOptions;
use crate::error::{Component, CustomError, ErrorCode, System};
#[cfg(target_os = "windows")]
use crate::printpdf::set_thread_priority_max;
use crate::printpdf::{PrintResponse, PrintTask};
use crate::utils::{parse_print_setting, wide_string_to_string};
use chrono::Local;
use log::{error, info};
use serde_json::{from_str, Value};
use std::env;
use std::path::PathBuf;
use std::str;
use std::str::from_utf8;
use std::time::Duration;
use std::time::Instant;
use tauri::api::process::Command;
use tauri::api::process::Output;
use tauri::api::Error as TauriError;
use tauri::command;
use tokio::task;
#[cfg(target_os = "windows")]
use windows::core::Error;
#[cfg(target_os = "windows")]
use windows::core::PWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::GetLastError;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Printing::{
    EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_1W,
};
// use windows::core::PWSTR;
// use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
// use windows::Win32::Graphics::Printing::{EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_LOCAL, PRINTER_INFO_6};
#[cfg(target_os = "windows")]
use crate::utils::can_use_std_command;
#[cfg(target_os = "windows")]
use crate::windows_version::is_64bit_system;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::event::emit_event;
use crate::windows10;
use crate::windows7;
use crate::windows_version;
#[cfg(target_os = "windows")]
use serde::Serialize;
#[cfg(target_os = "windows")]
use serde_json::json;
#[cfg(target_os = "windows")]
use std::mem;
use std::process::Command as StdCommand;
#[cfg(target_os = "windows")]
use std::ptr;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::thread;
use tauri::api::process::Command as TauriCommand;

pub fn execute_print_task_sync(task: &PrintTask) -> Result<(), String> {
    // 从 print_settings 中提取相关参数
    let print_setting_str: String = match task.print_settings.get("printsetting") {
        Some(value) => value.as_str().unwrap_or("").to_string(),
        None => String::new(),
    };

    let print_setting: Value = if !print_setting_str.is_empty() {
        from_str(&print_setting_str).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let id: &str = match print_setting.get("id") {
        Some(value) => value.as_str().unwrap_or(""),
        None => "",
    };

    let page_size = match print_setting.get("pageSize") {
        Some(value) => value.as_str().unwrap_or(""),
        None => "",
    };

    let auto_fit = match print_setting.get("autoFit") {
        Some(value) => match value {
            Value::Bool(b) => *b,
            Value::String(s) => s.parse().unwrap_or(false),
            _ => {
                info!("【桌面端】【execute_print_task】auto_fit value is not a boolean or string: {:?}", value);
                false
            }
        },
        None => {
            info!("【桌面端】【execute_print_task】auto_fit value is missing");
            false
        }
    };

    let remove_after_print = match print_setting.get("removeAfterPrint") {
        Some(value) => match value {
            Value::Bool(b) => *b,
            Value::String(s) => s.parse().unwrap_or(false),
            _ => {
                info!("【桌面端】【execute_print_task】remove_after_print value is not a boolean or string: {:?}", value);
                false
            }
        },
        None => {
            info!("【桌面端】【execute_print_task】remove_after_print value is missing");
            false
        }
    };

    info!(
      "【桌面端】【execute_print_task】page_size {}",
      page_size
    );
    info!(
        "【桌面端】【execute_print_task】print_setting {}",
        print_setting
    );
    info!(
        "【桌面端】【execute_print_task】remove_after_print {}",
        remove_after_print
    );
    info!("【桌面端】【execute_print_task】auto_fit {}", auto_fit);

    // 直接调用 print_pdf_async 并 await 其结果
    let print_result = print_pdf_async(
        id.to_string(),
        task.printer.clone(),
        task.task_id.clone(),
        task.pdf_path.clone(), // 使用绘制完成后的 PDF 路径
        print_setting_str.clone(),
        remove_after_print,
        page_size.to_string(),
        auto_fit,
    );
    info!(
        "【桌面端】【execute_print_task_sync】print_result: {:?}",
        print_result
    );
    // 如果 print_result 是 String 类型
    if print_result.contains("成功") {
        Ok(())
    } else {
        Err(print_result)
    }
}

pub fn print_pdf_async(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    printer_setting: String,
    remove_after_print: bool,
    page_size: String,
    auto_fit: bool,
) -> String {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-打印方法--对应function-print_pdf_async-taskid={}】-开始执行（MacOS）",
        id
    );
    // ----------------------------------------

    info!(
        "[桌面端主入口调用] print_pdf_async - 打印 PDF 文件, 文件路径: {}",
        path
    );
    info!(
        "[桌面端主入口调用] print_pdf_async , 文件page_size: {}",
        page_size
    );
    info!(
        "[桌面端主入口调用] print_pdf_async, 文件remove_after_print: {}",
        remove_after_print
    );
    info!(
        "[桌面端主入口调用] print_pdf_async, 文件auto_fit: {}",
        auto_fit
    );

    let options = declare::PrintOptions {
        id: id.clone(),
        printer: printer.clone(),
        taskid: taskid.clone(),
        path: path.clone(),
        print_setting: printer_setting.clone(),
        remove_after_print,
        auto_fit,
    };
    let page_size_json: Value = match serde_json::from_str(&page_size) {
        Ok(json) => json,
        Err(err) => {
            let error_message = format!(
                "{}解析 page_size 出错: {}",
                ErrorCode::new(System::MacOS, Component::PrinterCommandModule, 0x061),
                err
            );
            info!("{}", error_message);

            // ---------------- 新增日志 ----------------
            info!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf_async-taskid={}】-失败，解析 page_size 出错: {}",
                id, err
            );
            // ----------------------------------------

            return error_message;
        }
    };

    let definition = page_size_json
        .get("definition")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    info!("页面设置的 definition 值: {}", definition);

    let mut print_response = PrintResponse {
        taskid: taskid.clone(),
        printer: printer.clone(),
        pdf_path: path.to_string(),
        success: false,
        message: String::new(),
    };

    let result = {
        #[cfg(target_os = "macos")]
        {
            match print_pdf_macos_sync(
                id.clone(),
                printer.clone(),
                taskid.clone(),
                path.clone(),
                printer_setting.clone(),
                remove_after_print,
                auto_fit,
            ) {
                Ok(_) => {
                    // ---------------- 新增日志 ----------------
                    info!(
                        "【电子面单打印核心方法-打印方法--对应function-print_pdf_async-taskid={}】-执行完毕，打印成功（MacOS）",
                        id
                    );
                    // ----------------------------------------
                    let message = "MacOS-打印成功".to_string();
                    print_response.success = true;
                    print_response.message = message.clone();
                    message
                }
                Err(err) => {
                    let msg = format!(
                        "{}MacOS-打印失败: {}",
                        ErrorCode::new(System::MacOS, Component::PrinterCommandModule, 0x062),
                        err
                    );
                    // ---------------- 新增日志 ----------------
                    error!(
                        "【电子面单打印核心方法-打印方法--对应function-print_pdf_async-taskid={}】-执行失败: {}",
                        id, msg
                    );
                    print_response.message = msg.clone();
                    // ----------------------------------------
                    msg
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let result = print_pdf_windows_sync(
                id.clone(),
                printer.clone(),
                taskid.clone(),
                path.clone(),
                printer_setting.clone(),
                remove_after_print,
                page_size.to_string(),
                auto_fit,
            );
            print_response.success = result.contains("成功");
            print_response.message = result.clone();
            result
        }
    };
    info!("【桌面端主入口调用】print_pdf_async - 打印任务完成，即将发送事件");
    emit_event("print_pdf_task_finished", print_response);
    info!(
        "【桌面端主入口调用】print_pdf_async - 打印任务完成，结果: {:?}",
        taskid.clone()
    );

    result
}

#[tauri::command]
pub fn print_pdf_macos_sync(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    print_setting: String,
    remove_after_print: bool,
    auto_fit: bool,
) -> Result<(), String> {
    let options = declare::PrintOptions {
        id,
        printer: printer.clone(),
        taskid: taskid.clone(),
        path: path.clone(),
        print_setting,
        remove_after_print,
        auto_fit,
    };

    let initial_log_info = "【桌面端MACOS-print_pdf_macos】".to_string();

    // 设置打印参数
    let media_size = "Custom.76x130mm";
    let args: Vec<String> = vec![
        "-d".to_string(),
        options.id.clone(),
        "-o".to_string(),
        format!("media={}", media_size),
        format!("'{}'", options.path.clone()), // 这里手动加上单引号
    ];

    // 日志：打印命令
    let exec_log = format!(
        "{} 文件路径: {}, 执行命令: lp {}",
        initial_log_info,
        options.path,
        args.join(" ")
    );
    info!("【桌面端MACOS-print_pdf_macos】exec_log：{}", exec_log);

    info!(
        "【桌面端MACOS-print_pdf_macos_sync】打印命令开始执行时间2: {:?}, 文件路径: {:?}",
        Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        path
    );
    let print_result = StdCommand::new("sh")
        .arg("-c")
        .arg(format!("lp {}", args.join(" ")))
        .output()
        .map_err(|e| format!("打印任务执行失败: {}", e))?;

    // 判断执行结果
    if print_result.status.success() {
        info!(
            "【桌面端MACOS-print_pdf_macos_sync】打印命令执行完成时间: {:?}, 文件路径: {:?}",
            Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            path
        );

        Ok(())
    } else {
        let error_message = String::from_utf8_lossy(&print_result.stderr).to_string();
        let error_msg = format!(
            "{}{} 打印 PDF 文件失败: {}",
            ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x029),
            initial_log_info,
            error_message
        );
        error!("{}", error_msg);
        Err(error_message)
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub fn print_pdf_windows_sync(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    printer_setting: String,
    remove_after_print: bool,
    page_size: String,
    auto_fit: bool,
) -> String {
    info!(
        "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-开始执行（Windows）",
        id
    );

    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, 文件路径: {}",
        path
    );
    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, page_size对象: {}",
        page_size
    );
    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, remove_after_print: {}",
        remove_after_print
    );

    #[cfg(target_os = "windows")]
    set_thread_priority_max();

    let options = declare::PrintOptions {
        id: id.clone(),
        printer: printer.clone(),
        taskid: taskid.clone(),
        path: path.clone(),
        print_setting: printer_setting.clone(),
        remove_after_print,
        auto_fit,
    };

    let page_size_json: Value = match serde_json::from_str(&page_size) {
        Ok(json) => json,
        Err(err) => {
            let error_message = format!("解析 page_size 时出错: {}", err);
            info!("{}", error_message);
            error!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-解析 page_size 时出错: {}",
                id, err
            );
            return error_message;
        }
    };

    let definition = page_size_json
        .get("definition")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    info!("页面设置的 definition 值: {}", definition);

    // 提取宽度和高度
    let width = page_size_json["width"].as_i64().unwrap_or(0);
    let height = page_size_json["height"].as_i64().unwrap_or(0);

    // 判断宽度是否大于高度
    let is_add_landscape = width > height;

    // 输出信息
    info!("宽度是否大于高度: {}", is_add_landscape);

    let result = unsafe {
        if windows_version::check_windows_version() {
            info!("检测 Windows 系统，准备打印 PDF");
            if definition {
                info!("打印模式: 高清打印模式 (definition: true)");
                match windows10::print_pdf_sync(options, page_size) {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，高清打印成功（Windows10+）",
                            id
                        );
                        // ----------------------------------------
                        Ok("Windows-打印成功".to_string())
                    }
                    Err(err) => {
                        let error_message = format!("标准打印失败: {}", err);
                        error!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-高清打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        Err(error_message)
                    }
                }
            } else {
                match print_pdf_compatible_mode_sync(options, is_add_landscape) {
                    Ok(_) => {
                        let msg = format!("Windows-打印成功");
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，兼容模式打印成功（Windows10+）",
                            id
                        );
                        Ok(msg)
                    }
                    Err(err) => {
                        let error_message = format!("兼容模式打印失败: {}", err);
                        info!("{}", error_message);
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-兼容模式打印失败: {}",
                            id, error_message
                        );
                        Err(error_message)
                    }
                }
            }
        } else {
            info!("检测到 Windows 7 系统，准备使用 Windows 7 打印接口");
            if definition {
                info!("打印模式: Win7高清打印模式 (definition: true)");
                match windows7::print_pdf_sync(options, page_size) {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，Win7高清模式打印成功",
                            id
                        );
                        // ----------------------------------------
                        Ok("Windows7-打印成功".to_string())
                    }
                    Err(err) => {
                        let error_message = format!("Windows7-高清模式打印失败: {}", err);
                        info!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-Win7高清模式打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        Err(error_message)
                    }
                }
            } else {
                info!("打印模式: 兼容打印模式 (definition: false)");
                match print_pdf_compatible_mode_sync(options, is_add_landscape) {
                    Ok(_) => {
                        let msg = format!("Windows-打印成功");
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，兼容模式打印成功（Windows10+）",
                            id
                        );
                        Ok(msg)
                    }
                    Err(err) => {
                        let error_message = format!("兼容模式打印失败: {}", err);
                        info!("{}", error_message);
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-兼容模式打印失败: {}",
                            id, error_message
                        );
                        Err(error_message)
                    }
                }
            }
        }
    };

    let msg = match result {
        Ok(_) => "Windows-打印成功".to_string(),
        Err(_) => "Windows-打印失败".to_string(),
    };

    msg
}

#[cfg(target_os = "windows")]
pub fn print_pdf_compatible_mode_sync(options: PrintOptions, is_add_landscape: bool) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    set_thread_priority_max();

    let log_info = "【桌面端-print_pdf_compatible_mode_sync】".to_string();

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    let dir: PathBuf = env::temp_dir();
    let sumatra_exe_path = dir.join("SumatraPDF-prerel-64.exe");

    info!(
        "{} SumatraPDF 可执行文件路径: {}",
        log_info,
        sumatra_exe_path.display()
    );

    let printer_id = options.id.trim_matches('"').to_string();
    info!("{} 打印机 ID: {}", log_info, printer_id);

    let mut print_settings_str = String::new();
    if options.auto_fit {
        print_settings_str += "fit";
    }

    // 如果宽大于高时增加配置 保证方向
    if is_add_landscape {
      if !print_settings_str.is_empty() {
        print_settings_str.push_str(",landscape");
      } else {
        print_settings_str.push_str("landscape,noscale");
      }
    }

    info!("{} 执行 std::process::Command 执行打印命令 打印设置print_settings_str={}", log_info, print_settings_str);

    let mut command = std::process::Command::new(&sumatra_exe_path);
    command.arg("-print-to").arg(&printer_id);

    if !print_settings_str.is_empty() {
        command.arg("-print-settings").arg(&print_settings_str);
    }

    command
        .arg(&options.path)
        .creation_flags(0x08000000 | 0x04000000);

    let command_line = format!(
        "{} -print-to {} -print-settings {} {}",
        sumatra_exe_path.display(),
        printer_id,
        &print_settings_str,
        &options.path
    );
    info!("{} 执行的命令: {}", log_info, command_line);

    match command.output() {
        Ok(output) => {
            if output.status.success() {
                info!("{} 打印命令执行成功", log_info);
                print_response.success = true;
                print_response.message = "Windows-打印成功".to_string();
                let taskid = print_response.taskid.clone(); // 在发送事件前克隆 taskid
                emit_event("print_pdf_task_finished", print_response);
                Ok(taskid)
            } else {
                let stderr_message = String::from_utf8_lossy(&output.stderr);
                let error_message = format!(
                    "{} - Windows-打印失败: {}",
                    ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x042),
                    stderr_message
                );
                print_response.message = error_message.clone();
                error!("{} {}", log_info, error_message);
                emit_event("print_pdf_task_finished", print_response);
                Err("Windows-打印失败".to_string())
            }
        }
        Err(e) => {
            let error_msg = format!(
                "{} {} Windows-命令执行错误: {}",
                ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x043),
                log_info,
                e
            );
            print_response.message = error_msg.clone();
            error!("{} {}", log_info, error_msg);
            emit_event("print_pdf_task_finished", print_response);
            Err("Windows-命令执行错误".to_string())
        }
    }
}
