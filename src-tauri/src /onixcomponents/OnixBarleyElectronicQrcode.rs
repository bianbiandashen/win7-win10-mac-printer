use crate::onixcomponents::utils::{
    convert_to_pdf_coordinates, get_component_visible, mm2px, px2mm, remove_alpha_channel,
};
use ::image::{DynamicImage, GenericImageView, Luma, imageops}; // 确保引入 GenericImageView
use printpdf::*;
use qrcode::QrCode;
use serde_json::Value;

/// 绘制条形码或二维码
pub fn draw(current_layer: &PdfLayerReference, json_str: &str, page_height: Mm) {
    println!("Drawing OnixBarleyElectronicQrcode with JSON: {}", json_str);

    // 解析 JSON 字符串
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

    // 从 JSON 获取二维码内容
    let content = json
        .get("props")
        .and_then(|props| props.get("contentSection"))
        .and_then(|section| section.get("value"))
        .and_then(|value| value.as_str())
        .unwrap_or("http://www.xiaohongshu.com");

    let height = json
        .get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("height"))
        .and_then(|height| height.as_f64())
        .map(|height| Mm(height as f32))
        .unwrap_or(Mm(0.0));

    let width = json
        .get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("width"))
        .and_then(|width| width.as_f64())
        .map(|width| Mm(width as f32))
        .unwrap_or(Mm(0.0));

    // 获取二维码的位置
    let left = json
        .get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("left"))
        .and_then(|left| left.as_f64())
        .unwrap_or(0.0);

    let top = json
        .get("props")
        .and_then(|props| props.get("basic"))
        .and_then(|basic| basic.get("top"))
        .and_then(|top| top.as_f64())
        .unwrap_or(0.0);

    // 从 JSON 获取宽度和高度
    let width_px = mm2px(width.0 as f64);

    let height_px = mm2px(height.0 as f64);

    // 创建 QR Code
    let code = match QrCode::new(content) {
        Ok(code) => code,
        Err(e) => {
            println!("生成二维码失败: {:?}", e);
            return;
        }
    };

    let qrcode_image = code.render::<Luma<u8>>().build();

    let resized_img = imageops::resize(&qrcode_image, width_px as u32, height_px as u32, imageops::FilterType::Lanczos3);
    // 将 RgbaImage 转换为 DynamicImage
    let dynamic_img = remove_alpha_channel(DynamicImage::ImageLuma8(resized_img));

    // 获取图像的宽度和高度
    let (img_width, img_height) = dynamic_img.dimensions();
    println!("图片宽度: {}, 图片高度: {}, 图片宽度mm: {}, 图片高度mm: {}", img_width, img_height, px2mm(img_width as f64), px2mm(img_height as f64));

    let img_xobject = ImageXObject {
        width: Px(img_width as usize),
        height: Px(img_height as usize),
        color_space: ColorSpace::Rgb,
        bits_per_component: ColorBits::Bit8,
        interpolate: true,
        image_data: dynamic_img.to_rgb8().into_raw(),
        image_filter: None,
        smask: None,
        clipping_bbox: None,
    };

    let image = Image::from(img_xobject);

    let pdf_top = convert_to_pdf_coordinates(page_height, Mm(top as f32), Some(height));
    let image_position = (Mm(left as f32), pdf_top);
    let scale_x = 2.6; // 图片宽度缩放比例
    let scale_y = 2.6; // 图片高度缩放比例

    println!("二维码图像位置: 左 = {:?} mm, 上 = {:?} mm, 二维码高度 = {:?} mm, 页高 = {:?} mm, 实际高度 = {:?} mm", left, top, height, page_height, pdf_top); // 将 Mm 转换为可打印的数值
    println!("scale_x: {}", scale_x);
    println!("scale_y: {}", scale_y);

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

    println!("二维码已绘制，内容: {}", content);
}
