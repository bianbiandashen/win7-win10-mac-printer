use printpdf::Line;
use printpdf::Pt;
use printpdf::*;
use serde_json::Value; // 导入 printpdf 库，用于创建 PDF 文档

use crate::onixcomponents::utils::convert_to_pdf_coordinates;
use crate::onixcomponents::utils::get_component_visible;
use crate::onixcomponents::utils::px2pt;
use crate::utils::hex_to_rgb;

#[derive(serde::Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
enum BorderStyle {
  Solid,
  Dashed,
}

pub fn draw(current_layer: &PdfLayerReference, json_str: &str, page_height: Mm) {
  // 解析 JSON 字符串
  let data: Value = match serde_json::from_str(json_str) {
      Ok(data) => data,
      Err(e) => {
          println!("解析 JSON 时出错: {:?}", e);
          return;
      }
  };
  let visible = get_component_visible(&data);

  if !visible {
    return;
  }

  let original_top = data.get("props")
    .and_then(|props| props.get("basic"))
    .and_then(|basic| basic.get("top"))
    .and_then(|top| top.as_f64())
    .unwrap_or(0.0);

  let border_size = data.get("props")
    .and_then(|props| props.get("styleSection"))
    .and_then(|style_section| style_section.get("borderSize"))
    .and_then(|border_size| border_size.as_f64())
    .unwrap_or(1.0);

  let left = data.get("props")
    .and_then(|props| props.get("basic"))
    .and_then(|basic| basic.get("left"))
    .and_then(|left| left.as_f64())
    .unwrap_or(0.0);

  let width = data.get("props")
    .and_then(|props| props.get("basic"))
    .and_then(|basic| basic.get("width"))
    .and_then(|width| width.as_f64())
    .unwrap_or(0.0);

  let bg_color = data.get("props")
    .and_then(|props| props.get("styleSection"))
    .and_then(|style_section| style_section.get("bgColor"))
    .and_then(|bg_color| bg_color.as_str())
    .unwrap_or("");

  let border_style = data.get("props")
    .and_then(|props| props.get("styleSection"))
    .and_then(|style_section| style_section.get("borderStyle"))
    .and_then(|border_style| border_style.as_str())
    .unwrap_or("");

    let top = data.get("props") 
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("top"))
        .and_then(|top| top.as_f64())
        .unwrap_or(0.0);

    let height = data.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("height"))
        .and_then(|height| height.as_f64())
        .unwrap_or(0.0);

    // 计算起点的 y 坐标
    let start_y = convert_to_pdf_coordinates(page_height, Mm(top as f32), None);

    // 计算终点的 y 坐标（考虑高度的影响）
    let end_y = convert_to_pdf_coordinates(page_height, Mm((top + height) as f32), None);

    // 定义起点
    let start_point = Point {
        x: Pt::from(Mm(left as f32)),
        y: Pt::from(start_y),
    };

    // 定义终点
    let end_point = Point {
        x: Pt::from(Mm(left as f32)),
        y: Pt::from(end_y),
    };

    // 设置线条颜色
    if !bg_color.is_empty() {
        let line_color = Color::Rgb(hex_to_rgb(bg_color.to_string(), None).unwrap());
        current_layer.set_outline_color(line_color);
    }

    // 设置线条粗细
    if border_size > 0.0 {
        current_layer
            .set_outline_thickness(px2pt(border_size as f64) as f32);
    }

    // 设置线条样式
    if border_style == "dashed" {
        let mut dash_pattern: LineDashPattern = LineDashPattern::default();
        dash_pattern.dash_1 = Some(2);
        dash_pattern.dash_2 = Some(2);
        current_layer.set_line_dash_pattern(dash_pattern);
    }

    let line = Line::from_iter(vec![(start_point, false), (end_point, false)]);

    println!("线条组件 left: '{}'", left);

    current_layer.add_line(line);
}
