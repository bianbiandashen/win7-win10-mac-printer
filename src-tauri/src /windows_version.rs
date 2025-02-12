use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use winapi::um::winnt::OSVERSIONINFOW;
#[cfg(windows)]
use std::mem;
#[cfg(windows)]
use winapi::um::sysinfoapi::{GetNativeSystemInfo, SYSTEM_INFO};
#[cfg(windows)]
use winapi::um::winnt::{PROCESSOR_ARCHITECTURE_INTEL, PROCESSOR_ARCHITECTURE_AMD64, PROCESSOR_ARCHITECTURE_ARM64};
#[cfg(windows)]
use winapi::shared::minwindef::DWORD;
#[cfg(windows)]
use winapi::um::winnt::HANDLE;


use log::info;

static INIT: Once = Once::new();
static IS_WINDOWS_7_OR_NEWER: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
pub unsafe fn check_windows_version() -> bool {
    INIT.call_once(|| {
        let os_info = get_windows_version();
        // 修改条件：检测 Windows 8 (6.2) 或更高版本
        let is_newer = os_info.dwMajorVersion > 6 
            || (os_info.dwMajorVersion == 6 && os_info.dwMinorVersion >= 2)
            || (os_info.dwMajorVersion == 10 && os_info.dwBuildNumber >= 22000);
        IS_WINDOWS_7_OR_NEWER.store(is_newer, Ordering::SeqCst);
        
        info!(
            "[windows_version] Detected Windows version: {}.{} (Build {})",
            os_info.dwMajorVersion, 
            os_info.dwMinorVersion, 
            os_info.dwBuildNumber
        );
    });
    // 使用 `SeqCst` 排序获取 `IS_WINDOWS_7_OR_NEWER` 的值
    // `Ordering::SeqCst` 表示对获取和存储都使用“顺序一致性”的排序，
    // 保证跨线程的访问顺序，避免不同线程访问该值时产生不一致。
    IS_WINDOWS_7_OR_NEWER.load(Ordering::SeqCst)
}


#[cfg(windows)]
pub unsafe fn get_windows_version() -> OSVERSIONINFOW {
    let mut osvi: OSVERSIONINFOW = mem::zeroed();
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
        osvi
    } else {
        panic!("Failed to get Windows version information.");
    }
}

#[cfg(windows)]
pub unsafe fn get_windows_version_string() -> String {
    let osvi = get_windows_version();
    format!(
        "{}.{}.{}",
        osvi.dwMajorVersion,
        osvi.dwMinorVersion,
        osvi.dwBuildNumber
    )
} 

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowsArch {
    X86,
    X64,
    Arm64,
    Unknown,  // 添加未知类型用于错误处理
}

#[cfg(windows)]
impl WindowsArch {
    pub fn is_64bit(&self) -> bool {
        matches!(self, WindowsArch::X64 | WindowsArch::Arm64)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            WindowsArch::X86 => "x86",
            WindowsArch::X64 => "x64",
            WindowsArch::Arm64 => "arm64",
            WindowsArch::Unknown => "unknown",
        }
    }
}

/**
 * 获取系统架构
 */
#[cfg(windows)]
pub fn get_windows_arch() -> WindowsArch {
    // 方法1: GetNativeSystemInfo
    let arch = unsafe {
        let mut system_info: SYSTEM_INFO = mem::zeroed();
        GetNativeSystemInfo(&mut system_info);
        
        match system_info.u.s().wProcessorArchitecture {
            PROCESSOR_ARCHITECTURE_AMD64 => Some(WindowsArch::X64),
            PROCESSOR_ARCHITECTURE_ARM64 => Some(WindowsArch::Arm64),
            PROCESSOR_ARCHITECTURE_INTEL => Some(WindowsArch::X86),
            _ => None
        }
    };

    if let Some(detected_arch) = arch {
        return detected_arch;
    }

    // 方法2: 环境变量检测
    if let Ok(arch) = std::env::var("PROCESSOR_ARCHITECTURE") {
        return match arch.to_uppercase().as_str() {
            "AMD64" | "X86_64" => WindowsArch::X64,
            "ARM64" => WindowsArch::Arm64,
            "X86" | "I386" => WindowsArch::X86,
            _ => WindowsArch::Unknown,
        };
    }

    // 方法3: 检查 PROCESSOR_ARCHITEW6432 环境变量
    // 在32位程序运行在64位系统上时很有用
    if let Ok(arch) = std::env::var("PROCESSOR_ARCHITEW6432") {
        return match arch.to_uppercase().as_str() {
            "AMD64" => WindowsArch::X64,
            "ARM64" => WindowsArch::Arm64,
            _ => WindowsArch::Unknown,
        };
    }

    // 方法4: 通过指针大小判断
    if mem::size_of::<usize>() == 8 {
        WindowsArch::X64
    } else {
        WindowsArch::X86
    }
}

/**
 * 判断是否为64位系统架构
 */
#[cfg(windows)]
pub fn is_64bit_system() -> bool {
    let arch = get_windows_arch();
    let is_64bit = arch.is_64bit();
    
    info!("检测到{}位系统架构 ({}), 请注意使用支持{}位的API", 
        if is_64bit { "64" } else { "32" },
        arch.as_str(),
        if is_64bit { "64" } else { "32" }
    );
    
    is_64bit
}