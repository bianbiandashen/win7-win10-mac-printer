use printpdf::*;
use ::image::{open, GenericImageView};
use serde_json::Value;
use crate::onixcomponents::utils::{convert_to_pdf_coordinates, get_font_path, get_or_add_font, mm2px, px2mm, remove_alpha_channel, mm2pt};
use crate::utils::get_image_file_path;
use std::time::Instant;
use super::utils::get_component_visible;
use ::image;
use log::info;
use image::DynamicImage;
/// 绘制条形码或二维码
pub fn draw(current_layer: &PdfLayerReference, json_str: &str, doc: Option<&PdfDocumentReference>, page_height: Mm, subset_font_path_str: String) {

    let json: Value = match serde_json::from_str(json_str) {
        Ok(data) => data,
        Err(e) => {
            info!("【桌面端-OnixBarleyElectronicImage】- JSON 解析错误: {:?}", e);
            return;
        }
    };

    let visible = get_component_visible(&json);

    if !visible {
        return;
    }

    let width_mm = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("width"))
        .and_then(|width| width.as_f64())
        .unwrap_or(50.0);

    let height_mm = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("height"))
        .and_then(|height| height.as_f64())
        .unwrap_or(50.0);

    let left = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("left"))
        .and_then(|left| left.as_f64())
        .unwrap_or(10.0);

    let top = json.get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("top"))
        .and_then(|top| top.as_f64())
        .unwrap_or(10.0);

    // 提取图片的 URL
    if let Some(url) = json.get("props")
        .and_then(|props| props.get("cover"))
        .and_then(|cover| cover.get("url"))
        .and_then(|url| url.as_str()) {

        if url == "https://growth-img.xhscdn.com/ditto/104000n0316i7fatk0q0cp1dsbk" {
            draw_xhs_logo(current_layer, doc, left, convert_to_pdf_coordinates(page_height, Mm(top as f32), Some(Mm(height_mm as f32))).0 as f64, 8.00, height_mm, subset_font_path_str);
            return;
        }
        // 使用 get_image_file_path 获取图片路径
        if let Some(img_path) = get_image_file_path(url) {
            let load_image_start_time = Instant::now();
            // 加载图片
            let img = match open(&img_path) {
                Ok(img) => {
                    info!("【桌面端-OnixBarleyElectronicImage】- 加载图片耗时: {:?}", load_image_start_time.elapsed());
                    img
                },
                Err(e) => {
                    info!("【桌面端-OnixBarleyElectronicImage】- 直接读取图片失败: {:?}; 本地图片路径是：{:?}; 图片链接是：{}", e, img_path, url);
                    let load_image_start_time = Instant::now();
                     match std::fs::read(&img_path) {
                        Ok(buffer) => {
                            match image::load_from_memory(&buffer) {
                                Ok(img) => {
                                    info!("【桌面端-OnixBarleyElectronicImage】- 加载图片耗时: {:?}", load_image_start_time.elapsed());
                                    img
                                },
                                Err(e) => {
                                    info!("【桌面端-OnixBarleyElectronicImage】- 从内存加载图片也失败了: {:?}; 本地图片路径是：{:?}; 图片链接是：{}", e, img_path, url);
                                    return;
                                }
                            }
                        },
                        Err(e) => {
                            info!("【桌面端-OnixBarleyElectronicImage】- 读取图片文件到内存失败: {:?}; 本地图片路径是：{:?}; 图片链接是：{}", e, img_path, url);
                            return;
                        }
                    }
                }
            };


            let remove_alpha_channel_start_time = Instant::now();
            let img = remove_alpha_channel(img); // 确保去除 alpha 通道
            info!("【桌面端-OnixBarleyElectronicImage】- 去除 alpha 通道耗时: {:?}", remove_alpha_channel_start_time.elapsed());

            // 获取图像的宽度和高度
            let (width, height) = img.dimensions();
            let convert_start = Instant::now();
            let rgb_img = img.to_rgb8();
            info!("【桌面端-OnixBarleyElectronicImage】- 转换为 RGB8 耗时: {:?}", convert_start.elapsed());

            let raw_start = Instant::now();
            let raw_data = rgb_img.into_raw();
            info!("【桌面端-OnixBarleyElectronicImage】- 转换为 raw 耗时: {:?}", raw_start.elapsed());
            info!("【桌面端-OnixBarleyElectronicImage】- Raw image data size: {} bytes", raw_data.len());
            let img_xobject = ImageXObject {
                width: Px(width as usize),
                height: Px(height as usize),
                color_space: ColorSpace::Rgb,
                bits_per_component: ColorBits::Bit8,
                interpolate: true,
                image_data: raw_data,
                image_filter: None,
                smask: None,
                clipping_bbox: None,
            };

            let image = Image::from(img_xobject);

            // 获取二维码的位置
            let left = json.get("props")
                .and_then(|props| props.get("basic"))
                .and_then(|basic| basic.get("left"))
                .and_then(|left| left.as_f64())
                .unwrap_or(10.0);

            let top = json.get("props")
                .and_then(|props| props.get("basic"))
                .and_then(|basic| basic.get("top"))
                .and_then(|top| top.as_f64())
                .map(|top| Mm(top as f32))  // 将 f64 转换为 Mm 类型的 Option
                .unwrap_or(Mm(10.0));       // 如果没有值，使用默认值


           let entity_height = json.get("props")
                .and_then(|props| props.get("basic"))
                .and_then(|basic| basic.get("height"))
                .and_then(|height| height.as_f64())
                .map(|h| Mm(h as f32));
            let pdf_top = convert_to_pdf_coordinates(page_height, top, entity_height);
            let image_position = (Mm(left as f32), pdf_top);

            let ratio = 2.2;

            let scale_x = (mm2px(width_mm) / width as f64 * ratio) as f32; // 图片宽度缩放比例
            let scale_y = (mm2px(height_mm) / height as f64 * ratio) as f32; // 图片高度缩放比例

            info!("【桌面端-OnixBarleyElectronicImage】- 图片链接是：{}，加载完成后的最终宽高：{}，{}", url, width as f64 * scale_x as f64, height as f64 * scale_y as f64);
            image.add_to_layer(
                current_layer.clone(),
                ImageTransform {
                    translate_x: Some(image_position.0.into()),
                    translate_y: Some(image_position.1.into()),
                    rotate: None,
                    scale_x: Some(scale_x),
                    scale_y: Some(scale_y),
                    ..Default::default()
                },
            );
        } else {
            info!("【桌面端-OnixBarleyElectronicImage】- 无法解析 URL 中的文件名: {}", url);
        }
    }
}

fn draw_xhs_logo(current_layer: &PdfLayerReference, doc: Option<&PdfDocumentReference>, left: f64, top: f64, width: f64, height: f64, subset_font_path_str: String) {
    let svg_string = include_str!("../../binaries/bin/logo.svg");
    let svg = Svg::parse(svg_string).unwrap();
    let ratio = 2.2;
    let scale_x = mm2px(width) as f32 / svg.width.0 as f32 * ratio;
    let scale_y = mm2px(height) as f32 / svg.height.0 as f32 * ratio;
    svg.add_to_layer(
        current_layer,
        SvgTransform {
            translate_x: Some(Pt(left as f32)),
            translate_y: Some(Pt(top as f32)),
            scale_x: Some(scale_x),
            scale_y: Some(scale_y),
            ..Default::default()
        },
    );

    if let Some(doc) = doc {
        let font_size_mm = px2mm(12.00);
        let padding_x = width + 0.0;
        let padding_y = 0.4;
        draw_text(current_layer, doc, "生活指南", left + padding_x, top + padding_y, font_size_mm, subset_font_path_str.clone());
        draw_text(current_layer, doc, "你的", left + padding_x, top + font_size_mm + padding_y * 2.0, font_size_mm, subset_font_path_str.clone());
    }

}

fn draw_text(layer: &PdfLayerReference, doc: &PdfDocumentReference, content: &str, x: f64, y: f64, font_size_mm: f64, subset_font_path_str: String) {
    let font_path = get_font_path("SimHei, Arial, sans-serif", "normal");
    info!("【桌面端-OnixBarleyElectronicImage】- 开始绘制文本, 字体path: {:?}",  subset_font_path_str);

    let (font,_) = get_or_add_font(doc, subset_font_path_str, font_path);
    let font_size_pt = mm2pt(font_size_mm);
    layer.use_text(&content.to_string(), font_size_pt as f32, Mm(x as f32), Mm(y as f32), &font);
}

fn resize_image(img: &DynamicImage, width_mm: f64, height_mm: f64) -> DynamicImage {
    let resize_start = Instant::now();
    let target_width = (mm2px(width_mm) as u32) * 1;
    let target_height = (mm2px(height_mm) as u32) * 1;
    let new_img = if img.width() > target_width * 4 || img.height() > target_height * 4 {
        // 1. 第一步：快速粗略缩放到中间尺寸
        let intermediate_width = target_width * 2;
        let intermediate_height = target_height * 2;
        let rough_img = img.resize(
            intermediate_width,
            intermediate_height,
            image::imageops::FilterType::Nearest  // 使用最快的算法
        );

        // 2. 第二步：精细缩放到目标尺寸
        rough_img.resize(
            target_width,
            target_height,
            image::imageops::FilterType::Gaussian  // 使用质量更好的算法
        )
    } else {
        // 直接缩放
        img.resize(
            target_width,
            target_height,
            image::imageops::FilterType::Gaussian
        )
    };
    info!("【桌面端-OnixBarleyElectronicImage】- 调整图片尺寸耗时: {:?}", resize_start.elapsed());
    new_img
}
