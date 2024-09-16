use std::{sync::mpsc};
use std::thread;

use std::env;
use tauri::api::process::Command;
use tauri::api::Error as TauriError;
use tauri::api::process::Output;
use std::path::{ PathBuf};
use crate::{declare::PrintOptions, fsys::remove_file};
use std::str::from_utf8;

/**
 * Get printers by name on windows using powershell
 */
pub fn get_printers_by_name(printername: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Get-Printer -Name \"", 
            &printername, 
            "\" | Select-Object Name, DriverName, JobCount, PrintProcessor, PortName, ShareName, ComputerName, PrinterStatus, Shared, Type, Priority | ConvertTo-Json"
        ])
        .output()
        .unwrap();
    output.stdout
}

pub fn get_printers() -> String {
    // Create a channel for communication
    let (sender, receiver) = mpsc::channel();
    println!("Detected Windows 8 or higher");
    let command = 
        vec![
            "-Command",
            "Get-Printer | Select-Object Name, DriverName, JobCount, PrintProcessor, PortName, ShareName, ComputerName, PrinterStatus, Shared, Type, Priority | ConvertTo-Json"
        ];
  
    
    // Spawn a new thread to execute the command
    thread::spawn(move || {
        println!("Spawned thread to execute PowerShell command.");

        // Execute the PowerShell command
        let output = Command::new("powershell")
            .args(&command)
            .output();
        
        match output {
            Ok(output) => {
                println!("PowerShell command executed successfully.");

               let stdout_string = String::from_utf8_lossy(output.stdout.as_bytes()).to_string();
                println!("Command output: {}", stdout_string);
                
                if let Err(e) = sender.send(stdout_string) {
                    println!("Failed to send output through channel: {:?}", e);
                }
            }
            Err(e) => {
                println!("Failed to execute PowerShell command: {:?}", e);
                if let Err(send_err) = sender.send(String::new()) {
                    println!("Failed to send error through channel: {:?}", send_err);
                }
            }
        }
    });

    println!("Main thread is doing other non-blocking work.");

    // Receive the result from the spawned thread
    match receiver.recv() {
        Ok(res) => {
            println!("Successfully received result from the spawned thread.");
            res
        }
        Err(e) => {
            println!("Failed to receive result from the spawned thread: {:?}", e);
            String::new()
        }
    }
}


pub fn print_pdf(options: PrintOptions) -> Result<String, String> {
    // 获取临时目录路径
    let dir: PathBuf = env::temp_dir();
    println!("临时目录: {}", dir.display());

    // 构建打印命令
    let print_arg = format!("-print-to {}", options.id);
    let shell_command = format!("{}sm {} {} {}", dir.display(), print_arg, options.print_setting, options.path);
    println!("生成的命令: {}", shell_command);

    // 执行命令
    let output = Command::new("powershell")
        .args(&["-Command", &shell_command]) // 使用 args 方法传递多个参数
        .output()
        .map_err(|e| format!("执行命令失败: {}", e))?;

    // 根据命令执行结果返回相应的信息
    if output.status.success() {
        println!("打印成功");
        if options.remove_after_print {
            remove_file(&options.path).map_err(|e| format!("删除文件失败: {}", e))?;
            println!("文件已删除: {}", options.path);
        }
        Ok("Windows-打印成功".to_string())
    } else {
        let error_message = String::from_utf8_lossy(output.stderr.as_bytes()); // 使用 as_bytes() 转换为 &[u8]
        eprintln!("打印失败: {}", error_message);
        Err(format!("Windows-打印失败: {}", error_message))
    }
}

 pub fn get_jobs(printer_name: String) -> String {
    use std::process::Command;

    let command = format!(
        "Get-PrintJob -PrinterName \"{}\" | Select-Object DocumentName,SubmittedTime,UserName,PrinterName| ConvertTo-Json",
        printer_name
    );

    let output = Command::new("powershell")
        .arg("-Command")
        .arg(command)
        .output()
        .map_err(|e| e.to_string());

    match output {
        Ok(output_data) => {
            if output_data.status.success() {
                from_utf8(&output_data.stdout)
                    .unwrap_or("Error converting output to UTF-8")
                    .to_string()
            } else {
                let error_message = from_utf8(&output_data.stderr)
                    .unwrap_or("Error converting error output to UTF-8")
                    .to_string();
                format!("Command failed with error: {}", error_message)
            }
        }
        Err(error) => format!("Failed to execute PowerShell command: {}", error)
    }
}

/**
 * Get printer job by id on windows using powershell
 */
pub fn get_jobs_by_id(printername: String, jobid: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Get-PrintJob -PrinterName \"", 
            &printername, 
            "\" -ID \"", 
            &jobid, 
            "\"  | Select-Object DocumentName,Id,TotalPages,Position,Size,SubmmitedTime,UserName,PagesPrinted,JobTime,ComputerName,Datatype,PrinterName,Priority,SubmittedTime,JobStatus | ConvertTo-Json"
        ])
        .output()
        .unwrap();
    output.stdout
}

/**
 * Resume printers job on windows using powershell
 */
pub fn resume_job(printername: String, jobid: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Resume-PrintJob -PrinterName \"", 
            &printername, 
            "\" -ID \"", 
            &jobid, 
            "\" "
        ])
        .output()
        .unwrap();
    output.stdout
}

/**
 * Restart printers job on windows using powershell
 */
pub fn windows_restart_job(printername: String, jobid: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Restart-PrintJob -PrinterName \"", 
            &printername, 
            "\" -ID \"", 
            &jobid, 
            "\" "
        ])
        .output()
        .unwrap();
    output.stdout
}

/**
 * Pause printers job on windows using powershell
 */
pub fn pause_job(printername: String, jobid: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Suspend-PrintJob -PrinterName \"", 
            &printername, 
            "\" -ID \"", 
            &jobid, 
            "\" "
        ])
        .output()
        .unwrap();
    output.stdout
}

/**
 * Remove printers job on windows using powershell
 */
pub fn remove_job(printername: String, jobid: String) -> String {
    let output = Command::new("powershell")
        .args([
            "Remove-PrintJob -PrinterName \"", 
            &printername, 
            "\" -ID \"", 
            &jobid, 
            "\" "
        ])
        .output()
        .unwrap();
    output.stdout
}
