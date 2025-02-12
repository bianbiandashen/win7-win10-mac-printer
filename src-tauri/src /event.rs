use log::{error, info};
use once_cell::sync::OnceCell;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Window};
use tokio;

/// 全局 AppHandle 存储，用于在任意位置获取 Window 实例
/// 使用 OnceCell 确保线程安全的一次性初始化
static APP_HANDLE: OnceCell<Arc<AppHandle>> = OnceCell::new();

/// 初始化全局 AppHandle
///
/// # Arguments
/// * `app_handle` - Tauri 应用的 AppHandle 实例
///
/// 通常在应用启动时的 setup 阶段调用此函数
pub fn init_event_app_handle(app_handle: AppHandle) {
    let _ = APP_HANDLE.set(Arc::new(app_handle));
}

/// 获取主窗口实例
///
/// # Returns
/// * `Option<Window>` - 如果成功则返回主窗口实例，否则返回 None
///
/// 使用全局存储的 AppHandle 获取名为 "main" 的窗口实例
pub fn get_main_window() -> Option<Window> {
    APP_HANDLE
        .get()
        .and_then(|handle| handle.get_window("main"))
}

/// 向前端发送事件
///
/// # Type Parameters
/// * `T` - 实现了 Serialize 和 Clone 的事件载荷类型
///
/// # Arguments
/// * `event_name` - 事件名称
/// * `payload` - 事件载荷，会被序列化后发送给前端
///
/// # Examples
/// ```rust
/// // 发送简单的字符串消息
/// emit_event("print-status", "打印完成");
///
/// // 发送结构化数据
/// emit_event("print-progress", json!({
///     "task_id": "123",
///     "progress": 0.5
/// }));
/// ```
pub fn emit_event<T>(event_name: &str, payload: T)
where
    T: Serialize + Clone + Send + 'static, // 添加 Send + 'static 约束以支持跨线程发送
{
    if let Some(window) = get_main_window() {
        let event_name = event_name.to_string();
        let window = window.clone();

        if let Err(e) = window.emit(&event_name, payload) {
            error!("Failed to emit event {}: {}", event_name, e);
        }
    }
}
