use std::process::Command;
#[cfg(windows)] extern crate winapi;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tauri::State;
use crate::utils::{get_build_version};
use crate::store::AppState;
use crate::apikit;
use crate::utils::{get_system_info, get_os_name};
use log::{info, error};
use std::mem::zeroed;
#[cfg(windows)]
use winapi::um::winnt::OSVERSIONINFOW;
use reqwest::Client;
use tauri::command;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use crate::globalcache::{DEVICE_ID_CACHE, SYSTEM_INFO_CACHE, OS_NAME_CACHE, USER_AGENT_CACHE, APP_VERSION};

impl Default for ContextData {
    fn default() -> Self {
        let start = SystemTime::now();
        let client_time = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64;

        Self {
            clientTime: client_time,
            context_nameTracker: "wapT".to_string(),
            context_platform: OS_NAME_CACHE.clone(),
            context_appVersion: get_build_version(),
            context_osVersion: SYSTEM_INFO_CACHE.clone(),
            context_deviceModel: "".to_string(),
            context_deviceId: DEVICE_ID_CACHE.clone().unwrap_or_else(|| "unknown".to_string()),
            context_package: APP_VERSION.try_lock().unwrap().clone(),
            context_networkType: "unknown".to_string(),
            context_matchedPath: "/apm/errorlistdetail".to_string(),
            context_route: "http://local.xiaohongshu.com:1388/apm/errorlistdetail".to_string(),
            context_userAgent: USER_AGENT_CACHE.lock().unwrap().clone(),
            context_artifactName: "xhs-electron-printer".to_string(),
            context_artifactVersion: "1.122.2-68".to_string(),
            context_networkQuality: "UNKNOWN".to_string(),
            context_deviceLevel: "0".to_string(),
            context_userId: DEVICE_ID_CACHE.clone().unwrap_or_else(|| "unknown".to_string()),
        }
    }
}


// Function to get the device ID for Windows or UUID for macOS
// #[cfg(not(windows))]
// #[tauri::command]
// pub fn get_device_id() -> Option<String> {
//     // macOS 获取 UUID
//     let output = Command::new("ioreg")
//         .arg("-rd1")
//         .arg("-c")
//         .arg("IOPlatformExpertDevice")
//         .output()
//         .ok()?;

//     if output.status.success() {
//         let uuid = String::from_utf8_lossy(&output.stdout)
//             .lines()
//             .find(|line| line.contains("\"IOPlatformUUID\""))
//             .and_then(|line| line.split('"').nth(3))
//             .map(String::from);
//         if let Some(uuid) = uuid {
//             return Some(uuid);
//         }
//         error!("【桌面端-APMRS】Failed to extract UUID from command output.");
//         return None;
//     }

//     error!("【桌面端-APMRS】Failed to execute ioreg command.");
//     None
// }

// #[cfg(target_os = "windows")]
// #[tauri::command]
// pub fn get_device_id() -> Option<String> {
//     use std::ffi::{CString};
//     use std::fs::{File, create_dir_all};
//     use std::io::{self, Read};
//     use std::ptr;
//     use winapi::um::handleapi::CloseHandle;
//     use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
//     use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0};
//     use winapi::um::synchapi::WaitForSingleObject;

//     unsafe {
//         // 确保临时文件目录存在
//         let temp_dir = "C:\\temp\\";
//         create_dir_all(temp_dir).expect("Failed to create temp directory");

//         let mut si: STARTUPINFOA = std::mem::zeroed();
//         si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
//         si.dwFlags = winapi::um::winbase::STARTF_USESHOWWINDOW;
//         si.wShowWindow = 0; // 隐藏窗口

//         let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
//         let temp_file_path = format!("{}uuid_output.txt", temp_dir);
//         let cmd = CString::new(format!("cmd /C wmic csproduct get UUID > {}", temp_file_path)).unwrap();
//         let cmd_ptr = cmd.into_raw();

//         // 创建进程
//         if CreateProcessA(
//             ptr::null_mut(),
//             cmd_ptr as *mut i8,
//             ptr::null_mut(),
//             ptr::null_mut(),
//             0,
//             CREATE_NO_WINDOW,
//             ptr::null_mut(),
//             ptr::null_mut(),
//             &mut si,
//             &mut pi,
//         ) == 0 {
//             error!("【桌面端-APMRS】Failed to create process for wmic command.");
//             return None;
//         }

//         // 等待命令完成
//         WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
//         CloseHandle(pi.hProcess);
//         CloseHandle(pi.hThread);
//         CString::from_raw(cmd_ptr); // 释放 CString

//         let mut output = Vec::new();
//         let mut file = File::open(&temp_file_path).expect("Failed to open output file");
//         file.read_to_end(&mut output).expect("Failed to read output file");
//         let output_str = String::from_utf8_lossy(&output);

//         // 处理输出
//         if let Some(uuid) = output_str.lines().nth(1).map(|line| line.trim().to_string()).filter(|line| !line.is_empty()) {
//             info!("【桌面端-APMRS】Successfully retrieved UUID: {}", uuid);
//             return Some(uuid);
//         }

//         error!("【桌面端-APMRS】Failed to retrieve UUID or UUID is empty.");
//         None
//     }
// }

// Define the ContextData struct for storing context information
#[derive(Serialize, Deserialize)]
pub struct ContextData {
    clientTime: u64,
    context_nameTracker: String,
    context_platform: String,
    context_appVersion: String,
    context_osVersion: String,
    context_deviceModel: String,
    context_deviceId: String,
    context_package: String,
    context_networkType: String,
    context_matchedPath: String,
    context_route: String,
    context_userAgent: String,
    context_artifactName: String,
    context_artifactVersion: String,
    context_networkQuality: String,
    context_deviceLevel: String,
    context_userId: String,
}

// // Implement a default constructor for ContextData
// impl Default for ContextData {
//     fn default() -> Self {
//         let start = SystemTime::now();
//         let client_time = start.duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis() as u64;
//         let device_id = get_device_id().unwrap_or_else(|| "unknown".to_string());

//         Self {
//             clientTime: client_time,
//             context_nameTracker: "wapT".to_string(),
//             context_platform: get_os_name(),
//             context_appVersion: get_build_version(),
//             context_osVersion: get_system_info(),
//             context_deviceModel: "".to_string(),
//             context_deviceId: device_id.clone(),
//             context_package: get_version_from_config(),
//             context_networkType: "unknown".to_string(),
//             context_matchedPath: "/apm/errorlistdetail".to_string(),
//             context_route: "http://local.xiaohongshu.com:1388/apm/errorlistdetail".to_string(),
//             context_userAgent: "".to_string(),
//             context_artifactName: "xhs-electron-printer".to_string(),
//             context_artifactVersion: "1.122.2-68".to_string(),
//             context_networkQuality: "UNKNOWN".to_string(),
//             context_deviceLevel: "0".to_string(),
//             context_userId: device_id.clone(),
//         }
//     }
// }




// Define the Measurement struct for holding measurement data
#[derive(Serialize, Deserialize)]
pub struct Measurement {
    measurement_name: String,
    measurement_data: HashMap<String, String>,
    #[serde(flatten)]
    context_data: ContextData,
}

// #[tauri::command]
// pub async fn set_user_agent(state: State<'_, AppState>, user_agent: String) -> Result<(), String> {
//     let mut lock = state.user_agent.lock().await;
//     *lock = Some(user_agent);
//     Ok(())
// }

// Tauri command to report a custom measurement
#[tauri::command]
pub async fn report_custom_measurement(
    measurement_name: String,
    custom_fields: HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let user_agent = {
        let lock = state.user_agent.lock().await;
        lock.clone().unwrap_or_else(|| "unknown".to_string())
    };

    // Record the start and current times
    let start_time = state.start_time;

    // Adjust custom fields if necessary
    let mut adjusted_custom_fields = custom_fields.clone();
    if measurement_name == "app_performance" {
        if let Some(lcp) = custom_fields.get("windows_lcp") {
            if let Ok(lcp_time) = lcp.parse::<u128>() {
                if lcp_time > start_time {
                    let duration_secs = (lcp_time - start_time) as f64;
                    adjusted_custom_fields.insert("windows_lcp".to_string(), duration_secs.to_string());
                }
            }
        }
        if let Some(lcp) = custom_fields.get("mac_lcp") {
            if let Ok(lcp_time) = lcp.parse::<u128>() {
                if lcp_time > start_time {
                    let duration_secs = (lcp_time - start_time) as f64;
                    adjusted_custom_fields.insert("mac_lcp".to_string(), duration_secs.to_string());
                }
            }
        }
    }

    // Create the context data and measurement instance
    let mut context_data = ContextData::default();
    context_data.context_userAgent = user_agent;
    context_data.context_userId = adjusted_custom_fields
        .get("sellerId")
        .cloned()
        .unwrap_or_else(|| "".to_string());
    let measurement = Measurement {
        measurement_name,
        measurement_data: adjusted_custom_fields,
        context_data,
    };

    let measurement_json = serde_json::to_string(&measurement).map_err(|e| e.to_string())?;
    let request = apikit::ApiRequest::new("https://apm-fe.xiaohongshu.com/api/data")
        .set_header("Content-Type", "application/json")
        .set_header("Biz-Type", "apm_fe")
        .set_body(&measurement_json);

    let request_log = format!("【桌面端-APMRS】APM Report Request");

    // Send POST request
    match apikit::send_request_command(request).await {
        Ok(response_body) => {
            info!("{}; Response Body: {}", request_log, response_body);
        }
        Err(error) => {
            error!("{}; Request Error: {}", request_log, error);
        }
    }

    Ok(())
}

#[derive(Serialize)]
pub struct BuildVersion {
    version: String,
}

#[command]
pub async fn get_last_build_version() -> Result<BuildVersion, String> {
    let url = "https://xhswaybill-printer-1251524319.cos.ap-shanghai.myqcloud.com/XHSPrintClient/prod/windows/xhs_printer_build_version.txt";
    let client = Client::new();
    match client.get(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.text().await {
                    Ok(text) => Ok(BuildVersion { version: text.trim().to_string() }),
                    Err(_) => Err("无法读取响应文本".into()),
                }
            } else {
                Err("请求失败".into())
            }
        }
        Err(_) => Err("无法发送请求".into()),
    }
}

#[command]
pub async fn get_last_build_version_macos() -> Result<BuildVersion, String> {
    let url = "https://xhswaybill-printer-1251524319.cos.ap-shanghai.myqcloud.com/XHSPrintClient/prod/macos/xhs_printer_build_version.txt";
    let client = Client::new();
    match client.get(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.text().await {
                    Ok(text) => Ok(BuildVersion { version: text.trim().to_string() }),
                    Err(_) => Err("无法读取响应文本".into()),
                }
            } else {
                Err("请求失败".into())
            }
        }
        Err(_) => Err("无法发送请求".into()),
    }
}

#[tauri::command]
pub async fn report_platform_crash(
    is_crash: bool,  // 区分是否是崩溃
    crash_reason: Option<String>,  // 崩溃原因，可选
    level: String,  // 错误等级
    state: Option<State<'_, AppState>>,  // 应用状态
    log_key: Option<String>,  // 日志key
) -> Result<(), String> {
    let system_info = get_system_info();
    let request_log = format!("【桌面端-APMRS】Report Platform Event: 系统类型：{}, 是否崩溃：{}", system_info, is_crash);

    let user_agent = {
        if let Some(state) = state {
            let lock = state.user_agent.lock().await;
            lock.clone().unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        }
    };

    // 获取当前时间（毫秒）
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();

    // 准备自定义字段
    let mut custom_fields = std::collections::HashMap::new();
    custom_fields.insert("platform".to_string(), system_info);
    custom_fields.insert("level".to_string(), level);
    custom_fields.insert("event_time".to_string(), current_time.to_string());
    custom_fields.insert("log_key".to_string(), log_key.unwrap_or_else(|| "unknown".to_string()));
    custom_fields.insert(
        "crash_reason".to_string(),
        crash_reason.unwrap_or_else(|| "unknown".to_string()),
    );

    // 如果是崩溃事件，记录崩溃信息
    if is_crash {
        custom_fields.insert("event_type".to_string(), "crash".to_string());
    } else {
        custom_fields.insert("event_type".to_string(), "normal_entry".to_string());
    }

    // 构建上下文数据和 measurement 实例
    let mut context_data = ContextData::default();
    context_data.context_userAgent = user_agent;

    let measurement_name = if is_crash {
        "platform_crash".to_string()
    } else {
        "platform_entry".to_string()  // 改为 "platform_entry"
    };

    let measurement = Measurement {
        measurement_name: measurement_name.clone(),
        measurement_data: custom_fields,
        context_data,
    };

    // 转换为 JSON 格式
    let measurement_json = match serde_json::to_string(&measurement) {
        Ok(json) => json,
        Err(e) => {
            error!("【桌面端-APMRS】将自定义上报数据转换为JSON失败: {}", e);
            return Err(e.to_string());
        }
    };

    // 发送 POST 请求
    let request = apikit::ApiRequest::new("https://apm-fe.xiaohongshu.com/api/data")
        .set_header("Content-Type", "application/json")
        .set_header("Biz-Type", "apm_fe")
        .set_body(&measurement_json);

    // 处理响应
    match apikit::send_request_command(request).await {
        Ok(response_body) => {
            info!("{}; Response Body: {}", request_log, response_body);
        }
        Err(error) => {
            error!("{}; Request Error: {}", request_log, error);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn report_platform_crash_async(
    is_crash: bool,  // 区分是否是崩溃
    crash_reason: Option<String>,  // 崩溃原因，可选
    level: String,  // 错误等级
    state: Option<State<'_, AppState>>,  // 应用状态
    log_key: Option<String>,  // 日志key
) -> Result<(), String> {
    tokio::spawn(async move {
        report_platform_crash(is_crash, crash_reason, level, None, log_key).await
    });
    Ok(())
}
#[cfg(windows)]
#[tauri::command]
pub unsafe fn is_windows_7_or_newer() -> bool {
    // 调用 RtlGetVersion 函数获取 Windows 版本信息
    let os_info = get_windows_version();

    // 打印检测到的 Windows 版本号
    println!(
        "Detected Windows version: {}.{} (Build {})",
        os_info.dwMajorVersion, os_info.dwMinorVersion, os_info.dwBuildNumber
    );

    // 检查是否为 Windows 7 或更高版本
    os_info.dwMajorVersion > 6 || (os_info.dwMajorVersion == 6 && os_info.dwMinorVersion > 1)
}

// 使用 RtlGetVersion 函数获取准确的 Windows 版本信息
#[cfg(windows)]
unsafe fn get_windows_version() -> OSVERSIONINFOW {
    let mut osvi: OSVERSIONINFOW = zeroed();
    osvi.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOW>() as u32;

    let ntdll = winapi::um::libloaderapi::GetModuleHandleA(b"ntdll.dll\0".as_ptr() as *const i8);
    let func_name = b"RtlGetVersion\0";
    let func = winapi::um::libloaderapi::GetProcAddress(ntdll, func_name.as_ptr() as *const i8);

    if func.is_null() {
        panic!("Failed to get RtlGetVersion function address");
    }

    let rtl_get_version: extern "system" fn(*mut OSVERSIONINFOW) -> i32 =
        std::mem::transmute(func);

    let status = rtl_get_version(&mut osvi);
    if status == 0 {
        osvi // 成功获取版本信息，返回 OSVERSIONINFOW 结构体
    } else {
        panic!("Failed to get Windows version information.");
    }
}
