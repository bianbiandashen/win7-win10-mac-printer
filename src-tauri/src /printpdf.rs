use crate::declare;
use crate::error::{Component, ErrorCode, System};
use crate::harfbuzz::processor::FontError;
use crate::harfbuzz::FontProcessor;
use crate::macos;
use crate::onixcomponents;
#[cfg(target_os = "macos")]
use crate::onixcomponents::utils::subset_font;
use crate::onixcomponents::utils::{clear_font_cache, reset_layer_defaults};
use crate::utils::get_build_version;
use crate::utils::set_image_filename_cache;
use crate::utils::{
    calculate_optimal_workers, extract_image_filename, generate_random_string, get_formatted_time,
};
use crate::windows10;
use crate::windows7;
use crate::windows_version;
use chrono::{Local, Utc};
use lazy_static::lazy_static;
use log::{error, info};
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use printpdf::*;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::{
    env::{self},
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock, Semaphore};
use tokio::time::timeout;

use crate::asyncfunc::execute_print_task_sync;
use crate::event::emit_event;
use dashmap::DashMap;
use std::collections::HashSet;
use std::sync::{mpsc as std_mpsc, Mutex as std_Mutex};
use std::thread;
use tokio::sync::Mutex;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore as TokioSemaphore;
use tokio::task::spawn_blocking;
// use crate::monitored_semaphore::MonitoredSemaphore;

// ==== ADD LOGS HERE ====
// 用于记录「推送任务的数量」和「处理任务的数量」。
lazy_static! {
    static ref TASK_PUSH_COUNT: AtomicUsize = AtomicUsize::new(0);    // 已推送到队列的任务数
    static ref TASK_PROCESS_COUNT: AtomicUsize = AtomicUsize::new(0); // 已被工作线程开始处理的任务数
    static ref TASK_SEQUENCE: AtomicU64 = AtomicU64::new(0); // 任务序列号
    static ref PRINTER_QUEUES: DashMap<String, (u64, BTreeMap<u64, PrintTask>)> = DashMap::new();
}

// 定义绘制任务的结构体
#[derive(Serialize, Deserialize, Debug)]
pub struct DrawTask {
    task_id: String,          // 复制的document_uuid
    original_task_id: String, // 原始的task_id
    printer: String,
    printdata: String,
    options: Value,
    sequence: u64, // 新增字段，用于排序
    web_status: String,
}

#[derive(Serialize, Clone)]
pub struct PrintResponse {
    pub taskid: String,
    pub pdf_path: String,
    pub printer: String,
    pub success: bool,
    pub message: String,
}

// 定义打印任务的结构体
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrintTask {
    pub task_id: String,
    pub pdf_path: String,
    pub printer: String,
    pub print_settings: Value, // 或更具体的字段
    pub sequence: u64,
    pub web_status: String,
}

// 定义监控信号量的结构体

static PRINT_SENDER: OnceCell<Arc<std_Mutex<std_mpsc::Sender<PrintTask>>>> = OnceCell::new();

/**
 *
 * 绘制任务的并发控制 在 initialize_task_queue 中，
 * 使用了多个绘制工作线程 (num_draw_workers，默认为4)，
 * 每个线程独立处理任务。这种方式通过固定线程池的数量来限制并发，
 * 而不是通过 MonitoredSemaphore 的许可控制。
 *
 */
struct MonitoredSemaphore {
    total_permits: usize,
    active_tasks: std::sync::atomic::AtomicUsize,
    waiting_tasks: std::sync::atomic::AtomicUsize,
}

impl MonitoredSemaphore {
    // 创建新的 MonitoredSemaphore
    fn new(max_permits: usize) -> Self {
        Self {
            /**
             *  绘制任务的并发控制 在 initialize_task_queue 中，使用了多个绘制工作线程 (num_draw_workers，默认为4)，
             * 每个线程独立处理任务。这种方式通过固定线程池的数量来限制并发，而不是通过 MonitoredSemaphore 的许可控制
             *
             * */


            // semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(max_permits)),
            total_permits: max_permits,
            active_tasks: std::sync::atomic::AtomicUsize::new(0),
            waiting_tasks: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 异步获取许可，并更新计数
    // pub async fn acquire(&self) -> OwnedSemaphorePermit {
    //     // ==== ADD LOGS HERE ====
    //     // 记录 acquire 执行的开始时间
    //     let acquire_start = Instant::now();

    //     // 等待队列 +1，表示有新任务开始等待
    //     self.waiting_tasks.fetch_add(1, Ordering::SeqCst);

    //     // 调用底层 semaphore 的 acquire_owned() 来获取许可
    //     let permit = self
    //         .semaphore
    //         .clone()
    //         .acquire_owned() // 不传入任何参数，则获取 1 个许可
    //         .await
    //         .expect("acquire semaphore failed");

    //     // 等待队列 -1，表示已不再等待
    //     self.waiting_tasks.fetch_sub(1, Ordering::SeqCst);

    //     // 活跃任务 +1，表示有任务正在被执行
    //     self.active_tasks.fetch_add(1, Ordering::SeqCst);

    //     // 计算并记录 acquire() 的总耗时
    //     let acquire_duration = acquire_start.elapsed();
    //     info!(
    //         "[MonitoredSemaphore] acquire() 执行完成，耗时: {:?}",
    //         acquire_duration
    //     );

    //     // 返回信号量许可（OwnedSemaphorePermit）
    //     permit
    // }

    // // 释放许可，并更新计数
    // pub fn release(&self) {
    //     // 活跃任务 -1，表示有任务执行完毕
    //     self.active_tasks.fetch_sub(1, Ordering::SeqCst);
    // }

    // 获取当前活跃任务数量
    fn active_tasks(&self) -> usize {
        self.active_tasks.load(Ordering::SeqCst)
    }

    // 获取当前等待任务数量
    fn waiting_tasks(&self) -> usize {
        self.waiting_tasks.load(Ordering::SeqCst)
    }

    // 获取总许可数量
    fn total_permits(&self) -> usize {
        self.total_permits
    }

    // 获取信号量实例
    // fn semaphore(&self) -> Arc<TokioSemaphore> {
    //     Arc::clone(&self.semaphore)
    // }
}

// 定义任务执行记录的结构体
#[derive(Debug, Clone, Serialize)]
pub struct ThreadExecution {
    pub task_id: String,
    pub method_name: String,
    pub thread_id: String,
    pub start_time: u128,
    pub end_time: Option<u128>,
    pub duration: Option<u128>,
}

lazy_static! {
    // 存储任务执行记录
    pub static ref EXECUTION_DATA: Arc<RwLock<Vec<ThreadExecution>>> = Arc::new(RwLock::new(Vec::new()));
    // 发送器，用于外部提交绘制任务
    static ref DRAW_TASK_SENDER: AsyncMutex<Option<mpsc::Sender<DrawTask>>> = AsyncMutex::new(None);
    // 发送器，用于外部提交打印任务
    // static ref PRINT_TASK_SENDER: AsyncMutex<Option<mpsc::Sender<PrintTask>>> = AsyncMutex::new(None);

    // // 发送器，用于外部提交绘制任务
    // static ref DRAW_TASK_SENDER: OnceCell<Arc<mpsc::Sender<DrawTask>>> = OnceCell::new();
      // 发送器，用于外部提交打印任务
    static ref PRINT_TASK_SENDER: OnceCell<Arc<mpsc::Sender<PrintTask>>> = OnceCell::new();


    static ref CLIENT: Arc<Client> = Arc::new(Client::builder()
        .timeout(Duration::from_secs(200))
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::REFERER,
                reqwest::header::HeaderValue::from_static("https://cloudprint.xiaohongshu.com/")
            );
            headers
        })
        .build()
        .unwrap());

    // 信号量用于控制并发任务数量
    static ref PRINT_SEMAPHORE: Arc<MonitoredSemaphore> = Arc::new(MonitoredSemaphore::new(40));
}

// 更新打印机序号的命令
#[tauri::command]
pub fn update_printer_sequence(printer_name: String, task_id: String) -> Result<(), String> {
    let mut entry = PRINTER_QUEUES
        .entry(printer_name.clone())
        .or_insert((1, BTreeMap::new()));

    entry.0 += 1;
    info!(
        "【update_printer_sequence】打印机 {} 的序号已更新到 {}",
        printer_name, entry.0
    );
    process_printer_queue_tasks(printer_name);
    Ok(())
}

pub fn process_printer_queue_tasks(printer_name: String) {
    loop {
        // 使用loop替代while let
        let next_task = {
            let mut entry = PRINTER_QUEUES.get_mut(&printer_name).unwrap();
            info!(
                "【打印线程】检查后续任务 - printer: {}, expected_sequence: {}, queue_size: {}",
                printer_name, entry.0, entry.1.len()
            );

            if let Some((&next_seq, _)) = entry.1.first_key_value() {
                info!(
                    "【打印线程】找到队列中的任务 - next_sequence: {}, expected_sequence: {}",
                    next_seq, entry.0
                );
                if next_seq == entry.0 {
                    entry.0 += 1;
                    entry.1.remove(&next_seq)
                } else {
                    info!(
                        "【打印线程】序号不匹配，等待中间任务 - next_sequence: {}, expected_sequence: {}",
                        next_seq, entry.0
                    );
                    break;
                }
            } else {
                info!("【打印线程】队列为空，无后续任务");
                break;
            }
        };

        match next_task {
            Some(task) => {
                let task_id = task.task_id.clone();
                info!(
                    "【打印线程】序号匹配next_task，准备执行后续任务,前端任务状态={}",
                    task.web_status
                );
                if task.web_status == "fail".to_string() {
                    info!(
                        "【打印线程工作线程2】前端任务状态失败task_id={}",
                        task.task_id.clone()
                    );
                } else {
                    info!("【打印线程】开始执行后续任务 - task_id: {}", task_id);
                    let print_result = execute_print_task_sync(&task);
                    info!(
                        "【打印线程】后续任务执行完成 - task_id: {}, result: {:?}",
                        task_id, print_result
                    );
                }
            }
            None => break, // 没有更多任务时退出循环
        }
    }
}

fn log_method_start(task_id: &str, method_name: &str) {
    let thread_id = format!("{:?}", std::thread::current().id());
    let start_time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(e) => {
            error!("System time is before UNIX_EPOCH: {}", e);
            return;
        }
    };

    let execution = ThreadExecution {
        task_id: task_id.to_string(),
        method_name: method_name.to_string(),
        thread_id: thread_id.clone(),
        start_time,
        end_time: None,
        duration: None,
    };

    let execution_clone = execution.clone();
    let exec_data = EXECUTION_DATA.clone();
    tokio::spawn(async move {
        let mut data = exec_data.write().await;
        data.push(execution_clone);

        // 保留最近 100 条记录，防止数据无限增长
        if data.len() > 100 {
            data.remove(0);
        }
    });

    info!(
        "【{}】【{}】【线程 {}】开始执行",
        task_id, method_name, thread_id
    );
}

fn log_method_end(task_id: &str, method_name: &str) {
    let thread_id = format!("{:?}", std::thread::current().id());
    let end_time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(e) => {
            error!("System time is before UNIX_EPOCH: {}", e);
            return;
        }
    };

    // 克隆拥有所有权的字符串
    let task_id_clone = task_id.to_string();
    let method_name_clone = method_name.to_string();
    let thread_id_clone = thread_id.clone();
    let exec_data = EXECUTION_DATA.clone();

    tokio::spawn(async move {
        let mut data = exec_data.write().await;
        if let Some(execution) = data.iter_mut().rev().find(|exec| {
            exec.task_id == task_id_clone
                && exec.method_name == method_name_clone
                && exec.end_time.is_none()
        }) {
            execution.end_time = Some(end_time);
            execution.duration = Some(end_time - execution.start_time);
            let duration = execution.duration.unwrap_or(0);
            info!(
                "【{}】【{}】【线程 {}】执行完成，耗时 {} ms",
                task_id_clone, method_name_clone, thread_id_clone, duration
            );
        } else {
            error!(
                "【{}】【{}】未找到对应的 ThreadExecution 记录",
                task_id_clone, method_name_clone
            );
        }
    });
}

pub async fn download_and_save_image(
    task_id: &str,
    url: &str,
    save_path: &PathBuf,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    log_method_start(task_id, "download_and_save_image");

    if url.trim().is_empty() {
        error!("【{}】【download_and_save_image】URL 不能为空", task_id);
        return Err("URL 不能为空".into());
    }

    info!(
        "【{}】【download_and_save_image】开始下载图片: {}",
        task_id, url
    );

    let response = match timeout(Duration::from_secs(200), CLIENT.get(url).send()).await {
        Ok(result) => match result {
            Ok(response) => response,
            Err(e) => {
                error!("【{}】【download_and_save_image】请求失败: {}", task_id, e);
                return Err(e.into());
            }
        },
        Err(_) => {
            error!("【{}】【download_and_save_image】请求超时", task_id);
            return Err("请求超时".into());
        }
    };

    if !response.status().is_success() {
        error!(
            "【{}】【download_and_save_image】HTTP 请求失败: {}",
            task_id,
            response.status()
        );
        return Err(format!("HTTP 请求失败: {}", response.status()).into());
    }

    println!(
        "【{:?}】【download_and_save_image】save_path: {:?}",
        url, save_path
    );
    set_image_filename_cache(url, file_name);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    let extension = match content_type {
        t if t.contains("image/png") => "png",
        t if t.contains("image/jpeg") || t.contains("image/jpg") => "jpg",
        t if t.contains("image/webp") => "webp",
        t if t.contains("image/gif") => "gif",
        _ => {
            error!(
                "【{}】【download_and_save_image】不支持的图片类型: {}",
                task_id, content_type
            );
            return Err(format!("不支持的图片类型: {}", content_type).into());
        }
    };

    let mut final_path = save_path.clone();
    if let Some(stem) = save_path.file_stem().and_then(|s| s.to_str()) {
        final_path.set_file_name(format!("{}.{}", stem, extension));
    } else {
        error!("【{}】【download_and_save_image】无效的文件路径", task_id);
        return Err("无效的文件路径".into());
    }

    info!(
        "【{}】【download_and_save_image】保存图片，Content-Type: {}, 路径: {:?}",
        task_id, content_type, final_path
    );

    let bytes = match timeout(Duration::from_secs(200), response.bytes()).await {
        Ok(result) => match result {
            Ok(bytes) => bytes,
            Err(e) => {
                error!(
                    "【{}】【download_and_save_image】获取图片数据失败: {}",
                    task_id, e
                );
                return Err(e.into());
            }
        },
        Err(_) => {
            error!("【{}】【download_and_save_image】获取图片数据超时", task_id);
            return Err("获取图片数据超时".into());
        }
    };

    if let Err(e) = fs::write(&final_path, &bytes) {
        error!(
            "【{}】【download_and_save_image】写入文件失败: {}",
            task_id, e
        );
        return Err(e.into());
    }

    log_method_end(task_id, "download_and_save_image");
    Ok(())
}

pub async fn process_image_component(task_id: &str, url: &str) {
    log_method_start(task_id, "process_image_component");

    let image_file_name = extract_image_filename(url);
    if let Some(file_name) = image_file_name {
        let mut img_dir = env::temp_dir();
        img_dir.push("imgs");

        if let Err(e) = fs::create_dir_all(&img_dir) {
            error!(
                "【{}】【process_image_component】无法创建 imgs 文件夹: {}",
                task_id, e
            );
            log_method_end(task_id, "process_image_component");
            return;
        }

        img_dir.push(&file_name);

        let extensions = ["png", "jpg", "jpeg", "webp", "gif"];
        let mut exists = false;
        for ext in extensions.iter() {
            let test_path = img_dir.with_extension(ext);
            if test_path.exists() {
                info!(
                    "【{}】【process_image_component】图片已存在: {:?}",
                    task_id, test_path
                );
                exists = true;
                break;
            }
        }

        if !exists {
            info!(
                "【{}】【process_image_component】开始下载图片: {}",
                task_id, url
            );
            if let Err(e) = download_and_save_image(task_id, url, &img_dir, &file_name).await {
                error!(
                    "【{}】【process_image_component】下载图片失败: {}",
                    task_id, e
                );
            }
        }
    } else {
        error!(
            "【{}】【process_image_component】无法从 URL 提取文件名: {}",
            task_id, url
        );
    }

    log_method_end(task_id, "process_image_component");
}

async fn execute_component_main(
    task_id: &str,
    json: &Value,
    doc: &PdfDocumentReference,
    page_width: Mm,
    page_height: Mm,
    (page1, layer1): (PdfPageIndex, PdfLayerIndex),
    subset_font_path: String,
    gap_width: f32,
    gap_height: f32,
) -> Result<String, String> {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-绘制PDF--对应function-execute_component_main-taskid={}】-开始执行",
        task_id
    );
    // ----------------------------------------

    log_method_start(task_id, "execute_component_main");

    info!(
        "【{}】【execute_component_main】subset_font_path:确保进入subset_font_path有数据 {}",
        task_id, subset_font_path
    );
    let execute_component_main_start_time = Instant::now();
    let data_list = match json.as_array() {
        Some(list) => list,
        None => {
            error!(
                "【{}】【execute_component_main】输入数据不是有效的数组",
                task_id
            );
            log_method_end(task_id, "execute_component_main");
            // ---------------- 新增日志 ----------------
            info!(
                "【电子面单打印核心方法-绘制PDF--对应function-execute_component_main-taskid={}】-执行失败，输入数据不是有效数组。",
                task_id
            );
            // ----------------------------------------
            return Err("输入数据不是有效的数组".to_string());
        }
    };

    for (index, data) in data_list.iter().enumerate() {
        let (page, layer) = if index == 0 {
            (page1, layer1)
        } else {
            let (current_page, current_layer_index) =
                doc.add_page(page_width, page_height, format!("Layer {}", index + 1));
            (current_page, current_layer_index)
        };

        let current_layer = doc.get_page(page).get_layer(layer);

        if let Some(children) = data.get("children").and_then(|c| c.as_array()) {
            if children.is_empty() {
                info!("【{}】【execute_component_main】children 数组为空", task_id);
                continue;
            }

            for child in children {
                let component_name = match child.get("componentName").and_then(|n| n.as_str()) {
                    Some(name) => name,
                    None => {
                        error!(
                            "【{}】【execute_component_main】componentName 无效或缺失",
                            task_id
                        );
                        continue;
                    }
                };

                if let Some(props) = child.get("props") {
                    if let Some(cover) = props.get("cover") {
                        if let Some(url) = cover.get("url").and_then(|u| u.as_str()) {
                            let task_id_clone = task_id.to_string();
                            let url_clone = url.to_string();
                            process_image_component(&task_id_clone, &url_clone).await;
                        }
                    }
                }

                let json_str = match serde_json::to_string(child) {
                    Ok(s) => s,
                    Err(e) => {
                        error!(
                            "【{}】【execute_component_main】序列化组件失败: {}",
                            task_id, e
                        );
                        continue;
                    }
                };

                let draw_component_start_time = Instant::now();

                match component_name {
                    "OnixBarleyElectronicBarcode" => {
                        onixcomponents::OnixBarleyElectronicBarcode::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone(),gap_width);
                    }
                    "OnixBarleyElectronicBarcodeVertical" => {
                        onixcomponents::OnixBarleyElectronicBarcodeVertical::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone(),gap_height);
                    }
                    "OnixBarleyElectronicImage" => {
                        onixcomponents::OnixBarleyElectronicImage::draw(
                            &current_layer,
                            &json_str,
                            Some(&doc),
                            page_height,
                            subset_font_path.clone(),
                        );
                    }
                    "OnixBarleyElectronicIsv" => {
                        onixcomponents::OnixBarleyElectronicIsv::draw(
                            &current_layer,
                            &json_str,
                            page_height,
                        );
                    }
                    "OnixBarleyElectronicLine" => {
                        onixcomponents::OnixBarleyElectronicLine::draw(
                            &current_layer,
                            &json_str,
                            page_height,
                        );
                    }
                    "OnixBarleyElectronicLineVertical" => {
                        onixcomponents::OnixBarleyElectronicLineVertical::draw(
                            &current_layer,
                            &json_str,
                            page_height,
                        );
                    }
                    "OnixBarleyElectronicQrcode" => {
                        onixcomponents::OnixBarleyElectronicQrcode::draw(
                            &current_layer,
                            &json_str,
                            page_height,
                        );
                    }
                    "OnixBarleyElectronicRectangle" => {
                        onixcomponents::OnixBarleyElectronicRectangle::draw(
                            &current_layer,
                            &json_str,
                            page_height,
                        );
                    }
                    "OnixBarleyElectronicText" => {
                        onixcomponents::OnixBarleyElectronicText::draw(
                            &current_layer,
                            &json_str,
                            Some(&doc),
                            page_height,
                            subset_font_path.clone(),
                        );
                    }
                    "OnixBarleyElectronicTextVertical" => {
                        onixcomponents::OnixBarleyElectronicTextVertical::draw(
                            &current_layer,
                            &json_str,
                            Some(&doc),
                            page_height,
                            subset_font_path.clone(),
                        );
                    }
                    _ => {
                        info!(
                            "【{}】【execute_component_main】未找到匹配的组件: {}",
                            task_id, component_name
                        );
                    }
                }
                info!(
                    "【{}】【execute_component_main】绘制组件耗时: {:?}",
                    component_name,
                    draw_component_start_time.elapsed()
                );
                reset_layer_defaults(&current_layer);
            }
        }
    }

    info!(
        "【{}】【execute_component_main】绘制组件总耗时: {:?}",
        task_id,
        execute_component_main_start_time.elapsed()
    );
    log_method_end(task_id, "execute_component_main");

    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-绘制PDF--对应function-execute_component_main-taskid={}】-执行完毕，总耗时: {:?}",
        task_id,
        execute_component_main_start_time.elapsed()
    );
    // ----------------------------------------

    // 返回生成的 PDF 文件路径
    let pdf_path = format!("{}/{}.pdf", env::temp_dir().to_str().unwrap(), task_id);
    Ok(pdf_path)
}

/**
 * 函数功能
 * 1. 遍历 JSON 中的 children，收集文本组里的文本内容，拼凑成一个完整的文本
 * 2. 使用 subset_font 从 Medium 字体文件中子集化出完整文本对应的字体
 * 3. 返回子集化字体的路径
 */
pub fn collect_text_from_json(json: &Value) -> Result<String, String> {
    let collect_text_start_time = Instant::now();
    let mut unique_chars = std::collections::HashSet::new();
    unique_chars.extend(
        "你的生活指南ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz*1234567890".chars(),
    );
    let data_list = match json.as_array() {
        Some(list) => list,
        None => {
            return Err("输入数据不是有效的数组".into());
        }
    };

    const TEXT_COMPONENTS: [&str; 5] = [
        "OnixBarleyElectronicText",
        "OnixBarleyElectronicTextVertical",
        "OnixBarleyElectronicQrcode",
        "OnixBarleyElectronicBarcode",
        "OnixBarleyElectronicBarcodeVertical",
    ];

    for data in data_list {
        if let Some(children) = data.get("children").and_then(|c| c.as_array()) {
            for child in children {
                let component_name = match child.get("componentName").and_then(|n| n.as_str()) {
                    Some(name) => name,
                    None => continue,
                };

                // Only collect text from text components
                if TEXT_COMPONENTS.contains(&component_name) {
                    if let Some(props) = child.get("props") {
                        if let Some(content_section) = props.get("contentSection") {
                            if let Some(value) =
                                content_section.get("value").and_then(|v| v.as_str())
                            {
                                unique_chars.extend(value.chars());
                            }
                        }
                    }
                }
            }
        }
    }
    let text: String = unique_chars.into_iter().collect();
    info!(
        "【collect_text_from_json】收集文本耗时: {:?}",
        collect_text_start_time.elapsed()
    );

    let create_subset_font_start_time = Instant::now();
    let mut resource_dir = env::temp_dir();
    resource_dir.push("xhs-printer-fonts");
    resource_dir.push("SiYuanHeiTi");
    resource_dir.push("Medium.otf");
    info!(
        "【collect_text_from_json】resource_dir目录: {:?}",
        resource_dir
    );
    let font_path = resource_dir.to_str().unwrap();
    info!(
        "【collect_text_from_json】resource_dir字体path2: {:?}",
        font_path
    );

    // 创建子集字体的目录
    let mut subset_dir = PathBuf::from(font_path);
    subset_dir.pop(); // 移除文件名
    subset_dir.push("subsets"); // 添加 subsets 子目录

    // 确保子集目录存在
    if !subset_dir.exists() {
        std::fs::create_dir_all(&subset_dir).expect("Failed to create subsets directory");
    }
    // 使用 rand 生成随机数
    let random_number: u64 = rand::thread_rng().gen();
    let subset_name = format!("subset_{}.otf", random_number);
    let subset_path = subset_dir.join(&subset_name);
    let subset_path_str = subset_path.to_str().unwrap();
    info!(
        "【collect_text_from_json】创建子集字体path1: {:?}",
        subset_path_str
    );

    let subset_font_path = match FontProcessor::new().and_then(|processor| {
        #[cfg(target_os = "windows")]
        {
            processor.subset_font(font_path, &text, subset_path_str)
        }

        #[cfg(target_os = "macos")]
        {
            match subset_font(font_path, &text, subset_path_str) {
                Ok(_) => Ok(()),
                Err(e) => Err(FontError::CommandError(e.to_string())),
            }
        }
    }) {
        Ok(_) => subset_path_str.to_string(),
        Err(_) => "".to_string(),
    };
    info!(
        "【collect_text_from_json】创建子集字体耗时: {:?}",
        create_subset_font_start_time.elapsed()
    );
    info!(
        "【collect_text_from_json】创建子集字体path: {:?}",
        subset_font_path.to_string()
    );
    Ok(subset_font_path.to_string())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PrintResult {
    pub taskid: String,
    pub document_uuid: String,
}

// =============== start_print_pdf ===============
#[tauri::command]
pub async fn start_print_pdf(
    taskid: String,
    printdata: String,
    options: serde_json::Value,
) -> Result<HashMap<String, String>, String> {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf-taskid={}】-开始执行",
        taskid
    );
    // ----------------------------------------

    let start_time = Instant::now();
    log_method_start(&taskid, "start_print_pdf");
    info!(
        "[{}]【{}】【start_print_pdf】开始生成PDF",
        get_build_version(),
        taskid
    );

    info!("【start_print_pdf】options: {:?}", options);
    let printer_name = options
        .get("printer")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let document_uuid = options
        .get("documentuuid")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let document_id = options
        .get("documentid")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    // 前端任务的成功或失败状态
    let web_status = options
        .get("webstatus")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // 当前队列里任务list
    let queue: tokio::sync::MutexGuard<'_, PrintTaskQueue> = PRINT_QUEUE.lock().await;
    // printername
    info!("【start_print_pdf】printer_name: {:?}", printer_name);
    let task_list = queue.get_task_list(&printer_name);
    info!(
        "【start_print_pdf】当前队列里任务list: {:?}, document_uuid: {:?}, document_id: {:?}",
        task_list, document_uuid, document_id
    );
    // 直接从task_list中查找序号
    let sequence = task_list
        .iter()
        .find(|task| task.document_uuid == document_uuid)
        .map(|task| task.sequence)
        .ok_or_else(|| format!("任务 {} 未在队列中找到", document_uuid))?;

    info!(
        "【start_print_pdf】任务 {} 获取到序号 {}",
        document_uuid, sequence
    );

    // 创建 DrawTask 实例
    let draw_task = DrawTask {
        task_id: document_uuid.clone(),
        original_task_id: taskid.clone(),
        printer: printer_name.clone(),
        printdata: printdata.clone(),
        options: options.clone(),
        web_status: web_status.clone(),
        sequence,
    };

    // ==== ADD LOGS HERE ====
    // 记录这是第几个被推送的任务
    let current_push_count = TASK_PUSH_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    info!(
        "【start_print_pdf】推送第 {} 个任务到绘制队列, document_uuid = {}",
        current_push_count, document_uuid
    );
    info!(
        "【start_print_pdf】推送任务时间 (taskid={}): {:?}",
        document_uuid,
        Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
    );

    // 将任务发送到绘制队列
    {
        let push_start = Instant::now();

        let sender_guard = DRAW_TASK_SENDER.lock().await;
        if let Some(sender) = sender_guard.as_ref() {
            sender
                .send(draw_task)
                .await
                .map_err(|e| format!("任务发送失败: {}", e))?;
            info!("【start_print_pdf】成功发送任务到绘制队列");
        } else {
            error!("【start_print_pdf】绘制任务队列未初始化");
            // ---------------- 新增日志 ----------------
            info!(
                "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf-taskid={}】-失败, 绘制任务队列未初始化",
                taskid
            );
            // ----------------------------------------
            return Err("绘制任务队列未初始化".to_string());
        }

        // 打印耗时
        let push_duration = push_start.elapsed();
        info!(
            "[start_print_pdf] 推送任务到绘制队列完成，耗时: {:?}",
            push_duration
        );
    }

    // 使用 HashMap 返回包含 taskid 和 document_uuid 的键值对
    let mut result_map = HashMap::new();
    result_map.insert("taskid".to_string(), taskid.clone());
    result_map.insert("document_uuid".to_string(), document_uuid.clone());

    info!(
        "【{}】【start_print_pdf】任务已提交 (document_uuid={}), 本次调用耗时: {:?}",
        document_uuid,
        document_uuid,
        start_time.elapsed()
    );

    log_method_end(&taskid, "start_print_pdf");

    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf-taskid={}】-执行完毕，总耗时: {:?}",
        taskid,
        start_time.elapsed()
    );
    // ----------------------------------------

    Ok(result_map)
}

// =============== start_print_pdf_task ===============
#[allow(dead_code)]
pub async fn start_print_pdf_task(task: &DrawTask) -> Result<String, String> {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf_task-taskid={}】-开始执行",
        task.task_id
    );
    // ----------------------------------------

    use crate::onixcomponents::utils::get_font_path;
    let task_id = task.task_id.clone();
    let original_task_id = task.original_task_id.clone();
    let printdata = task.printdata.clone();
    let options = task.options.clone();

    // 使用 spawn_blocking 处理阻塞任务
    let blocking_result = spawn_blocking(move || {
        let blocking_start_time = Instant::now();
        let blocking_thread_id = format!("{:?}", std::thread::current().id());
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 开始阻塞任务",
            task_id, blocking_thread_id
        );

        // 清理字体缓存
        let font_cache_start = Instant::now();
        clear_font_cache();
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 清理字体缓存完成，耗时: {:?}",
            task_id,
            blocking_thread_id,
            font_cache_start.elapsed()
        );

        // 获取页面尺寸
        let size_start = Instant::now();
        let (page_width, page_height) = match options.get("size") {
            Some(size) => {
                let size_str = size
                    .as_str()
                    .ok_or_else(|| "size 选项不是有效的字符串".to_string())?;
                let parts: Vec<&str> = size_str.split('x').collect();
                if parts.len() == 2 {
                    let width = parts[0]
                        .parse::<f32>()
                        .map_err(|_| "无法解析页面宽度".to_string())?;
                    let height = parts[1]
                        .parse::<f32>()
                        .map_err(|_| "无法解析页面高度".to_string())?;
                    info!(
                        "【{}】【start_print_pdf_task】[线程-{}] 页面尺寸: {}x{}, 解析耗时: {:?}",
                        task_id,
                        blocking_thread_id,
                        width,
                        height,
                        size_start.elapsed()
                    );
                    (Mm(width), Mm(height))
                } else {
                    info!(
                        "【{}】【start_print_pdf_task】[线程-{}] 使用默认尺寸: 76x130",
                        task_id, blocking_thread_id
                    );
                    (Mm(76.0), Mm(130.0))
                }
            }
            None => {
                info!(
                    "【{}】【start_print_pdf_task】[线程-{}] 使用默认尺寸: 76x130",
                    task_id, blocking_thread_id
                );
                (Mm(76.0), Mm(130.0))
            }
        };

        let pdf_start = Instant::now();
        let formatted_time = get_formatted_time(Utc::now());
        // 时间戳不能保证名称唯一随机字符串不能删
        let random_string = generate_random_string(6);
        let pdf_name = format!("{}{}_{}", formatted_time, random_string, original_task_id);

        info!(
            "【{}】【start_print_pdf_task】[线程-{}] pdf_name: {}",
            task_id, blocking_thread_id, pdf_name
        );

        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 开始创建PDF文档",
            task_id, blocking_thread_id
        );
        // 创建PDF
        let doc_result = PdfDocument::new(&pdf_name, page_width, page_height, "Layer 1");
        let (doc, page1, layer1) = doc_result;
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] PDF文档创建成功，耗时: {:?}",
            task_id,
            blocking_thread_id,
            pdf_start.elapsed()
        );

        let json_start = Instant::now();
        let json: serde_json::Value =
            serde_json::from_str(&printdata).map_err(|e| format!("JSON 解析失败: {}", e))?;
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] JSON解析成功，耗时: {:?}",
            task_id,
            blocking_thread_id,
            json_start.elapsed()
        );

        let font_start = Instant::now();
        let subset_font_path = if options
            .get("allfontmode")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            info!(
                "【{}】【start_print_pdf_task】[线程-{}] 启用全字体模式",
                task_id, blocking_thread_id
            );
            let path_buf = PathBuf::from(get_font_path("SimHei, Arial, sans-serif", "normal"));
            path_buf
                .to_str()
                .expect("Path is not valid UTF-8")
                .to_string()
        } else {
            match collect_text_from_json(&json) {
                Ok(path) => {
                    info!(
                        "【{}】【start_print_pdf_task】[线程-{}] 字体子集化成功，耗时: {:?}",
                        task_id,
                        blocking_thread_id,
                        font_start.elapsed()
                    );
                    PathBuf::from(path)
                        .to_str()
                        .expect("Path is not valid UTF-8")
                        .to_string()
                }
                Err(e) => {
                    error!(
                        "【{}】【start_print_pdf_task】[线程-{}] 字体子集化失败: {}, 耗时: {:?}; 注意：为了避免找不到子集化字体文件导致报错，即将使用全量字体打印",
                        task_id,
                        blocking_thread_id,
                        e,
                        font_start.elapsed()
                    );
                    let full_font_path =
                        PathBuf::from(get_font_path("SimHei, Arial, sans-serif", "normal"));
                    full_font_path
                        .to_str()
                        .expect("Path is not valid UTF-8")
                        .to_string()
                }
            }
        };
        info!(
            "【collect_text_from_json】创建子集字体path3: {:?}",
            subset_font_path
        );

        let runtime_start = Instant::now();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("创建运行时失败: {}", e))?;
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 运行时创建成功，耗时: {:?}",
            task_id,
            blocking_thread_id,
            runtime_start.elapsed()
        );

        let component_start = Instant::now();
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 开始执行组件绘制",
            task_id, blocking_thread_id
        );
        info!("【{}】【start_print_pdf_task】[线程-{}] options: {:?}", task_id, blocking_thread_id, options);
        let gap_width = options.get("gapwidth").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let gap_height = options.get("gapheight").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        rt.block_on(execute_component_main(
            &task_id,
            &json,
            &doc,
            page_width,
            page_height,
            (page1, layer1),
            subset_font_path,
            gap_width,
            gap_height,
        ))?;
        info!(
            "【{}】【start_print_pdf_task】[线程-{}] 组件绘制完成，耗时: {:?}",
            task_id,
            blocking_thread_id,
            component_start.elapsed()
        );

        let write_start = Instant::now();
        let mut pdf_bytes = Vec::new();
        {
            let mut writer = BufWriter::new(&mut pdf_bytes);
            doc.save(&mut writer)
                .map_err(|e| format!("保存 PDF 数据失败:  {}", e))?;
        }
        let mut file_path = std::env::temp_dir();
        file_path.push(format!("{}.pdf", pdf_name));
        let temp_path = file_path.with_extension("tmp");
        let task_id_for_blocking = task_id.clone();
        File::create(&temp_path)
            .and_then(|mut file| file.write_all(&pdf_bytes))
            .map_err(|e| format!("写入文件失败: {}", e))?;

        fs::rename(&temp_path, &file_path).map_err(|e| format!("重命名文件失败: {}", e))?;

        info!(
            "【{}】【start_print_pdf】[线程-{}] PDF保存完成，耗时: {:?}",
            task_id_for_blocking,
            blocking_thread_id,
            write_start.elapsed()
        );

        info!(
            "【{}】【start_print_pdf】[线程-{}] 任务完成，总耗时: {:?}",
            task_id_for_blocking,
            blocking_thread_id,
            blocking_start_time.elapsed()
        );
        // 返回生成的 PDF 文件路径
        Ok(file_path.to_str().unwrap().to_string())
    });

    // 等待阻塞任务执行完毕，并处理可能的错误
    let result = match blocking_result.await {
        Ok(result) => result, // result 是 Result<String, String>
        Err(e) => Err(format!("JoinError: {:?}", e)),
    };

    // ---------------- 新增日志 ----------------
    match &result {
        Ok(path) => {
            info!(
                "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf_task-taskid={}】-执行完毕，生成文件路径: {}",
                task.task_id, path
            );
        }
        Err(e) => {
            error!(
                "【电子面单打印核心方法-绘制PDF--对应function-start_print_pdf_task-taskid={}】-执行失败: {}",
                task.task_id, e
            );
        }
    }
    // ----------------------------------------

    result
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn print_pdf(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    printer_setting: String,
    remove_after_print: bool,
    page_size: String,
    auto_fit: bool,
) -> String {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-开始执行（MacOS）",
        id
    );
    // ----------------------------------------

    info!(
        "[桌面端主入口调用] print_pdf - 打印 PDF 文件, 文件路径: {}",
        path
    );
    info!(
        "[桌面端主入口调用] print_pdf , 文件page_size: {}",
        page_size
    );
    info!(
        "[桌面端主入口调用] print_pdf, 文件remove_after_print: {}",
        remove_after_print
    );
    info!("[桌面端主入口调用] print_pdf, 文件auto_fit: {}", auto_fit);

    let options = declare::PrintOptions {
        id: id.clone(),
        printer: printer.clone(),
        taskid: taskid.clone(),
        path: path.clone(),
        print_setting: printer_setting.clone(),
        remove_after_print,
        auto_fit,
    };
    let page_size_json: Value = match serde_json::from_str(&page_size) {
        Ok(json) => json,
        Err(err) => {
            let error_message = format!(
                "{}解析 page_size 出错: {}",
                ErrorCode::new(System::MacOS, Component::PrinterCommandModule, 0x061),
                err
            );
            info!("{}", error_message);

            // ---------------- 新增日志 ----------------
            info!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-失败，解析 page_size 出错: {}",
                id, err
            );
            // ----------------------------------------

            return error_message;
        }
    };

    let definition = page_size_json
        .get("definition")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    info!("页面设置的 definition 值: {}", definition);

    let mut print_response = PrintResponse {
        taskid: taskid.clone(),
        printer: printer.clone(),
        pdf_path: path.to_string(),
        success: false,
        message: String::new(),
    };

    let result = match macos::print_pdf_macos(
        id.clone(),
        printer.clone(),
        taskid.clone(),
        path.clone(),
        printer_setting.clone(),
        remove_after_print,
        auto_fit,
    )
    .await
    {
        Ok(_) => {
            // ---------------- 新增日志 ----------------
            info!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，打印成功（MacOS）",
                id
            );
            // ----------------------------------------
            let message = "MacOS-打印成功".to_string();
            print_response.success = true;
            print_response.message = message.clone();
            message
        }
        Err(err) => {
            let msg = format!(
                "{}MacOS-打印失败: {}",
                ErrorCode::new(System::MacOS, Component::PrinterCommandModule, 0x062),
                err
            );
            // ---------------- 新增日志 ----------------
            error!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行失败: {}",
                id, msg
            );
            print_response.message = msg.clone();
            // ----------------------------------------
            msg
        }
    };
    emit_event("print_pdf_task_finished", print_response);

    result
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn print_pdf(
    id: String,
    printer: String,
    taskid: String,
    path: String,
    printer_setting: String,
    remove_after_print: bool,
    page_size: String,
    auto_fit: bool,
) -> String {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-开始执行（Windows）",
        id
    );
    // ----------------------------------------

    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, 文件路径: {}",
        path
    );
    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, page_size对象: {}",
        page_size
    );
    info!(
        "[桌面端主入口调用] print_pdf - 开始打印 PDF 文件, remove_after_print: {}",
        remove_after_print
    );

    #[cfg(target_os = "windows")]
    set_thread_priority_max(); // 设置绘制线程的最高优先级

    let options = declare::PrintOptions {
        id: id.clone(),
        printer: printer.clone(),
        taskid: taskid.clone(),
        path: path.clone(),
        print_setting: printer_setting.clone(),
        remove_after_print,
        auto_fit,
    };

    let page_size_json: Value = match serde_json::from_str(&page_size) {
        Ok(json) => json,
        Err(err) => {
            let error_message = format!("解析 page_size 时出错: {}", err);
            info!("{}", error_message);

            // ---------------- 新增日志 ----------------
            error!(
                "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-解析 page_size 时出错: {}",
                id, err
            );
            // ----------------------------------------

            return error_message;
        }
    };

    let definition = page_size_json
        .get("definition")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    info!("页面设置的 definition 值: {}", definition);

    let result = unsafe {
        if windows_version::check_windows_version() {
            info!("检测 Windows 系统，准备打印 PDF");

            if definition {
                info!("打印模式: 高清打印模式 (definition: true)");
                match windows10::print_pdf(options, page_size).await {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，高清打印成功（Windows10+）",
                            id
                        );
                        // ----------------------------------------
                        "Windows-打印成功".to_string()
                    }
                    Err(err) => {
                        let error_message = format!("标准打印失败: {}", err);
                        error!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-高清打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        error_message
                    }
                }
            } else {
                info!("打印模式: 兼容打印模式 (definition: false)");
                match windows10::print_pdf_compatible_mode(options).await {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，兼容模式打印成功（Windows10+）",
                            id
                        );
                        // ----------------------------------------
                        "Windows-打印成功".to_string()
                    }
                    Err(err) => {
                        let error_message = format!("兼容模式打印失败: {}", err);
                        info!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-兼容模式打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        error_message
                    }
                }
            }
        } else {
            info!("检测到 Windows 7 系统，准备使用 Windows 7 打印接口");
            if definition {
                info!("打印模式: Win7高清打印模式 (definition: true)");
                match windows7::print_pdf(options, page_size).await {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，Win7高清模式打印成功",
                            id
                        );
                        // ----------------------------------------
                        "Windows7-打印成功".to_string()
                    }
                    Err(err) => {
                        let error_message = format!("Windows7-高清模式打印失败: {}", err);
                        info!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-Win7高清模式打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        error_message
                    }
                }
            } else {
                info!("打印模式: 兼容打印模式 (definition: false)");
                match windows10::print_pdf_compatible_mode(options).await {
                    Ok(_) => {
                        // ---------------- 新增日志 ----------------
                        info!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-执行完毕，Win7兼容模式打印成功",
                            id
                        );
                        // ----------------------------------------
                        "Windows-打印成功".to_string()
                    }
                    Err(err) => {
                        let error_message = format!("兼容模式打印失败: {}", err);
                        info!("{}", error_message);
                        // ---------------- 新增日志 ----------------
                        error!(
                            "【电子面单打印核心方法-打印方法--对应function-print_pdf-taskid={}】-Win7兼容模式打印失败: {}",
                            id, error_message
                        );
                        // ----------------------------------------
                        error_message
                    }
                }
            }
        }
    };

    if remove_after_print {
        info!("删除打印后的临时文件: {}", path);
        cleanup_temp_file(&path, remove_after_print);
    } else {
        info!("不删除临时文件: {}", path);
    }

    result
}

pub fn cleanup_temp_file(path: &str, remove_after_print: bool) {
    if remove_after_print && Path::new(path).exists() {
        if let Err(e) = std::fs::remove_file(path) {
            error!("[删除文件失败] 无法删除临时文件: {}", e);
        }
    }
}

// =============== execute_print_task ===============
async fn execute_print_task(task: &PrintTask) {
    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-打印方法--对应function-execute_print_task-taskid={}】-开始执行",
        task.task_id
    );
    // ----------------------------------------

    #[cfg(target_os = "windows")]
    set_thread_priority_max(); // 设置绘制线程的最高优先级
    log_method_start(&task.task_id, "execute_print_pdf");

    info!(
        "【{}】【execute_print_task】开始调用 print_pdf",
        task.task_id
    );

    // 从 print_settings 中提取相关参数
    let print_setting_str: String = match task.print_settings.get("printsetting") {
        Some(value) => value.as_str().unwrap_or("").to_string(),
        None => String::new(),
    };

    let print_setting: Value = if !print_setting_str.is_empty() {
        from_str(&print_setting_str).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let id: &str = match print_setting.get("id") {
        Some(value) => value.as_str().unwrap_or(""),
        None => "",
    };

    let page_size = match print_setting.get("pageSize") {
        Some(value) => value.as_str().unwrap_or(""),
        None => "",
    };

    let auto_fit = match print_setting.get("autoFit") {
        Some(value) => match value {
            Value::Bool(b) => *b,
            Value::String(s) => s.parse().unwrap_or(false),
            _ => {
                info!("【桌面端】【execute_print_task】auto_fit value is not a boolean or string: {:?}", value);
                false
            }
        },
        None => {
            info!("【桌面端】【execute_print_task】auto_fit value is missing");
            false
        }
    };

    let remove_after_print = match print_setting.get("removeAfterPrint") {
        Some(value) => match value {
            Value::Bool(b) => *b,
            Value::String(s) => s.parse().unwrap_or(false),
            _ => {
                info!("【桌面端】【execute_print_task】remove_after_print value is not a boolean or string: {:?}", value);
                false
            }
        },
        None => {
            info!("【桌面端】【execute_print_task】remove_after_print value is missing");
            false
        }
    };

    info!(
        "【桌面端】【execute_print_task】print_setting {}",
        print_setting
    );
    info!(
        "【桌面端】【execute_print_task】remove_after_print {}",
        remove_after_print
    );
    info!("【桌面端】【execute_print_task】auto_fit {}", auto_fit);

    // 直接调用 print_pdf 并 await 其结果
    let print_result = print_pdf(
        id.to_string(),
        task.printer.clone(),
        task.task_id.clone(),
        task.pdf_path.clone(), // 使用绘制完成后的 PDF 路径
        print_setting_str.clone(),
        remove_after_print,
        page_size.to_string(),
        auto_fit,
    )
    .await;

    info!(
        "【{}】【execute_print_task】print_pdf 调用结束: {}",
        task.task_id, print_result
    );

    info!("【{}】【execute_print_task】任务完成", task.task_id);
    log_method_end(&task.task_id, "execute_print_pdf");

    // ---------------- 新增日志 ----------------
    info!(
        "【电子面单打印核心方法-打印方法--对应function-execute_print_task-taskid={}】-执行完毕",
        task.task_id
    );
    // ----------------------------------------
}

// =============== initialize_task_queue ===============
// =============== initialize_task_queue ===============
pub async fn initialize_task_queue() {
    // #[cfg(target_os = "windows")]
    // set_thread_priority_max();

    // 创建 mpsc 通道
    let (print_tx, print_rx) = std_mpsc::channel::<PrintTask>();
    thread::spawn(move || {
        info!("打印线程已启动");
        #[cfg(target_os = "windows")]
        set_thread_priority_max();

        while let Ok(task) = print_rx.recv() {
            info!(
                "【打印线程】收到打印任务 - task_id: {}, printer: {}, sequence: {}",
                task.task_id, task.printer, task.sequence
            );
            let printer_name = task.printer.clone();
            let task_id = task.task_id.clone();
            let sequence = task.sequence;
            let should_print = {
                let mut entry = PRINTER_QUEUES
                    .entry(printer_name.clone())
                    .or_insert((1, BTreeMap::new()));
                info!(
                    "【打印线程】当前打印机队列状态, 打印机 {}: 期望序号={}, 待处理任务={}, 收到任务序号={}, 前端任务状态={}",
                    printer_name,
                    entry.0,
                    entry.1.len(),
                    task.sequence,
                    task.web_status,
                );
                if sequence == entry.0 {
                    entry.0 += 1;
                    if task.web_status == "fail".to_string() {
                        info!(
                            "【打印线程工作线程】前端任务状态失败task_id={}",
                            task.task_id.clone()
                        );
                        false
                    } else {
                        true
                    }
                } else {
                    if task.web_status != "fail".to_string() {
                        // 序号不匹配时，存储任务到队列
                        let stored_task = task.clone();
                        entry.1.insert(sequence, stored_task);
                        info!(
                            "【打印线程】序号不匹配，存入队列 - task_id: {}, sequence: {}, queue_size: {}",
                            task_id, sequence, entry.1.len()
                        );
                    }
                    false
                }
            };

            if should_print {
                info!("【打印线程】开始执行打印任务 - task_id: {}", task_id);
                let print_result = execute_print_task_sync(&task);
                info!(
                    "【打印线程】执行打印任务完成 - task_id: {:?}, result: {:?}",
                    task_id, print_result
                );

                info!(
                    "【打印线程】准备进入后续任务检查循环 - printer: {}",
                    printer_name
                );
                process_printer_queue_tasks(printer_name);
            }
        }
        info!("【打印线程】打印通道已关闭，线程退出");
    });

    info!("打印发送器初始化");
    // 新增: 保存打印队列发送端
    PRINT_SENDER
        .set(Arc::new(std_Mutex::new(print_tx)))
        .expect("Failed to set print sender");

    let (draw_tx, draw_rx) = mpsc::channel::<DrawTask>(1000);
    // 设置 DRAW_TASK_SENDER
    {
        let mut draw_sender_guard = DRAW_TASK_SENDER.lock().await;
        *draw_sender_guard = Some(draw_tx);
    }

    // // 包装接收端以便共享
    let draw_rx = Arc::new(AsyncMutex::new(draw_rx));

    // 启动绘制工作线程（多个并发）
    let worker_num = calculate_optimal_workers();
    let num_draw_workers = worker_num * 2; // 根据需要调整并发数量
    for i in 0..num_draw_workers {
        info!("[初始化] 启动第 {} 个『绘制工作线程』", i);
        let draw_rx = Arc::clone(&draw_rx);

        tokio::spawn(async move {
            loop {
                // 从绘制通道接收任务
                let task_option = {
                    let mut rx_guard = draw_rx.lock().await;
                    rx_guard.recv().await
                };

                let task = match task_option {
                    Some(task) => task,
                    None => {
                        info!("[绘制工作线程 {}] 通道已关闭, 退出循环", i);
                        break;
                    }
                };

                // 这里可记录剩余任务等
                info!(
                    "[绘制工作线程 {}] 开始处理任务 (sequence={}, task_id={}, 前端状态={})",
                    i, task.sequence, task.task_id, task.web_status
                );

                // ---------------- 新增日志 ----------------
                info!(
                    "【电子面单打印核心方法-绘制PDF--对应function-initialize_task_queue-draw_worker{}-taskid={}】-开始执行",
                    i, task.task_id
                );
                // ----------------------------------------

                // 执行绘制 PDF
                match start_print_pdf_task(&task).await {
                    Ok(pdf_path) => {
                        info!(
                            "[绘制工作线程 {}] 绘制任务完成，生成的 PDF 路径: {}",
                            i, pdf_path
                        );
                        // 创建 PrintTask
                        let print_task = PrintTask {
                            task_id: task.task_id.clone(),
                            pdf_path,
                            printer: task.printer.clone(),
                            print_settings: task.options.clone(),
                            sequence: task.sequence,
                            web_status: task.web_status,
                        };

                        // 推送到打印队列
                        if let Some(sender) = PRINT_SENDER.get() {
                            match sender.lock() {
                                Ok(guard) => {
                                    if let Err(e) = guard.send(print_task) {
                                        error!("发送打印任务失败: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("获取打印发送器锁失败: {}", e);
                                }
                            }
                        } else {
                            error!("打印任务队列未初始化");
                        }
                    }
                    Err(e) => {
                        error!("绘制任务失败 (task_id={}): {}", task.task_id, e);
                        let mut drag_response = PrintResponse {
                            taskid: task.task_id.clone(),
                            printer: task.printer.clone(),
                            pdf_path: "".to_string(),
                            success: false,
                            message: "pdf绘制任务失败".to_string(),
                        };
                        emit_event("print_pdf_task_finished", drag_response);
                    }
                }
            }
        });
    }

    // 启动信号量监控任务，每 10 秒记录一次信号量状态
    tokio::spawn(async move {
        loop {
            let active = PRINT_SEMAPHORE.active_tasks();
            let waiting = PRINT_SEMAPHORE.waiting_tasks();
            let total = PRINT_SEMAPHORE.total_permits();
            info!(
                "信号量状态: 活跃任务={}, 等待任务={}, 总许可={}",
                active, waiting, total
            );
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

#[cfg(target_os = "windows")]
pub fn set_thread_priority_max() {
    use log::{debug, error};
    use std::io::Error;
    use winapi::shared::minwindef::BOOL;
    use winapi::shared::ntdef::HANDLE;
    use winapi::um::processthreadsapi::{GetCurrentThread, SetThreadPriority};

    const THREAD_PRIORITY_HIGHEST: i32 = 2;

    unsafe {
        let thread_handle: HANDLE = GetCurrentThread();
        let result: BOOL = SetThreadPriority(thread_handle, THREAD_PRIORITY_HIGHEST);
        if result == 0 {
            error!(
                "Failed to set thread priority: {:?}",
                Error::last_os_error()
            );
        } else {
            debug!("Thread priority set to HIGHEST successfully.");
        }
    }
}

// 修改 PrintTaskQueue 结构
struct PrintTaskQueue {
    tasks: HashMap<String, Vec<InitialTask>>,
}

impl PrintTaskQueue {
    fn new() -> Self {
        Self {
            tasks: HashMap::new(),
        }
    }

    fn add_task(&mut self, printer_name: &str, task: InitialTask) {
        info!(
            "【PrintTaskQueue】准备添加任务，打印机: {}, task_id: {}",
            printer_name, task.task_id
        );

        // 获取当前打印机的最大序号
        let max_sequence = self
            .tasks
            .get(printer_name)
            .map(|tasks| tasks.iter().map(|t| t.sequence).max().unwrap_or(0))
            .unwrap_or(0);

        // 创建新任务，序号在现有最大序号基础上加1
        let new_task = InitialTask {
            sequence: max_sequence + 1,
            printer: printer_name.to_string(),
            ..task
        };

        // 添加到任务列表
        self.tasks
            .entry(printer_name.to_string())
            .or_insert_with(Vec::new)
            .push(new_task);

        info!("【PrintTaskQueue】当前队列状态:");
        for (printer, tasks) in &self.tasks {
            info!("打印机 {}: {} 个任务", printer, tasks.len());
        }
    }
    // 获取任务列表（包括已完成的）
    fn get_task_list(&self, printer_name: &str) -> Vec<InitialTask> {
        info!("【PrintTaskQueue】获取任务列表，打印机: {}", printer_name);
        self.tasks
            .get(printer_name)
            .map(|tasks| tasks.clone())
            .unwrap_or_default()
    }
}

// 初始化全局打印队列
lazy_static! {
    static ref PRINT_QUEUE: Mutex<PrintTaskQueue> = Mutex::new(PrintTaskQueue::new());
}

#[derive(Debug, Clone, Deserialize)]
pub struct Document {
    #[serde(flatten)]
    pub data: Value, // 使用 Value 接收任意 JSON 数据
}

#[derive(Debug, Clone)]
pub struct InitialTask {
    pub task_id: String,
    pub document_uuid: String,
    pub document_id: String,
    pub printer: String,
    pub sequence: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BatchPrintTask {
    pub task_id: String,
    pub document_uuid: String,
    pub document_id: String,
    pub printer: String,
}

#[tauri::command]
pub async fn add_batch_print_tasks(tasks: Vec<BatchPrintTask>) -> Result<(), String> {
    let printer_name = tasks
        .first()
        .ok_or_else(|| "任务列表为空".to_string())?
        .printer
        .clone();

    info!(
        "【add_batch_print_tasks】开始添加批量打印任务，打印机: {}, 任务数量: {}",
        printer_name,
        tasks.len()
    );

    {
        let mut queue = PRINT_QUEUE.lock().await;
        for task in tasks {
            queue.add_task(
                &printer_name,
                InitialTask {
                    task_id: task.task_id,
                    document_uuid: task.document_uuid,
                    document_id: task.document_id,
                    printer: task.printer,
                    sequence: 0, // 序号会在 add_task 中自动分配
                },
            );
        }
    }

    info!("【add_batch_print_tasks】批量任务已添加到队列");
    Ok(())
}
