use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use tauri::api::path::resource_dir;
use log::{info, error};
use std::time::Instant;
use crate::scripts::INSTALL_CERT_SCRIPT;

/// 自定义错误类型
#[derive(Debug)]
pub struct CustomError {
    pub key: &'static str,
    pub message: String,
    pub source: Option<std::io::Error>,
    pub error_code: Option<i32>,
}

impl From<std::io::Error> for CustomError {
    fn from(error: std::io::Error) -> Self {
        CustomError {
            key: "IOError",
            message: error.to_string(),
            source: Some(error),
            error_code: None,
        }
    }
}


/// 安装 CA 证书的函数
#[tauri::command]
pub fn install_ca() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {

        return Err("此功能仅支持 macOS 系统".into())
        // 检查是否以管理员身份运行
        // if !check_admin() {
        //     return Err("需要管理员权限来安装 CA 证书。请以管理员身份运行应用。".into());
        // }

        // // 开始计时
        // let step_start = Instant::now();

        // // 获取系统的临时目录
        // let dir: PathBuf = env::temp_dir();
        // let duration_dir = step_start.elapsed();
        // info!("【桌面端-install_ca 耗时分析】-获取临时目录耗时: {:?}", duration_dir);

        // // 嵌入 installCa.bat 和 ca.crt
        // let install_ca_bat = include_bytes!("../binaries/bin/installCa.bat");
        // let ca_crt = include_bytes!("../binaries/bin/ca.crt");

        // // 创建 install_ca.bat 文件路径
        // let bat_path = dir.join("installCa.bat");
        // // 创建 ca.crt 文件路径
        // let crt_path = dir.join("ca.crt");

        // // 写入 installCa.bat
        // if let Err(e) = create_install_ca_file(&bat_path, install_ca_bat) {
        //     error!("【桌面端主入口调用】install_ca: Failed to create installCa.bat file in Windows environment: {}", e);
        //     return Err("安装失败: 无法创建 installCa.bat 文件。".into());
        // }

        // // 写入 ca.crt
        // if let Err(e) = create_ca_crt_file(&crt_path, ca_crt) {
        //     error!("【桌面端主入口调用】install_ca: Failed to create ca.crt file in Windows environment: {}", e);
        //     return Err("安装失败: 无法创建 ca.crt 文件。".into());
        // }

        // // 结束计时
        // let duration_write = step_start.elapsed();
        // info!("【桌面端-install_ca 耗时分析】-写入文件耗时: {:?}", duration_write);

        // // 构造执行命令
        // let shell_command = format!("cmd /c {}", bat_path.to_str().unwrap());

        // // 执行 installCa.bat
        // let status = Command::new("cmd")
        //     .args(&["/c", bat_path.to_str().unwrap()])
        //     .status()
        //     .map_err(|e| format!("执行命令时出错: {}", e))?;

        // if status.success() {
        //     Ok("CA 证书成功安装。".into())
        // } else {
        //     Err("执行 installCa.bat 时出错，退出状态: {}".into())
        // }
    }

    #[cfg(target_os = "macos")]
    {
        info!("开始在 macOS 上安装证书");
        
        // 检查证书是否已经安装
        let check_cert = std::process::Command::new("security")
            .args(["find-certificate", "-c", "XHsPrint", "-a"])
            .output()
            .map_err(|e| {
                error!("检查证书失败: {}", e);
                format!("检查证书失败: {}", e)
            })?;
        
        // 证书已存在
        if check_cert.status.success() && !check_cert.stdout.is_empty() {
            info!("已检测到 XHsPrint 证书");
            
            // 显示提示框
            let alert_script = r#"display dialog "证书已经安装过了，无需重复安装。" buttons {"确定"} default button "确定" with icon note with title "证书安装提示""#;
            
            std::process::Command::new("osascript")
                .args(["-e", alert_script])
                .output()
                .map_err(|e| {
                    error!("显示提示框失败: {}", e);
                    format!("显示提示框失败: {}", e)
                })?;
            
            return Ok("证书已经安装过了".into());
        }
        
        let cert_data = include_bytes!("../binaries/cert/server.crt");
        // 获取临时目录
        let temp_dir = env::temp_dir();
        let cert_path = temp_dir.join("localhost.crt");
        let script_path = temp_dir.join("install_cert.scpt");
        
        // 创建证书文件
        if !cert_path.exists() {
            info!("证书文件不存在，开始创建");
            fs::write(&cert_path, cert_data)
                .map_err(|e| {
                    error!("写入证书文件失败: {}", e);
                    format!("写入证书文件失败: {}", e)
                })?;
            info!("证书文件已创建: {}", cert_path.display());
        } else {
            info!("使用已存在的证书文件: {}", cert_path.display());
        }
        
        // 检查并创建脚本文件
        if !script_path.exists() {
            info!("脚本文件不存在，开始创建");
            let script_content = INSTALL_CERT_SCRIPT.replace(
                "./localhost.cer",
                cert_path.to_str().unwrap()
            );
            
            fs::write(&script_path, script_content)
                .map_err(|e| {
                    error!("写入脚本文件失败: {}", e);
                    format!("写入脚本文件失败: {}", e)
                })?;
            info!("脚本文件已创建: {}", script_path.display());
        } else {
            info!("使用已存在的脚本文件: {}", script_path.display());
        }

        // 执行脚本
        let output = std::process::Command::new("osascript")
            .arg(&script_path)
            .output()
            .map_err(|e| {
                error!("执行证书安装脚本失败: {}", e);
                format!("执行证书安装脚本失败: {}", e)
            })?;

        // 检查执行结果并返回
        let result = if output.status.success() {
            info!("证书安装成功");
            Ok("证书安装成功".into())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            error!("证书安装失败: {}", error);
            Err(format!("证书安装失败: {}", error))
        };
        
        result
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("此功能仅支持 macOS 系统".into())
    }
}

// /// 创建 installCa.bat 文件
// #[cfg(target_os = "windows")]
// fn create_install_ca_file(path: &PathBuf, content: &[u8]) -> std::io::Result<()> {
//     let mut file = File::create(path)?;
//     file.write_all(content)?;
//     Ok(())
// }

// /// 创建 ca.crt 文件
// #[cfg(target_os = "windows")]
// fn create_ca_crt_file(path: &PathBuf, content: &[u8]) -> std::io::Result<()> {
//     let mut file = File::create(path)?;
//     file.write_all(content)?;
//     Ok(())
// }

// /// 检查当前进程是否以管理员身份运行
// #[cfg(target_os = "windows")]
// fn check_admin() -> bool {
//     use windows::Win32::Security::{CheckTokenMembership, SID};
//     use windows::Win32::Foundation::BOOL;
//     use windows::Win32::Security::SECURITY_NT_AUTHORITY;
//     use windows::Win32::Security::WinBuiltinAdministratorsSid;
//     use std::ptr::null_mut;

//     unsafe {
//         let mut is_admin = false;
//         let mut is_admin_bool: BOOL = BOOL(0);

//         // 创建管理员组 SID
//         let mut admin_sid: *mut SID = null_mut();
//         let result = SID::CreateWellKnownSid(WinBuiltinAdministratorsSid, None, &mut admin_sid);

//         if result.is_err() {
//             return false;
//         }

//         // 检查 token 是否属于管理员组
//         let success = CheckTokenMembership(None, admin_sid, &mut is_admin_bool);

//         if success.as_bool() && is_admin_bool.as_bool() {
//             is_admin = true;
//         }

//         // 释放 SID 内存
//         windows::Win32::System::Memory::LocalFree(admin_sid as _);
//         is_admin
//     }
// }

// #[cfg(not(target_os = "windows"))]
// fn check_admin() -> bool {
//     // 非 Windows 平台暂不实现
//     true
// }
