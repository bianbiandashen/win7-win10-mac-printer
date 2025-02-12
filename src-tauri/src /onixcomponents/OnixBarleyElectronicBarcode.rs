use printpdf::*; // 引入 printpdf 库，用于创建和操作 PDF 文档

use rxing::{
    oned::Code128Writer, // 引入 Code128Writer，用于生成 Code 128 条形码
    BarcodeFormat,       // 引入 BarcodeFormat 枚举，用于指定条形码格式
    Writer,              // 引入 Writer trait，Code128Writer 需要实现该 trait
}; // 使用 rxing 库来生成条形码

use serde_json::Value; // 从 serde_json 中引入 Value 类型，用于处理 JSON 数据结构

use crate::onixcomponents::{utils::{
    convert_to_pdf_coordinates, get_component_visible, get_font_path, get_or_add_font, mm2pt, mm2px, pt2mm, px2mm                     // 引入单位转换函数，将毫米转换为像素
}, OnixBarleyElectronicText::calculate_line_width}; // 引入工具函数，用于坐标和单位转换

use log::{error, info}; // 引入日志宏，用于记录错误和信息日志

use lopdf::{content::Operation, Object}; // 从 lopdf 中引入 Operation 和 Object，用于 PDF 内容操作

use std::fs; // 引入 std::fs 模块，用于文件操作

// 绘制条形码的主函数
pub fn draw(
    current_layer: &PdfLayerReference, // 当前 PDF 图层的引用
    json_str: &str,                    // 包含条形码数据和属性的 JSON 字符串
    doc: Option<&PdfDocumentReference>, // 可选的 PDF 文档引用
    page_height: Mm,                   // PDF 页面的高度，以毫米为单位
    subset_font_path_str: String,          // 子集字体路径
    gap_width: f32,                    // 条形码之间的间隔宽度
) {
    // 记录开始解析 JSON 数据的信息日志
    info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 开始解析 JSON 数据");

    // 尝试将 JSON 字符串解析为 serde_json::Value
    let json: Value = match serde_json::from_str(json_str) {
        Ok(data) => {
            // 如果解析成功，记录成功日志并返回解析后的数据
            info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- JSON 解析成功");
            data
        }
        Err(e) => {
            // 如果解析失败，记录错误日志并退出函数
            error!(
                "【桌面端-打印组件OnixBarleyElectronicBarcode】- JSON 解析错误: {:?}",
                e
            );
            return;
        }
    };

    let visible = get_component_visible(&json);

    if !visible {
        return;
    }

    // 从解析后的 JSON 数据中提取 "props" 字段
    let props = match json.get("props") {
        Some(p) => {
            // 如果 "props" 存在，记录成功日志并继续
            info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 获取到 props 字段");
            p
        }
        None => {
            // 如果 "props" 不存在，记录错误日志并退出函数
            error!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 找不到 props 字段");
            return;
        }
    };

    // 从 JSON 数据中获取条形码的内容
    let content = props
        .get("contentSection") // 访问 "contentSection" 字段
        .and_then(|section| section.get("value")) // 访问 "value" 字段
        .and_then(|value| value.as_str()) // 将值转换为字符串
        .unwrap_or("http://www.xiaohongshu.com"); // 如果未指定，则使用默认内容
    // 记录获取到的内容
    info!(
        "【桌面端-打印组件OnixBarleyElectronicBarcode】- 获取内容为: {}",
        content
    );

    // 提取基本属性，例如宽度和高度
    let basic = match props.get("basic") {
        Some(b) => {
            // 如果 "basic" 存在，记录成功日志并继续
            info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 获取到 basic 字段");
            b
        }
        None => {
            // 如果 "basic" 不存在，记录错误日志并退出函数
            error!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 找不到 basic 字段");
            return;
        }
    };

    // 获取宽度（毫米），如果未指定，则默认为 50.0 mm
    let width_mm = basic
        .get("width")
        .and_then(|w| w.as_f64())
        .unwrap_or(50.0);
    // 获取高度（毫米），如果未指定，则默认为 50.0 mm
    let height_mm = basic
        .get("height")
        .and_then(|h| h.as_f64())
        .unwrap_or(50.0) as f32;
    // 记录设置的宽度和高度
    info!(
        "【桌面端-打印组件OnixBarleyElectronicBarcode】- 设置条形码宽度: {}, 高度: {}",
        width_mm, height_mm
    );

    // 获取左边距并转换为 f32 类型，默认为 0.0 mm
    let left = basic
        .get("left")
        .and_then(|l| l.as_f64())
        .unwrap_or(0.0) as f32;
    // 记录设置的左边距
    info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 设置左边距: {}", left);

    // 获取上边距并转换为 f32 类型，默认为 0.0 mm
    let top = basic
        .get("top")
        .and_then(|l| l.as_f64())
        .unwrap_or(0.0) as f32;
    // 记录设置的上边距
    info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 设置上边距: {}", top);

    // 在转换坐标之前记录顶部位置
    info!(
        "【桌面端-打印组件OnixBarleyElectronicBarcode】- 设置顶部位置: {}",
        top
    );
    // 将顶部位置转换为 PDF 坐标系
    let new_top = convert_to_pdf_coordinates(page_height, Mm(top), Some(Mm(height_mm)));

    // 打印转换后的顶部坐标用于调试
    println!("new top: {:?}====>", new_top);

    // 确定是否显示条形码内容文本
    let show_content = props
        .get("materialSection")
        .and_then(|material| material.get("isDescribeShow"))
        .and_then(|is_describe_show| is_describe_show.as_bool())
        .unwrap_or(true);
    // 记录是否显示内容
    info!(
        "【桌面端-打印组件OnixBarleyElectronicBarcode】- 是否显示内容: {}",
        show_content
    );

    // 初始化 Code 128 格式的条形码生成器
    let writer = Code128Writer;
    // 生成条形码的位矩阵
    let encoded = match writer.encode(
        content,                           // 要编码的内容
        &BarcodeFormat::CODE_128,          // 指定条形码格式为 Code 128
        mm2px(width_mm) as i32,            // 将宽度从毫米转换为像素
        1,                                 // 设置高度为 1，因为我们只需要宽度数据
    ) {
        Ok(bit_matrix) => {
            // 如果生成成功，记录成功日志并继续
            info!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 条形码生成成功");
            bit_matrix
        }
        Err(e) => {
            // 如果生成失败，记录错误日志并退出函数
            error!(
                "【桌面端-打印组件OnixBarleyElectronicBarcode】- 条形码生成错误: {:?}",
                e
            );
            return;
        }
    };

    // 获取条形码的宽度（位数）
    let width = encoded.width() as usize;
    // 初始化一个向量来存储编码后的位数据
    let mut encoded_bits: Vec<u8> = Vec::with_capacity(width);

    // 遍历位矩阵的每一位
    for x in 0..width {
        // 如果位是 1（黑色），则添加 1；否则添加 0
        encoded_bits.push(if encoded.get(x as u32, 0) { 1 } else { 0 });
    }

    // 调用函数在 PDF 图层上绘制条形码
    draw_barcode(
        current_layer,                  // PDF 图层引用
        &encoded_bits,                  // 编码后的位数据
        Mm(left),                       // 左边距，以毫米为单位
        new_top,                        // 转换后的顶部坐标
        Mm(width_mm as f32),            // 条形码的宽度，以毫米为单位
        Mm(height_mm),                  // 条形码的高度，以毫米为单位
        content,                        // 条形码的内容
        show_content,                   // 是否显示内容文本
        doc,                            // PDF 文档引用
        subset_font_path_str,
        gap_width,                      // 条形码之间的间隔宽度
    );

    // 记录条形码已绘制的信息日志
    info!(
        "【桌面端-打印组件OnixBarleyElectronicBarcode】- 条形码已绘制，内容: {}",
        content
    );
}

fn draw_barcode(
    layer: &PdfLayerReference,
    encoded: &[u8],
    x: Mm,
    y: Mm,
    width: Mm,
    height: Mm,
    content: &str,
    show_content: bool,
    doc: Option<&PdfDocumentReference>,
    subset_font_path_str: String,
    gap_width: f32,
) {
    // 常量定义
    const FONT_SIZE_BASE: f64 = 14.0;
    const PADDING_RATIO: f32 = 0.08;
    const LINE_HEIGHT_RATIO: f64 = 1.2;

    // 文本高度计算
    let font_size_mm = px2mm(FONT_SIZE_BASE);
    let text_height = if show_content {
        (font_size_mm * LINE_HEIGHT_RATIO) as f32
    } else {
        0.0
    };
    // padding和可用空间计算
    let padding_x = width.0 * PADDING_RATIO - gap_width;
    let padding_y = height.0 * PADDING_RATIO;
    let available_width = width.0 - (padding_x * 2.0);
    let available_height = height.0 - (padding_y * 2.0) - text_height;

    // 查找黑条位置
    let (first_black, last_black) = encoded.iter()
        .enumerate()
        .filter(|(_, &module)| module == 1)
        .map(|(i, _)| i)
        .fold((encoded.len(), 0), |(min, max), i| (min.min(i), max.max(i)));

    println!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 第一个黑条位置: {}, 最后一个黑条位置: {}, 条码总数: {}, 内容: {}", first_black, last_black, encoded.len(), content);

    // 计算条形码尺寸和位置
    let barcode_width = last_black - first_black + 1;
    let bar_width = available_width / barcode_width as f32;
    let total_width = bar_width * barcode_width as f32;
    let start_x = x.0 + (width.0 - total_width) / 2.0;

    // 设置颜色并绘制条形码
    layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));

    let mut current_x = start_x;
    let mut bar_start: Option<f32> = None;

    // 绘制条形码
    for &module in &encoded[first_black..=last_black] {
        if module == 1 {
            bar_start.get_or_insert(current_x);
        } else if let Some(start) = bar_start.take() {
            layer.add_rect(Rect::new(
                Mm(start + (bar_width * PADDING_RATIO)),
                Mm(y.0 + padding_y + text_height),
                Mm(current_x - (bar_width * PADDING_RATIO)),
                Mm(y.0 + padding_y + text_height + available_height) // 修正这里：直接使用 available_height
            ));
        }
        current_x += bar_width;
    }

    // 遇到白条才渲染黑条，所以会出现最后一个黑条后没有白条的情况，在此处补全
    if let Some(start) = bar_start {
        layer.add_rect(Rect::new(
            Mm(start + (bar_width * PADDING_RATIO)),
            Mm(y.0 + padding_y + text_height),
            Mm(current_x - (bar_width * PADDING_RATIO)),
            Mm(y.0 + padding_y + text_height + available_height) // 修正这里：直接使用 available_height
        ));
    }

    // 绘制文本
    if show_content {
        if let Some(doc) = doc {
            println!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 开始绘制文本, 字体path: {:?}",  subset_font_path_str);
            let font_path = get_font_path("SimHei, Arial, sans-serif", "normal");
            let (font, subset_font_path) = get_or_add_font(doc, subset_font_path_str, font_path);

            if let Ok(font_data) = fs::read(&subset_font_path) {
                let mut final_font_size_pt = mm2pt(font_size_mm);
                let mut text_width = 0.0;
                loop {
                    text_width = calculate_line_width(
                        content,
                        final_font_size_pt,
                        &font_data,
                        0.2,
                        width
                    );
                    if text_width <= total_width as f64 {
                        break;
                    }
                    final_font_size_pt *= 0.9;
                }
                // 计算文本起始位置
                let text_x = start_x + (width.0 - text_width as f32) / 2.0;
                let text_y = y.0 + padding_y;
                // 绘制文本
                layer.use_text(
                    content,
                    final_font_size_pt as f32,
                    Mm(text_x),
                    Mm(text_y),
                    &font
                );
            } else {
                error!("【桌面端-打印组件OnixBarleyElectronicBarcode】- 无法读取字体文件");
            }
        }
    }
}
