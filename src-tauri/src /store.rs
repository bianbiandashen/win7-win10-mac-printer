use std::sync::Arc;
use tokio::sync::Mutex;
use crate::error::CustomError;  
use crate::monitor::MemoryMonitor;
use tokio::sync::RwLock;
use std::process::Child;

/// 为每个预先创建的 PowerShell 进程定义一个结构体
pub struct PreSpawnedProcess {
    pub id: String,
    pub child: Child,
}

pub struct AppState {
    pub user_agent: Arc<Mutex<Option<String>>>,
    pub start_time: u128,
    pub seller_id: Arc<Mutex<Option<String>>>,
    monitor: Arc<RwLock<Option<MemoryMonitor>>>, 
    /// 存放预先启动的 PowerShell 子进程队列
    /// 使用 Arc<Mutex<>> 来确保多线程安全访问
    pub powershell_children: Arc<Mutex<Vec<PreSpawnedProcess>>>,
}

impl AppState {
    pub fn new(start_time: u128) -> Self {
        Self {
            user_agent: Arc::new(Mutex::new(None)),
            start_time,
            seller_id: Arc::new(Mutex::new(None)),
            monitor: Arc::new(RwLock::new(None)),
            powershell_children: Arc::new(Mutex::new(Vec::new()))
        }
    }

    pub async fn get_seller_id(&self) -> Option<String> {
        let seller_id = self.seller_id.lock().await;
        seller_id.clone()
    }

    pub async fn set_seller_id(&self, new_seller_id: String) {
        let mut seller_id_guard = self.seller_id.lock().await;
        *seller_id_guard = Some(new_seller_id);
    }

    pub async fn init_monitor(&self) -> Result<(), CustomError> {
        let mut monitor = self.monitor.write().await;
        if monitor.is_none() {
            *monitor = Some(MemoryMonitor::new(None));
        }
        Ok(())
    }
 
    pub async fn get_monitor(&self) -> Option<Arc<MemoryMonitor>> {
        let monitor = self.monitor.read().await;
        monitor.as_ref().map(|m| Arc::new(m.clone()))
    }
}
