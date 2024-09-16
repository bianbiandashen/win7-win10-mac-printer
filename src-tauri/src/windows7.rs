use std::{process::Command, sync::mpsc, thread};
use std::env;
use crate::{declare::PrintOptions, fsys::remove_file};
use std::path::{PathBuf};

// 检查 PowerShell 版本的函数
fn check_powershell_version() -> String {
    // 执行 PowerShell 命令获取版本信息
    let output = Command::new("powershell")
        .arg("-Command")
        .arg("Get-Host | Select-Object Version | ConvertTo-Json")
        .output();

    match output {
        // 成功获取版本返回版本字符串
        Ok(output) => {
            let version = String::from_utf8_lossy(&output.stdout).to_string();
            println!("PowerShell version: {}", version);
            version
        }
        // 异常时返回未知版本
        Err(e) => {
            println!("Failed to check PowerShell version: {:?}", e);
            "Unknown version".to_string()
        }
    }
}

// 检查用户是否具有管理员权限的函数
fn check_admin_privileges() -> bool {
    // 执行 PowerShell 命令检查管理员权限 (SID: S-1-5-32-544)
    let output = Command::new("powershell")
        .arg("-Command")
        .arg("whoami /groups | Select-String -Pattern 'S-1-5-32-544'")
        .output();

    match output {
        // 如果输出包含管理员组，则拥有管理员权限
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let is_admin = stdout.contains("S-1-5-32-544");
            println!("Running as admin: {}", is_admin);
            is_admin
        }
        // 异常时默认为无管理员权限
        Err(e) => {
            println!("Failed to check admin privileges: {:?}", e);
            false
        }
    }
}

// 获取所有打印机的函数
pub fn get_printers_win7() -> String {
    let powershell_version = check_powershell_version();
    println!("PowerShell version is: {}", powershell_version);

    let is_admin = check_admin_privileges();
    if !is_admin {
        println!("User does not have administrative privileges.");
        return "Administrative privileges required.".to_string();
    }

    // 创建线程间通信的通道
    let (sender, receiver) = mpsc::channel();

    // PowerShell 命令以获取打印机信息
    let command = vec![
        "-Command",
        r#"
        $printers = Get-WmiObject -Query 'SELECT * FROM Win32_Printer' |
        Select-Object Name, DriverName, JobCount, PrintProcessor |
        ForEach-Object {
            $json = '{' +
            '"id":"' + ($_.Name.Trim() -replace '"', '\"') + '",' +
            '"name":"' + ($_.Name.Trim() -replace '"', '\"') + '"' +
            '}'
            $json
        }
        '[' + ($printers -join ',') + ']'
        "#
    ];

    println!("Executing command: powershell {:?}", command.join(" "));

    // 新建线程来执行 PowerShell 命令
    thread::spawn(move || {
        println!("Spawned thread to execute PowerShell command.");

        let output = Command::new("powershell")
            .args(&command)
            .output();

        match output {
            // 成功时发送命令输出
            Ok(output) => {
                println!("PowerShell command executed successfully.");
                if !output.stderr.is_empty() {
                    eprintln!("Command stderr: {}", String::from_utf8_lossy(&output.stderr));
                }
                let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
                println!("Command stdout: {}", stdout_string); 
                let _ = sender.send(stdout_string);
            }
            // 失败时发送空字符串
            Err(e) => {
                eprintln!("Failed to execute PowerShell command: {:?}", e);
                let _ = sender.send(String::new());
            }
        }
    });

    println!("Main thread is doing other non-blocking work.");

    // 接收线程发回的结果
    match receiver.recv() {
        Ok(res) => {
            println!("Successfully received result from the spawned thread.");
            res
        }
        Err(e) => {
            eprintln!("Failed to receive result from the spawned thread: {:?}", e);
            String::new()
        }
    }
}

// 根据打印机名称获取打印机信息的函数
pub fn get_printers_by_name_win7(printername: String) -> String {
    // 格式化 WMI 查询以选择指定打印机名称
    let query = format!(
        r#"
        $printers = Get-WmiObject -Query "SELECT * FROM Win32_Printer WHERE Name='{}'" |
        Select-Object Name, DriverName, JobCount, PrintProcessor, PortName, ShareName, SystemName, PrinterStatus, Shared, Type, Priority |
        ForEach-Object {{
            '{{' +
            '"id":"' + ($_.Name.Trim() -replace '"', '\"') + '",' +
            '"name":"' + ($_.Name.Trim() -replace '"', '\"') + '",' +
            '"DriverName":"' + ($_.DriverName.Trim() -replace '"', '\"') + '",' +
            '"JobCount":' + [string]($_.JobCount) + ',' +
            '"PrintProcessor":"' + ($_.PrintProcessor.Trim() -replace '"', '\"') + '",' +
            '"PortName":"' + ($_.PortName.Trim() -replace '"', '\"') + '",' +
            '"ShareName":"' + ($_.ShareName.Trim() -replace '"', '\"') + '",' +
            '"ComputerName":"' + ($_.SystemName.Trim() -replace '"', '\"') + '",' +
            '"PrinterStatus":' + [string]($_.PrinterStatus) + ',' +
            '"Shared":' + [string]($_.Shared) + ',' +
            '"Type":"' + [string]($_.Type) + '",' +
            '"Priority":' + [string]($_.Priority) +
            '}}'
        }}
        '[' + ($printers -join ',') + ']'
        "#,
        printername
    );



    // 设置 PowerShell 命令
    let command = vec![
        "-Command",
        &query,
    ];

    println!("Executing command: powershell {:?}", command.join(" "));

    // 执行命令
    let output = Command::new("powershell")
        .args(&command)
        .output();

    match output {
        // 成功时返回输出
        Ok(output) => {
            if !output.stderr.is_empty() {
                eprintln!("Command stderr: {}", String::from_utf8_lossy(&output.stderr));
            }
            let stdout_string = String::from_utf8_lossy(&output.stdout).to_string();
            println!("Command stdout get_printers_by_name_win7: {}", stdout_string); 
            stdout_string
        }
        // 失败时返回空字符串
        Err(e) => {
            eprintln!("Failed to execute PowerShell command: {:?}", e);
            String::new()
        }
    }
}

// 获取打印作业信息的函数
pub fn get_jobs_win7(printer_name: String) -> String {
    use std::process::Command;
    use std::str::from_utf8;

    // 格式化 WMI 查询以获取特定打印机的作业信息
    let query = format!(
        r#"
        $jobs = Get-WmiObject -Query "SELECT * FROM Win32_PrintJob WHERE Name LIKE '%{}%'" |
        Select-Object Document, JobId, TotalPages, Position, Size, TimeSubmitted, Owner, PagesPrinted, StartTime, HostPrintQueue, DataType, PrinterName, Priority, JobStatus |
        ForEach-Object {{
            '{{' +
            '"DocumentName":"' + $_.Document + '",' +
            '"SubmittedTime":"' + $_.TimeSubmitted + '",' +
            '"UserName":"' + $_.Owner + '",' +
            '"PrinterName":"' + $_.PrinterName + '"' +
            '}}'
        }}
        '[' + ($jobs -join ',') + ']'
        "#,
        printer_name
    );

    let command = vec![
        "-Command",
        &query,
    ];
    let full_command = command.join(" ");
    println!("get_jobs_win7  {}", full_command);

    let output = Command::new("powershell")
        .args(&command)
        .output();

    match output {
        // 成功时返回作业信息
        Ok(output_data) => {
            if output_data.status.success() {
                from_utf8(&output_data.stdout)
                    .unwrap_or("Error converting output to UTF-8")
                    .trim() // 去除空白和换行
                    .to_string()
            } else {
                // 失败时返回错误信息
                let error_message = from_utf8(&output_data.stderr)
                    .unwrap_or("Error converting error output to UTF-8")
                    .trim() // 去除空白和换行
                    .to_string();
                format!("Command failed with error: {}", error_message)
            }
        }
        Err(error) => format!("Failed to execute PowerShell command: {}", error)
    }
}

// 打印PDF文件的函数 (适用于Windows 7)
pub fn print_pdf_win7(options: PrintOptions) -> Result<String, String> {
    // 获取临时目录路径
    let dir: PathBuf = env::temp_dir();
    println!("临时目录: {}", dir.display());

    // 构建打印命令
    let print_arg = format!("-print-to {}", options.id);
    let shell_command = format!(
        "{}sm {} {}",
        dir.display(),
        print_arg,
        // options.print_setting,
        options.path
    );
    println!("生成的命令: {}", shell_command);
    // 执行命令
    let output = Command::new("powershell")
        .args(&["-Command", &shell_command])
        .output()
        .map_err(|e| format!("执行命令失败: {}", e))?;

    // 根据命令执行结果返回相应的信息
    if output.status.success() {
        println!("打印成功_win7");
        if options.remove_after_print {
            // 打印成功后按需删除文件
            remove_file(&options.path).map_err(|e| format!("删除文件失败: {}", e))?;
            println!("文件已删除: {}", options.path);
        }
        Ok("Windows7-打印成功".to_string())
    } else {
        eprintln!("打印失败");
        Err(format!("Windows-打印失败"))
    }
}
