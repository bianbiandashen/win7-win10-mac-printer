use sysinfo::{System, Pid};  // 添加 SystemExt trait
use std::{time::Duration, sync::Arc};
use tokio::{time, sync::RwLock};
use log::{info, warn, error};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Window, State};  
use crate::store::AppState;  
use crate::error::CustomError;  


/// 内存告警级别
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]  // 添加 Serialize
pub enum AlertLevel {
    Normal = 0,     // 添加数值以明确顺序
    Warning = 1,    
    Critical = 2,   
    Danger = 3      
}

/// 内存统计信息
#[derive(Debug, Serialize, Clone)]
pub struct MemoryStats {
    pub total_memory: u64,      // 总内存(字节)
    pub used_memory: u64,       // 已用内存(字节)
    pub process_memory: u64,    // 进程内存(字节)
    pub memory_usage: f32,      // 使用率(百分比)
    pub alert_level: AlertLevel,// 修改为 AlertLevel 类型
}

/// 监控配置
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub warning_threshold: f32,    // 警告阈值
    pub critical_threshold: f32,   // 严重阈值
    pub danger_threshold: f32,     // 危险阈值
    pub check_interval: Duration,  // 检查间隔
    pub alert_duration: Duration,  // 告警持续时间阈值
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            warning_threshold: 70.0,
            critical_threshold: 85.0,
            danger_threshold: 95.0,
            check_interval: Duration::from_secs(5),
            alert_duration: Duration::from_secs(30),
        }
    }
}

#[derive(Clone)]  // 添加 Clone 派生
pub struct MemoryMonitor {
    stats: Arc<RwLock<Option<MemoryStats>>>,
    config: MonitorConfig,
    running: Arc<AtomicBool>,
    last_alert: Arc<RwLock<Option<(AlertLevel, std::time::Instant)>>>,
}

impl MemoryMonitor {
    pub fn new(config: Option<MonitorConfig>) -> Self {
        Self {
            stats: Arc::new(RwLock::new(None)),
            config: config.unwrap_or_default(),
            running: Arc::new(AtomicBool::new(false)),
            last_alert: Arc::new(RwLock::new(None)),
        }
    }

    pub async fn check_memory_pressure() -> bool {
        let mut sys = System::new();
        sys.refresh_memory();  // 刷新内存信息
        
        let total = sys.total_memory();
        if total == 0 {
            return false;
        }
        
        let memory_usage = (sys.used_memory() as f32 / total as f32) * 100.0;
        info!("当前内存使用率: {:.1}%", memory_usage);
        
        // 如果内存使用率超过90%，认为存在内存压力
        memory_usage > 90.0
    }
    fn get_alert_level(usage: f32, config: &MonitorConfig) -> AlertLevel {
        if usage >= config.danger_threshold {
            AlertLevel::Danger
        } else if usage >= config.critical_threshold {
            AlertLevel::Critical
        } else if usage >= config.warning_threshold {
            AlertLevel::Warning
        } else {
            AlertLevel::Normal
        }
    }

    async fn handle_alert(&self, current_level: AlertLevel, stats: &MemoryStats) -> Result<(), CustomError> {
        let mut last_alert = self.last_alert.write().await;
        let now = std::time::Instant::now();

        match *last_alert {
            Some((prev_level, time)) => {
                if current_level > prev_level || 
                   (current_level == prev_level && now.duration_since(time) >= self.config.alert_duration) {
                    self.trigger_alert(current_level, stats);
                    *last_alert = Some((current_level, now));
                }
            },
            None => {
                if current_level != AlertLevel::Normal {
                    self.trigger_alert(current_level, stats);
                    *last_alert = Some((current_level, now));
                }
            }
        }
        Ok(())
    }

    fn trigger_alert(&self, level: AlertLevel, stats: &MemoryStats) {
        let message = match level {
            AlertLevel::Warning => format!(
                "内存使用率警告: {:.1}% (进程使用: {:.1}MB)",
                stats.memory_usage,
                stats.process_memory as f64 / 1_048_576.0
            ),
            AlertLevel::Critical => format!(
                "内存使用率严重: {:.1}% (进程使用: {:.1}MB)",
                stats.memory_usage,
                stats.process_memory as f64 / 1_048_576.0
            ),
            AlertLevel::Danger => format!(
                "内存使用率危险: {:.1}% (进程使用: {:.1}MB)",
                stats.memory_usage,
                stats.process_memory as f64 / 1_048_576.0
            ),
            AlertLevel::Normal => return,
        };

        match level {
            AlertLevel::Warning => warn!("{}", message),
            AlertLevel::Critical | AlertLevel::Danger => error!("{}", message),
            AlertLevel::Normal => {}
        }
    }

    pub async fn start(&self, pid: u32) -> Result<(), CustomError> {
        if self.running.load(Ordering::SeqCst) {
            warn!("内存监控已在运行中");
            return Ok(());
        }
    
        self.running.store(true, Ordering::SeqCst);
        let stats = self.stats.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let last_alert = self.last_alert.clone();
    
        // 创建一个独立的监控器实例
        let monitor = Self {
            stats: stats.clone(),
            config: config.clone(),
            running: running.clone(),
            last_alert: last_alert.clone(),
        };
    
        tokio::spawn(async move {
            let mut sys = System::new();
            let mut interval = time::interval(config.check_interval);
            
            while running.load(Ordering::SeqCst) {
                interval.tick().await;
                
                sys.refresh_all();
                
                if let Some(process) = sys.process(Pid::from(pid as usize)) {
                    let memory_usage = (process.memory() as f32 / sys.total_memory() as f32) * 100.0;
                    let alert_level = Self::get_alert_level(memory_usage, &config);
                    
                    let current_stats = MemoryStats {
                        total_memory: sys.total_memory(),
                        used_memory: sys.used_memory(),
                        process_memory: process.memory(),
                        memory_usage,
                        alert_level,
                    };
    
                    // 直接使用监控器实例，而不是尝试创建新的
                    if let Err(e) = monitor.handle_alert(alert_level, &current_stats).await {
                        error!("处理内存告警失败: {}", e);
                    }
    
                    // 更新统计信息
                    if let Err(e) = {
                        let mut stats_guard = stats.write().await;
                        *stats_guard = Some(current_stats);
                        Ok::<_, CustomError>(())
                    } {
                        error!("更新内存统计信息失败: {}", e);
                    }
                }
            }
        });
    
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub async fn get_current_stats(&self) -> Option<MemoryStats> {
        self.stats.read().await.clone()
    }

    pub async fn init_and_start_monitoring(
        app_state: State<'_, AppState>,
        window: Window,
    ) -> Result<(), CustomError> {
        // 初始化监控器
        app_state.init_monitor().await?;
        
        if let Some(monitor) = app_state.get_monitor().await {
            // 启动监控
            let current_pid = std::process::id();
            println!("当前进程ID: {}", current_pid);
            monitor.start(current_pid).await?;
    
            // 启动状态报告任务
            Self::start_stats_reporting(monitor, window).await;
        }
    
        Ok(())
    }
    
    /// 启动内存状态报告任务
    async fn start_stats_reporting(monitor: Arc<MemoryMonitor>, window: Window) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                match monitor.get_current_stats().await {
                    Some(stats) => {
                        if let Err(e) = window.emit("memory-stats", stats) {
                            error!("发送内存状态失败: {}", e);
                        }
                    }
                    None => {
                        warn!("获取内存状态失败");
                    }
                }
            }
        });
    }
}