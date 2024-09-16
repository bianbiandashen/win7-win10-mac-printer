use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tauri::{State};
use crate::AppState;
use crate::apikit; 


// Function to detect the platform: macOS, Windows 7 or above, or unknown
pub fn detect_platform() -> String {
    #[cfg(target_os = "macos")]
    return "mac".to_string();

    #[cfg(target_os = "windows")]
    unsafe {
        return if is_windows_7_or_newer() {
            "windows_7_above".to_string()
        } else {
            "windows_7".to_string()
        };
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return "unknown".to_string();
}

// Function to get the device ID for Windows or UUID for macOS
pub fn get_device_id() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // Execute a command to get the UUID on Windows
        let output = Command::new("wmic").arg("csproduct").arg("get").arg("uuid").output().ok()?;
        if output.status.success() {
            // Parse and trim the UUID
            let uuid = String::from_utf8_lossy(&output.stdout).lines().nth(1)?.trim().to_string();
            if !uuid.is_empty() {
                return Some(uuid);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Execute a command to get the UUID on macOS
        let output = Command::new("ioreg").arg("-rd1").arg("-c").arg("IOPlatformExpertDevice").output().ok()?;
        if output.status.success() {
            // Parse the UUID from the output
            if let Some(uuid_line) = output.stdout.split(|&b| b == b'\n').find(|line| {
                let line = String::from_utf8_lossy(line);
                line.contains("\"IOPlatformUUID\"")
            }) {
                let uuid = String::from_utf8_lossy(uuid_line).split('"').nth(3)?.trim().to_string();
                if !uuid.is_empty() {
                    return Some(uuid);
                }
            }
        }
    }

    None
}

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

// Implement a default constructor for ContextData
impl Default for ContextData {
    fn default() -> Self {
        // Get the current system time and convert to milliseconds
        let start = SystemTime::now();
        let client_time = start.duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis() as u64;

        // Get the device ID or UUID
        let device_id = get_device_id().unwrap_or_else(|| "unknown".to_string());

        Self {
            clientTime: client_time,
            context_nameTracker: "wapT".to_string(),
            context_platform: detect_platform(),
            context_appVersion: "discovery-0.0.0".to_string(),
            context_osVersion: "unknown".to_string(),
            context_deviceModel: "".to_string(),
            context_deviceId: device_id.clone(),
            context_package: "".to_string(),
            context_networkType: "unknown".to_string(),
            context_matchedPath: "/apm/errorlistdetail".to_string(),
            context_route: "http://local.xiaohongshu.com:1388/apm/errorlistdetail".to_string(),
            context_userAgent: "".to_string(),
            context_artifactName: "xhs-electron-printer".to_string(),
            context_artifactVersion: "1.122.2-68".to_string(),
            context_networkQuality: "UNKNOWN".to_string(),
            context_deviceLevel: "0".to_string(),
            context_userId: device_id.clone(),
        }
    }
}

// Define the Measurement struct for holding measurement data
#[derive(Serialize, Deserialize)]
pub struct Measurement {
    measurement_name: String,
    measurement_data: HashMap<String, String>,
    // Flatten context data into the measurement
    #[serde(flatten)]
    context_data: ContextData,
}

// Tauri command to set the user agent in the application state
#[tauri::command]
pub async fn set_user_agent(state: State<'_, AppState>, user_agent: String) -> Result<(), String> {
    // Lock the user agent for access and set it
    let mut lock = state.user_agent.lock().await;
    *lock = Some(user_agent);
    Ok(())
}

// Tauri command to report a custom measurement
#[tauri::command]
pub async fn report_custom_measurement(
    measurement_name: String,
    custom_fields: HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Retrieve the user agent from the current state
    let user_agent = {
        let lock = state.user_agent.lock().await;
        lock.clone().unwrap_or_else(|| "unknown".to_string())
    };

    // Record the start and current times
    let start_time = state.start_time;
    let current_time = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_millis();

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

    let measurement = Measurement {
        measurement_name,
        measurement_data: adjusted_custom_fields,
        context_data,
    };

    let measurement_json = serde_json::to_string(&measurement).map_err(|e| e.to_string())?;
    // println!("measurement_json : {}", measurement_json);
    // Create ApiRequest instance
    let request = apikit::ApiRequest::new("https://apm-fe.xiaohongshu.com/api/data")
    .set_header("Content-Type", "application/json")
    .set_header("Biz-Type", "apm_fe")
    .set_body(&measurement_json);

    // Send POST request
    match apikit::send_request_command(request).await {
        Ok(response_body) => {
            // The entire response is already a String
            println!("APM Response Body: {}", response_body);
        }
        Err(error) => {
            println!("APM Error sending request: {}", error);
        }
    }

    Ok(())
}

// Function to determine if Windows version is 7 or newer
// This function is unsafe because it uses platform-specific code
#[tauri::command]
pub unsafe fn is_windows_7_or_newer() -> bool {
    // 定义一个结构体来存储 Windows 的操作系统版本信息
    #[repr(C)]
    struct OSVERSIONINFOEXW {
        dwOSVersionInfoSize: u32,  // 结构体的大小
        dwMajorVersion: u32,       // 主版本号
        dwMinorVersion: u32,       // 次版本号
        dwBuildNumber: u32,        // 内部版本号（编译号）
        dwPlatformId: u32,         // 平台ID
        szCSDVersion: [u16; 128],  // 服务包版本号（以宽字符数组形式存储）
        wServicePackMajor: u16,    // 服务包主版本号
        wServicePackMinor: u16,    // 服务包次版本号
        wSuiteMask: u16,           // 产品套件掩码
        wProductType: u8,          // 系统类型
        wReserved: u8,             // 保留字段
    }

    // 引入外部 Windows API 函数，用于获取操作系统版本信息
    extern "system" {
        fn GetVersionExW(lpVersionInformation: *mut OSVERSIONINFOEXW) -> i32;
    }

    // 初始化一个 OSVERSIONINFOEXW 结构体用于存储获取到的版本信息
    let mut os_info: OSVERSIONINFOEXW = std::mem::zeroed();
    // 设置结构体大小
    os_info.dwOSVersionInfoSize = std::mem::size_of::<OSVERSIONINFOEXW>() as u32;

    // 调用 GetVersionExW 函数获取操作系统版本信息并存储到 os_info 中
    if GetVersionExW(&mut os_info as *mut _) != 0 {
        // 如果成功获取到版本信息，打印检测到的 Windows 版本号
        println!(
            "Detected Windows version: {}.{}",
            os_info.dwMajorVersion, os_info.dwMinorVersion
        );
        // 检查主版本号和次版本号以判断是否为 Windows 7 或更高版本
        // Windows 7 的主版本号为 6，次版本号为 1
        os_info.dwMajorVersion > 6 || (os_info.dwMajorVersion == 6 && os_info.dwMinorVersion > 1)
    } else {
        // 如果获取版本信息失败，打印错误信息
        println!("Failed to get Windows version information.");
        false // 返回 false 表示无法判断版本
    }
}