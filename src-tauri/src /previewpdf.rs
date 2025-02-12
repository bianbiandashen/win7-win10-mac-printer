use printpdf::*;
use crate::onixcomponents;
use crate::onixcomponents::utils::{clear_font_cache, reset_layer_defaults};
#[cfg(target_os = "macos")]
use crate::onixcomponents::utils::subset_font;
use crate::utils::get_build_version;
use crate::utils::set_image_filename_cache;
use crate::utils::{extract_image_filename, generate_random_string, get_formatted_time};
use serde_json::Value;
use std::{
    env::{self, temp_dir},
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use futures::executor::block_on;
use log::{error, info};
use reqwest::Client;
use tokio::time::timeout;
use lazy_static::lazy_static;
use tokio::sync::{Semaphore, RwLock};
use tokio::task;
use tokio::runtime::Handle;
use chrono::{ Utc, Local };

use crate::error::{CustomError, ErrorCode, System, Component};
use crate::macos;
use crate::windows10;
use crate::windows7;
use crate::windows_version;
use crate::declare;
use reqwest;
use tokio::sync::{mpsc};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
// use crate::fileserver::{ThreadExecution, EXECUTION_DATA};
use crate::harfbuzz::FontProcessor;
use rand::Rng;
use crate::harfbuzz::processor::FontError;
use tokio::sync::mpsc::error::TryRecvError;
use dashmap::DashMap;

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
    pub static ref EXECUTION_DATA: Arc<RwLock<Vec<ThreadExecution>>> = Arc::new(RwLock::new(Vec::new()));
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

    info!("【{}】【{}】【线程 {}】开始执行", task_id, method_name, thread_id);
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
        if let Some(execution) = data.iter_mut().rev().find(|exec| exec.task_id == task_id_clone && exec.method_name == method_name_clone && exec.end_time.is_none()) {
            execution.end_time = Some(end_time);
            execution.duration = Some(end_time - execution.start_time);
            let duration = execution.duration.unwrap_or(0);
            info!("【{}】【{}】【线程 {}】执行完成，耗时 {} ms", task_id_clone, method_name_clone, thread_id_clone, duration);
        } else {
            error!("【{}】【{}】未找到对应的 ThreadExecution 记录", task_id_clone, method_name_clone);
        }
    });
}


lazy_static! {
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
    // 如果系统有 8 个 CPU 核心，阻塞线程池大小为 40，那么将信号量设置为 40-80 之间可能更合适，而不是 500
    static ref PRINT_SEMAPHORE: Arc<Semaphore> = Arc::new(Semaphore::new(40));
    static ref FONT_CACHE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // static ref TASK_RESULTS: Arc<tokio::sync::Mutex<HashMap<String, mpsc::Receiver<Result<String, String>>>>> =
    //     Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    static ref TASK_STATUS: DashMap<String, Result<String, String>> = DashMap::new();
}

pub async fn download_and_save_image(task_id: &str, url: &str, save_path: &PathBuf, file_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    log_method_start(task_id, "download_and_save_image");

    if url.trim().is_empty() {
        error!("【{}】【download_and_save_image】URL 不能为空", task_id);
        return Err("URL 不能为空".into());
    }

    info!("【{}】【download_and_save_image】开始下载图片: {}", task_id, url);

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
        error!("【{}】【download_and_save_image】HTTP 请求失败: {}", task_id, response.status());
        return Err(format!("HTTP 请求失败: {}", response.status()).into());
    }

    println!("【{:?}】【download_and_save_image】save_path: {:?}", url, save_path);
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
            error!("【{}】【download_and_save_image】不支持的图片类型: {}", task_id, content_type);
            return Err(format!("不支持的图片类型: {}", content_type).into());
        }
    };

    let mut final_path = save_path.clone();
    if let Some(stem) = save_path.file_stem().and_then(|s| s.to_str()) {
        final_path.set_file_name(format!("{}.{}", stem, extension));
    } else {
        error!("【{}】【download_and_save_image】效的文件路径", task_id);
        return Err("无效的文件路径".into());
    }

    info!("【{}】【download_and_save_image】保存图片，Content-Type: {}, 路径: {:?}", task_id, content_type, final_path);

    let bytes = match timeout(Duration::from_secs(200), response.bytes()).await {
        Ok(result) => match result {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("【{}】【download_and_save_image】获取图片数据失败: {}", task_id, e);
                return Err(e.into());
            }
        },
        Err(_) => {
            error!("【{}】【download_and_save_image】获取图片数据超时", task_id);
            return Err("获取图片数据超时".into());
        }
    };

    if let Err(e) = fs::write(&final_path, &bytes) {
        error!("【{}】【download_and_save_image】写入文件失败: {}", task_id, e);
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
            error!("【{}】【process_image_component】无法创建 imgs 文件夹: {}", task_id, e);
            log_method_end(task_id, "process_image_component");
            return;
        }

        img_dir.push(&file_name);

        let extensions = ["png", "jpg", "jpeg", "webp", "gif"];
        let mut exists = false;
        for ext in extensions.iter() {
            let test_path = img_dir.with_extension(ext);
            if test_path.exists() {
                info!("【{}】【process_image_component】图片已存在: {:?}", task_id, test_path);
                exists = true;
                break;
            }
        }

        if !exists {
            info!("【{}】【process_image_component】开始下载图片: {}", task_id, url);
            if let Err(e) = download_and_save_image(task_id, url, &img_dir, &file_name).await {
                error!("【{}】【process_image_component】下载图片失败: {}", task_id, e);
            }
        }
    } else {
        error!("【{}】【process_image_component】无法从 URL 提取文件名: {}", task_id, url);
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
) {
    log_method_start(task_id, "execute_component_main");

    info!(
        "【{}】【execute_component_main】subset_font_path:确保进入subset_font_path有数据 {}",
        task_id, subset_font_path
    );
    let execute_component_main_start_time = Instant::now();
    let data_list = match json.as_array() {
        Some(list) => list,
        None => {
            error!("【{}】【execute_component_main】输入数据不是有效的数组", task_id);
            log_method_end(task_id, "execute_component_main");
            return;
        }
    };

    for (index, data) in data_list.iter().enumerate() {
        let (page, layer) = if index == 0 {
            (page1, layer1)
        } else {
            let (current_page, current_layer_index) = doc.add_page(page_width, page_height, format!("Layer {}", index + 1));
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
                        error!("【{}】【execute_component_main】componentName 无效或缺失", task_id);
                        continue;
                    }
                };

                if let Some(props) = child.get("props") {
                    if let Some(cover) = props.get("cover") {
                        if let Some(url) = cover.get("url").and_then(|u| u.as_str()) {
                            let task_id_clone = task_id.to_string();
                            let url_clone = url.to_string();

                            // 生成下载任务，但不等待其��成
                            // tokio::spawn(async move {
                            //     process_image_component(&task_id_clone, &url_clone).await;
                            // });
                            process_image_component(&task_id_clone, &url_clone).await;
                        }
                    }
                }

                let json_str = match serde_json::to_string(child) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("【{}】【execute_component_main】序列化组件失败: {}", task_id, e);
                        continue;
                    }
                };

                let draw_component_start_time = Instant::now();

                match component_name {
                    "OnixBarleyElectronicBarcode" => {
                        onixcomponents::OnixBarleyElectronicBarcode::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone(), gap_width);
                    }
                    "OnixBarleyElectronicBarcodeVertical" => {
                        onixcomponents::OnixBarleyElectronicBarcodeVertical::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone(), gap_height);
                    }
                    "OnixBarleyElectronicImage" => {
                        onixcomponents::OnixBarleyElectronicImage::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone());
                    }
                    "OnixBarleyElectronicIsv" => {
                        onixcomponents::OnixBarleyElectronicIsv::draw(&current_layer, &json_str, page_height);
                    }
                    "OnixBarleyElectronicLine" => {
                        onixcomponents::OnixBarleyElectronicLine::draw(&current_layer, &json_str, page_height);
                    }
                    "OnixBarleyElectronicLineVertical" => {
                        onixcomponents::OnixBarleyElectronicLineVertical::draw(&current_layer, &json_str, page_height);
                    }
                    "OnixBarleyElectronicQrcode" => {
                        onixcomponents::OnixBarleyElectronicQrcode::draw(&current_layer, &json_str, page_height);
                    }
                    "OnixBarleyElectronicRectangle" => {
                        onixcomponents::OnixBarleyElectronicRectangle::draw(&current_layer, &json_str, page_height);
                    }
                    "OnixBarleyElectronicText" => {
                        onixcomponents::OnixBarleyElectronicText::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone());
                    }
                    "OnixBarleyElectronicTextVertical" => {
                        onixcomponents::OnixBarleyElectronicTextVertical::draw(&current_layer, &json_str, Some(&doc), page_height, subset_font_path.clone());
                    }
                    _ => {
                        info!("【TaskID: {:?}, 线程ID: {:?}】【{}】【execute_component_main】未找到匹配的组件: {}", task_id, std::thread::current().id(), component_name, component_name);
                    }
                }
                info!("【TaskID: {:?}, 线程ID: {:?}】【{}】【execute_component_main】绘制组件耗时: {:?}", task_id, std::thread::current().id(), component_name, draw_component_start_time.elapsed());
                reset_layer_defaults(&current_layer);
            }
        }
    }

    info!("【TaskID: {:?}, 线程ID: {:?}】【execute_component_main】绘制组件总耗时: {:?}", task_id, std::thread::current().id(), execute_component_main_start_time.elapsed());
    log_method_end(task_id, "execute_component_main");
}


/**
 * 函数功能
 * 1. 遍历 JSON 中的 children，收集文本组里的文本内容，拼凑成一个完整的文本
 * 2. 使用 subset_font 从Medium字体文件中子集化出完整文本对应的字体
 * 3. 返回子集化字体的路径
 */
pub fn collect_text_from_json(json: &Value) -> Result<String, String> {
    let collect_text_start_time = Instant::now();
    let mut unique_chars = std::collections::HashSet::new();
    unique_chars.extend("你的生活指南ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz*1234567890".chars());
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
        "OnixBarleyElectronicBarcodeVertical"
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
                            if let Some(value) = content_section.get("value").and_then(|v| v.as_str()) {
                                unique_chars.extend(value.chars());
                            }
                        }
                    }
                }
            }
        }

    }
    let text: String = unique_chars.into_iter().collect();
    info!("【collect_text_from_json】收集文本耗时: {:?}",  collect_text_start_time.elapsed());

    let create_subset_font_start_time = Instant::now();
    let mut resource_dir = env::temp_dir();
    resource_dir.push("xhs-printer-fonts");
    resource_dir.push("SiYuanHeiTi");
    resource_dir.push("Medium.otf");
    info!("【collect_text_from_json】resource_dir目录: {:?}",  resource_dir);
    let font_path = resource_dir.to_str().unwrap();
    info!("【collect_text_from_json】resource_dir字体path2: {:?}",  font_path);
    // 创建子集字体的目录
    let mut subset_dir = PathBuf::from(font_path);
    subset_dir.pop(); // 移除文件名
    subset_dir.push("subsets"); // 添加 subsets 子目录

    // 确保子集目录存在
    if !subset_dir.exists() {
        std::fs::create_dir_all(&subset_dir)
            .expect("Failed to create subsets directory");
    }
    // 使用 rand 生成随机数
    let random_number: u64 = rand::thread_rng().gen();
    let subset_name = format!("subset_{}.otf", random_number);
    let subset_path = subset_dir.join(&subset_name);
    let subset_path_str = subset_path.to_str().unwrap();
    info!("【collect_text_from_json】创建子集字体path1: {:?}",  subset_path_str);
    let subset_font_path = match FontProcessor::new()
        .and_then(|processor| {

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
        Ok(_) => {
            subset_path_str.to_string()
        }
        Err(_) => {
            "".to_string()
        }
    };
    info!("【collect_text_from_json】创建子集字体耗时: {:?}",  create_subset_font_start_time.elapsed());
    info!("【collect_text_from_json】创建子集字体path: {:?}",  subset_font_path.to_string());
    Ok(subset_font_path.to_string())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PrintResult {
    pub taskid: String,
    pub document_uuid: String,
}


#[tauri::command]
pub async fn start_preview_print_pdf(taskid: String, printdata: String, options: Value) -> Result<HashMap<String, String>, String> {
    use crate::onixcomponents::utils::get_font_path;
    let start_time = Instant::now();
    let thread_id = format!("{:?}", std::thread::current().id());
    log_method_start(&taskid, "start_print_pdf");
    info!("[{}]【{}】【start_print_pdf】[线程-{}] 开始生成PDF", get_build_version(), taskid, thread_id);

    let task_id = taskid.clone();
    let task_id_for_name = taskid.clone();
    let document_uuid = options.get("documentuuid")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // Ensure the document_uuid is valid
    if document_uuid.is_empty() {
        return Err("无效的 document_uuid".to_string());
    }


    // 克隆 document_uuid 以在异步任务中使用
    let document_uuid_clone = document_uuid.clone();

    // 启动异步任务
    tokio::spawn(async move {
        let spawn_start_time = Instant::now();
        let spawn_thread_id = format!("{:?}", std::thread::current().id());
        let task_id_for_spawn = task_id.clone();
        info!("【{}】【start_print_pdf】[线程-{}] 开始异步任务", task_id_for_spawn, spawn_thread_id);

        // 使用了 PRINT_SEMAPHORE.acquire().await 来限制并发打印任务的数量

        let permit = match PRINT_SEMAPHORE.acquire().await {
            Ok(permit) => {
                info!("【{}】【start_print_pdf】[线程-{}] 获取信号量成功，耗时: {:?}",
                    task_id_for_spawn, spawn_thread_id, spawn_start_time.elapsed());
                permit
            },
            Err(e) => {
                let err = format!("无法获取打印许可: {}", e);
                error!("【{}】【start_print_pdf】[线程-{}] {}", task_id_for_spawn, spawn_thread_id, err);
                return;
            }
        };

        let task_id_for_blocking = task_id.clone();
        let printdata = printdata.clone();
        let options = options.clone();
        let task_id_for_spawn_clone = task_id_for_spawn.clone();
        let document_uuid_for_task = document_uuid.clone(); // 使用 document_uuid


        // Tokio 的阻塞线程池默认并不会为每个 permit 创建一个线程。Tokio 的 spawn_blocking
        // 使用的是一个有限的阻塞线程池，默认情况下，线程池的大小是当前系统 CPU 核心数的 5 倍。
        // 例如，在 8 核 CPU 的系统上，默认线程池大小为 40


        let result = tokio::task::spawn_blocking(move || {
            let blocking_start_time = Instant::now();
            let blocking_thread_id = format!("{:?}", std::thread::current().id());
            info!("【{}】【start_print_pdf】[线程-{}] 开始阻塞任务", task_id_for_blocking, blocking_thread_id);

            let clear_cache_start = Instant::now();
            clear_font_cache();
            info!("【{}】【start_print_pdf】[线程-{}] 清理字体缓存完成，耗时: {:?}", task_id_for_blocking, blocking_thread_id, clear_cache_start.elapsed());

            // 获取页面尺寸
            let size_start = Instant::now();
            let (page_width, page_height) = match options.get("size") {
                Some(size) => {
                    let size_str = size.as_str()
                        .ok_or_else(|| "size 选项不是有效的字符串".to_string())?;
                    let parts: Vec<&str> = size_str.split('x').collect();
                    if parts.len() == 2 {
                        let width = parts[0].parse::<f32>()
                            .map_err(|_| "无法解析页面宽度".to_string())?;
                        let height = parts[1].parse::<f32>()
                            .map_err(|_| "无法解析页面高度".to_string())?;
                        info!("【{}】【start_print_pdf】[线程-{}] 页面尺寸: {}x{}, 解析耗时: {:?}",
                            task_id_for_blocking, blocking_thread_id, width, height, size_start.elapsed());
                        (Mm(width), Mm(height))
                    } else {
                        info!("【{}】【start_print_pdf】[线程-{}] 使用默认尺寸: 76x130",
                            task_id_for_blocking, blocking_thread_id);
                        (Mm(76.0), Mm(130.0))
                    }
                }
                None => {
                    info!("【{}】【start_print_pdf】[线程-{}] 使用默认尺寸: 76x130",
                        task_id_for_blocking, blocking_thread_id);
                    (Mm(76.0), Mm(130.0))
                }
            };

            let pdf_start = Instant::now();
            // let random_string = generate_random_string(8);
            let formatted_time = get_formatted_time(Utc::now());
            let pdf_name = format!("{}_{}", formatted_time, task_id_for_name);

            info!("【{}】【start_print_pdf】[线程-pdf_name{}", task_id_for_blocking, pdf_name);

            info!("【{}】【start_print_pdf】[线程-{}] 开始创建PDF文档", task_id_for_blocking, blocking_thread_id);
            // 创建PDF
            let doc_result = PdfDocument::new(&pdf_name, page_width, page_height, "Layer 1");
            let (doc, page1, layer1) = doc_result;
            info!("【{}】【start_print_pdf】[线程-{}] PDF文档创建成功，耗时: {:?}", task_id_for_blocking, blocking_thread_id, pdf_start.elapsed());

            let json_start = Instant::now();
            let json: Value = serde_json::from_str(&printdata)
                .map_err(|e| format!("JSON 解析失败: {}", e))?;
            info!("【{}】【start_print_pdf】[线程-{}] JSON解析成功，耗时: {:?}",
            task_id_for_blocking, blocking_thread_id, json_start.elapsed());

            let font_start = Instant::now();
            let subset_font_path = if options.get("allfontmode").and_then(|v| v.as_bool()).unwrap_or(false) {
                info!("【{}】【start_print_pdf】[线程-{}] 启用全字体模式", task_id_for_spawn, blocking_thread_id);
                // let path_buf = PathBuf::from(get_font_path("SimHei, Arial, sans-serif", "light"));
                let font_type = options.get("fontType").and_then(|v| v.as_str()).unwrap_or("SimHei, Arial, sans-serif");
                let path_buf = PathBuf::from(get_font_path("SimHei, Arial, sans-serif", font_type));
                path_buf.to_str()
                        .expect("Path is not valid UTF-8")
                        .to_string()
            } else {
                match collect_text_from_json(&json) {
                    Ok(path) => {

                        info!("【{}】【start_print_pdf】[线程-{}] 字体子集化成功，耗时: {:?}",
                        task_id_for_spawn, blocking_thread_id, font_start.elapsed());
                        PathBuf::from(path).to_str().expect("Path is not valid UTF-8").to_string()
                    },
                    Err(e) => {
                        error!("【{}】【start_print_pdf】[线程-{}] 字体子集化失败: {}, 耗时: {:?}",
                            task_id_for_spawn, blocking_thread_id, e, font_start.elapsed());
                        String::new()
                    }
                }
            };
            info!("【collect_text_from_json】创建子集字体path3: {:?}",  subset_font_path);

            let runtime_start = Instant::now();
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("创建运行时失败: {}", e))?;
            info!("【{}】【start_print_pdf】[线程-{}] 运行时创建成功，耗时: {:?}",
            task_id_for_blocking, blocking_thread_id, runtime_start.elapsed());

            let component_start = Instant::now();
            info!("【{}】【start_print_pdf】[线程-{}] 开始执行组件绘制", task_id_for_blocking, blocking_thread_id);
            info!("【start_print_pdf_task】[线程-{}] options: {:?}", blocking_thread_id, options);
            let gap_width = options.get("gapwidth").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let gap_height = options.get("gapheight").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            rt.block_on(execute_component_main(
                &task_id_for_blocking,
                &json,
                &doc,
                page_width,
                page_height,
                (page1, layer1),
                subset_font_path,
                gap_width,
                gap_height,
            ));
            info!("【{}】【start_print_pdf】[线程-{}] 组件绘制完成，耗时: {:?}",
                task_id_for_blocking, blocking_thread_id, component_start.elapsed());

            let save_start = Instant::now();
            let mut pdf_bytes = Vec::new();
            {
                let mut writer = BufWriter::new(&mut pdf_bytes);
                doc.save(&mut writer)
                    .map_err(|e| format!("保存 PDF 数据失败: {}", e))?;
            }

            let mut file_path = temp_dir();
            file_path.push(format!("{}.pdf", pdf_name));
            let temp_path = file_path.with_extension("tmp");

            File::create(&temp_path)
                .and_then(|mut file| file.write_all(&pdf_bytes))
                .map_err(|e| format!("写入文件失败: {}", e))?;

            fs::rename(&temp_path, &file_path)
                .map_err(|e| format!("重命名文件失败: {}", e))?;

            info!("【{}】【start_print_pdf】[线程-{}] PDF保存完成，耗时: {:?}",
                task_id_for_blocking, blocking_thread_id, save_start.elapsed());

            info!("【{}】【start_print_pdf】[线程-{}] 任务完成，总耗时: {:?}, 当前时间: {:?}, 文件路径: {:?}",
                task_id_for_blocking, blocking_thread_id, blocking_start_time.elapsed(), Local::now().format("%Y-%m-%d %H:%M:%S").to_string(), file_path.to_string_lossy());

            Ok(file_path.to_string_lossy().into_owned())
        })
        .await
        .unwrap_or_else(|e| Err(format!("线程执行错误: {}", e)));
        info!("【{}】【start_print_pdf】准备发送任务结果", task_id);

        TASK_STATUS.insert(document_uuid_for_task.clone(), result.clone());
        info!("taskid {} 任务结果已保存，document_uuid: {}", task_id, document_uuid_for_task);

        info!("异步任务结束，总耗时: {:?}", spawn_start_time.elapsed());
    });

    // 使用 HashMap 返回包含 taskid 和 document_uuid 的键值对
    let mut result_map = HashMap::new();
    result_map.insert("taskid".to_string(), taskid);
    result_map.insert("document_uuid".to_string(), document_uuid_clone); // 使用克隆值

    Ok(result_map)
}

#[tauri::command]
pub async fn check_preview_status(taskid: String) -> Result<String, String> {
    let check_start = Instant::now();
    info!("【{}】【check_print_status】开始检查任务状态", taskid);
    let result =  match TASK_STATUS.get(&taskid) {
        Some(status) => {
            info!("【{}】【check_print_status】找到任务结果", taskid);
            status.clone()
        }
        None => {
            info!("【{}】【check_print_status】任务进行中", taskid);
            Err("任务正在进行中".to_string())
        }
    };
    // 打印结果
    info!("【{}】【check_print_status】找到已完成的任务结果: {:?}", taskid, result);
    log_method_end(&taskid, "check_print_status");
    result

    // info!("【{}】【check_print_status】获取锁耗时={:?}, 当前任务数={}, 任务列表={:?}",
    //     taskid, lock_time, current_tasks.len(), current_tasks);

    // if let Some(rx) = task_results.get_mut(&taskid) {
    //     let try_recv_result = rx.try_recv();
    //     info!("【{}】【check_print_status】接收结果: {:?}", taskid, try_recv_result);

    //     match try_recv_result {
    //         Ok(result) => {
    //             // 保存任务结果
    //             let mut task_status = TASK_STATUS.lock().await;
    //             task_status.insert(taskid.clone(), result.clone());
    //             info!("【{}】【check_print_status】任务完成并保存结果", taskid);
    //             result
    //         }
    //         Err(TryRecvError::Empty) => {
    //             info!("【{}】【check_print_status】任务进行中: 已等待时间={:?}",
    //                 taskid, check_start.elapsed());
    //             Err("任务正在进行中".to_string())
    //         }
    //         Err(TryRecvError::Disconnected) => {
    //             // 检查是否有保存的结果
    //             let task_status = TASK_STATUS.lock().await;
    //             if let Some(result) = task_status.get(&taskid) {
    //                 info!("【{}】【check_print_status】通道已断开，但找到之前的结果", taskid);
    //                 return result.clone();
    //             }
    //             error!("【{}】【check_print_status】通道已断开且无历史结果: 总耗时={:?}",
    //                 taskid, check_start.elapsed());
    //             Err("任务执行失败".to_string())
    //         }
    //     }
    // } else {
    //     error!("【{}】【check_print_status】未找到任务: 当前任务列表={:?}, 总耗时={:?}",
    //         taskid, current_tasks, check_start.elapsed());
    //     Err("未找到对应的任务".to_string())
    // }
}


