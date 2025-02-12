// src/onixcomponents/OnixBarleyElectronicBarcodeVertical.rs
use printpdf::*;
use serde_json::Value;
use crate::onixcomponents::utils::{convert_to_pdf_coordinates, get_component_visible, get_font_path, get_or_add_font, mm2pt, mm2px, pt2mm, px2mm};
use crate::onixcomponents::OnixBarleyElectronicText::{calculate_line_width,get_char_width};
use std::fs;
use log::{error, info};
use rxing::{
    oned::Code128Writer, // 引入 Code128Writer，用于生成 Code 128 条形码
    BarcodeFormat,       // 引入 BarcodeFormat 枚举，用于指定条形码格式
    Writer,              // 引入 Writer trait，Code128Writer 需要实现该 trait
};

/// 绘制条形码或二维码
pub fn draw(current_layer: &PdfLayerReference, json_str: &str, doc: Option<&PdfDocumentReference>, page_height: Mm, subset_font_path_str: String, gap_height: f32) {
    // 解析 JSON 字符串，获取条形码的配置信息
    let json: Value = match serde_json::from_str(json_str) {
        Ok(data) => data,
        Err(e) => {
            println!("JSON 解析错误: {:?}", e);
            return;
        }
    };

    let visible = get_component_visible(&json);

    if !visible {
        return;
    }

    // 提取用于条形码内容的字符串信息
    let content = json.get("props")
        .and_then(|props| props.get("contentSection"))
        .and_then(|section| section.get("value"))
        .and_then(|value| value.as_str())
        .unwrap_or("http://www.xiaohongshu.com");
    println!("条形码内容: {}", content);

    // 从 JSON 获取条形码的宽度配置，默认值为50.0毫米
    let width_mm = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("width"))
        .and_then(|width| width.as_f64())
        .unwrap_or(50.0);

    // 从 JSON 获取条形码的高度配置，默认值为50.0毫米
    let height_mm = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("height"))
        .and_then(|height| height.as_f64())
        .unwrap_or(50.0);

    // 计算条形码在页面上的顶部位置
    let top = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("top"))
        .and_then(|top| top.as_f64())
        .map(|top| convert_to_pdf_coordinates(page_height, Mm(top as f32), Some(Mm(height_mm as f32))).0)
        .unwrap_or(0.0) as f32;

    // 从 JSON 获取条形码左侧的偏移位置，默认值为0.0
    let left = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("left"))
        .and_then(|left| left.as_f64())
        .unwrap_or(0.0) as f32;

    // 确定是否显示条形码下方的内容文字
    let show_content = json.get("props")
        .and_then(|props| props.get("materialSection"))
        .and_then(|material| material.get("isDescribeShow"))
        .and_then(|isDescribeShow| isDescribeShow.as_bool())
        .unwrap_or(true);

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

    // 绘制生成的条形码到PDF
    draw_barcode(
        current_layer,
        &encoded_bits,
        Mm(left),
        Mm(top),
        Mm(width_mm as f32),
        Mm(height_mm as f32),
        content,
        show_content,
        doc,
        subset_font_path_str,
        gap_height
    );

    println!("条形码已绘制，内容: {}", content);
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
    gap_height: f32
) {
    // 常量定义，与横向条码保持一致
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
    let padding_x = width.0 * PADDING_RATIO;
    let padding_y = height.0 * PADDING_RATIO - gap_height;
    let available_width = width.0 - (padding_x * 2.0) - text_height; // 减去文本占用空间
    let available_height = height.0 - (padding_y * 2.0);

    // 查找黑条位置
    let (first_black, last_black) = encoded.iter()
        .enumerate()
        .filter(|(_, &module)| module == 1)
        .map(|(i, _)| i)
        .fold((encoded.len(), 0), |(min, max), i| (min.min(i), max.max(i)));

    println!("【桌面端-打印组件OnixBarleyElectronicBarcodeVertical】- 第一个黑条位置: {}, 最后一个黑条位置: {}, 条码总数: {}, 内容: {}", first_black, last_black, encoded.len(), content);

    // 计算条形码尺寸和位置
    let barcode_height = last_black - first_black + 1;
    let bar_height = available_height / barcode_height as f32;
    let total_height = bar_height * barcode_height as f32;
    let start_y = y.0 + (height.0 - total_height) / 2.0;

    // 设置颜色
    layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));

    let mut current_y = start_y;
    let mut bar_start: Option<f32> = None;

    // 绘制条形码
    for &module in &encoded[first_black..=last_black] {
        if module == 1 {
            bar_start.get_or_insert(current_y);
        } else if let Some(start) = bar_start.take() {
            layer.add_rect(Rect::new(
                Mm(x.0 + padding_x + text_height),
                Mm(start + (bar_height * PADDING_RATIO)),
                Mm(x.0 + padding_x + text_height + available_width),
                Mm(current_y - (bar_height * PADDING_RATIO))
            ));
        }
        current_y += bar_height;
    }

    // 处理最后一个黑条
    if let Some(start) = bar_start {
        layer.add_rect(Rect::new(
            Mm(x.0 + padding_x + text_height),
            Mm(start + (bar_height * PADDING_RATIO)),
            Mm(x.0 + padding_x + text_height + available_width),
            Mm(current_y - (bar_height * PADDING_RATIO))
        ));
    }

    // 绘制文本
    if show_content {
        if let Some(doc) = doc {
            layer.save_graphics_state();
            println!("【桌面端-OnixBarleyElectronicBarcodeVertical】- 开始绘制文本, 字体path: {:?}",  subset_font_path_str);
            let font_path = get_font_path("SimHei, Arial, sans-serif", "normal");
            let (font, subset_font_path) = get_or_add_font(doc, subset_font_path_str, font_path);

            if let Ok(font_data) = fs::read(&subset_font_path) {
                let font_size_pt = mm2pt(font_size_mm);
                let text_width = pt2mm(calculate_line_width(
                    content,
                    font_size_pt,
                    &font_data,
                    0.2,
                    Mm(total_height)
                ));

                let text_x = x.0 + padding_x;
                let text_y = y.0 + (height.0 / 2.0) + (text_width / 2.0) as f32;

                // 设置文本旋转变换
                let text_x_pt = mm2pt(text_x as f64) as f32;
                let text_y_pt = mm2pt(text_y as f64) as f32;

                layer.set_ctm(CurTransMat::Translate(Pt(text_x_pt), Pt(text_y_pt)));
                layer.set_ctm(CurTransMat::Rotate(-90.0));
                layer.set_ctm(CurTransMat::Translate(
                    Pt(-text_x_pt - (font_size_pt * 2.0) as f32),
                    Pt(-text_y_pt)
                ));

                layer.use_text(content, font_size_pt as f32, Mm(text_x), Mm(text_y), &font);
            } else {
                error!("【桌面端-打印组件OnixBarleyElectronicText】- 无法读取字体文件");
            }

            layer.restore_graphics_state();
        }
    }
}
