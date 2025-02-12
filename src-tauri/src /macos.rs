use std::process::{Command, Stdio};
use printpdf::Mm;
use serde_json;
use serde_json::json;
use serde::{Serialize, Deserialize};
use regex::Regex;
use log::{info, error};
use crate::declare;
use crate::printer::remove_temp_file;
use crate::error::{CustomError, ErrorCode, System, Component};
use std::fs;
use std::time::Instant;
use std::str;
use chrono::Local;

#[derive(Debug, Serialize, Deserialize)]
struct Printer {
    id: String,
    name: String,
}

#[tauri::command]
pub fn get_default_printer_macos() -> Result<String, String> {
    let log_info = "【桌面端MACOS-get_default_printer_macos】";
    info!("{}", log_info);

    let output_result = Command::new("sh")
        .arg("-c")
        .arg("lpstat -d | awk -F'：' '{print $2}'")
        .output();

    match output_result {
        Ok(output) => {
            if output.status.success() {
                // 将标准输出转换为字符串
                let default_printer = String::from_utf8_lossy(&output.stdout).trim().to_string();
                info!("{} 桌面端MACOS获取到默认打印机信息：{}", log_info, default_printer);

                // 解析默认打印机
                if default_printer.is_empty() {
                    error!("{} 桌面端MACOS解析默认打印机失败或为空", log_info);
                    return Err("桌面端MACOS解析默认打印机失败".to_string());
                } else {
                    info!("{} 桌面端MACOS默认打印机名称：{}", log_info, default_printer);
                    return Ok(default_printer.to_string());
                }
            } else {
                // lpstat 命令执行失败，解析错误信息
                let error_message = String::from_utf8_lossy(&output.stderr);
                error!("{} 桌面端MACOS获取默认打印机失败: {}", log_info, error_message);

                if error_message.contains("no system default destination") || error_message.contains("未添加目的位置") {
                    error!("{} 桌面端MACOS没有配置任何默认打印机", log_info);
                    return Err("桌面端MACOS没有配置任何默认打印机".to_string());
                } else {
                    error!("{} 发生其他错误: {}", log_info, error_message);
                    return Err("桌面端MACOS获取默认打印机时发生错误".to_string());
                }
            }
        },
        Err(e) => {
            // 无法执行 `lpstat` 命令
            error!("{} 无法执行 lpstat 命令: {}", log_info, e);
            return Err("无法执行 lpstat 命令".to_string());
        }
    }
}

#[tauri::command]
pub fn get_printers_macos() -> String {
    let mut log_info = "【桌面端MACOS-get_printers_macos】".to_string();

    // 检查 CUPS 服务是否在运行
    let service_status_result = Command::new("lpstat")
        .arg("-r")
        .output();

    match service_status_result {
        Ok(output) => {
            let service_status = String::from_utf8_lossy(&output.stdout);
            if !service_status.contains("scheduler is running") {
                log_info = format!("{} CUPS 服务未运行，正在尝试启动;", log_info);

                let start_service_result = Command::new("launchctl")
                    .arg("start")
                    .arg("org.cups.cupsd")
                    .stdout(Stdio::null()) // 隐藏标准输出
                    .stderr(Stdio::null()) // 隐藏错误输出
                    .output();

                match start_service_result {
                    Ok(_) => {
                        log_info = format!("{} 成功启动 CUPS 服务;", log_info);
                    }
                    Err(e) => {
                        log_info = format!("{}{} 启动 CUPS 服务失败: {}", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x031), log_info, e);
                        error!("{}", log_info);
                        return "[]".to_string(); // 返回空的打印机列表，避免应用崩溃
                    }
                }
            } else {
                log_info = format!("{} CUPS 服务正在运行;", log_info);
            }
        }
        Err(e) => {
            log_info = format!("{}{} 无法检查 CUPS 服务状态: {}", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x032), log_info, e);
            error!("{}", log_info);
            return "[]".to_string(); // 返回空的打印机列表，避免应用崩溃
        }
    }

    let output_result = Command::new("lpstat")
        .arg("-p")
        .output();

    match output_result {
        Ok(output) => {
            let error_message = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                // 将标准输出转换为字符串
                let printers_output = String::from_utf8_lossy(&output.stdout);
                log_info = format!("{} 获取到打印机列表：{}", log_info, printers_output);

                // 解析输出并转换为结构化的打印机列表 JSON
                let printers_json = parse_printers(&printers_output);
                if printers_json.is_empty() {
                    log_info = format!("{},{} 解析输出失败或解析为空", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x022), log_info);
                    error!("{}", log_info);
                    return "[]".to_string();
                } else {
                    log_info = format!("{} 结构化的 MAC 打印机列表 JSON：{}", log_info, printers_json);
                    info!("{}", log_info);
                    return printers_json;
                }
            } else {
                // lpstat 命令失败的处理逻辑
                log_info = format!("{} 获取打印机列表失败: {}", log_info, error_message);

                if error_message.contains("未添加目的位置") || error_message.contains("No destinations added") {
                    log_info = format!("{}{} 没有配置任何打印机，返回空的打印机列表。", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x023), log_info);
                    error!("{}", log_info);
                    return "[]".to_string(); // 返回空列表
                } else {
                    // 处理其他可能的错误
                    log_info = format!("{} {}发生其他错误: {}", log_info, ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x024),error_message);
                    error!("{}", log_info);
                    return "[]".to_string(); // 返回空的打印机列表
                }
            }
        },
        Err(e) => {
            // 处理无法执行 lpstat 命令的情况
            log_info = format!("{} {}无法执行 lpstat 命令: {}", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x025),log_info, e);
            error!("{}", log_info);

            // 返回空的打印机列表，而不是错误信息
            "[]".to_string()
        }
    }
}

// 解析打印作业信息并转换成 JSON 格式
fn parse_jobs(jobs_output: &str) -> String {
    let mut jobs = Vec::new();

    for line in jobs_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }

        let job_id = parts[0];
        let user = parts[1];
        let file = parts[2];
        let created = format!("{} {}", parts[3], parts[4]); // 解析时间戳
        let status = parts[5..].join(" "); // 解析状态信息

        let job = json!({
            "job_id": job_id,
            "user": user,
            "file": file,
            "created": created,
            "status": status,
        });

        jobs.push(job);
    }

    serde_json::to_string(&jobs).unwrap()
}

// 获取指定打印机名称的打印作业信息
#[tauri::command]
pub fn get_jobs_macos(printer_name: &str) -> String {
    let mut log_info = "【桌面端MACOS-get_jobs_macos】".to_string();

    // 执行 lpstat 命令来获取指定打印机的打印作业信息
    let output = Command::new("lpstat")
        .arg("-o")
        .arg(printer_name)
        .output()
        .expect("【桌面端MACOS-get_jobs_macos】无法执行lpstat命令");

    if output.status.success() {
        let jobs_output = String::from_utf8_lossy(&output.stdout);
        log_info = format!("{} 成功获取到MAC-PRINT_JOB打印作业：{}", log_info, jobs_output);

        // 解析输出并转换为结构化的打印任务 JSON
        let jobs_json = parse_jobs(&jobs_output);
        log_info = format!("{} 结构化的MAC-PRINT_JOB打印作业 JSON：{}", log_info, jobs_json);
        info!("{}", log_info);

        return jobs_json;
    } else {
        let error_message = String::from_utf8_lossy(&output.stderr);
        log_info = format!("{}{} 获取MAC-PRINT_JOB打印作业失败: {}", ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x027), log_info, error_message);
        error!("{}", log_info);
        return String::new();
    }
}

// 获取语言和地区首选项
fn get_locale_preferences() -> Result<(String, String), Box<dyn std::error::Error>> {
    info!("【桌面端MACOS-get_locale_preferences】调用");
    // 获取语言首选项
    let lang_output = Command::new("defaults")
        .arg("read")
        .arg("-g")
        .arg("AppleLanguages")
        .output()?;
    let lang_str = str::from_utf8(&lang_output.stdout)?;
    let lang = lang_str
        .lines()
        .nth(1)
        .and_then(|line| line.trim().trim_matches('"').split(',').next())
        .unwrap_or("Unknown")
        .to_string();

    // 获取地区首选项
    let locale_output = Command::new("defaults")
        .arg("read")
        .arg("-g")
        .arg("AppleLocale")
        .output()?;
    let locale = str::from_utf8(&locale_output.stdout)?.trim().to_string();
    info!("【桌面端MACOS-get_locale_preferences】解析后的地区首选项: {}", locale);

    Ok((lang, locale))
}

fn parse_printers(printers_output: &str) -> String {
    #[derive(Debug, Serialize, Deserialize)]
    struct Printer {
        id: String,
        name: String,
    }

    let mut printers = Vec::<Printer>::new();
    let han_re = Regex::new(r"[\p{Han}]").unwrap();
    info!("【桌面端MACOS-parse_printers】正则表达式 han_re 初始化成功");

    let re = match get_locale_preferences() {
        Ok((lang, _)) if lang.starts_with("zh") => {
            info!("【桌面端MACOS-parse_printers】检测到中文环境，使用匹配规则: 中文打印机状态");
            Regex::new(r"打印机([\w\p{Han}_\-]+)(?:已停用|现在正在打印|已脱机|闲置)").unwrap()
        }
        Ok((lang, _)) => {
            info!("【桌面端MACOS-parse_printers】检测到非中文环境，使用匹配规则: 英文打印机状态");
            Regex::new(r"(printer)\s*([\w\p{Han}_\-]+)\s*(?:is\s*(idle|printing|disabled))").unwrap()
        }
        Err(e) => {
            error!("【桌面端MACOS-parse_printers】获取语言和地区首选项时出错: {}", e);
            Regex::new(r".*").unwrap() // 确保 match 语句返回一致类型
        }
    };
    info!("【桌面端MACOS-parse_printers】正则表达式 re 初始化成功");

    for caps in re.captures_iter(printers_output) {
        info!("【桌面端MACOS-parse_printers】匹配到的正则结果: {:?}", caps);
        let name: String = match get_locale_preferences() {
            Ok((lang, _)) if lang.starts_with("zh") => {
                let n = caps.get(1).unwrap().as_str().to_string();
                info!("【桌面端MACOS-parse_printers】匹配到中文打印机名称: {}", n);
                n
            }
            _ => {
                let n = caps.get(2).unwrap().as_str().to_string();
                info!("【桌面端MACOS-parse_printers】匹配到英文打印机名称: {}", n);
                n
            }
        };
        info!("【桌面端MACOS-parse_printers】去除前: {}", name);

        let name_without_han = han_re.replace_all(&name, "").to_string();
        info!("【桌面端MACOS-parse_printers】去除汉字后的打印机名称: {}", name_without_han);

        let name_without_han_clone = name_without_han.clone();
        printers.push(Printer {
            name: name_without_han,
            id: name_without_han_clone,
        });
    }


    if printers.is_empty() {
        info!("【桌面端MACOS-parse_printers】未匹配到任何打印机，返回空列表");
        return "[]".to_string();
    }

    match serde_json::to_string(&printers) {
        Ok(json) => {
            info!("【桌面端MACOS-parse_printers】成功转换打印机列表为 JSON: {}", json);
            json
        }
        Err(e) => {
            info!("{}【桌面端MACOS-parse_printers】将打印机列表转换为 JSON 失败: {}",
                ErrorCode::new(System::MacOS, Component::PrinterListModule, 0x026), e);
            "[]".to_string()
        }
    }
}


#[tauri::command]
pub async fn print_pdf_macos(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    print_setting: String,
    remove_after_print: bool,
    auto_fit: bool
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


    info!("【桌面端MACOS-print_pdf_macos】打印命令开始执行时间1: {:?}, 文件路径: {:?}", Local::now().format("%Y-%m-%d %H:%M:%S").to_string(), path);

    // 使用 tokio 的 spawn_blocking 来异步执行打印命令
    let path_clone = path.clone();
    let print_result = tokio::task::spawn_blocking(move || {
        Command::new("sh")
            .arg("-c")
            .arg(format!("lp {}", args.join(" ")))
            .output()
    }).await
        .map_err(|e| format!("打印任务执行失败: {}", e))?
        .map_err(|e| format!("执行 lp 命令失败: {}", e))?;

    // 判断执行结果
    if print_result.status.success() {
        info!("【桌面端MACOS-print_pdf_macos】打印命令执行完成时间: {:?}, 文件路径: {:?}", Local::now().format("%Y-%m-%d %H:%M:%S").to_string(), path);

        // 如果需要删除临时文件，则异步执行删除逻辑
        if options.remove_after_print {
            tokio::task::spawn_blocking(move || {
                if remove_temp_file(path_clone.clone()) {
                    info!("【桌面端MACOS】删除pdfPath成功: {}", path_clone);
                } else {
                    error!(
                        "{}【桌面端MACOS】删除pdfPath失败: {}",
                        ErrorCode::new(System::MacOS, Component::PrinterCommandModule, 0x028),
                        path_clone
                    );
                }
            });
        }
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
