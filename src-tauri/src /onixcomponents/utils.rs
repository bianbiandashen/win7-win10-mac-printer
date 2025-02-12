use image::{DynamicImage, Rgba, RgbaImage};
use lazy_static::lazy_static;
use log::{error, info};
use printpdf::{
    Color, IndirectFontRef, LineDashPattern, Mm, PdfDocumentReference, PdfLayerReference, Rgb,
};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;
use rand::Rng;
use std::io::Write;
use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static THREAD_FONTS: RefCell<HashMap<String, (IndirectFontRef, String)>> = RefCell::new(HashMap::new());
}

pub const TEXT_RESIZE_RATIO: f32 = 1.485;

/// 将左上角坐标转换为 PDF 左下角坐标
pub fn convert_to_pdf_coordinates(
    page_height: Mm,
    top_left_y: Mm,
    entity_height: Option<Mm>,
) -> Mm {
    match entity_height {
        Some(height) => page_height - top_left_y - height,
        None => page_height - top_left_y,
    }
}

/// 移除图片的 Alpha 通道，使用白色背景替代透明区域
pub fn remove_alpha_channel(image: DynamicImage) -> DynamicImage {
    if let Some(rgba) = image.as_rgba8() {
        let mut img_with_white_bg =
            RgbaImage::from_pixel(rgba.width(), rgba.height(), Rgba([255, 255, 255, 255]));

        for (x, y, pixel) in rgba.enumerate_pixels() {
            let alpha = pixel[3] as f32 / 255.0;
            let blended_pixel = [
                (pixel[0] as f32 * alpha + 255.0 * (1.0 - alpha)) as u8,
                (pixel[1] as f32 * alpha + 255.0 * (1.0 - alpha)) as u8,
                (pixel[2] as f32 * alpha + 255.0 * (1.0 - alpha)) as u8,
                255,
            ];
            img_with_white_bg.put_pixel(x, y, Rgba(blended_pixel));
        }
        DynamicImage::ImageRgba8(img_with_white_bg)
    } else {
        image
    }
}

const DPI: f64 = 130.0;

/// 毫米转换为像素
pub fn mm2px(mm: f64) -> f64 {
    (mm * (DPI / 25.4)).round()
}

/// 像素转换为磅
pub fn px2pt(px: f64) -> f64 {
    px * 72.0 / DPI
}

pub fn pt2px(pt: f64) -> f64 {
    (pt * DPI / 72.0).round()
}

pub fn mm2pt(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

pub fn pt2mm(pt: f64) -> f64 {
    pt * 25.4 / 72.0
}

pub fn px2mm(px: f64) -> f64 {
    px * 25.4 / DPI
}

/// 重置图层的默认样式
pub fn reset_layer_defaults(layer: &PdfLayerReference) {
    layer.set_fill_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
    layer.set_outline_color(Color::Rgb(Rgb::new(0.0, 0.0, 0.0, None)));
    layer.set_outline_thickness(1.0);
    layer.set_line_dash_pattern(LineDashPattern::default());
}

/// 获取字体路径
pub fn get_font_path(font_name: &str, font_style: &str) -> PathBuf {
    let mut resource_dir = env::temp_dir();
    resource_dir.push("xhs-printer-fonts");

    let dir_name = match font_name {
        "SimHei, Arial, sans-serif" => "SiYuanHeiTi",
        "SimSun, Arial, sans-serif" => "SiYuanHeiTi",
        _ => "SiYuanHeiTi",
    };
    resource_dir.push(&dir_name);

    let file_name = match font_style {
        "normal" => "Normal.otf",
        "light" => "Light.otf",
        "medium" => "Medium.otf",
        _ => "Medium.otf",
    };

    resource_dir.push(&file_name);

    return resource_dir;
}

#[cfg(target_os = "macos")]
pub fn subset_font(font_path: &str, text: &str, output_path: &str) -> Result<bool, Box<dyn std::error::Error>> {
    use allsorts::binary::read::ReadScope;
    use allsorts::font::MatchingPresentation;
    use allsorts::font_data::FontData;
    use allsorts::subset::subset;
    use allsorts::Font;
    use std::collections::HashSet;
    use std::fs::read;



    let font_data_bytes = read(font_path)?;
    let scope = ReadScope::new(&font_data_bytes);
    let font_data = scope.read::<FontData<'_>>()?;
    let provider = font_data.table_provider(0)?;
    let mut font = Font::new(provider)?;

    // 收集需要的字形ID
    let mut glyph_ids = HashSet::new();
    for id in 0..=3u16 {  // 添加前几个基本字形，包括 .notdef
        glyph_ids.insert(id);
    }

    // 确保包含基本标点和空格
    let basic_chars = " .,!?-_";
    let mut all_chars = HashSet::new();
    all_chars.extend(basic_chars.chars());
    all_chars.extend(text.chars().filter(|c| !c.is_whitespace() && *c != '\n' && *c != '\r'));
    // 字符到字形ID的映射记录
    let mut char_to_glyph = HashMap::new();


    for ch in all_chars {
        match font.lookup_glyph_index(ch, MatchingPresentation::NotRequired, None) {
            (glyph_id, _) if glyph_id != 0 => {
                glyph_ids.insert(glyph_id);
                char_to_glyph.insert(ch, glyph_id);
            }
            _ => {
                // 如果找不到字形或返回 0，记录警告
                info!("字符 '{}' (U+{:04X}) 未找到有效的字形ID", ch, ch as u32);
            }
        }
    }

    let glyph_ids_vec: Vec<u16> = glyph_ids.into_iter().collect();

    // 使用同一个字体数据进行子集化
    let scope = ReadScope::new(&font_data_bytes);
    let font_data = scope.read::<FontData<'_>>()?;
    let provider = font_data.table_provider(0)?;

    let subset_font = subset(&provider, &glyph_ids_vec)?;

    if subset_font.is_empty() {
        return Err("子集化失败：生成的字体数据为空".into());
    }

    File::create(output_path)?
        .write_all(&subset_font)?;

    Ok(true)
}

pub fn get_real_cache_key(cache_key: String) -> String {
    // 获取当前平台的路径分隔符

    let main_separator = std::path::MAIN_SEPARATOR.to_string();
    let real_cache_key = cache_key
        .replace(
            &['\\', '/', ':', '*', '?', '"', '<', '>', '|', '\n'][..],
            "_",
        )
        .replace(&main_separator, "_");
    real_cache_key
}

pub fn get_or_add_font(
    doc: &PdfDocumentReference,
    subset_font_path: String,
    font_path: PathBuf,
) -> (IndirectFontRef, String) {
    let start_time = Instant::now();
    let font_path_str = font_path.to_string_lossy().to_string();

    // 检查线程本地缓存是否存在已添加的字体
    if let Some(cached) = THREAD_FONTS.with(|cache| {
        cache.borrow().get(&font_path_str).cloned()
    }) {
        info!("【线程-{:?}】从缓存获取字体: {} 耗时: {:?}", std::thread::current().id(), font_path_str, start_time.elapsed());
        return cached;
    }
    // 2. 加载字体（耗时操作）
    info!("【线程-{:?}】即将加载字体: {}", std::thread::current().id(), subset_font_path);

    let font_ref = doc.add_external_font(File::open(&subset_font_path).expect("Failed to open subset font"))
        .unwrap();

    let result = (font_ref.clone(), subset_font_path.clone());

    // 3. 更新缓存
    THREAD_FONTS.with(|cache| {
        cache.borrow_mut().insert(font_path_str.clone(), result.clone());
    });
    info!("【线程-{:?}】字体已加载并缓存: {} 耗时: {:?}", std::thread::current().id(), font_path_str, start_time.elapsed());

    result
}

/// 清除字体缓存
pub fn clear_font_cache() {
    THREAD_FONTS.with(|cache| {
        cache.borrow_mut().clear();
    });
    info!("【线程-{:?}】字体缓存已清除", std::thread::current().id());
}

/**
 * 解析组件的JSON配置，判断组件是否渲染
 * 优先使用 value 字段，如果 value 不存在或不是 bool 类型，则使用 visible 字段
 */
pub fn get_component_visible(data: &Value) -> bool {
    data.get("props")
        .and_then(|props| props.get("visibleSection"))
        .and_then(|visible_section| {
            let value_result = visible_section
                .get("value")
                .and_then(|value| value.as_bool());
            match value_result {
                Some(value) => Some(value),
                None => visible_section
                    .get("visible")
                    .and_then(|visible| visible.as_bool()),
            }
        })
        .unwrap_or(true)
}
