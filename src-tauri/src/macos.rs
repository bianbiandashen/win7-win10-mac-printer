use std::process::Command;
use serde_json;
use serde_json::json;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
struct Printer {
    id: String,
    name: String,
}

#[tauri::command]
pub fn get_printers_macos() -> String {
    println!("正在获取打印机列表...");

    // 执行 lpstat 命令来获取打印机信息
    let output = Command::new("lpstat")
        .arg("-p")
        .output()
        .expect("无法执行lpstat命令");

    // 检查命令是否成功执行
    if output.status.success() {
        // 将标准输出转换为字符串
        let printers_output = String::from_utf8_lossy(&output.stdout);
        println!("成功获取到打印机列表：{}", printers_output);

        // 解析输出并转换为结构化的打印机列表
        let printers_json = parse_printers(&printers_output);
        println!("结构化的打印机列表 JSON：{}", printers_json);

        return printers_json;
    } else {
        // 将标准错误转换为字符串并输出
        let error_message = String::from_utf8_lossy(&output.stderr);
        println!("获取打印机列表失败: {}", error_message);
        return String::new();
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
    println!("正在获取打印作业...");

    // 执行 lpstat 命令来获取指定打印机的打印作业信息
    let output = Command::new("lpstat")
        .arg("-o")
        .arg(printer_name)
        .output()
        .expect("无法执行lpstat命令");

    if output.status.success() {
        let jobs_output = String::from_utf8_lossy(&output.stdout);
        println!("成功获取到打印作业：{}", jobs_output);

        // 解析输出并转换为结构化的打印任务 JSON
        let jobs_json = parse_jobs(&jobs_output);
        println!("结构化的打印作业 JSON：{}", jobs_json);

        return jobs_json;
    } else {
        let error_message = String::from_utf8_lossy(&output.stderr);
        println!("获取打印作业失败: {}", error_message);
        return String::new();
    }
}
// 解析打印机信息
fn parse_printers(printers_output: &str) -> String {
    use serde::{Serialize, Deserialize};
    use serde_json;

    #[derive(Debug, Serialize, Deserialize)]
    struct Printer {
        id: String,
        name: String,
    }

    let mut printers = Vec::<Printer>::new();
    let han_re = regex::Regex::new(r"[\p{Han}]").unwrap();
    
    // let re = regex::Regex::new(r"打印机([\w\p{Han}]+)[,，][\w\p{Han}]+([\w\p{Han}(,，)]{0,2})").unwrap();
    let re = regex::Regex::new(r"打印机([\w\p{Han}_]+)(?:已停用|现在正在打印|已脱机|闲置)").unwrap();

    for caps in re.captures_iter(printers_output) {
        let name: String = caps.get(1).unwrap().as_str().to_string();
        let name_without_han = han_re.replace_all(&name, "").to_string();
        let name_without_han_clone = name_without_han.clone();
        printers.push(Printer { 
            name: name_without_han, 
            id: name_without_han_clone 
        });
    }

    serde_json::to_string(&printers).unwrap_or_else(|_| String::new())
}


#[tauri::command]
fn print_pdf_macos(id: String, path: String, print_setting: String, remove_after_print: bool) -> Result<(), String> {
    let options = declare::PrintOptions {
        id,
        path,
        print_setting,
        remove_after_print,
    };

    let media_size = "Custom.76x130mm";
    let args: Vec<String> = vec![
        "-d".to_string(), options.id.clone(),
        "-o".to_string(), format!("media={}", media_size),
        options.path.clone(),
    ];
    println!("文件路径: {}", options.path);

    // 打印调试信息
    println!("执行命令: lp {}", args.join(" "));

    // 使用 `sh -c` 方式执行命令
    match Command::new("sh")
        .arg("-c")
        .arg(format!("lp {}", args.join(" ")))
        .output() {
        Ok(output) => {
            if output.status.success() {
                println!("成功打印 PDF 文件。");
                Ok(())
            } else {
                let error_message = String::from_utf8_lossy(&output.stderr);
                eprintln!("打印 PDF 文件失败: {}", error_message);
                Err(error_message.to_string())
            }
        }
        Err(e) => {
            eprintln!("执行 lp 命令失败: {}", e);
            Err(format!("执行 lp 命令失败: {}", e))
        }
    }
}