use crate::error::{CustomError, ErrorCode, System, Component};
use serde_json::Value;
use sys_info::os_type;
use log::{info, error, warn};
use std::cmp::max;
use printpdf::Rgb;
use std::path::PathBuf;
use lazy_static::lazy_static;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs::File;
use std::fs;
use std::io::{self, Write};
use tauri::api::http::{ClientBuilder, HttpRequestBuilder};
use tauri::{AppHandle, Manager, Window, SystemTray, SystemTrayMenu, SystemTrayMenuItem, SystemTrayEvent, CustomMenuItem};
use tauri::api::dialog::MessageDialogBuilder;
use webbrowser;
use crate::windows_version;
use crate::wss;
use std::mem;
use std::process::Command;
use os_info::{Type};
use std::process::Command as StdCommand;
use chrono::{DateTime, Utc, Datelike, Timelike};
use crate::globalcache::{get_app_version};
#[cfg(windows)]
use winapi::um::sysinfoapi::GetVersionExW;
#[cfg(windows)]
use winapi::um::winnt::OSVERSIONINFOW;
#[cfg(target_os = "windows")]
use windows_version::check_windows_version;
use zip::ZipArchive;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use dashmap::DashMap;
use once_cell::sync::{Lazy, OnceCell};
use num_cpus;


lazy_static! {
    static ref URL_FILENAME_CACHE: DashMap<String, String> = DashMap::new(); // TODO: 贝林
    // static ref CAN_USE_STD_COMMAND: Mutex<Option<bool>> = Mutex::new(None);
    // static ref IS_CHECKING_COMMAND: Mutex<bool> = Mutex::new(false);
}

static CAN_USE_STD_COMMAND: Lazy<OnceCell<bool>> = Lazy::new(OnceCell::new);
static IS_CHECKING_COMMAND: Lazy<OnceCell<bool>> = Lazy::new(OnceCell::new);
static OPTIMAL_WORKERS: OnceCell<usize> = OnceCell::new();



/// 提取并返回完整的图片文件名（基于特定的URL模式）
///
/// 根据输入的URL，解析并提取图片的文件名。
/// 支持的URL模式：
/// - 包含 "ditto/"：提取 "ditto/" 之后的部分作为文件名。
/// - 包含 "cloud_print_image/"：提取 "cloud_print_image/" 之后的部分，
///   并移除可能存在的 "print_image/" 前缀。
/// - 其他情况：取URL最后一个斜杠后的部分，并移除查询参数。
///
/// # 参数
/// - `url`: 图片的URL字符串。
///
/// # 返回
/// 返回提取的图片文件名，如果无法解析则返回 `None`。
pub fn extract_image_filename(url: &str) -> Option<String> {
    // 添加 URL 有效性检查
    if url.trim().is_empty() {
        error!("【extract_image_filename】URL 为空");
        return None;
    }

    // 使用无锁缓存处理图片  // TODO: 贝林
    if let Some(cached_name) = URL_FILENAME_CACHE.get(url) {
        info!("【extract_image_filename】使用缓存的文件名: {:?}", cached_name.value());  // 使用 .value() 获取引用的值
        return Some(cached_name.value().clone());  // 使用 .value() 获取引用的值
    }

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let url_hash = hasher.finish();

    let base_name = format!("{:016x}", url_hash);

    info!("【extract_image_filename】生成新的文件名: {}", base_name);
    Some(base_name)
}

pub fn set_image_filename_cache(url: &str, base_name: &str) {
    URL_FILENAME_CACHE.insert(url.to_string(), base_name.to_string());
}

/// 获取用于存储图片的目标文件夹路径
///
/// 目标文件夹位于系统的临时目录下的 "imgs" 目录。
///
/// # 返回
/// 返回目标文件夹的 `PathBuf`。
fn get_target_directory() -> PathBuf {
    // 获取系统临时目录
    let mut target_dir = env::temp_dir();
    target_dir.push("imgs");

    // 如果目标文件夹不存在，则创建
    if !target_dir.exists() {
        match fs::create_dir_all(&target_dir) {
            Ok(_) => info!("【get_target_directory】成功创建目标文件夹: {:?}", target_dir),
            Err(e) => {
                error!("【get_target_directory】无法创建目标文件夹: {:?}, 错误: {}", target_dir, e);
                panic!("无法创建目标文件夹: {:?}", target_dir);
            }
        }
    } else {
        info!("【get_target_directory】目标文件夹已存在: {:?}", target_dir);
    }

    target_dir
}

/// 获取图片文件的完整路径（不执行下载操作）
///
/// 根据给定的URL，提取图片文件名，并构建在目标文件夹中的完整路径。
///
/// # 参数
/// - `url`: 图片的URL字符串。
///
/// # 返回
/// 返回图片文件的完整路径，如果无法解析则返回 `None`。
pub fn get_image_file_path(url: &str) -> Option<PathBuf> {
    if let Some(base_name) = extract_image_filename(url) {
        let mut img_dir = get_target_directory();

        // 尝试查找所有可能的扩展名
        let extensions = ["png", "jpg", "jpeg", "webp", "gif"];
        for ext in extensions.iter() {
            let file_name = format!("{}.{}", base_name, ext);
            img_dir.push(&file_name);

            if img_dir.exists() {
                info!("【get_image_file_path】找到匹配的图片文件: {:?}", img_dir);
                return Some(img_dir);
            }
            img_dir.pop();
        }

        // 如果文件不存在，返回基础路径
        img_dir.push(&base_name);
        info!("【get_image_file_path】返回基础文件路径: {:?}", img_dir);
        Some(img_dir)
    } else {
        error!("【get_image_file_path】无法生成文件名: {}", url);
        None
    }
}

/// 规范化路径，移除指定的路径片段
///
/// 将给定的 `PathBuf` 转换为字符串，移除指定的路径片段后，
/// 再转换回 `PathBuf`。
///
/// # 参数
/// - `path`: 原始路径。
/// - `segment_to_remove`: 需要移除的路径片段。
///
/// # 返回
/// 返回规范化后的路径。
pub fn normalize_path(path: PathBuf, segment_to_remove: &str) -> PathBuf {
    // 将路径转换为字符串并移除指定的片段
    let path_str = path.to_string_lossy().replace(segment_to_remove, "");
    let normalized_path = PathBuf::from(path_str);

    info!("【normalize_path】原始路径: {:?}, 移除的片段: {}, 规范化后的路径: {:?}", path, segment_to_remove, normalized_path);

    normalized_path
}

#[tauri::command]
/// 获取配置中的版本号
///
/// # 返回
/// 返回应用程序的版本号字符串。
pub fn get_version_from_config() -> String {
    info!("【get_version_from_config】进入函数");

    let version = get_app_version();

    info!("【get_version_from_config】获取到的版本号: {}", version);

    version
}

/// 获取当前的运行环境
///
/// 根据环境变量 `RUST_ENV`，判断当前是 "production" 还是 "development"。
///
/// # 返回
/// 返回环境字符串："production" 或 "development"。
pub fn get_environment() -> String {
    info!("【get_environment】进入函数");

    let env_value = env::var("RUST_ENV").unwrap_or_else(|_| "development".to_string());
    let environment = if env_value.to_lowercase() == "production" {
        "production".to_string()
    } else {
        "development".to_string()
    };

    info!("【get_environment】当前环境: {}", environment);

    environment
}

#[tauri::command(rename_all = "snake_case")]
/// 获取系统信息（操作系统类型和版本）
///
/// # 返回
/// 返回操作系统的信息字符串。
pub fn get_system_info() -> String {
    info!("【get_system_info】进入函数");

    // 获取操作系统类型
    let os_type = os_type().unwrap_or_else(|e| {
        let error_message = "Unknown".to_string();
        error!("【get_system_info】无法获取操作系统类型: {}，返回: {}", e, error_message);
        error_message
    });

    info!("【get_system_info】操作系统类型: {}", os_type);

    // 判断是否为 macOS 系统
    #[cfg(target_os = "macos")]
    {
        if os_type == "Darwin" {
            info!("【get_system_info】检测到 macOS 系统");
            return "Mac OS X".to_string();
        }
}

    // 判断是否为 Windows 系统，获取 Windows 版本
    #[cfg(target_os = "windows")]
    {
        if os_type == "Windows" {
            let version = unsafe { windows_version::get_windows_version_string() };
            info!("【get_system_info】检测到 Windows 系统，版本: {}", version);
            return version;
        }
    }

    // 其他系统类型
    let unknown_os_message = format!("Unknown operating system: {}", os_type);
    info!("【get_system_info】返回未知的操作系统信息: {}", unknown_os_message);
    unknown_os_message
}


#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
}

pub fn get_os_name() -> String {
    info!("【桌面端】调用 get_os_name() 开始");
    let info = os_info::get();
    let os_name = match info.os_type() {
        Type::Windows => {
            let version = info.version().to_string();
            if version.contains("10.0") {
                "Windows10".to_string()
            } else if version.contains("11.0") {
                "Windows11".to_string()
            } else if version.contains("6.3") {
                "Windows8.1".to_string()
            } else if version.contains("6.2") {
                "Windows8".to_string()
            } else if version.contains("6.1") {
                "Windows7".to_string()
            } else {
                format!("未知的 Windows 版本: {}", version)
            }
        }
        Type::Macos => "macOS".to_string(),
        Type::Linux => "Linux".to_string(),
        _ => "未知的操作系统".to_string(),
    };
    info!("【桌面端】get_os_name() 返回结果: {}", os_name);
    os_name
}

/// 将十六进制颜色字符串转换为 `Rgb` 结构
///
/// 支持三位、六位或八位的十六进制颜色字符串，自动去除开头的 '#' 符号。
/// 如果包含透明度，将与白色背景进行混合。
///
/// # 参数
/// - `hex`: 十六进制颜色字符串。
/// - `alpha`: 可选的透明度值（0.0 - 1.0），如果提供，将覆盖十六进制中的透明度。
///
/// # 返回
/// 返回转换后的 `Rgb` 结构，如果转换失败则返回 `Err`。
pub fn hex_to_rgb(hex: String, alpha: Option<f32>) -> Result<Rgb, String> {
    // 验证 alpha 值范围
    if let Some(a) = alpha {
        if !(0.0..=1.0).contains(&a) {
            error!("【hex_to_rgb】无效的 alpha 值: {}，应该在 0.0 到 1.0 之间", a);
            return Err("Alpha value must be between 0.0 and 1.0".to_string());
        }
    }

    // 去掉开头的 '#' 符号
    let hex = hex.trim_start_matches('#');

    // 检查长度是否为3、6或8
    if hex.len() != 3 && hex.len() != 6 && hex.len() != 8 {
        error!("【hex_to_rgb】无效的十六进制颜色长度: {}", hex.len());
        return Err("Invalid hex color length".to_string());
    }

    // 如果是3位，扩展为6位
    let hex = if hex.len() == 3 {
        let expanded_hex: String = hex.chars().map(|c| format!("{0}{0}", c)).collect();
        expanded_hex
    } else {
        hex.to_string()
    };

    // 解析RGB值
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| {
        error!("【hex_to_rgb】解析红色值失败: {}", e);
        "Invalid hex value".to_string()
    })?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| {
        error!("【hex_to_rgb】解析绿色值失败: {}", e);
        "Invalid hex value".to_string()
    })?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| {
        error!("【hex_to_rgb】解析蓝色值失败: {}", e);
        "Invalid hex value".to_string()
    })?;

    // 解析或使用提供的透明度值
    let alpha_value = if hex.len() == 8 {
        let a = u8::from_str_radix(&hex[6..8], 16).map_err(|e| {
            error!("【hex_to_rgb】解析透明度值失败: {}", e);
            "Invalid hex value".to_string()
        })? as f32 / 255.0;
        alpha.unwrap_or(a)
    } else {
        alpha.unwrap_or(1.0)
    };

    // 如果完全不透明，直接返回RGB值
    if alpha_value >= 1.0 {
        return Ok(Rgb::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            None,
        ));
    }

    // 与白色背景混合
    let final_r = (r as f32 / 255.0) * alpha_value + 1.0 * (1.0 - alpha_value);
    let final_g = (g as f32 / 255.0) * alpha_value + 1.0 * (1.0 - alpha_value);
    let final_b = (b as f32 / 255.0) * alpha_value + 1.0 * (1.0 - alpha_value);

    info!("【hex_to_rgb】最终RGB值: R={}, G={}, B={} (alpha={})",
          final_r, final_g, final_b, alpha_value);

    Ok(Rgb::new(final_r, final_g, final_b, None))
}

/// 解析打印设置，获取偏移量
///
/// 从给定的 JSON 字符串中解析 "offsetx" 和 "offsety" 值。
///
/// # 参数
/// - `page_size`: 包含打印设置的 JSON 字符串。
///
/// # 返回
/// 返回偏移量的元组 `(offset_x, offset_y)`。
pub fn parse_print_setting(page_size: String) -> (f64, f64) {
    // 解析 JSON 字符串
    let page_size_json: Value = match serde_json::from_str(&page_size) {
        Ok(setting) => setting,
        Err(e) => {
            error!("【parse_print_setting】解析打印设置失败: {}", e);
            return (0.0, 0.0);
        }
    };

    // 提取偏移量
    let offset_x = page_size_json.get("offsetx").and_then(|offsetx| offsetx.as_f64()).unwrap_or(0.0);
    let offset_y = page_size_json.get("offsety").and_then(|offsety| offsety.as_f64()).unwrap_or(0.0);

    info!("【parse_print_setting】解析得到的偏移量: offset_x={}, offset_y={}", offset_x, offset_y);

    (offset_x, offset_y)
}

#[cfg(target_os = "windows")]
pub fn create_harfbuzz_env_zip_file(path: String, bin: &[u8], file_name: String) -> io::Result<()> {
    fs::create_dir_all(&path)?;
    let mut file_path = PathBuf::from(&path);
    file_path.push(file_name);
    let mut f = File::create(file_path)?;
    f.write_all(bin)?;
    f.sync_all()?;
    info!("【桌面端主入口调用】create_font_file: File created successfully at path: {}", path);
    Ok(())
}

// harfbuzz-win64-10.1.0
#[cfg(target_os = "windows")]
pub fn init_harfbuzz_env() -> Result<bool, CustomError> {
    info!("【桌面端主入口调用】init_harfbuzz_env 进入");

    let dir = env::temp_dir();
    let harfbuzz_env_path = dir.join("xhs-printer").join("harfbuzz");
    info!("【桌面端主入口调用】init_harfbuzz_env {}", harfbuzz_env_path.display());

    // 如果目录存在，先清空它  不清空
    // if harfbuzz_env_path.exists() {
    //     info!("【桌面端主入口调用】init_harfbuzz_env 目录存在调用清空");
    //     fs::remove_dir_all(&harfbuzz_env_path).map_err(|e| CustomError {
    //         key: "InitharfbuzzEnvError",
    //         message: format!("【init_harfbuzz_env】清空目录失败: {}", e),
    //         source: None,
    //         error_code: None,
    //     })?;
    //     info!("【桌面端主入口调用】init_harfbuzz_env 目录存在调用清空完成");
    // }

    let mut harfbuzz_env_zip_file: Option<&[u8]> = None;
    let mut file_name = "";

    unsafe {
        // 检查 Windows 版本
        if windows_version::check_windows_version() {
            info!("【桌面端主入口调用】init_harfbuzz_env win64");
            // Win7以上系统
            harfbuzz_env_zip_file = Some(include_bytes!("../harfbuzz/win64.zip"));
            file_name = "win64.zip"

        } else {
            info!("【桌面端主入口调用】init_harfbuzz_env win32");
            // Win7及以下系统
            harfbuzz_env_zip_file = Some(include_bytes!("../harfbuzz/win32.zip"));
            file_name = "win32.zip"
        }
    }
    if create_harfbuzz_env_zip_file(harfbuzz_env_path.display().to_string(), harfbuzz_env_zip_file.unwrap(), file_name.to_string()).is_err() {
        return Err(CustomError {
            key: "InitharfbuzzEnvError",
            message: format!("【桌面端主入口调用】init_harfbuzz_env: 创建harfbuzz压缩包失败"),
            source: None,
            error_code: None,
        });
    }
    // 使用zip解压缩harfbuzz.zip
    let zip_path = harfbuzz_env_path.join(file_name);
    match extract_zip(&zip_path, &harfbuzz_env_path) {
        Ok(_) => {
            // 解压成功后删除zip文件
            // if let Err(e) = fs::remove_file(&zip_path) {
            //     error!("【init_harfbuzz_env】删除zip文件失败: {}", e);
            // }
            info!("【桌面端主入口调用】init_harfbuzz_env 解压成功");
            Ok(true)
        },
        Err(e) => Err(CustomError {
            key: "InitharfbuzzEnvError",
            message: format!("【桌面端主入口调用】init_harfbuzz_env: 解压harfbuzz环境失败: {}", e),
            source: None,
            error_code: None,
        })
    }
}

/// 解压zip文件到指定目录
fn extract_zip<P: AsRef<Path>>(zip_path: P, output_dir: P) -> io::Result<()> {
    let file = File::open(&zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => output_dir.as_ref().join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        // 获取文件权限并设置
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

// 生成随机字符串
pub fn generate_random_string(length: usize) -> String {
    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789"
        .chars()
        .collect();

    let mut random_string = String::new();

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    let mut rng = SimpleRng::new(seed);

    for _ in 0..length {
        let index = rng.next() as usize % chars.len();
        random_string.push(chars[index]);
    }

    random_string
}

/// 获取当前时间的格式化字符串为 YYYYMMDD_HHMMSS.sss 格式（sss为毫秒）
pub fn get_formatted_time(datetime: DateTime<Utc>) -> String {
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}{:03}",
        datetime.year(),
        datetime.month(),
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second(),
        datetime.timestamp_subsec_millis() // 获取毫秒
    )
}

#[tauri::command(rename_all = "snake_case")]
pub fn is_window_visible(window: Window) -> bool {
    // `is_visible` 返回当前窗口的可见状态
    window.is_visible().unwrap_or(false)
}
/// A simple pseudo-random number generator.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        SimpleRng { state: seed }
    }

    fn next(&mut self) -> u64 {
        // Use a simple linear congruential generator
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }
}

// 用于展示错误对话框
fn show_error_dialog(file_name: &str) {
    MessageDialogBuilder::new("Error", &format!("Please close {} before proceeding.", file_name))
        .show(|_| {});
}

// 通用创建文件方法
pub fn create_file(path: String, file_name: &str, bin: &[u8]) -> io::Result<()> {
    let full_path = PathBuf::from(path).join(file_name);
    info!("【桌面端主入口调用】create_file: File created successfully at path: {}", full_path.display());

    let mut f = File::create(&full_path).map_err(|e| {
        show_error_dialog(file_name);
        e
    })?;
    f.write_all(bin)?;
    f.sync_all()?;
    info!("【桌面端主入口调用】create_file: File created successfully at path: {}", full_path.display());
    Ok(())
}

#[tauri::command]
pub fn get_build_version() -> String {
    let version_bytes = include_bytes!("../binaries/.xhs_printer_build_version");
    String::from_utf8_lossy(version_bytes).trim().to_string()
}

#[tauri::command]
pub fn install_ca_fun() -> String {

    // if cfg(macos) {
    //     return wss::install_ca().map_err(|e| e.to_string()).unwrap_or_else(|e| e)
    // }
    // "Unsupported OS".to_string()

    // 判断是否为 macOS 系统
    #[cfg(target_os = "macos")]
    {
        return  wss::install_ca().map_err(|e| e.to_string()).unwrap_or_else(|e| e)
    }
    "Unsupported OS".to_string()


}

#[tauri::command]
pub async fn fetch_image(url: String) -> Result<Vec<u8>, String> {
    info!("【桌面端主入口调用】fetch_image: Starting to fetch image from URL: {}", url);
    let client = ClientBuilder::new().build().map_err(|e| {
        error!(
            "【桌面端主入口调用】fetch_image: Failed to build HTTP client: {}",
            e
        );
        e.to_string()
    })?;
    let request = HttpRequestBuilder::new("GET", &url).map_err(|e| {
        error!("【桌面端主入口调用】fetch_image: Failed to build HTTP request: {}", e);
        e.to_string()
    })?;
    let response = client.send(request).await.map_err(|e| {
        error!("【桌面端主入口调用】fetch_image: HTTP request failed: {}", e);
        e.to_string()
    })?;
    let data = response.bytes().await.map_err(|e| {
        error!("【桌面端主入口调用】fetch_image: Failed to read response bytes: {}", e);
        e.to_string()
    })?.data;
    info!("【桌面端主入口调用】fetch_image: Successfully fetched image, data length: {}", data.len());
    Ok(data.to_vec())
}

pub fn get_start_time() -> Result<u128, CustomError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|e| CustomError {
            key: "TimeError",
            message: format!("【桌面端前置ERROR】获取应用启动时间失败: {}", e),
            source: Some(Box::new(e)),
            error_code: None,
        })
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    info!("【桌面端主入口调用】open_file: Attempting to open file or URL at path: {}", path);

    // Check if the path starts with "http://localhost/"
    if path.starts_with("http://localhost/") {
        // Use webbrowser crate to open the URL in the default browser
        webbrowser::open(&path).map_err(|e| {
            error!("【桌面端主入口调用】open_file: Failed to open URL in browser: {}", e);
            e.to_string()
        })?;
    } else {
        // Use the open crate to open the file
        open::that(&path).map_err(|e| {
            error!("【桌面端主入口调用】open_file: Failed to open file: {}", e);
            e.to_string()
        })?;
    }

    info!("【桌面端主入口调用】open_file: Successfully opened: {}", path);
    Ok(())
}

pub fn setup_system_tray() -> SystemTray {
    let show_item = CustomMenuItem::new("show".to_string(), "打开");
    let quit_item = CustomMenuItem::new("quit".to_string(), "退出");
    let tray_menu = SystemTrayMenu::new()
        .add_item(show_item)
        .add_item(quit_item);
    SystemTray::new().with_menu(tray_menu)
}

#[tauri::command]
pub fn get_os_type() -> String {
    #[cfg(target_os = "windows")]
    {
        "WIN".to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "MAC".to_string()
    }
}

#[cfg(target_os = "windows")]
pub fn can_use_std_command() -> bool {
    info!("【桌面端】刚进入can_use_std_command");

    // 如果已有缓存结果，直接返回
    if let Some(result) = CAN_USE_STD_COMMAND.get() {
        info!("【桌面端】can_use_std_command 使用缓存结果: {}", result);
        return *result;
    }

    // 检查是否正在进行首次检查
    if let Some(is_checking) = IS_CHECKING_COMMAND.get() {
        if *is_checking {
            info!("【桌面端】can_use_std_command 首次检查进行中，返回 true");
            return true;
        }
    }

    // 设置首次检查标志
    if IS_CHECKING_COMMAND.set(true).is_err() {
        info!("【桌面端】can_use_std_command 首次检查进行中，返回 true");
        return true;
    }
    info!("【桌面端】can_use_std_command 开始首次检查");

    // 执行检查
    let result = match std::process::Command::new("cmd")
        .arg("/C")
        .arg("echo test")
        .creation_flags(0x08000000 | 0x04000000)
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };

    // 缓存检查结果
    if CAN_USE_STD_COMMAND.set(result).is_err() {
        info!("【桌面端】can_use_std_command 缓存结果设置失败");
    }

    // 清除首次检查标志
    if IS_CHECKING_COMMAND.set(false).is_err() {
        info!("【桌面端】can_use_std_command 清除首次检查标志失败");
    }

    info!("【桌面端】can_use_std_command 检查结果: {}", result);

    result
}

/// 计算最优工作线程数
///
/// 基于系统 CPU 核心数计算合适的工作线程数：
/// 1. 获取系统 CPU 信息（逻辑核心数和物理核心数）
/// 2. 使用物理核心数作为基准
/// 3. 预留一个核心给系统和其他任务
/// 4. 确保线程数在合理范围内
pub fn calculate_optimal_workers() -> usize {
    // 如果已有缓存结果，直接返回
    if let Some(&workers) = OPTIMAL_WORKERS.get() {
        info!("【工作线程】使用缓存的工作线程数: {}", workers);
        return workers;
    }

    // 获取 CPU 信息
    let cpu_count = num_cpus::get();
    let physical_cpu_count = num_cpus::get_physical();

    info!(
        "【系统信息】CPU逻辑核心数: {}, 物理核心数: {}",
        cpu_count,
        physical_cpu_count
    );

    // 使用物理核心数作为基准，预留一个核心给系统
    let suggested_workers = if physical_cpu_count > 1 {
        physical_cpu_count - 1
    } else {
        1
    };

    // 确保至少有一个工作线程
    let optimal_workers = max(suggested_workers, 1);


    // 缓存计算结果
    if OPTIMAL_WORKERS.set(optimal_workers).is_err() {
        warn!("【工作线程】工作线程数已被其他线程设置");
    }

    info!("【工作线程】首次计算得到的工作线程数: {}", optimal_workers);
    optimal_workers
}

/// 辅助函数：将宽字符串转换为 Rust 的 String
pub fn wide_string_to_string(wide: *const u16) -> String {
    if wide.is_null() {
        return String::new();
    }
    unsafe {
        let len = (0..).take_while(|&i| *wide.add(i) != 0).count();
        let slice = std::slice::from_raw_parts(wide, len);
        String::from_utf16_lossy(slice)
    }
}
