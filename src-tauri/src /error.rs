use std::collections::HashMap;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct CustomError {
    pub key: &'static str,
    pub message: String,
    pub source: Option<Box<dyn Error>>,
    pub error_code: Option<ErrorCode>, // 将 error_code 修改为可选类型
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.error_code {
            Some(code) => write!(f, "错误码: {} Log: {}", code, self.message),
            None => write!(f, "Log: {}", self.message),
        }
    }
}

impl Error for CustomError {}

#[derive(Debug)]
pub struct CustomAsyncError {
    pub key: &'static str,
    pub message: String,
    pub source: Option<Box<dyn Error + Send + Sync>>,
    pub error_code: Option<u32>,
}

impl fmt::Display for CustomAsyncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.error_code {
            Some(code) => write!(f, "Error (Code: {:#010x}): {}", code, self.message),
            None => write!(f, "Error: {}", self.message),
        }
    }
}

impl Error for CustomAsyncError {}

#[derive(Debug)] // 自动生成Debug实现
pub struct ErrorCode {
    pub code: u32,
}

impl ErrorCode {
    pub fn new(system: System, component: Component, error_type: u32) -> Self {
        let code = (system as u32) | ((component as u32 & 0xFF) << 12) | (error_type & 0xFFF);
        ErrorCode { code }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#010x}", self.code)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum System {
    Windows7 = 0x00100000,
    Windows10 = 0x00200000,
    MacOS = 0x00300000,
    GeneralReport = 0x00500000,
}

#[derive(Debug, Clone, Copy)]
pub enum Component {
    NetworkModule = 0x00001,
    PrinterListModule = 0x00002,
    BuildPrinterArtifactModule = 0x00003,
    PrinterCommandModule = 0x00004,
    StartingModule = 0x00005,
}

fn get_error_type_map() -> HashMap<u32, &'static str> {
    let mut error_map = HashMap::new();
    error_map.insert(0x001, "文件未找到");
    error_map.insert(0x002, "访问被拒绝");
    error_map.insert(0x003, "连接超时");
    error_map.insert(0x004, "参数无效");
    error_map.insert(0x005, "获取打印机失败");
    error_map.insert(0x006, "页面启动失败");
    error_map.insert(0x007, "打印机应用启动失败");
    // 启动
    error_map.insert(0x008, "无法获取配置目录");
    error_map.insert(0x009, "无法创建配置目录");
    error_map.insert(0x010, "无法复制 log4rs 文件");
    error_map.insert(0x011, "无法读取 log4rs 配置文件");
    error_map.insert(0x012, "解析 log4rs 配置失败");
    error_map.insert(0x013, "将修改后的配置序列化失败");
    error_map.insert(0x014, "无法写入更新后的 log4rs 配置文件");
    error_map.insert(0x015, "日志初始化失败");
    error_map.insert(0x016, "广播通道初始化失败");
    error_map.insert(0x017, "Windows 环境初始化失败");
    error_map.insert(0x018, "字体初始化失败");
    error_map.insert(0x019, "WebSocket 绑定失败");
    // error_map.insert(0x020, "启动webWebSocket失败");
    error_map.insert(0x021, "WebSocket 握手错误");
    // error_map.insert(0x200, "下发打印指令成功");
    // error_map.insert(0x200, "下发打印指令成功");
    // error_map.insert(0x200, "下发打印指令成功");

    // mac打印机模块
    error_map.insert(0x022, "MACOS:解析输出失败或解析为空");
    error_map.insert(0x023, "MACOS:没有配置任何打印机，返回空的打印机列表");
    error_map.insert(0x024, "MACOS:发生其他错误");
    error_map.insert(0x025, "MACOS:无法执行 lpstat 命令");
    error_map.insert(0x026, "MACOS:MACOS将打印机列表转换为 JSON 失败");
    error_map.insert(0x027, "MACOS:获取MAC-PRINT_JOB打印作业失败");
    error_map.insert(0x061, "MACOS: 打印解析 page_size 时出错");
    error_map.insert(0x062, "MACOS: 打印失败");

    // win10打印机模块
    error_map.insert(0x034, "Windows10: 获取打印机推送广播消息输出失败：");
    error_map.insert(0x035, "Windows10: 获取打印机推送广播消息执行失败：");
    error_map.insert(0x036, "Windows10: 获取打印机接收线程结果失败");
    error_map.insert(0x037, "Windows10: 获取任务列表推送广播消息输出失败");
    error_map.insert(0x038, "Windows10: 获取任务列表推送广播消息执行失败");
    error_map.insert(0x039, "Windows10: 获取任务列表接收线程结果失败");
    error_map.insert(0x040, "Windows10: 通过id获取任务列表命令执行失败");
    error_map.insert(0x041, "Windows10: 通过打印机名称获取打印机命令执行失败");

    // win7打印机模块
    error_map.insert(0x045, "Windows7: 获取打印机无法创建临时目录");
    error_map.insert(0x046, "Windows7: 获取打印机创建进程失败，无法执行 PowerShell 脚本");
    error_map.insert(0x047, "Windows7: 获取打印机无法执行 PowerShell 脚本");
    error_map.insert(0x048, "Windows7: 获取打印机无法打开输出文件");
    error_map.insert(0x049, "Windows7: 获取打印机无法读取输出文件");
    error_map.insert(0x050, "Windows7: 通过名称获取打印机无法创建临时目录");
    error_map.insert(0x051, "Windows7: 通过名称获取打印机创建进程失败，无法执行 PowerShell 命令");
    error_map.insert(0x052, "Windows7: 通过名称获取打印机无法打开输出文件");
    error_map.insert(0x053, "Windows7: 通过名称获取打印机无法读取输出文件");
    error_map.insert(0x054, "Windows7: 获取任务列表无法创建临时目录");
    error_map.insert(0x055, "Windows7: 获取任务列表创建进程失败，无法执行 PowerShell 脚本");
    error_map.insert(0x056, "Windows7: 获取任务列表无法打开输出文件");
    error_map.insert(0x057, "Windows7: 获取任务列表无法打开输出文件");
    error_map.insert(0x058, "Windows7: 兼容模式打印无法执行 PowerShell 脚本");
    error_map.insert(0x059, "Windows7: 兼容模式打印写入输出文件失败");
    error_map.insert(0x060, "Windows7: 兼容模式打印删除临时文件失败");

    // win10打印模块
    error_map.insert(0x042, "Windows10: 兼容模式打印失败");
    error_map.insert(0x043, "Windows10: 兼容模式打印命令执行错误");
    error_map.insert(0x044, "Windows10: 兼容模式任务执行失败");
    error_map.insert(0x063, "Windows10: 高清打印Ghostscript可执行文件不存在");
    error_map.insert(0x070, "Windows10: 高清打印CreateProcessW命令执行失败");
    error_map.insert(0x064, "Windows10: 高清打印待打印的PDF文件不存在");
    error_map.insert(0x065, "Windows10: 高清打印记录命令行日志失败");
    error_map.insert(0x066, "Windows10: 高清打印记录打印状态文件失败");
    error_map.insert(0x067, "Windows10: 高清打印删除临时文件失败");
  

    // win7打印模块
    error_map.insert(0x068, "Windows7: 高清打印Ghostscript可执行文件不存在");
    error_map.insert(0x069, "Windows7: 高清打印待打印的PDF文件不存在");
    error_map.insert(0x071, "Windows7: 高清打印CreateProcessW命令执行失败");
    error_map.insert(0x072, "Windows7: 高清打印记录命令行日志失败");
    error_map.insert(0x073, "Windows7: 高清打印记录打印状态文件失败");
    error_map.insert(0x074, "Windows7: 高清打印删除临时文件失败");
    // error_map.insert(0x075, "Windows7: ");

    // http
    error_map.insert(0x075, "http: 创建 HTTP 客户端失败");
    error_map.insert(0x076, "http: 请求构建器克隆失败，无法发送请求");
    error_map.insert(0x077, "http: 请求超时");
    error_map.insert(0x078, "http: 网络连接失败");
    error_map.insert(0x079, "http: 请求错误，请求发送失败");

    // webSocket
    error_map.insert(0x080, "webSocket: 无法打开证书文件");
    error_map.insert(0x081, "webSocket: 解析证书失败");
    error_map.insert(0x082, "webSocket: 无法读取私钥文件");
    error_map.insert(0x083, "webSocket: 解密私钥失败");
    error_map.insert(0x084, "webSocket: 解析私钥失败");
    error_map.insert(0x085, "webSocket: 未找到私钥");
    error_map.insert(0x086, "webSocket: 设置 TLS 配置失败");
    error_map.insert(0x087, "webSocket: 私钥解密失败");
    error_map.insert(0x088, "webSocket: 私钥格式转换失败");
    error_map.insert(0x089, "webSocket: 私钥PEM编码失败");
    error_map.insert(0x090, "webSocket: 无法创建证书文件");
    error_map.insert(0x091, "webSocket: 无法创建私钥文件");
    error_map.insert(0x092, "webSocket: 证书路径转换失败");
    error_map.insert(0x093, "webSocket: 私钥路径转换失败");
    error_map.insert(0x094, "webSocket: WebSocket Secure 绑定失败");
    error_map.insert(0x095, "webSocket: ");
    error_map.insert(0x095, "webSocket: ");
    error_map.insert(0x095, "webSocket: ");
    error_map.insert(0x095, "webSocket: ");
    error_map.insert(0x095, "webSocket: ");
    error_map.insert(0x095, "webSocket: ");
    
    error_map
}

#[derive(Debug)]
pub enum ErrorLevel {
    Error,
    Warning,
    Info,
}

impl ErrorLevel {
    pub fn to_string(&self) -> &str {
        match self {
            ErrorLevel::Info => "low",
            ErrorLevel::Warning => "medium",
            ErrorLevel::Error => "high",
        }
    }
}
