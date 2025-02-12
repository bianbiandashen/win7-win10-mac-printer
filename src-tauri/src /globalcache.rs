use std::process::Command;
use once_cell::sync::Lazy;
use std::sync::Mutex;
// use tokio::sync::Mutex;
use async_lock::Mutex as asyncLockMutex;
use crate::utils::{get_system_info, get_os_name};
use log::{info, error};

// 全局缓存静态变量
pub static DEVICE_ID_CACHE: Lazy<Option<String>> = Lazy::new(|| initialize_device_id());
pub static SYSTEM_INFO_CACHE: Lazy<String> = Lazy::new(|| get_system_info());
pub static OS_NAME_CACHE: Lazy<String> = Lazy::new(|| get_os_name());
pub static USER_AGENT_CACHE: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new("unknown".to_string()));
pub static APP_VERSION: Lazy<asyncLockMutex<String>> = Lazy::new(|| asyncLockMutex::new(String::new()));

fn initialize_device_id() -> Option<String> {
  info!("【桌面端-globalcache】initialize_device_id");

  #[cfg(not(windows))]
  {
      let output = Command::new("ioreg")
          .arg("-rd1")
          .arg("-c")
          .arg("IOPlatformExpertDevice")
          .output()
          .ok();

      if let Some(output) = output {
          if output.status.success() {
              if let Some(uuid) = String::from_utf8_lossy(&output.stdout)
                  .lines()
                  .find(|line| line.contains("\"IOPlatformUUID\""))
                  .and_then(|line| line.split('"').nth(3))
              {
                  return Some(uuid.to_string());
              }
          }
      }
      None
  }

  #[cfg(target_os = "windows")]
  {
      use std::ffi::{CString};
      use std::fs::{File, create_dir_all};
      use std::io::{Read};
      use std::ptr;
      use winapi::um::handleapi::CloseHandle;
      use winapi::um::processthreadsapi::{CreateProcessA, PROCESS_INFORMATION, STARTUPINFOA};
      use winapi::um::winbase::{CREATE_NO_WINDOW, WAIT_OBJECT_0};
      use winapi::um::synchapi::WaitForSingleObject;

      unsafe {
          let temp_dir = "C:\\temp\\";
          create_dir_all(temp_dir).expect("Failed to create temp directory");

          let mut si: STARTUPINFOA = std::mem::zeroed();
          si.cb = std::mem::size_of::<STARTUPINFOA>() as u32;
          si.dwFlags = winapi::um::winbase::STARTF_USESHOWWINDOW;
          si.wShowWindow = 0;

          let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
          let temp_file_path = format!("{}uuid_output.txt", temp_dir);
          let cmd = CString::new(format!("cmd /C wmic csproduct get UUID > {}", temp_file_path)).unwrap();
          let cmd_ptr = cmd.into_raw();

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
              error!("【桌面端-APMRS】Failed to create process for wmic command.");
              return None;
          }

          WaitForSingleObject(pi.hProcess, WAIT_OBJECT_0);
          CloseHandle(pi.hProcess);
          CloseHandle(pi.hThread);
          CString::from_raw(cmd_ptr);

          let mut output = Vec::new();
          let mut file = File::open(&temp_file_path).expect("Failed to open output file");
          file.read_to_end(&mut output).expect("Failed to read output file");
          let output_str = String::from_utf8_lossy(&output);

          if let Some(uuid) = output_str
              .lines()
              .nth(1)
              .map(|line| line.trim().to_string())
              .filter(|line| !line.is_empty())
          {
              info!("【桌面端-APMRS】Successfully retrieved UUID: {}", uuid);
              return Some(uuid);
          }
          None
      }
  }
}

#[tauri::command]
pub async fn set_user_agent(user_agent: String) -> Result<(), String> {
    info!("【桌面端-globalcache】set_user_agent");
    let mut cache = USER_AGENT_CACHE.lock().unwrap();
    *cache = user_agent;
    Ok(())
}


// 获取应用程序版本号
pub fn get_app_version() -> String {
  info!("【桌面端-globalcache】get_app_version");
  let mut app_version = APP_VERSION.try_lock().unwrap();

  // 如果缓存中有版本号，直接返回
  if !app_version.is_empty() {
      info!("【get_app_version】使用缓存的版本号: {}", app_version);
      return app_version.clone();
  }

  // 缓存为空时，从环境变量读取
  info!("【get_app_version】APP_VERSION 为空，开始读取配置文件");
  let version = env!("CARGO_PKG_VERSION");

  if version.is_empty() {
      error!("【get_app_version】无法从环境变量读取版本号，返回默认版本号 0.1.2");
      return "0.1.2".to_string();
  }

  // 更新缓存
  *app_version = version.to_string();

  info!("【get_app_version】读取到的版本号: {}", version);

  version.to_string()
}
