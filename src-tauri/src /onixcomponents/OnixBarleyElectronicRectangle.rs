use crate::onixcomponents::utils::{convert_to_pdf_coordinates, get_component_visible, px2mm, px2pt};
use crate::utils::hex_to_rgb;
use printpdf::Pt;
use printpdf::*;
use serde_json::Value;

pub fn draw(current_layer: &PdfLayerReference, json_str: &str, page_height: Mm) {
    let data: Value = match serde_json::from_str(json_str) {
        Ok(data) => data,
        Err(e) => {
            println!("onixComponents: 矩形解析 JSON 时出错: {:?}", e);
            return;
        }
    };

    let visible = get_component_visible(&data);

    if !visible {
        return;
    }

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

    let bg_color = data.get("props")
        .and_then(|props| props.get("styleSection"))
        .and_then(|style_section| style_section.get("bgColor"))
        .and_then(|bg_color| bg_color.as_str())
        .unwrap_or("");

    let border_size = data.get("props")
        .and_then(|props| props.get("styleSection"))
        .and_then(|style_section| style_section.get("borderSize"))
        .and_then(|border_size| border_size.as_f64())
        .unwrap_or(1.0);

    let border_style = data.get("props")
        .and_then(|props| props.get("styleSection"))
        .and_then(|style_section| style_section.get("borderStyle"))
        .and_then(|border_size| border_size.as_str())
        .unwrap_or("solid");

    let border_color = "#000000";

    if border_size > 0.0 {
        // 设置线条粗细
        current_layer.set_outline_thickness(px2pt(border_size as f64) as f32);

        // 设置线条颜色
        let line_color = Color::Rgb(hex_to_rgb(border_color.to_string(), None).unwrap());
        current_layer.set_outline_color(line_color);
    }

    // 设置填充颜色
    if !bg_color.is_empty() {
        current_layer.set_fill_color(Color::Rgb(hex_to_rgb(bg_color.to_string(), None).unwrap()));
    }

    // 设置虚线
    if border_style == "dashed" {
        let mut dash_pattern: LineDashPattern = LineDashPattern::default();
        dash_pattern.dash_1 = Some(2);
        dash_pattern.dash_2 = Some(2);
        current_layer.set_line_dash_pattern(dash_pattern);
    }

    // 绘制矩形背景
    let points = vec![
        (Point::new(left, convert_to_pdf_coordinates(page_height, top, Some(height))), false),
        (Point::new(left + width, convert_to_pdf_coordinates(page_height, top, Some(height))), false),
        (Point::new(left + width, convert_to_pdf_coordinates(page_height, top, None)), false),
        (Point::new(left, convert_to_pdf_coordinates(page_height, top, None)), false),
        (Point::new(left, convert_to_pdf_coordinates(page_height, top, Some(height))), false),
    ];
    
    // 创建一个闭合的路径
    let line = Line::from_iter(points);
    
    // 先填充再描边
    current_layer.add_line(line);   

    println!(
        "onixComponents: 矩形组件draw完成"
    );
}
