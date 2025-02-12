use std::time::Duration;
use std::time::Instant;
use std::env;
use std::str;
use std::path::PathBuf;
use tauri::api::process::Command;
use tauri::api::Error as TauriError;
use tauri::api::process::Output;
use tauri::command;
use crate::utils::{parse_print_setting, wide_string_to_string};
use crate::declare::PrintOptions;
use std::str::from_utf8;
use log::{ info, error};
use tokio::task;
use chrono::Local;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::GetLastError;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Printing::{EnumPrintersW, PRINTER_ENUM_LOCAL, PRINTER_ENUM_CONNECTIONS, PRINTER_INFO_1W};
#[cfg(target_os = "windows")]
use windows::core::Error;
#[cfg(target_os = "windows")]
use windows::core::PWSTR;
// use windows::core::PWSTR;
// use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
// use windows::Win32::Graphics::Printing::{EnumPrintersW, GetDefaultPrinterW, PRINTER_ENUM_LOCAL, PRINTER_INFO_6};
#[cfg(target_os = "windows")]
use crate::windows_version::is_64bit_system;
use crate::error::{CustomError, ErrorCode, System, Component};
#[cfg(target_os = "windows")]
use crate::utils::can_use_std_command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;


#[cfg(target_os = "windows")]
use serde::Serialize;
#[cfg(target_os = "windows")]
use serde_json::json;
#[cfg(target_os = "windows")]
use std::mem;
#[cfg(target_os = "windows")]
use std::ptr;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use std::thread;
use std::process::Command as StdCommand;
use tauri::api::process::Command as TauriCommand;
use crate::event::emit_event;
use crate::printpdf::PrintResponse;
#[cfg(target_os = "windows")]
use crate::printpdf::set_thread_priority_max;

#[cfg(target_os = "windows")]
pub async fn print_pdf_compatible_mode(options: PrintOptions) -> Result<String, String> {

    #[cfg(target_os = "windows")]
    set_thread_priority_max(); // 设置绘制线程的最高优先级
    let log_info = "【桌面端-print_pdf】".to_string();
    let log_info_clone = log_info.clone();

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    let result = task::spawn_blocking({
        let mut print_response = print_response.clone(); // 克隆并标记为 mut
        move || {
            // 获取临时目录并构建 SumatraPDF 可执行文件路径
            let dir: PathBuf = env::temp_dir();
            let sumatra_exe_path = dir.join("SumatraPDF-prerel-64.exe");
            #[cfg(target_os = "windows")]
            set_thread_priority_max(); // 设置绘制线程的最高优先级
            info!(
                "{} SumatraPDF 可执行文件路径: {}",
                log_info,
                sumatra_exe_path.display()
            );

            // 获取打印机 ID 并记录
            let printer_id = options.id.trim_matches('"').to_string();
            info!("{} 打印机 ID: {}", log_info, printer_id);

            // 判断是否使用 std::process::Command
            let use_std_command = can_use_std_command();
            info!("{} 是否使用 std::process::Command: {}", log_info, use_std_command);
            // info!("【Window10-print_pdf】打印命令开始执行时间: {:?}, 文件路径: {:?}", log_info, Local::now().format("%Y-%m-%d %H:%M:%S").to_string(), options.path);
            let mut print_settings_str = String::new(); // 使用 String 类型
            if options.auto_fit {
                print_settings_str += "fit"; // 连接字符串
            }

            if use_std_command {
                info!(
                    "{} 执行 std::process::Command 执行打印命令",
                    log_info
                );

                // 使用 std::process::Command 执行打印命令
                let mut command = std::process::Command::new(&sumatra_exe_path);
                command.arg("-print-to")
                    .arg(&printer_id);

                if !print_settings_str.is_empty() {
                    command.arg("-print-settings").arg(&print_settings_str);
                }

                command.arg(&options.path)
                    .creation_flags(0x08000000 | 0x04000000);

                // 获取命令的输出
                let output = command.output();

                // 打印命令行的值
                let command_line = format!(
                    "{} -print-to {} -print-settings {} {}",
                    sumatra_exe_path.display(),
                    printer_id,
                    &print_settings_str,
                    &options.path
                );
                let command_exec_start_time = Instant::now();
                info!("{} 执行的命令: {}", log_info, command_line);
                match output {
                    Ok(output) => {
                        if output.status.success() {
                            // info!("【Window10-print_pdf】打印命令开始执行完成时间: {:?}, 文件路径: {:?}, 使用std_command", log_info, Local::now().format("%Y-%m-%d %H:%M:%S"), options.path);
                            info!("{} 打印命令执行成功, 执行耗时： {:?}", log_info, command_exec_start_time.elapsed());
                            let message = "Windows-打印成功".to_string();
                            print_response.success = true;
                            print_response.message = message.clone();
                            Ok(print_response)
                        } else {
                            let stderr_message = String::from_utf8_lossy(&output.stderr);
                            let error_message = format!(
                                "{} - Windows-打印失败: {}",
                                ErrorCode::new(
                                    System::Windows10,
                                    Component::PrinterCommandModule,
                                    0x042
                                ),
                                stderr_message
                            );
                            print_response.message = error_message.clone();
                            error!("{} {}", log_info, error_message);
                            Err(print_response)
                        }
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "{} {} Windows-命令执行错误: {}",
                            ErrorCode::new(
                                System::Windows10,
                                Component::PrinterCommandModule,
                                0x043
                            ),
                            log_info,
                            e
                        );
                        print_response.message = error_msg.clone();
                        error!("{} {}", log_info, error_msg);
                        Err(print_response)
                    }
                }
            } else {
                info!(
                    "{} 执行 TauriCommand 执行打印命令",
                    log_info
                );

                // 使用 TauriCommand 执行打印命令
                let mut args = vec!["-print-to", &printer_id];
                if !print_settings_str.is_empty() {
                    args.push("-print-settings");
                    args.push(&print_settings_str);
                }
                args.push(&options.path);

                let tauri_command = TauriCommand::new(sumatra_exe_path.to_string_lossy().into_owned())
                    .args(args)
                    .output();

                // 打印命令行的值
                let command_line = format!(
                    "{} -print-to {} -print-settings {} {}",
                    sumatra_exe_path.display(),
                    printer_id,
                    &print_settings_str,
                    &options.path
                );
                info!("{} 执行的命令: {}", log_info, command_line);

                match tauri_command {
                    Ok(output) => {
                        if output.status.success() {
                            // info!("【Window10-print_pdf】打印命令开始执行完成时间: {:?}, 文件路径: {:?}, 使用tauri_command", log_info, Local::now().format("%Y-%m-%d %H:%M:%S"), options.path);
                            info!(
                                "{} 打印命令执行成功 via TauriCommand",
                                log_info
                            );
                            let message = "Windows-打印成功".to_string();
                            print_response.success = true;
                            print_response.message = message.clone();
                            Ok(print_response)
                        } else {
                            // let stderr_message = String::from_utf8_lossy(&output.stderr);
                            let error_message = format!(
                                "{} - Windows-打印失败 via TauriCommand: {}",
                                ErrorCode::new(
                                    System::Windows10,
                                    Component::PrinterCommandModule,
                                    0x042
                                ),
                                "Windows-打印失败 via TauriCommand".to_string()
                            );
                            print_response.message = error_message.clone();
                            error!("{} {}", log_info, error_message);
                            Err(print_response)
                        }
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "{} {} Windows-命令执行错误 via TauriCommand: {}",
                            ErrorCode::new(
                                System::Windows10,
                                Component::PrinterCommandModule,
                                0x043
                            ),
                            log_info,
                            e
                        );
                        print_response.message = error_msg.clone();
                        error!("{} {}", log_info, error_msg);
                        Err(print_response)
                    }
                }
            }
        }
    }).await;

    match result {
        Ok(response) => {
            match response {
                Ok(res) => {
                    print_response = res;
                },
                Err(err_response) => {
                    print_response = err_response;
                }
            }
        },
        Err(e) => {
            let error_msg = format!("{} {} 任务执行失败: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x044), log_info_clone, e);
            error!("{}", error_msg);
            print_response.message = error_msg.clone();
            emit_event("print_pdf_task_finished", print_response.clone());
            return Err("任务执行失败".to_string());
        }
    }

    emit_event("print_pdf_task_finished", print_response.clone());

    Ok(print_response.taskid)
}
// 启动和视图选项
// -presentation：以演模式打开文件。
// -fullscreen：全屏模式打开文件。
// -new-window：每次打开文件都使用新窗口（3.2+版本）。
// -appdata <目录>：指定 SumatraPDF 设置和缩略图缓存的自定义目录。
// -restrict：启用限制模式，禁用文件系统、注册表和网络访问（适合在封闭环境中使用）。
// 导航选项
// -named-dest <目标名称>：定位到文件中的特定位置（3.1+版本）。
// -page <页码>：滚动到指定页。
// -view <视图模式>：指定视图模式（如单页、连续页等）。
// -zoom <缩放级别>：设定缩放级别（例如"fit page"或百分比值）。
// 搜索选项
// -search <关键字>：打开文件时搜索关键字（3.4+版本）。
// 打印选项
// -print-to-default：打印到系统默认打印机，并退出。
// -print-to <打印机名称>：指定打印机进行打印，并退出。
// -print-settings <设置列表>：用于自定义打印设置，支持以下选项：
// even 或 odd：选择打印偶数或奇数页。
// portrait 或 landscape：页面方向。
// noscale、shrink 和 fit：缩放选项。
// color 或 monochrome：颜色设置。
// duplex：双面打印选项（如 duplexshort，duplexlong）。
// bin=<托盘号或名称>：选择纸盒。
// paper=<页面大小>：指定纸张大小（如 A4、Letter 等）。
// 错误抑制和退出
// -silent：屏蔽打印错误消息。
// -print-dialog：显示打印对话框。
// -exit-when-done：打印后退出。
// 性能和测试选项
// -stress-test：渲染所有页面以测试稳定性和性能。
// -bench <文件路径>：渲染指定页面并输出渲染时间（用于性能测试）。
// 过时选项
// 包括颜色和界面语言设置，这些选项已被新的设置文件替代，将来可能被删除。

// 使用设置影响打印效果的方式
// 为了更好地控制打印果，可以在命令行中的 -print-settings 参数中进行设置。例如：

// -print-settings "1-5,odd,fit,bin=2"：打印第1到第5页中的奇数页，适配页面大小，并使用2号纸盒。
// -print-settings "3x"：打印3份。
// 此外，通过指定 paper 和 bin 可以精确控制纸张大小和纸盒选择，以满足特定的打印需求

#[cfg(target_os = "windows")]
pub fn print_pdf_sync(options: PrintOptions, page_size: String) -> Result<String, String> {
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
    use crate::onixcomponents::utils::get_font_path;

    let log_info = "【桌面端-print_pdf_sync】";

    // 获取系统临时目录路径
    let dir: PathBuf = env::temp_dir();
    info!("{} 临时目录: {}", log_info, dir.display());

    // 根据系统架构读取对应的可执行文件
    let mut file_name = "gswin64c.exe";
    if !is_64bit_system() {
        file_name = "gswin32c.exe"
    }

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    // 设置 Ghostscript 可执行文件的路径
    let gs_path = dir.join(file_name);
    info!("{} Ghostscript路径: {}", log_info, gs_path.display());

    // 检查 Ghostscript 文件是否存在
    if !gs_path.exists() {
        let error_msg = format!("{} Ghostscript可执行文件不存在: {}",
            ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x063),
            gs_path.display()
        );
        info!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 确认待打印的 PDF 文件路径
    let pdf_path = Path::new(&options.path);
    if !pdf_path.exists() {
        let error_msg = format!("{} 待打印的PDF文件不存在: {}",
            ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x064),
            pdf_path.display()
        );
        info!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 设置打印机名称
    let printname = options.id.as_str();

    let (offset_x, offset_y) = parse_print_setting(page_size);
    info!("【桌面端-print_pdf】页面偏移量 - offset_x: {}, offset_y: {}", offset_x, offset_y);

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

    info!("{} 处理后的打印机名称: {}", log_info, printname);
    info!("{} 完整命令行: {}", log_info, command_line);

    // 优化 command_line 格式，去除冗余符号
    let command_line = command_line.replace("@@\"", "").replace("\"@@", "");
    info!("{} 构建的命令行: {}", log_info, command_line);

    // 解析命令行并支持宽字符集
    let os_command_line = OsString::from(&command_line);
    let mut cmd: Vec<u16> = os_command_line.encode_wide().chain(std::iter::once(0)).collect();
    let cmd_ptr = cmd.as_mut_ptr();

    // 设置启动信息，隐藏进程窗口
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    si.dwFlags = STARTF_USESHOWWINDOW;
    si.wShowWindow = 0; // 隐藏窗口

    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

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
        let error_msg = format!("{} CreateProcessW命令执行失败",
            ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x070)
        );
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }
    info!("{} CreateProcessW命令执行成功", log_info);

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
    }
    info!("{} 进程和线程句柄已关闭", log_info);

    print_response.success = true;
    print_response.message = "Windows10-打印成功".to_string();
    emit_event("print_pdf_task_finished", print_response.clone());

    info!("{} PDF打印流程完成", log_info);
    Ok("Windows10-打印成功".to_string())
}


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

    use crate::onixcomponents::utils::get_font_path;
    // 记录异步任务开始时间
    let task_start = Instant::now();

    // 获取系统临时目录路径
    let dir: PathBuf = env::temp_dir();
    info!("【桌面端-print_pdf】临时目录: {}", dir.display());

    // 根据系统架构读取对应的可执行文件
    let mut file_name = "gswin64c.exe";
    if !is_64bit_system() {
        file_name = "gswin32c.exe"
    }

    let mut print_response = PrintResponse {
        taskid: options.taskid.clone(),
        printer: options.printer.clone(),
        pdf_path: options.path.to_string(),
        success: false,
        message: String::new(),
    };

    // 设置 Ghostscript 可执行文件的路径
    let gs_path = dir.join(file_name);
    info!("【桌面端-print_pdf】Ghostscript路径: {}", gs_path.display());

    // 输出 page_size 设置
    info!("【桌面端-print_pdf】page_size: {}", &page_size);
    let (offset_x, offset_y) = parse_print_setting(page_size);
    info!("【桌面端-print_pdf】页面偏移量 - offset_x: {}, offset_y: {}", offset_x, offset_y);

    // 检查 Ghostscript 文件是否存在
    if !gs_path.exists() {
        let error_msg = format!("{} Ghostscript可执行文件不存在: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x063), gs_path.display());
        info!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 确认待打印的 PDF 文件路径
    let pdf_path = Path::new(&options.path);
    if !pdf_path.exists() {
        let error_msg = format!("{} 待打印的PDF文件不存在: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x064), pdf_path.display());
        info!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    }

    // 设置打印机名称
    let printname = options.id.as_str();



    let font_path: PathBuf = get_font_path("SimHei, Arial, sans-serif", "normal");

    // -sDEVICE=mswinpr2: 指定设备类型为 mswinpr2，用于直接发送输出到 Windows 打印系统。
    // -dNOPAUSE: 禁用页面之间的暂停，自动打印所有页面。
    // -dNOPROMPT: 禁用所有用户提示，使打印过程不被中断。
    // -dSILENT: 静默模式，抑制输出日志和错误信息。
    // -dBATCH: 在处理完所有文件后自动退出。
    // -dQUIET: 静默模式，无任何提示输出。
    // -sstdout=%stderr: 将标准输出重定向到标准错误输出，以便获取 Ghostscript 的输出信息。
    // -dPDFUSEEMBEDDEDFONTS=false 强制GhostScript忽略PDF中嵌入的字体
    // -sFONTPATH=/111 指定字体路径
    // -sSUBSTFONT=Helvetica 指定替代字体名称

    // -dNOSAFER: 禁用 Ghostscript 的安全模式（注意：可能会带来安全险）。
    // -sOutputFile="%printer%<PrinterName>": 指定输出到指定打印机，其中 <PrinterName> 是目标打印机的名称。
    // -c "<</PageOffset [X Y]>> setpagedevice": 设置页面偏移量（X 和 Y）以调整打印位置。
    // -f "<PDF Path>": 指定待打印的 PDF 文件路径


    /**
     * @贝林 2024-12-10 10:40:00
     * 新增3参数dPDFUUSEEMBEDDEDFONTS、sFONTPATH、sSUBSTFONT，详细说明请参考上方备注信息
     */
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

    // 添加日志记录
    info!("【桌面端-print_pdf】处理后的打印机名称: {}", printname);
    info!("【桌面端-print_pdf】完整命令行: {}", command_line);

    // 优化 command_line 格式，去除冗余符号
    let command_line = command_line.replace("@@\"", "").replace("\"@@", "");
    info!("【桌面端-print_pdf】构建的命令行: {}", command_line);

    // 解析命令行并支持宽字符集，确保支持中文字符+
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
        let error_msg = format!("{} 【桌面端-print_pdf】CreateProcessW命令执行失败", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x070));
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
        let error_msg = format!("{}【桌面端-print_pdf】记录命令行日志失败: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x065),e);
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
        let error_msg = format!("{}【桌面端-记录打印状态文件失败: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x066),e);
        error!("{}", error_msg);
        print_response.message = error_msg.clone();
        emit_event("print_pdf_task_finished", print_response.clone());
        return Err(error_msg);
    } else {
        info!("【桌面端-print_pdf】打印结果已记录到文件: {}", temp_file_path.display());
    }

    // 记录异步任务总耗时
    let task_duration = task_start.elapsed();
    info!("【桌面端-print_pdf 耗时分析】-异步任务总耗时: {:?}", task_duration);


    print_response.success = true;
    print_response.message = "Windows10-打印成功".to_string();
    emit_event("print_pdf_task_finished", print_response.clone());

    // 删除临时文件
    // if let Err(e) = remove_file(&temp_file_path) {
    //     let error_msg = format!("{}【桌面端-删除临时文件失败: {}", ErrorCode::new(System::Windows10, Component::PrinterCommandModule, 0x067),e);
    //     error!("{}", error_msg);
    //     return Err(error_msg);
    // } else {
    //     info!("【桌面端-print_pdf】临时文件已删除: {}", temp_file_path.display());
    // }

    info!("【桌面端-print_pdf】PDF打印流程完成");
    Ok("Windows10-打印成功".to_string())
}


#[cfg(target_os = "windows")]
pub fn get_jobs(printer_name: String) -> String {
    let (sender, receiver) = mpsc::channel();
    let mut log_info = "【桌面端-get_jobs】".to_string();
    // 先存储格式化的命令字符串
    let formatted_command = format!(
        "Get-PrintJob -PrinterName \"{}\" | Select-Object DocumentName,SubmittedTime,UserName,PrinterName | ConvertTo-Json",
        printer_name
    );
    let command = vec![
        "-NoProfile".to_string(),
        "-Command".to_string(),
        formatted_command,  // 使用 String 而非 &str
    ];

    let mut log_info_move = log_info.clone();
    thread::spawn(move || {
        let full_command = format!("powershell -WindowStyle Hidden {}", command.join(" "));
        log_info_move = format!("{} 执行命令: {}", log_info_move, full_command);
        let output = Command::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe")
            .args(&["-WindowStyle", "Hidden"])
            .args(&command)
            .output();
            match output {
                Ok(output) => {
                    let stdout_string = String::from_utf8_lossy(output.stdout.as_bytes()).to_string();
                    log_info_move = format!("{} 命令执行成功", log_info_move);
                    if let Err(e) = sender.send(stdout_string) {
                        log_info_move = format!("{}{} 推送广播消息输出失败：{}", ErrorCode::new(System::Windows10, Component::PrinterListModule, 0x037), log_info_move, e);
                        error!("{}", log_info_move);
                    }else {
                        log_info_move = format!("{} 推送广播消息成功", log_info_move);
                        info!("{}", log_info_move);
                    }
                }
                Err(e) => {
                    log_info_move = format!("{} 执行命令失败：{}", log_info_move, e);
                    if let Err(send_err) = sender.send(String::new()) {
                        log_info_move = format!("{}{} 推送广播消息执行失败：{}", ErrorCode::new(System::Windows10, Component::PrinterListModule, 0x038), log_info_move, send_err);
                        error!("{}", log_info_move);
                    }else {
                        log_info_move = format!("{} 推送广播消息成功", log_info_move);
                        info!("{}", log_info_move);
                    }
                }
            }
    });

    match receiver.recv() {
        Ok(res) => {
            log_info = format!("{} 成功接收线程结果", log_info);
            info!("{}", log_info);
            res
        }
        Err(e) => {
            log_info = format!("{}{} 接收线程结果失败：{}",ErrorCode::new(System::Windows10, Component::PrinterListModule, 0x039), log_info, e);
            error!("{}", log_info);
            String::new()
        }
    }
}


// #[cfg(target_os = "windows")]
// pub fn get_jobs_new(printer_name: String) -> String {
//     use windows::Win32::Graphics::Printing::{EnumJobsW, JOB_INFO_2W, PRINTER_ENUM_JOBS};
//     use windows::Win32::Foundation::{GetLastError, HANDLE};
//     use windows::Win32::Graphics::Printing::{OpenPrinterW, ClosePrinter};
//     use log::info;
//     use serde::Serialize;

//     #[derive(Serialize, Debug)]
//     struct JobInfo {
//         document_name: String,
//         submitted_time: String,
//         user_name: String,
//         printer_name: String,
//     }

//     unsafe {
//         info!("【桌面端-get_jobs】开始获取打印任务列表");

//         // 打开打印机
//         let mut h_printer: HANDLE = std::ptr::null_mut();
//         let printer_name_wide: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();

//         if OpenPrinterW(printer_name_wide.as_ptr(), &mut h_printer, std::ptr::null()).as_bool() == false {
//             let error_code = GetLastError().0;
//             info!(
//                 "【桌面端-get_jobs】打开打印机失败，错误代码: {}",
//                 error_code
//             );
//             return "[]".to_string();
//         }

//         // 第一次调用获取缓冲区大小
//         let mut buffer_size: u32 = 0;
//         let mut jobs_returned: u32 = 0;

//         let result = EnumJobsW(
//             h_printer,
//             0,  // First job
//             u32::MAX,  // Last job
//             2,  // Level (JOB_INFO_2)
//             std::ptr::null_mut(),
//             0,
//             &mut buffer_size,
//             &mut jobs_returned
//         );

//         if result == 0 && GetLastError().0 != 122 {  // 122 = ERROR_INSUFFICIENT_BUFFER
//             let error_code = GetLastError().0;
//             info!(
//                 "【桌面端-get_jobs】获取打印任务缓冲区大小失败，错误代码: {}",
//                 error_code
//             );
//             ClosePrinter(h_printer);
//             return "[]".to_string();
//         }

//         // 分配缓冲区
//         let mut buffer: Vec<u8> = vec![0; buffer_size as usize];

//         // 第二次调用获取实际数据
//         let result = EnumJobsW(
//             h_printer,
//             0,
//             u32::MAX,
//             2,
//             buffer.as_mut_ptr() as *mut _,
//             buffer_size,
//             &mut buffer_size,
//             &mut jobs_returned
//         );

//         if result == 0 {
//             let error_code = GetLastError().0;
//             info!(
//                 "【桌面端-get_jobs】获取打印任务数据失败，错误代码: {}",
//                 error_code
//             );
//             ClosePrinter(h_printer);
//             return "[]".to_string();
//         }

//         // 解析数据
//         let jobs = std::slice::from_raw_parts(
//             buffer.as_ptr() as *const JOB_INFO_2W,
//             jobs_returned as usize
//         );

//         let job_infos = jobs.iter().map(|job| JobInfo {
//             document_name: wide_string_to_string(job.pDocument),
//             submitted_time: format_system_time(job.Submitted),
//             user_name: wide_string_to_string(job.pUserName),
//             printer_name: printer_name.clone(),
//         }).collect::<Vec<JobInfo>>();

//         // 关闭打印机句柄
//         ClosePrinter(h_printer);

//         // 序列化为 JSON
//         serde_json::to_string(&job_infos).unwrap_or_else(|_| "[]".to_string())
//     }
// }

// 辅助函数：将SYSTEMTIME格式化为字符串
#[cfg(target_os = "windows")]
fn format_system_time(system_time: windows::Win32::Foundation::SYSTEMTIME) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        system_time.wYear,
        system_time.wMonth,
        system_time.wDay,
        system_time.wHour,
        system_time.wMinute,
        system_time.wSecond
    )
}

#[cfg(target_os = "windows")]
pub fn get_default_printer_new() -> Result<String, String> {
    use windows::Win32::Graphics::Printing::GetDefaultPrinterW;
    use windows::Win32::Foundation::GetLastError;
    use windows::core::PWSTR;
    use log::info;

    unsafe {
        info!("【桌面端-get_default_printer】开始获取默认打印机");

        // 初始化缓冲区大小
        let mut buffer_size: u32 = 0;

        // 第一次调用 GetDefaultPrinterW 获取缓冲区大小
        let result = GetDefaultPrinterW(PWSTR::null(), &mut buffer_size);
        if result.as_bool() == false {
            let error_code = GetLastError().0;
            if error_code != 122 {
                // 非缓冲区不足的错误，直接返回
                info!(
                    "【桌面端-get_default_printer】获取默认打印机缓冲区大小失败，错误代码: {}",
                    error_code
                );
                return Err(format!(
                    "【桌面端-get_default_printer】获取默认打印机缓冲区大小失败，错误代码: {}",
                    error_code
                ));
            }
        }

        if buffer_size == 0 {
            info!("【桌面端-get_default_printer】缓冲区大小为 0，无法继续。");
            return Err("缓冲区大小为 0，无法继续".to_string());
        }

        info!("【桌面端-get_default_printer】需要的缓冲区大小: {}", buffer_size);

        // 分配缓冲区以存储默认打印机名称
        let mut buffer: Vec<u16> = vec![0; buffer_size as usize];

        // 第二次调用 GetDefaultPrinterW 获取默认打印机名称
        let result = GetDefaultPrinterW(PWSTR(buffer.as_mut_ptr()), &mut buffer_size);
        if result.as_bool() == false {
            let error_code = GetLastError().0;
            info!(
                "【桌面端-get_default_printer】获取默认打印机名称失败，错误代码: {}",
                error_code
            );
            return Err(format!(
                "【桌面端-get_default_printer】获取默认打印机名称失败，错误代码: {}",
                error_code
            ));
        }

        // 转换宽字符串为 Rust 的 String
        let printer_name = wide_string_to_string(buffer.as_ptr());

        info!(
            "【桌面端-get_default_printer】获取默认打印机成功，名称: {}",
            printer_name
        );

        Ok(printer_name)
    }
}

#[cfg(target_os = "windows")]
pub fn get_printers_new() -> String {
    use std::ptr;
    use serde::Serialize;
    use windows::Win32::Graphics::Printing::{EnumPrintersW, PRINTER_ENUM_LOCAL, PRINTER_ENUM_CONNECTIONS, PRINTER_INFO_1W};
    use windows::Win32::Foundation::GetLastError;
    use windows::core::Error;
    use log::info;

    #[derive(Serialize, Debug)]
    struct PrinterInfo {
        name: String,
        description: Option<String>,
        comment: Option<String>,
    }

    unsafe {
        info!("【桌面端-get_printers_new】开始获取打印机列表");

        let mut buffer_size: u32 = 0;
        let mut printers_returned: u32 = 0;

        // 设置 flags 和 level
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let level = 1; // 获取基本信息

        info!(
            "【桌面端-get_printers_new】线程 ID: {:?}, 参数: flags={}, name=None, level={}",
            std::thread::current().id(),
            flags,
            level
        );

        // 第一次调用 EnumPrintersW 获取缓冲区大小
        let result = EnumPrintersW(
            flags,
            None, // name 为 None 表示所有打印机
            level,
            None, // 第一次调用不需要实际缓冲区
            &mut buffer_size, // 存储所需缓冲区大小
            &mut printers_returned, // 存储打印机数量
        );

        if result.is_err() {
            let error_code = GetLastError().0;
            if error_code != 122 {
                info!(
                    "【桌面端-get_printers_new】获取缓冲区大小失败，错误代码: {}",
                    error_code
                );
                return "[]".to_string();
            }
        }

        if buffer_size == 0 {
            info!("【桌面端-get_printers_new】缓冲区大小为 0，无法继续。");
            return "[]".to_string();
        }

        info!(
            "【桌面端-get_printers_new】需要的缓冲区大小: {}, 打印机数量: {}",
            buffer_size, printers_returned
        );

        // 分配缓冲区
        let mut buffer: Vec<u8> = vec![0; buffer_size as usize];

        // 第二次调用 EnumPrintersW 获取实际的打印机数据
        let result = EnumPrintersW(
            flags,
            None,
            level,
            Some(&mut buffer),
            &mut buffer_size,
            &mut printers_returned,
        );

        if result.is_err() {
            let error_code = GetLastError().0;
            info!(
                "【桌面端-get_printers_new】获取打印机数据失败，错误代码: {}",
                error_code
            );
            return "[]".to_string();
        }

        info!(
            "【桌面端-get_printers_new】成功获取打印机数据，数量: {}",
            printers_returned
        );

        // 解析打印机数据
        let num_printers = printers_returned as usize;
        let printers_slice = std::slice::from_raw_parts(
            buffer.as_ptr() as *const PRINTER_INFO_1W,
            num_printers,
        );

        let printers = printers_slice
            .iter()
            .map(|info| PrinterInfo {
                name: wide_string_to_string(info.pName.0),
                description: if info.pDescription.is_null() {
                    None
                } else {
                    Some(wide_string_to_string(info.pDescription.0))
                },
                comment: if info.pComment.is_null() {
                    None
                } else {
                    Some(wide_string_to_string(info.pComment.0))
                },
            })
            .collect::<Vec<PrinterInfo>>();

        // 序列化为 JSON 格式
        let json_result = serde_json::to_string(&printers).unwrap_or_else(|_| "[]".to_string());
        info!("【桌面端-get_printers_new】打印机列表序列化为 JSON 成功");

        json_result
    }
}

