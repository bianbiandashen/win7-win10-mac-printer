use std::{process::{Command, Stdio}, sync::mpsc, thread};
use std::env;
use crate::{declare::PrintOptions, fsys::remove_file};
use std::path::{PathBuf};
use ::log::{error, info};
use serde_json::Value;
use std::ffi::CString;
use std::fs::File;
use std::io::Read;
use std::ptr;
use crate::utils::parse_print_setting;
use std::time::Instant;
#[cfg(windows)]
use crate::windows_version::is_64bit_system;
use crate::error::{CustomError, ErrorCode, System, Component};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
use winapi::um::handleapi::CloseHandle;
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
#[cfg(target_os = "windows")]
use winapi::um::synchapi::WaitForSingleObject;
#[cfg(target_os = "windows")]
use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0, STARTF_USESHOWWINDOW};
#[cfg(target_os = "windows")]
use crate::POWERSHELL_PATH;
use crate::event::emit_event;
use crate::printpdf::PrintResponse;

#[cfg(target_os = "windows")]
pub fn get_default_printer_win7() -> Result<String, String> {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0, STARTF_USESHOWWINDOW};
    use winapi::um::synchapi::WaitForSingleObject;
    use std::ffi::CString;
    use std::fs::File;
    use std::io::Read;
    use std::ptr;
    use base64::encode; // 使用 base64::encode
    println!("start default printer win7");
    let temp_dir = "C:\\temp\\";
    let temp_file_path = format!("{}default_printer_output.txt", temp_dir);
    println!("start default printer win71");

    // 构建 PowerShell 脚本
    let ps_script = format!(
        r#"
        $defaultPrinter = Get-WmiObject -Query "SELECT * FROM Win32_Printer WHERE Default = True" |
        Select-Object -ExpandProperty Name
        $defaultPrinter | Out-File -FilePath '{}' -Encoding utf8
        "#,
        temp_file_path
    );
    info!("start default printer win72");

    // 确保临时目录存在
    if let Err(e) = std::fs::create_dir_all(temp_dir) {
        error!("【桌面端-get_default_printer_win7】无法创建临时目录: {:?}", e);
        return Err("无法创建临时目录".to_string());
    }

    // 将 PowerShell 脚本编码为 Base64
    let ps_script_utf16le: Vec<u8> = ps_script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let ps_script_base64 = encode(&ps_script_utf16le);
    info!("start default printer win73");
    let powershell_path = POWERSHELL_PATH.try_lock()
        .map_err(|_| "无法获取 PowerShell 路径".to_string())?;
    info!("start default printer win74");

    let cmd_str = format!(
        "\"{}\" -WindowStyle Hidden -NoProfile -EncodedCommand {}",
        powershell_path,
        ps_script_base64
    );
    info!("【桌面端win7】PowerShell 命令: {}", cmd_str);
    let cmd = CString::new(cmd_str).unwrap();
    let cmd_ptr = cmd.into_raw();

    unsafe {
        // 设置进程信息以隐藏窗口
        let mut si: STARTUPINFOA = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
        si.dwFlags = STARTF_USESHOWWINDOW;
        si.wShowWindow = 0; // 隐藏窗口

        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

        // 创建进程以隐藏窗口执行 PowerShell 命令
        if CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        ) == 0
        {
            let error_msg = format!("【桌面端-get_default_printer_win7】创建进程失败，无法执行 PowerShell 命令");
            error!("{}", error_msg);
            CString::from_raw(cmd_ptr); // 释放 CString
            return Err(error_msg);
        }

        // 等待进程完成
        WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString
    }

    // 读取输出文件内容
    let mut output = Vec::new();
    info!("【桌面端win7】PowerShell 命令output {:?}", output);

    let mut file = match File::open(&temp_file_path) {
        Ok(file) => file,
        Err(e) => {
            error!("【桌面端-get_default_printer_win7】无法打开输出文件: {:?}", e);
            return Err("无法读取输出文件".to_string());
        }
    };
    if let Err(e) = file.read_to_end(&mut output) {
        error!("【桌面端-get_default_printer_win7】无法读取输出文件: {:?}", e);
        return Err("无法读取输出文件".to_string());
    }
    let output_str = String::from_utf8_lossy(&output);

    // 清理临时文件
    if let Err(e) = std::fs::remove_file(&temp_file_path) {
        error!("【桌面端-get_default_printer_win7】无法删除临时文件: {:?}", e);
    }

    if output_str.trim().is_empty() {
        error!("【桌面端-get_default_printer_win7】未找到默认打印机名称");
        Err("无法获取默认打印机名称".to_string())
    } else {
        info!("【桌面端-get_default_printer_win7】成功获取默认打印机名称: {}", output_str.trim());
        Ok(output_str.trim().to_string())
    }
}

// 检查 PowerShell 版本的函数
#[cfg(target_os = "windows")]
fn check_powershell_version() -> String {
    use std::fs::{self, File};
    use std::io::Read;
    use std::ffi::CString;
    use std::path::Path;
    use std::ptr;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0};
    use winapi::um::synchapi::WaitForSingleObject;

    // 创建临时文件路径
    let temp_dir = "C:\\temp\\";
    let temp_file_path = format!("{}ps_version_output.txt", temp_dir);

    // 确保临时目录存在
    if !Path::new(temp_dir).exists() {
        info!("【桌面端win7】Temp directory not found, attempting to create: {}", temp_dir);
        if let Err(e) = fs::create_dir_all(temp_dir) {
            error!("【桌面端win7】Failed to create temp directory: {}. Error: {:?}", temp_dir, e);
            return "Directory creation failed".to_string();
        }
    } else {
        info!("【桌面端win7】Temp directory already exists: {}", temp_dir);
    }

    unsafe {
        // 创建命令字符串
        // let cmd = CString::new(format!("powershell -NoProfile -Command '$PSVersionTable.PSVersion.ToString()' > {}", temp_file_path)).unwrap();
        // 构建查询默认打印机的 PowerShell 命令，并将输出重定向到指定的临时文件
        let powershell_path = match POWERSHELL_PATH.try_lock() {
            Ok(path) => path,
            Err(_) => return "PowerShell path lock failed".to_string(),
        };
        let ps_command = format!(
            "\"{}\" -NoProfile -Command \"$defaultPrinter = Get-WmiObject -Query \\\"SELECT * FROM Win32_Printer WHERE Default = True\\\" | Select-Object -ExpandProperty Name; Add-Content -Path '{}' -Value $defaultPrinter\"",
            powershell_path,
            temp_file_path
        );

        let cmd = CString::new(ps_command).unwrap();
        let cmd_ptr = cmd.as_ptr();
        info!("【桌面端win7】PowerShell command prepared: {:?}", cmd);

        // 设置启动信息，确保窗口隐藏
        let mut si: STARTUPINFOA = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
        si.dwFlags = winapi::um::winbase::STARTF_USESHOWWINDOW;
        si.wShowWindow = 0;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

        // 执行 PowerShell 命令并隐藏窗口
        info!("【桌面端win7】Attempting to create process for PowerShell command.");
        if CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        ) == 0 {
            error!("【桌面端win7】Failed to create process for PowerShell version command.");
            return "Unknown version".to_string();
        }
        info!("【桌面端win7】PowerShell process created successfully, waiting for completion.");

        // 等待命令完成
        WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        info!("【桌面端win7】PowerShell process completed, attempting to read output file: {}", temp_file_path);

        // 读取输出文件
        if !Path::new(&temp_file_path).exists() {
            error!("【桌面端win7】Output file not found: {}", temp_file_path);
            return "Output file missing".to_string();
        }

        let mut output = Vec::new();
        match File::open(&temp_file_path) {
            Ok(mut file) => {
                info!("【桌面端win7】Successfully opened output file: {}", temp_file_path);
                if let Err(e) = file.read_to_end(&mut output) {
                    error!("【桌面端win7】Failed to read output file: {}. Error: {:?}", temp_file_path, e);
                    return "File read failed".to_string();
                }
            }
            Err(e) => {
                error!("【桌面端win7】Failed to open output file: {}. Error: {:?}", temp_file_path, e);
                return "File open failed".to_string();
            }
        }

        let output_str = String::from_utf8_lossy(&output);
        let version = output_str.trim().to_string();
        info!("【桌面端win7】PowerShell version obtained: {}", version);

        if version.is_empty() {
            "Unknown version".to_string()
        } else {
            version
        }
    }
}


// 检查用户是否具有管理员权限的函数
#[cfg(target_os = "windows")]
fn check_admin_privileges() -> bool {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0};
    use winapi::um::synchapi::WaitForSingleObject;
    unsafe {
        info!("【桌面端win7】调用方法: check_admin_privileges - 开始检查管理员权限...");

        // 确保临时文件目录存在
        let temp_dir = "C:\\temp\\";
        if let Err(e) = std::fs::create_dir_all(temp_dir) {
            error!("【桌面端win7】无法创建临时目录: {:?}", e);
            return false;
        }
        let temp_file_path = format!("{}admin_check_output.txt", temp_dir);
        info!("【桌面端win7】临时输出文件路径: {}", temp_file_path);

        // 构建要执行的命令
        let powershell_path = match POWERSHELL_PATH.try_lock() {
            Ok(path) => path,
            Err(_) => {
                error!("【桌面端win7】无法获取 PowerShell 路径锁");
                return false;
            }
        };
        let cmd_str = format!(
            "\"{}\" -NoProfile -Command \"whoami /groups | Select-String -Pattern 'S-1-5-32-544'\" > {}",
            powershell_path,
            temp_file_path
        );
        info!("【桌面端win7】执行的命令: {}", cmd_str);

        let cmd = CString::new(cmd_str).unwrap();
        let cmd_ptr = cmd.into_raw();

        // 设置启动信息以隐藏窗口
        let mut si: STARTUPINFOA = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
        si.dwFlags = winapi::um::winbase::STARTF_USESHOWWINDOW;
        si.wShowWindow = 0; // 隐藏窗口

        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();

        // 创建进程以执行命令
        info!("【桌面端win7】创建进程以执行管理员检查命令...");
        if CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        ) == 0
        {
            error!("【桌面端win7】创建进程失败，无法执行管理员检查命令。");
            CString::from_raw(cmd_ptr); // 确保 CString 释放
            return false;
        }

        // 等待命令执行完成
        WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString

        // 读取命令输出
        let mut output = Vec::new();
        let mut file = match File::open(&temp_file_path) {
            Ok(file) => file,
            Err(e) => {
                error!("【桌面端win7】无法打开输出文件: {:?}", e);
                return false;
            }
        };
        if let Err(e) = file.read_to_end(&mut output) {
            error!("【桌面端win7】无法读取输出文件: {:?}", e);
            return false;
        }
        let output_str = String::from_utf8_lossy(&output);
        info!("【桌面端win7】命令输出内容: {}", output_str);

        // 检查输出中是否包含管理员组的 SID
        let is_admin = output_str.lines().any(|line| {
            // 打印每一行内容
            info!("【桌面端win7】输出行: {}", line);
            // 检查是否包含管理员组的 SID
            line.contains("S-1-5-32-544")
        });

        // 打印检查结果
        info!(
            "【桌面端win7】检查管理员权限结果: {}",
            if is_admin { "有管理员权限" } else { "无管理员权限" }
        );
        info!(
            "【桌面端win7】检查管理员权限结果: {}",
            if is_admin { "有管理员权限" } else { "无管理员权限" }
        );

        // 清理临时文件
        if let Err(e) = std::fs::remove_file(&temp_file_path) {
            error!("【桌面端win7】无法删除临时文件: {:?}", e);
        }

        is_admin
    }
}

// 获取所有打印机的函数
#[cfg(target_os = "windows")]
pub fn get_printers_win7() -> Result<String, String> {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0, STARTF_USESHOWWINDOW};
    use winapi::um::synchapi::WaitForSingleObject;
    use std::ffi::CString;
    use std::fs::File;
    use std::io::Read;
    use std::ptr;
    use std::env;
    use base64::encode;  // 使用 base64::encode 代替
    use serde_json::Value;

    info!("【桌面端win7】调用方法: get_printers_win7 - 开始获取打印机列表...");

    // 检查 PowerShell 版本
    let powershell_version = check_powershell_version();
    info!("【桌面端win7】PowerShell 版本: {}", powershell_version);

    // 检查管理员权限
    let is_admin = check_admin_privileges();
    if !is_admin {
        error!("【桌面端win7】用户没有管理员权限。");
    }

    let temp_dir = "C:\\temp\\";
    let temp_file_path = "C:\\temp\\printers_output.txt";
    let ps_script = format!(r#"
    $printers = Get-WmiObject -Query 'SELECT * FROM Win32_Printer' |
    Where-Object {{ $_.Name -ne $null }} |
    Select-Object Name, DriverName, JobCount, PrintProcessor |
    ForEach-Object {{
        $id = if ($_.Name) {{ $_.Name.Trim() -replace '"', '\\\"' }} else {{ 'Unknown' }}
        $name = if ($_.Name) {{ $_.Name.Trim() -replace '"', '\\\"' }} else {{ 'Unknown' }}

        # 构建 JSON
        $json = '{{' +
        '\"id\":\"' + $id + '\",' +
        '\"name\":\"' + $name + '\"' +
        '}}'
        $json
    }}

    # 输出为 JSON 数组
    '[' + ($printers -join ',') + ']' | Out-File -FilePath '{}' -Encoding utf8
    "#, temp_file_path);
    // 将脚本编码为 UTF-16LE
    let ps_script_utf16le: Vec<u8> = ps_script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    // 使用 base64::encode 进行编码
    let ps_script_base64 = encode(&ps_script_utf16le);
    // 打印 temp_file_path
    info!("【桌面端win7】temp_file_path: {}", temp_file_path);
    // 确保临时目录存在
    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        error!("{}【桌面端win7】无法创建临时目录: {:?}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x045), e);
        return Err("无法创建临时目录。".to_string());
    }
    let powershell_path = POWERSHELL_PATH.try_lock()
        .map_err(|_| "无法获取 PowerShell 路径".to_string())?;
    let cmd_str = format!(
        "\"{}\" -WindowStyle Hidden -NoProfile -EncodedCommand {}",
        powershell_path,
        ps_script_base64
    );
    info!("【桌面端win7】执行的命令: {}", cmd_str);

    // 转换为 CString
    let cmd = CString::new(cmd_str.clone()).unwrap();
    let cmd_ptr = cmd.into_raw();

    // 设置启动信息以隐藏窗口
    let mut si: STARTUPINFOA = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // 创建进程以执行命令
    info!("【桌面端win7】创建进程以执行 PowerShell 脚本...");
    if unsafe {
        CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        )
    } == 0
    {

        error!("{}【桌面端win7】创建进程失败，无法执行 PowerShell 脚本。", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x046));
        unsafe {
            CString::from_raw(cmd_ptr); // 释放 CString
        }
        // TODO
        // return Err("{}无法执行 PowerShell 脚本。".to_string(), ErrorCode::new(System::Windows7, Component::PrinterListModule,0x047));
        return Err("{}无法执行 PowerShell 脚本。".to_string());
    }

    // 等待命令执行完成
    unsafe {
        WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString
    }
    // 读取 PowerShell 脚本的输出结果
    let mut output = Vec::new();
    let mut file = match File::open(&temp_file_path) {
        Ok(file) => file,
        Err(e) => {
            let error_message = format!("{}【桌面端win7】无法打开输出文件: {:?}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x048), e);
            // 记录错误日志
            error!("{}", error_message);
            // 返回错误
            return Err(error_message);
        }
    };
    if let Err(e) = file.read_to_end(&mut output) {
        let log_info = format!("{} 【桌面端win7】无法读取输出文件: {:?}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x049), e);
        error!("{}", log_info);
        return Err(log_info);
    }
    let output_str = String::from_utf8_lossy(&output).to_string();

    info!("【桌面端win7】成功获取打印机信息: {}", output_str);
    Ok(output_str)
}

// 获取打印作业信息的函数
#[cfg(target_os = "windows")]
pub fn get_jobs_win7(printer_name: String) -> Result<String, String> {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, STARTF_USESHOWWINDOW};
    use winapi::um::synchapi::WaitForSingleObject;
    use std::ffi::CString;
    use std::ptr;
    use std::fs::File;
    use std::io::Read;
    use std::env;

    info!("【桌面端win7】调用方法: get_jobs_win7 - 开始获取打印作业列表...");

    let temp_dir = "C:\\temp\\";
    let temp_file_path = format!("{}print_jobs_output.txt", temp_dir);
    let ps_script = format!(r#"
        $jobs = Get-WmiObject -Query "SELECT * FROM Win32_PrintJob WHERE Name LIKE '%{}%'" |
        Select-Object Document, JobId, TotalPages, Position, Size, TimeSubmitted, Owner, PagesPrinted, StartTime, HostPrintQueue, DataType, PrinterName, Priority, JobStatus |
        ForEach-Object {{
            $job = '{{' +
            '\"Document\":\"' + $_.Document.Trim() + '\",' +
            '\"JobId\":\"' + $_.JobId + '\",' +
            '\"TotalPages\":\"' + $_.TotalPages + '\",' +
            '\"Position\":\"' + $_.Position + '\",' +
            '\"Size\":\"' + $_.Size + '\",' +
            '\"TimeSubmitted\":\"' + $_.TimeSubmitted + '\",' +
            '\"Owner\":\"' + $_.Owner.Trim() + '\",' +
            '\"PagesPrinted\":\"' + $_.PagesPrinted + '\",' +
            '\"PrinterName\":\"' + $_.PrinterName.Trim() + '\",' +
            '\"JobStatus\":\"' + $_.JobStatus + '\"' +
            '}}'
            $job
        }}
        '[' + ($jobs -join ',') + ']' | Out-File -FilePath '{}' -Encoding utf8
    "#, printer_name, temp_file_path);

    // 将 PowerShell 脚本编码为 UTF-16LE，并使用 Base64 编码
    let ps_script_utf16le: Vec<u8> = ps_script.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let ps_script_base64 = base64::encode(&ps_script_utf16le);

    // 打印 PowerShell 脚本路径
    info!("【桌面端win7】临时文件路径: {}", temp_file_path);

    // 确保临时目录存在
    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        let error_msg = format!("{}【桌面端win7】无法创建临时目录: {}", ErrorCode::new(System::Windows7, Component::PrinterListModule, 0x054), e);
        error!("{}", error_msg);
        return Err(error_msg);
    }

    let powershell_path = POWERSHELL_PATH.try_lock()
        .map_err(|_| "无法获取 PowerShell 路径".to_string())?;
    let cmd_str = format!(
        "\"{}\" -WindowStyle Hidden -NoProfile -EncodedCommand {}",
        powershell_path,
        ps_script_base64
    );

    info!("【桌面端win7】执行的命令: {}", cmd_str);

    // 转换为 CString
    let cmd = CString::new(cmd_str.clone()).unwrap();
    let cmd_ptr = cmd.into_raw();

    // 设置启动信息以隐藏窗口
    let mut si: STARTUPINFOA = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // 创建进程执行 PowerShell 脚本
    info!("【桌面端win7】创建进程以执行 PowerShell 脚本...");
    if unsafe {
        CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        )
    } == 0
    {
        let error_msg = format!("{}【桌面端win7】创建进程失败，无法执行 PowerShell 脚本。", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x055));
        error!("{}", error_msg);
        unsafe {
            CString::from_raw(cmd_ptr); // 释放 CString
        }
        return Err(error_msg);
    }

    // 等待进程执行完成
    unsafe {
        WaitForSingleObject(pi.hProcess, 0xFFFFFFFF);
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString
    }

    // 读取 PowerShell 脚本输出的结果
    let mut output = Vec::new();
    let mut file = match File::open(&temp_file_path) {
        Ok(file) => file,
        Err(e) => {
            let error_msg = format!("{}【桌面端win7】无法打开输出文件: {}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x056), e);
            error!("{}", error_msg);
            return Err(error_msg);
        }
    };
    if let Err(e) = file.read_to_end(&mut output) {
        let error_msg = format!("{}【桌面端win7】无法打开输出文件: {}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x057), e);
        error!("{}", error_msg);
        return Err(error_msg);
    }

    let output_str = String::from_utf8_lossy(&output).to_string();
    info!("【桌面端win7】成功获取打印作业信息: {}", output_str);

    Ok(output_str)
}

/** 兼容模式，使用SM打印，普通打印机有锯齿感 */
#[cfg(target_os = "windows")]
pub fn print_pdf_compatible_mode(options: PrintOptions) -> Result<String, String> {
    use std::env;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::path::PathBuf;
    use std::ffi::CString;
    use std::ptr;
    use std::time::Instant;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, STARTF_USESHOWWINDOW};
    // 已移除 WaitForSingleObject 导入
    use base64;

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    // 记录异步任务开始时间
    let task_start = Instant::now();

    // 创建打印命令耗时起点
    let create_print_command_start = Instant::now();

    // 获取临时目录
    let dir: PathBuf = env::temp_dir();

    // 构建直接执行的打印命令
    let print_arg = format!("-print-to {}", options.id);
    let shell_command = format!("{}sm {} {}", dir.display(), print_arg, options.path);

    // 将 PowerShell 脚本编码为 UTF-16LE 并转换为 Base64
    let ps_script_utf16le: Vec<u8> = shell_command.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let ps_script_base64 = base64::encode(&ps_script_utf16le);

    // 构建 PowerShell 执行命令 (完整的命令行)
    let powershell_path = POWERSHELL_PATH.try_lock()
    .map_err(|_| "无法获取 PowerShell 路径".to_string())?;
    let cmd_str = format!("\"{}\" -WindowStyle Hidden -NoProfile -EncodedCommand {}", powershell_path, ps_script_base64);
    info!("【桌面端windows7-print_pdf_compatible_mode】print_command cmd_str: {}", cmd_str);

    // 转换为 CString 并获取指针
    let cmd = match CString::new(cmd_str.clone()) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("无法创建 CString: {}", e);
            error!("{}", error_msg);
            print_response.message = error_msg.clone();
            emit_event("print_pdf_task_finished", print_response.clone());
            return Err(error_msg);
        }
    };
    info!("【桌面端windows7-print_pdf_compatible_mode】print_command cmd: {:?}", cmd);

    let cmd_ptr = cmd.into_raw(); // 转为裸指针
    info!("【桌面端windows7-print_pdf_compatible_mode】print_command cmd_ptr: {:?}", cmd_ptr);

    // 设置启动信息以隐藏窗口
    let mut si: STARTUPINFOA = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // 创建打印命令耗时
    let create_print_command_duration = create_print_command_start.elapsed();
    info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-创建打印命令耗时: {:?}", create_print_command_duration);

    // 开始执行打印命令耗时统计（从 CreateProcess 开始计时）
    let run_print_command_start = Instant::now();

    // 测量 CreateProcessA 耗时
    let create_process_start = Instant::now();
    let create_ret = unsafe {
        CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        )
    };
    let create_process_duration = create_process_start.elapsed();

    if create_ret == 0 {
        let error_msg = format!("{}无法执行 PowerShell 脚本", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x058));
        error!("{}", error_msg);
        // 释放 CString 内存
        unsafe { CString::from_raw(cmd_ptr); }
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }
    print_response.success = true;
    print_response.message = "Windows7-打印指令已发送".to_string();
    emit_event("print_pdf_task_finished", print_response.clone());
    info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-CreateProcessA 耗时: {:?}", create_process_duration);

    // 不再等待进程执行完成，直接关闭句柄并释放资源
    let close_handle_start = Instant::now();
    unsafe {
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString 内存
    }
    let close_handle_duration = close_handle_start.elapsed();
    info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-CloseHandle + CString::from_raw 耗时: {:?}", close_handle_duration);

    // 计算执行打印命令的总耗时（包含CreateProcess、CloseHandle等，不含等待）
    let run_print_command_duration = run_print_command_start.elapsed();
    info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-执行打印命令总耗时(不等待进程结束): {:?}", run_print_command_duration);

    // 打印完整的 PowerShell 命令行
    info!("【桌面端windows7-print_pdf_compatible_mode】-完整的 PowerShell 命令行: {}", cmd_str);

    // 创建临时文件记录打印结果耗时
    let temp_file_result_start = Instant::now();
    let temp_file_path = dir.join("print_pdf_output.txt");

    // 写入结果 (此时打印任务可能尚未完成)
    if let Err(e) = File::create(&temp_file_path).and_then(|mut file| {
        file.write_all("PDF打印指令已发送".as_bytes())
    }) {
        let error_msg = format!("{}写入输出文件失败 {}", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x059), e);
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    let temp_file_result_duration = temp_file_result_start.elapsed();
    info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-创建临时文件记录打印结果耗时: {:?}", temp_file_result_duration);

    // 删除临时文件
    if remove_file(&temp_file_path).is_ok() {
        let task_duration = task_start.elapsed();
        info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-异步任务总耗时: {:?}", task_duration);
        Ok("Windows7-打印指令已发送".to_string())
    } else {
        let task_duration = task_start.elapsed();
        info!("【桌面端windows7-print_pdf_compatible_mode 耗时分析】-异步任务总耗时-失败时: {:?}", task_duration);
        let error_msg = format!("{}删除临时文件失败", ErrorCode::new(System::Windows7, Component::PrinterListModule,0x060));
        error!("{}", error_msg);
        Err(error_msg)
    }
}

/** 高清模式，通过GhostScript打印 */
#[cfg(target_os = "windows")]
pub async fn print_pdf(options: PrintOptions, page_size: String) -> Result<String, String> {
    use std::env;
    use std::fs::{File, remove_file};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessW, PROCESS_INFORMATION, STARTUPINFOW};
    use winapi::um::winbase::{CREATE_NO_WINDOW, STARTF_USESHOWWINDOW};
    use winapi::um::synchapi::WaitForSingleObject;

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    // 获取系统临时目录路径
    let dir: PathBuf = env::temp_dir();
    info!("【桌面端-print_pdf】临时目录: {}", dir.display());

    // 根据系统架构读取对应的可执行文件
    let mut file_name = "gswin32c.exe";
    // if is_64bit_system() {
    //     file_name = "gswin64c.exe"
    // }

    // 设置 Ghostscript 可执行文件的路径
    let gs_path = dir.join(file_name);
    info!("【桌面端-print_pdf】Ghostscript路径: {}", gs_path.display());

    // 输出 page_size 设置
    info!("【桌面端-print_pdf】page_size: {}", &page_size);
    let (offset_x, offset_y) = parse_print_setting(page_size);
    info!("【桌面端-print_pdf】页面偏移量 - offset_x: {}, offset_y: {}", offset_x, offset_y);

    // 检查 Ghostscript 文件是否存在
    if !gs_path.exists() {
        let error_msg = format!("{} Ghostscript可执行文件不存在: {}", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x068), gs_path.display());
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 确认待打印的 PDF 文件路径
    let pdf_path = Path::new(&options.path);
    if !pdf_path.exists() {
        let error_msg = format!("{} 待打印的PDF文件不存在: {}", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x069), pdf_path.display());
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 设置打印机名称
    let printname = options.id.as_str();

    // -sDEVICE=mswinpr2: 指定设备类型为 mswinpr2，用于直接发送输出到 Windows 打印系统。
    // -dNOPAUSE: 禁用页面之间的暂停，自动打印所有页面。
    // -dNOPROMPT: 禁用所有用户提示，使打印过程不被中断。
    // -dSILENT: 静默模式，抑制输出日志和错误信息。
    // -dBATCH: 在处理完所有文件后自动退出。
    // -dQUIET: 静默模式，无任何提示输出。
    // -sstdout=%stderr: 将标准输出重定向到标准错误输出，以便获取 Ghostscript 的输出信息。
    // -dNOSAFER: 禁用 Ghostscript 的安全模式（注意：可能会带来安全风险）。
    // -sOutputFile="%printer%<PrinterName>": 指定输出为指定打印机，其中 <PrinterName> 是目标打印机的名称。
    // -c "<</PageOffset [X Y]>> setpagedevice": 设置页面偏移量（X 和 Y）以调整打印位置。
    // -f "<PDF Path>": 指定待打印的 PDF 文件路径
    // 构建 Ghostscript 打印命令行
    let command_line = format!(
        "\"{}\" -sDEVICE=mswinpr2 -dNOPAUSE -dNOPROMPT -dSILENT \
        -dBATCH -dQUIET -sstdout=%stderr -dNOSAFER -sOutputFile=\"%printer%@@{}@@\" \
        -c \"<</PageOffset [{} {}]>> setpagedevice\" -f \"{}\"",
        gs_path.display(),
        printname,
        offset_x,
        offset_y,
        pdf_path.display()
    );

    // 优化 command_line 格式，去除冗余符号
    let command_line = command_line.replace("@@\"", "").replace("\"@@", "");
    info!("【桌面端-print_pdf】构建的命令行: {}", command_line);

    // 解析命令行并支持宽字符集，确保支持中文字符
    let os_command_line = OsString::from(&command_line);
    let mut cmd: Vec<u16> = os_command_line.encode_wide().chain(std::iter::once(0)).collect();
    let cmd_ptr = cmd.as_mut_ptr();

    info!("【桌面端-print_pdf】宽字符数组内容: {:?}", cmd);

    // 设置启动信息，隐藏进程窗口
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口
    info!("【桌面端-print_pdf】启动信息已配置");

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    info!("【桌面端-print_pdf】进程信息已初始化");

    // 创建并执行进程
    let create_process_result = unsafe {
        CreateProcessW(
            ptr::null_mut(),
            cmd_ptr,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        )
    };
    if create_process_result == 0 {
        let error_msg = format!("{} 【桌面端-print_pdf】CreateProcessW命令执行失败", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x071));
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }
    info!("【桌面端-print_pdf】CreateProcessW命令执行成功");

    // 等待进程完成执行
    let wait_result = unsafe { WaitForSingleObject(pi.hProcess, 0xFFFFFFFF) };
    info!("【桌面端-print_pdf】进程等待返回值: {}", wait_result);

    // 检查进程的退出代码
    let mut exit_code: u32 = 0;
    let get_exit_code_result = unsafe {
        winapi::um::processthreadsapi::GetExitCodeProcess(pi.hProcess, &mut exit_code)
    };
    if get_exit_code_result == 0 {
        info!("【桌面端-print_pdf】获取进程退出代码失败");
    } else {
        info!("【桌面端-print_pdf】进程退出代码: {}", exit_code);
    }

    // 关闭进程和线程句柄
    unsafe {
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
    }
    info!("【桌面端-print_pdf】进程和线程句柄已关闭");

    // 记录命令行到日志文件
    let command_log_path = dir.join("command_log.txt");
    if let Err(e) = File::create(&command_log_path).and_then(|mut file| {
        writeln!(file, "{}", command_line) // 写入完整的命令行
    }) {
        let error_msg = format!("{} 【桌面端-print_pdf】记录命令行日志失败 {}", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x072), e);
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    } else {
        info!("【桌面端-print_pdf】命令行已记录到日志: {}", command_log_path.display());
    }

    // 创建临时文件记录打印成功状态
    let temp_file_path = dir.join("print_pdf_output.txt");
    if let Err(e) = File::create(&temp_file_path).and_then(|mut file| {
        file.write_all("PDF打印成功".as_bytes())
    }) {
        let error_msg = format!("{} 【桌面端-print_pdf】记录打印状态文件失败 {}", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x073), e);
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    } else {
        info!("【桌面端-print_pdf】打印结果已记录到文件: {}", temp_file_path.display());
    }

    // 删除临时文件
    // if let Err(e) = remove_file(&temp_file_path) {
    //     let error_msg = format!("{} 【桌面端-删除临时文件失败 {}", ErrorCode::new(System::Windows7, Component::PrinterCommandModule, 0x074), e);
    //     error!("{}", error_msg);
    //     return Err(error_msg);
    // } else {
    //     info!("【桌面端-print_pdf】临时文件已删除: {}", temp_file_path.display());
    // }
    print_response.success = true;
    print_response.message = "Windows10-打印成功".to_string();
    emit_event("print_pdf_task_finished", print_response.clone());

    info!("【桌面端-print_pdf】PDF打印流程完成");
    Ok("Windows10-打印成功".to_string())
}

#[cfg(target_os = "windows")]
pub fn print_pdf_sync(options: PrintOptions, page_size: String) -> Result<String, String> {
    use std::env;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::path::PathBuf;
    use std::ffi::CString;
    use std::ptr;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
    use winapi::um::winbase::{CREATE_NO_WINDOW, STARTF_USESHOWWINDOW};
    use winapi::um::synchapi::WaitForSingleObject;
    use base64;

    let log_info = "【桌面端windows7-print_pdf_sync】";

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    // 获取临时目录
    let dir: PathBuf = env::temp_dir();
    info!("{} 临时目录: {}", log_info, dir.display());

    // 构建直接执行的打印命令
    let print_arg = format!("-print-to {}", options.id);
    let shell_command = format!("{}sm {} {}", dir.display(), print_arg, options.path);

    // 将 PowerShell 脚本编码为 UTF-16LE 并转换为 Base64
    let ps_script_utf16le: Vec<u8> = shell_command.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let ps_script_base64 = base64::encode(&ps_script_utf16le);

    // 构建 PowerShell 执行命令
    let powershell_path = POWERSHELL_PATH.try_lock()
        .map_err(|_| "无法获取 PowerShell 路径".to_string())?;
    let cmd_str = format!("\"{}\" -WindowStyle Hidden -NoProfile -EncodedCommand {}",
        powershell_path,
        ps_script_base64
    );
    info!("{} print_command cmd_str: {}", log_info, cmd_str);

    // 转换为 CString 并获取指针
    let cmd = match CString::new(cmd_str.clone()) {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("无法创建 CString: {}", e);
            error!("{}", error_msg);
            print_response.message = error_msg.clone();
            emit_event("print_pdf_task_finished", print_response.clone());
            return Err(error_msg);
        }
    };
    info!("{} print_command cmd: {:?}", log_info, cmd);

    let cmd_ptr = cmd.into_raw(); // 转为裸指针
    info!("{} print_command cmd_ptr: {:?}", log_info, cmd_ptr);

    // 设置启动信息以隐藏窗口
    let mut si: STARTUPINFOA = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    // 创建并执行进程
    let create_ret = unsafe {
        CreateProcessA(
            ptr::null_mut(),
            cmd_ptr as *mut i8,
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            CREATE_NO_WINDOW,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut si,
            &mut pi,
        )
    };

    if create_ret == 0 {
        let error_msg = format!("{}无法执行 PowerShell 脚本",
            ErrorCode::new(System::Windows7, Component::PrinterListModule,0x058)
        );
        error!("{}", error_msg);
        // 释放 CString 内存
        unsafe { CString::from_raw(cmd_ptr); }
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 等待进程完成执行
    let wait_result = unsafe { WaitForSingleObject(pi.hProcess, 0xFFFFFFFF) };
    info!("{} 进程等待返回值: {}", log_info, wait_result);

    // 检查进程的退出代码
    let mut exit_code: u32 = 0;
    let get_exit_code_result = unsafe {
        winapi::um::processthreadsapi::GetExitCodeProcess(pi.hProcess, &mut exit_code)
    };

    if get_exit_code_result == 0 {
        info!("{} 获取进程退出代码失败", log_info);
    } else {
        info!("{} 进程退出代码: {}", log_info, exit_code);
    }

    // 关闭进程和线程句柄
    unsafe {
        CloseHandle(pi.hProcess);
        CloseHandle(pi.hThread);
        CString::from_raw(cmd_ptr); // 释放 CString 内存
    }
    info!("{} 进程和线程句柄已关闭", log_info);

    print_response.success = true;
    print_response.message = "Windows7-打印成功".to_string();
    emit_event("print_pdf_task_finished", print_response.clone());

    info!("{} PDF打印流程完成", log_info);
    Ok("Windows7-打印成功".to_string())
}

