use std::process::Command;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;
use log::{info, error};
use std::ffi::{OsStr, OsString};

#[cfg(target_os = "windows")]
use crate::utils::can_use_std_command;

#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use std::process::Command as StdCommand;
use tauri::api::process::Command as TauriCommand;

#[derive(Debug)]
pub enum FontError {
    CommandError(String),
}

pub type Result<T> = std::result::Result<T, FontError>;

pub struct FontProcessor;

impl FontProcessor {
    pub fn new() -> Result<Self> {
        Ok(FontProcessor)
    }

    /// 将给定文本转成 Unicode 列表（以逗号分隔，16进制大写）。
    /// 例如 "你好" -> "4F60,597D"
    fn text_to_unicodes_str(input: &str) -> String {
    use std::fmt::Write;
    
    // 预分配字符串缓冲区以减少分配次数，估算为 input 长度的 5 倍
    let mut result = String::with_capacity(input.len() * 5);

    for c in input.chars() {
        let code = c as u32;
        if !result.is_empty() {
            result.push(',');
        }
        // 使用大写16进制并填充0至4位
        write!(&mut result, "{:04X}", code).expect("Failed to write to string");
    }
    
    result
}

    /// 
    /// # subset_font
    ///
    /// - `font_path`: 原始字体文件路径
    /// - `text`: 需要进行子集化的文本（会自动清理多余空格）
    /// - `output_path`: 生成的子字体输出路径
    ///
    /// 返回：
    /// - `Ok(true)`: 子集化执行成功
    /// - `Err(FontError::CommandError)`: 执行过程中发生错误
    ///
    #[cfg(target_os = "windows")]
    pub fn subset_font(
        &self, 
        font_path: &str, 
        text: &str, 
        output_path: &str
    ) -> Result<bool> {
        // 1. 记录调用此函数的初始信息
        info!("【桌面端】subset_font 被调用");
        info!(" - 原始文本 (text)       : {}", text);
        info!(" - 原始字体路径 (font)   : {}", font_path);
        info!(" - 原始输出路径 (output) : {}", output_path);
        let exec_start = Instant::now();

        // 2. 清理文本中多余的空格或全角空格 
        let cleaned_text: String = text
            .chars()
            .filter(|c| !(*c == ' ' || *c == '\u{3000}'))  // 去除半角/全角空格
            .collect();

        info!("【桌面端】subset_font 清理后的文本 (cleaned_text): {}", cleaned_text);

        // 3. 基于清理后的文本，生成 Unicode 列表
        let derived_unicodes_str = Self::text_to_unicodes_str(&cleaned_text);
        info!("【桌面端】subset_font 根据 text 生成的 Unicode 列表: {}", derived_unicodes_str);

        // 4. 拼接 hb-subset.exe 的路径（在临时目录下 xhs-printer/harfbuzz）
        let dir = env::temp_dir();
        let harfbuzz_env_path = dir.join("xhs-printer").join("harfbuzz");
        let hb_subset_path = harfbuzz_env_path.join("hb-subset.exe");
        info!("【桌面端】subset_font hb_subset_path: {}", hb_subset_path.display());

        // 5. 检查 hb-subset.exe 是否存在
        if !hb_subset_path.exists() {
            error!("【桌面端】subset_font hb_subset_path 不存在: {}", hb_subset_path.display());
            return Err(FontError::CommandError(format!(
                "hb-subset executable not found at {}",
                hb_subset_path.display()
            )));
        }

        // 6. 判断路径中是否包含中文，如果包含则需要驱动映射
        let needs_mapping = contains_chinese(font_path) || contains_chinese(output_path);
        info!("【桌面端】路径中是否包含中文 (needs_mapping): {}", needs_mapping);

        // 7. 构造命令行参数 (文本形式) 同时支持 --text 与 --unicodes
        let text_arg = format!("--text={}", cleaned_text);
        let unicodes_arg = format!("--unicodes={}", derived_unicodes_str);
        let output_path_arg = format!("--output-file={}", output_path);

        info!("【桌面端】命令行参数预览:");
        info!(" - {}", text_arg);
        info!(" - {}", unicodes_arg);
        info!(" - {}", output_path_arg);

        // 8. 对输入输出路径做可能的驱动映射
        let (font_path_wide, output_path_wide): (Vec<u16>, Vec<u16>);
        if needs_mapping {
            let font_path = PathBuf::from(font_path);
            let output_path = PathBuf::from(output_path);
            let root_dir = font_path.parent().and_then(|p| p.parent())
                .ok_or_else(|| FontError::CommandError("无法获取字体文件的根目录".to_string()))?;

            let check_output = Command::new("cmd")
                .args(&["/C", "subst"])
                .creation_flags(0x08000000)
                .output()
                .map_err(|e| FontError::CommandError(format!("检查 subst 映射失败: {}", e)))?;

            let subst_list = String::from_utf8_lossy(&check_output.stdout);
            let x_drive_mapped = subst_list.contains("X:");

            if !x_drive_mapped {
                let root_dir_str = root_dir.to_string_lossy().replace('\\', "\\\\");
                info!("【桌面端】subset_font formatted root dir: {}", root_dir_str);

                let subst_output = Command::new("cmd")
                    .args(&["/C", &format!("subst X: {}", root_dir_str)])
                    .creation_flags(0x08000000)
                    .output()
                    .map_err(|e| FontError::CommandError(format!("subst 命令执行失败: {}", e)))?;
                
                if !subst_output.status.success() {
                    error!("【桌面端】subset_font subst error: {}", String::from_utf8_lossy(&subst_output.stderr));
                    return Err(FontError::CommandError("subst 命令执行失败".to_string()));
                }
            }

            let relative_font_path = font_path.strip_prefix(root_dir)
                .map_err(|e| FontError::CommandError(format!("无法获取相对路径: {}", e)))?;
            let relative_output_path = output_path.strip_prefix(root_dir)
                .map_err(|e| FontError::CommandError(format!("无法获取相对路径: {}", e)))?;
            
            let mapped_font_path = PathBuf::from("X:").join(relative_font_path);
            let mapped_output_path = PathBuf::from("X:").join(relative_output_path);
            
            font_path_wide = OsStr::new(mapped_font_path.to_str().unwrap()).encode_wide().collect();
            let output_path_arg = format!("--output-file={}", mapped_output_path.display());
            output_path_wide = OsStr::new(&output_path_arg).encode_wide().collect();

        } else {
            // 如果不需要映射，直接使用原路径
            font_path_wide = OsStr::new(font_path).encode_wide().collect();
            output_path_wide = OsStr::new(&output_path_arg).encode_wide().collect();
        }

        // 9. 将 --text、--unicodes 转换为宽字符
        let text_wide: Vec<u16> = OsStr::new(&text_arg).encode_wide().collect();
        let unicodes_wide: Vec<u16> = OsStr::new(&unicodes_arg).encode_wide().collect();

        info!("【桌面端】准备执行子集化命令:");
        info!(" - 字体路径 (wide)       : {:?}", font_path_wide);
        info!(" - --text 参数 (wide)    : {:?}", text_wide);
        info!(" - --unicodes 参数 (wide): {:?}", unicodes_wide);
        info!(" - 输出文件 (wide)       : {:?}", output_path_wide);

        // 10. 检测是否可以使用 std::process::Command 来执行命令
        let use_std_command = can_use_std_command();
        info!("【桌面端】是否使用 std_command (use_std_command): {}", use_std_command);

        // 11. 依据 use_std_command 的结果选择不同方式执行命令
        if use_std_command {
            info!("【桌面端】subset_font 执行execute_std_command");
            // 1. 创建一个命令对象
            let mut command = Command::new(&hb_subset_path);

            // 2. 加入参数
            command
                // 第一个参数：字体路径
                .arg(OsString::from_wide(&font_path_wide))
                // 第二个参数：--text=XXX
                .arg(OsString::from_wide(&text_wide))
                // 第三个参数：--unicodes=XXX
                .arg(OsString::from_wide(&unicodes_wide))
                // 第四个参数：--output-file=XXX
                .arg(OsString::from_wide(&output_path_wide))
                // 下面是其他固定参数
                .arg("--name-IDs=1,2")
                .arg("--name-languages=*")
                .arg("--desubroutinize")
                .arg("--drop-tables=DSIG,hdmx,VDMX,LTSH,PCLT,vhea,vmtx,VORG,gasp")
                .arg("--no-hinting")
                .arg("--retain-gids")
                .arg("--glyph-names")
                .arg("--name-legacy")
                .arg("--layout-features=")
                // 通过 creation_flags 可以隐藏命令行窗口（SW_HIDE）
                .creation_flags(0x08000000 | 0x04000000)
                // 设置一些必要的环境变量
                .env("LANG", "zh_CN.UTF-8")
                .env("LC_ALL", "zh_CN.UTF-8");

            // 3. 打印命令行（含所有参数）
            info!("【桌面端】std_command 最终执行命令: {:?}", command);

            // 4. 执行命令并获取输出
            let output = command.output().map_err(|e| {
                FontError::CommandError(format!("Command execution failed: {}", e))
            })?;

            // 5. 成功或失败处理
            if output.status.success() {
                info!("【桌面端】subset_font\nCommand executed successfully");
                let duration_exec = exec_start.elapsed();
                info!(
                    "【桌面端 subset_font hb-subset 提取文字总结耗时分析】-命令执行耗时: {:?}",
                    duration_exec
                );
                Ok(true)
            } else {
                let error_message = String::from_utf8_lossy(&output.stderr);
                info!(
                    "【桌面端】subset_font \nCommand failed with error: {}",
                    error_message
                );
                Err(FontError::CommandError(error_message.to_string()))
            }
        } else {
            info!("【桌面端】subset_font 执行execute_tauri_command");
            return execute_tauri_command(
                &hb_subset_path,
                &font_path_wide,
                &text_wide,
                &unicodes_wide,
                &output_path_wide,
            );
        }
    }
}

///
/// # execute_std_command
///
/// 通过标准库的 `Command` 执行子集化命令。
/// 此函数会打印所有被拼接的参数，方便调试。
#[cfg(target_os = "windows")]
fn execute_std_command(
    hb_subset_path: &Path,
    font_path_wide: &[u16],
    text_wide: &[u16],
    unicodes_wide: &[u16],
    output_path_wide: &[u16],
) -> Result<bool> {
    // 1. 创建一个命令对象
    let mut command = Command::new(hb_subset_path);

    // 2. 加入参数
    command
        // 第一个参数：字体路径
        .arg(OsString::from_wide(font_path_wide))
        // 第二个参数：--text=XXX
        .arg(OsString::from_wide(text_wide))
        // 第三个参数：--unicodes=XXX
        .arg(OsString::from_wide(unicodes_wide))
        // 第四个参数：--output-file=XXX
        .arg(OsString::from_wide(output_path_wide))
        // 下面是其他固定参数
        .arg("--name-IDs=1,2")
        .arg("--name-languages=*")
        .arg("--desubroutinize")
        .arg("--drop-tables=DSIG,hdmx,VDMX,LTSH,PCLT,vhea,vmtx,VORG,gasp")
        .arg("--no-hinting")
        .arg("--retain-gids")
        .arg("--glyph-names")
        .arg("--name-legacy")
        .arg("--layout-features=")
        // 通过 creation_flags 可以隐藏命令行窗口（SW_HIDE）
        .creation_flags(0x08000000 | 0x04000000)
        // 设置一些必要的环境变量
        .env("LANG", "zh_CN.UTF-8")
        .env("LC_ALL", "zh_CN.UTF-8");

    // 3. 打印命令行（含所有参数）
    info!("【桌面端】std_command 最终执行命令: {:?}", command);

    // 4. 执行命令并获取输出
    let output = command.output().map_err(|e| {
        FontError::CommandError(format!("Command execution failed: {}", e))
    })?;

    // 5. 成功或失败处理
    if output.status.success() {
        info!("【桌面端】std_command 执行成功");
        Ok(true)
    } else {
        let error_message = String::from_utf8_lossy(&output.stderr);
        error!("【桌面端】std_command 执行失败: {}", error_message);
        Err(FontError::CommandError(error_message.to_string()))
    }
}

///
/// # execute_tauri_command
///
/// 通过 Tauri 提供的命令行功能执行子集化命令，
/// 并在此处打印命令以及参数信息。
#[cfg(target_os = "windows")]
fn execute_tauri_command(
    hb_subset_path: &Path,
    font_path_wide: &[u16],
    text_wide: &[u16],
    unicodes_wide: &[u16],
    output_path_wide: &[u16],
) -> Result<bool> {
    // 1. 转换成普通字符串给 TauriCommand
    let exec_tauri_start = Instant::now();
    let exe_path = hb_subset_path.to_string_lossy().to_string();

    // 2. 将所有参数拼接成字符串数组
    let args = vec![
        OsString::from_wide(font_path_wide),
        OsString::from_wide(text_wide),
        OsString::from_wide(unicodes_wide),
        OsString::from_wide(output_path_wide),
        OsString::from("--name-IDs=1,2"),
        OsString::from("--name-languages=*"),
        OsString::from("--desubroutinize"),
        OsString::from("--drop-tables=DSIG,hdmx,VDMX,LTSH,PCLT,vhea,vmtx,VORG,gasp"),
        OsString::from("--no-hinting"),
        OsString::from("--retain-gids"),
        OsString::from("--glyph-names"),
        OsString::from("--name-legacy"),
        OsString::from("--layout-features="),
    ]
    .into_iter()
    .map(|os_str| os_str.into_string().unwrap_or_default())
    .collect::<Vec<String>>();

    // 打印最终要执行的命令及参数
    info!("【桌面端】tauri_command 准备执行的命令: {}", exe_path);
    info!("【桌面端】tauri_command 准备执行的参数: {:?}", args);

    // 3. 执行 TauriCommand
    let result = TauriCommand::new(exe_path).args(args).output();

    // 4. 成功或失败处理
    match result {
        Ok(output) => {
            if output.status.success() {
                info!("【桌面端】subset_font\nCommand executed successfully");
                let duration_tauri_exec = exec_tauri_start.elapsed();
                info!(
                    "【桌面端 subset_font hb-subset tauri提取文字总结耗时分析】-命令执行耗时: {:?}",
                    duration_tauri_exec
                );
                Ok(true)
            } else {
                let error_message = String::from_utf8_lossy(&output.stderr.as_bytes());
                error!("【桌面端】tauri_command 执行失败: {}", error_message);
                Err(FontError::CommandError(error_message.to_string()))
            }
        }
        Err(e) => {
            error!("【桌面端】tauri_command 执行异常: {}", e);
            Err(FontError::CommandError(format!(
                "TauriCommand execution failed: {}",
                e
            )))
        },
    }
}

///
/// # contains_chinese
///
/// 判断字符串中是否包含中文字符。
fn contains_chinese(s: &str) -> bool {
    s.chars().any(|c| '\u{4E00}' <= c && c <= '\u{9FFF}')
}

///
/// # map_drive_paths
///
/// 针对 Windows 可能存在的盘符映射需求进行处理（占位示例）。
#[cfg(target_os = "windows")]
fn map_drive_paths(font_path: &str, output_path: &str) -> Result<(PathBuf, PathBuf)> {
    // 此处仅做示例，实际需按业务逻辑映射
    let mapped_font_path = PathBuf::from("X:").join(font_path);
    let mapped_output_path = PathBuf::from("X:").join(output_path);

    info!("【桌面端】map_drive_paths -> 原始 font_path: {}", font_path);
    info!("【桌面端】map_drive_paths -> 原始 output_path: {}", output_path);
    info!("【桌面端】map_drive_paths -> 映射后 font_path: {}", mapped_font_path.display());
    info!("【桌面端】map_drive_paths -> 映射后 output_path: {}", mapped_output_path.display());

    Ok((mapped_font_path, mapped_output_path))
}