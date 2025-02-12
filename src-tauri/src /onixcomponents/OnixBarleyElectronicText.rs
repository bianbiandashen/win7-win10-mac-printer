// src/onixcomponents/OnixBarleyElectronicText.rs
use printpdf::*;
use std::fs;
use crate::onixcomponents::utils::{convert_to_pdf_coordinates, get_component_visible, get_font_path, get_or_add_font, mm2pt, pt2mm, px2mm, TEXT_RESIZE_RATIO};
use rusttype::{Font, Scale};
use serde_json::Value;
use log::{info, error};
use crate::utils::hex_to_rgb;
use std::cmp::max;

/// 绘制文本
pub fn draw(current_layer: &PdfLayerReference, json_str: &str, doc: Option<&PdfDocumentReference>, page_height: Mm, subset_font_path: String) {
    // 日志记录 subset_font_path 的值
    info!(
        "【桌面端-打印组件OnixBarleyElectronicText】- subset_font_path: {}",
        subset_font_path
    );

    let data: Value = match serde_json::from_str(json_str) {
        Ok(data) => data,
        Err(e) => {
            error!("【桌面端-打印组件OnixBarleyElectronicText】- 解析 JSON 时出错: {:?}", e);
            return;
        }
    };

    let visible = get_component_visible(&data);

    let color = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("color"))
        .and_then(|color| color.as_str())
        .unwrap_or("");

    let background_color = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("backgroundColor"))
        .and_then(|background_color| background_color.as_str())
        .unwrap_or("");

    let content = data.get("props")
        .and_then(|props| props.get("contentSection"))
        .and_then(|content_section| content_section.get("value"))
        .map(|value| {
            if value.is_string() {
                value.as_str().unwrap_or("").to_string()
            } else if value.is_number() {
                value.to_string()
            } else {
                "".to_string()
            }
        })
        .unwrap_or("".to_string());

    info!("【桌面端-打印组件OnixBarleyElectronicText】- content: {:?}，完整内容: {:?}", &content, &json_str);

    let left = Mm(data.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("left"))
        .and_then(|left| left.as_f64())
        .unwrap_or(0.0) as f32);    

    let height = Mm(data.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("height"))
        .and_then(|height| height.as_f64())
        .unwrap_or(0.0) as f32);

    let top = Mm(data.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("top"))
        .and_then(|top| top.as_f64())
        .unwrap_or(0.0) as f32);

    let width = Mm(data.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("width"))
        .and_then(|width| width.as_f64())
        .unwrap_or(0.0) as f32);

    let font_size = Mm(data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("fontSize"))
        .and_then(|font_size| font_size.as_f64())
        .map(|font_size| px2mm(font_size))
        .unwrap_or(px2mm(15.0)) as f32);

    let line_height = Mm(data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("lineHeight"))
        .and_then(|line_height| line_height.as_f64())
        .unwrap_or(1.0) as f32);

    let letter_spacing = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("letterSpace"))
        .and_then(|letter_spacing| letter_spacing.as_f64())
        .unwrap_or(0.0);

    let justify_content = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("fontJustifyContent"))
        .and_then(|justify_content| justify_content.as_str())
        .unwrap_or("flex-start");

    let opacity = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("opacity")) 
        .and_then(|opacity| opacity.as_f64())
        .map(|value| if value > 10.0 { value / 10.0 } else { value })
        .unwrap_or(10.0); // 如果没有找到值则使用默认值10.0

    let align = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("fontAlignItems"))
        .and_then(|align| align.as_str())
        .unwrap_or("flex-start");

    let rotate = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("rotate"))
        .and_then(|rotate| rotate.as_f64())
        .unwrap_or(0.0);

    let font_name = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("fontFamily"))
        .and_then(|font_name| font_name.as_str())
        .unwrap_or("SimHei, Arial, sans-serif");

    let font_style = data.get("props")
        .and_then(|props| props.get("textSection"))
        .and_then(|text_section| text_section.get("fontWeight"))
        .and_then(|font_style| font_style.as_str())
        .unwrap_or("normal");

    if !visible || content.is_empty() {
        return;
    }

    if !background_color.is_empty() && background_color.starts_with("#") {
        // 绘制矩形背景
        let rect = Rect::new(left, convert_to_pdf_coordinates(page_height, top, Some(height)), left + width, convert_to_pdf_coordinates(page_height, top, None));
        current_layer.set_fill_color(Color::Rgb(hex_to_rgb(background_color.to_string(), None).unwrap()));
        current_layer.add_rect(rect);
    }

    // 绘制文本组件的边框，分页要用，不要删！！！
    // let points = vec![
    //     (Point::new(left, convert_to_pdf_coordinates(page_height, top, Some(height))),
    //      Point::new(left + width, convert_to_pdf_coordinates(page_height, top, Some(height)))),
    //     (Point::new(left + width, convert_to_pdf_coordinates(page_height, top, Some(height))),
    //      Point::new(left + width, convert_to_pdf_coordinates(page_height, top, None))),
    //     (Point::new(left + width, convert_to_pdf_coordinates(page_height, top, None)),
    //      Point::new(left, convert_to_pdf_coordinates(page_height, top, None))),
    //     (Point::new(left, convert_to_pdf_coordinates(page_height, top, None)),
    //      Point::new(left, convert_to_pdf_coordinates(page_height, top, Some(height))))
    // ];

    // current_layer.set_outline_color(Color::Rgb(hex_to_rgb("#000000".to_string(), None).unwrap()));
    // current_layer.set_outline_thickness(1.0);

    // // 绘制每条边
    // for (start, end) in points {
    //     let line = Line::from_iter(vec![(start, false), (end, false)]);
    //     current_layer.add_line(line);
    // }    



    // 检查是否提供了 PDF 文档对象
    if let Some(doc) = doc {
        let font_path = get_font_path(&font_name, &font_style);
        println!("【桌面端-打印组件OnixBarleyElectronicText】- subset_font_path: {}", subset_font_path);
        let (font, subset_font_path) = get_or_add_font(doc, subset_font_path, font_path);
        let font_data = match fs::read(&subset_font_path) {
            Ok(data) => data,
            Err(e) => {
                println!("【桌面端-打印组件OnixBarleyElectronicText】- 无法读取字体文件: {:?}", e);
                error!("【桌面端-打印组件OnixBarleyElectronicText】- 无法读取字体文件: {:?}", e);
                return;
            }
        };
        let text_x = left;
        let text_y = convert_to_pdf_coordinates(page_height, top, None);

        if !color.is_empty() && color.starts_with("#") {
            current_layer.set_fill_color(Color::Rgb(hex_to_rgb(color.to_string(), Some(opacity as f32 / 10.0)).unwrap()));
        }else {
            current_layer.set_fill_color(Color::Rgb(hex_to_rgb("#000000".to_string(), Some(opacity as f32 / 10.0)).unwrap()));
        }
        println!("【桌面端-打印组件OnixBarleyElectronicText】- 开始绘制文本");
        draw_text_with_wrapping(current_layer, &content,  font_size, width, text_x, text_y, &font, &font_data, line_height, letter_spacing, justify_content, align, rotate, height);
    } else {
        error!("【桌面端-打印组件OnixBarleyElectronicText】- 文档对象不存在，无法加载字体和绘制文本");
    }
}

/// 绘制带换行的文本
fn draw_text_with_wrapping(
    current_layer: &PdfLayerReference,
    content: &str,
    font_size: Mm,
    max_width: Mm,
    text_x: Mm,
    mut text_y: Mm,
    font: &IndirectFontRef,
    font_data: &[u8],
    line_height: Mm,
    letter_spacing: f64,
    justify_content: &str,
    align: &str,
    rotate: f64,
    max_height: Mm,
) {
    // 字体大小转换为点单位
    let font_size_pt = mm2pt(font_size.0 as f64);
    let text_line_height = font_size * line_height.0;
    let letter_spacing_mm = px2mm(letter_spacing);
    let single_line_height = text_line_height;
    let total_text_height = Mm(calculate_text_height(content, font_size_pt, font_data, letter_spacing_mm, max_width, single_line_height) as f32);
    text_y = adjust_y_for_top_alignment(text_y, font_size, font_data) - (font_size * 2.0);

    match align {
        "flex-end" => {
            text_y = text_y - (max_height - total_text_height);
        }
        "center" => {
            text_y = text_y - ((max_height - total_text_height) / 2.0);
        }
        _ => {} 
    }

    current_layer.save_graphics_state();
    let text_x_pt = mm2pt(text_x.0 as f64) as f32;
    let text_y_pt = mm2pt(text_y.0 as f64) as f32;

    // 设置文本旋转角度
    current_layer.set_ctm(CurTransMat::Translate(Pt(text_x_pt), Pt(text_y_pt)));
    current_layer.set_ctm(CurTransMat::Rotate(rotate as f32 * -1.0));
    current_layer.set_ctm(CurTransMat::Translate(Pt(-text_x_pt), Pt(-text_y_pt)));

    // 遍历文本内容的每一行
    for line in content.lines() {
        let mut current_x = text_x;
        let mut current_width = 0.0;

        let line_width = calculate_line_width(line, font_size_pt, &font_data, letter_spacing_mm, max_width);

        if justify_content == "flex-end" {
            current_x = text_x + max_width - Mm(line_width as f32);
        } else if justify_content == "center" {
            current_x = text_x + (max_width - Mm(line_width as f32)) / 2.0;
        }

        for ch in line.chars() {
            let char_width_pt = get_char_width(ch, font_size_pt, &font_data);
            let char_width_mm = pt2mm(char_width_pt);
                let max_width_f64 = max_width.0 as f64;

                if current_width + char_width_mm + letter_spacing_mm > max_width_f64 {
                    text_y = text_y - text_line_height;
                    current_x = text_x;
                current_width = 0.0;
            }

            current_layer.use_text(&ch.to_string(), font_size_pt as f32, current_x, text_y, font);

            current_x += Mm(char_width_mm as f32 + letter_spacing_mm as f32);
            current_width += char_width_mm + letter_spacing_mm;
        }

        if current_width > 0.0 {
            text_y = text_y - text_line_height;
        }
    }

    current_layer.restore_graphics_state();
}


// 获取字符宽度
pub fn get_char_width(ch: char, font_size: f64, font_data: &[u8]) -> f64 {

    let font = Font::try_from_bytes(font_data).expect("Error constructing Font");
    // Todo: 字体大小需要根据字体大小进行调整
    let scale = Scale::uniform(font_size as f32 * TEXT_RESIZE_RATIO);
    let glyph = font.glyph(ch).scaled(scale);
    let h_metrics = glyph.h_metrics();
    
    h_metrics.advance_width as f64
}

// 调整y轴位置，使文本顶部对齐
fn adjust_y_for_top_alignment(y: Mm, font_size: Mm, font_data: &[u8]) -> Mm {
    let font = Font::try_from_bytes(font_data).expect("Error constructing Font");
    let scale = Scale::uniform(mm2pt(font_size.0 as f64) as f32 * TEXT_RESIZE_RATIO);
    let v_metrics = font.v_metrics(scale);
    let ascent = pt2mm(v_metrics.ascent as f64); // 将上升度量从 pt 转换为 mm
    y + Mm(ascent as f32)
}

// 计算一行文本的宽度
pub fn calculate_line_width(line: &str, font_size_pt: f64, font_data: &[u8], letter_spacing_mm: f64, max_width: Mm) -> f64 {
    let mut total_width = 0.0;
    for ch in line.chars() {
        let char_width_pt = get_char_width(ch, font_size_pt, font_data);
        let char_width_mm = pt2mm(char_width_pt);
        total_width += char_width_mm + letter_spacing_mm;
    }
    // 如果总宽度超过最大宽度，返回最大宽度
    if total_width > max_width.0 as f64 {
        max_width.0 as f64
    } else {
        total_width
    }
}

// 计算文本绘制的实际高度
pub fn calculate_text_height(line: &str, font_size_pt: f64, font_data: &[u8], letter_spacing_mm: f64, max_width: Mm, single_line_height: Mm) -> f64 {
    let mut lines = 1.0;
    let mut current_width = 0.0;
    for ch in line.chars() {
        // 处理换行符
        if ch == '\n' {
            lines += 1.0;
            current_width = 0.0;
            continue;
        }

        let char_width_pt = get_char_width(ch, font_size_pt, font_data);
        let char_width_mm = pt2mm(char_width_pt);
        current_width += char_width_mm + letter_spacing_mm;

        if current_width > max_width.0 as f64 {
            lines += 1.0;
            current_width = char_width_mm + letter_spacing_mm;
        }
    }

    lines * single_line_height.0 as f64
}
