use serde_json::Value;
use skia_safe::{self as sk, Color, Data, Font, FontMgr, Image, Paint, Rect};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
};

pub const GROUPS: [&str; 6] = [
    "CommunityScore",
    "RottenTomatoesScore",
    "PopcornScore",
    "MyAnimeListScore",
    "FavoriteCount",
    "ParentalRating",
];
static ASSETS: OnceLock<Mutex<HashMap<String, Image>>> = OnceLock::new();
pub fn asset_count() -> usize {
    ASSETS.get().map(|c| c.lock().unwrap().len()).unwrap_or(0)
}
pub fn clear_assets() {
    if let Some(cache) = ASSETS.get() {
        cache.lock().unwrap().clear();
    }
}
pub fn number(v: &Value, key: &str, default: f64) -> f32 {
    v[key].as_f64().unwrap_or(default) as f32
}
pub fn flag(v: &Value, key: &str, default: bool) -> bool {
    v[key].as_bool().unwrap_or(default)
}
pub fn text<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v[key].as_str().unwrap_or(default)
}
pub fn alignment(v: &Value, key: &str) -> usize {
    v[key]
        .as_u64()
        .map(|i| i as usize)
        .unwrap_or_else(|| match text(v, key, "Disabled") {
            "TopLeft" => 1,
            "TopRight" => 2,
            "BottomLeft" => 3,
            "BottomRight" => 4,
            _ => 0,
        })
}

fn asset(name: &str) -> Option<Image> {
    let mut cache = ASSETS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .ok()?;
    if let Some(image) = cache.get(name) {
        return Some(image.clone());
    }
    let bytes: &[u8] = match name {
        "imdb" => include_bytes!("../../../EmbeddedIcons/rating_imdb.png"),
        "tomato" => include_bytes!("../../../EmbeddedIcons/t_tomato.png"),
        "splat" => include_bytes!("../../../EmbeddedIcons/t_splat.png"),
        "popcorn" => include_bytes!("../../../EmbeddedIcons/t_popcorn.png"),
        "spilled" => include_bytes!("../../../EmbeddedIcons/t.spilledpopcorn.png"),
        "fresh" => include_bytes!("../../../EmbeddedIcons/t_fresh.png"),
        "mal" => include_bytes!("../../../EmbeddedIcons/r_mal.png"),
        "heart" => include_bytes!("../../../EmbeddedIcons/f_heart.png"),
        "g" => include_bytes!("../../../EmbeddedIcons/pr_g.png"),
        "pg" => include_bytes!("../../../EmbeddedIcons/pr_pg.png"),
        "pg-13" => include_bytes!("../../../EmbeddedIcons/pr_pg-13.png"),
        "r" => include_bytes!("../../../EmbeddedIcons/pr_r.png"),
        "nc-17" => include_bytes!("../../../EmbeddedIcons/pr_nc-17.png"),
        "tv-14" => include_bytes!("../../../EmbeddedIcons/pr_tv-14.png"),
        "tv-ma" => include_bytes!("../../../EmbeddedIcons/pr_tv-ma.png"),
        "tv-pg" => include_bytes!("../../../EmbeddedIcons/pr_tv-pg.png"),
        _ => return None,
    };
    let image = Image::from_encoded(Data::new_copy(bytes))?;
    cache.insert(name.to_owned(), image.clone());
    Some(image)
}

pub fn used_icons(data: &Value) -> Vec<String> {
    let mut result = Vec::new();
    for (key, name) in [
        ("community", "imdb"),
        (
            "critic",
            if number(data, "critic", 100.0) < 60.0 {
                "splat"
            } else {
                "tomato"
            },
        ),
        (
            "popcorn",
            if number(data, "popcorn", 100.0) < 60.0 {
                "spilled"
            } else if number(data, "popcorn", 0.0) >= 90.0
                && number(data, "popcorn_votes", 0.0) >= 500.0
            {
                "fresh"
            } else {
                "popcorn"
            },
        ),
        ("mal", "mal"),
        ("favorites", "heart"),
    ] {
        if data[key].is_number() && (key != "favorites" || number(data, key, 0.0) > 0.0) {
            result.push(name.to_owned());
        }
    }
    if let Some(parental) = data["parental"].as_str() {
        if asset(parental).is_some() {
            result.push(parental.to_owned());
        }
    }
    result
}

fn score_text(index: usize, value: f64, settings: &Value) -> String {
    if index == 1 || index == 2 {
        format!(
            "{:.0}{}",
            value.round_ties_even(),
            text(settings, "RatingPercentageSuffix", "%")
        )
    } else if (value - value.round_ties_even()).abs() < 0.01 {
        format!("{:.0}", value.round_ties_even())
    } else {
        format!("{value:.1}")
    }
}
pub fn visual_data(settings: &Value, data: &Value) -> Value {
    let mut result = serde_json::Map::new();
    for (index, key) in [
        "community",
        "critic",
        "popcorn",
        "mal",
        "favorites",
        "parental",
    ]
    .iter()
    .enumerate()
    {
        if alignment(settings, &format!("{}IconAlignment", GROUPS[index])) == 0
            || data[*key].is_null()
        {
            continue;
        }
        let value = number(data, key, 0.0) as f64;
        if index == 4 && value <= 0.0 {
            continue;
        }
        let icon = match index {
            0 => "imdb",
            1 => {
                if value < 60.0 {
                    "splat"
                } else {
                    "tomato"
                }
            }
            2 => {
                if value < 60.0 {
                    "spilled"
                } else if value >= 90.0 && number(data, "popcorn_votes", 0.0) >= 500.0 {
                    "fresh"
                } else {
                    "popcorn"
                }
            }
            3 => "mal",
            4 => "heart",
            _ => text(data, "parental", ""),
        };
        if index == 5 && asset(icon).is_none() {
            continue;
        }
        let revision = asset(icon)
            .and_then(|image| image.encoded_data())
            .map(|data| crate::engine::hash_bytes(data.as_bytes()))
            .unwrap_or_default();
        result.insert(GROUPS[index].to_owned(),serde_json::json!({"icon":icon,"asset_revision":revision,"label":if index==5{String::new()}else{score_text(index,value,settings)}}));
    }
    Value::Object(result)
}

pub fn validate_output(original: &Path, output: &Path, cache: &Path) -> Result<(), String> {
    let original = fs::canonicalize(original).map_err(|e| format!("original artwork: {e}"))?;
    let parent = fs::canonicalize(output.parent().ok_or("output has no parent")?)
        .map_err(|e| e.to_string())?;
    let cache = fs::canonicalize(cache).map_err(|e| e.to_string())?;
    let resolved = parent.join(output.file_name().ok_or("output has no filename")?);
    if !parent.starts_with(cache)
        || resolved == original
        || fs::canonicalize(output).is_ok_and(|p| p == original)
    {
        return Err("output must be an Emby cache file, never original artwork".into());
    }
    Ok(())
}

struct Overlay {
    group: &'static str,
    image: Option<Image>,
    label: Option<String>,
    corner: usize,
    priority: f32,
    horizontal: bool,
}

fn decode_image(bytes: &[u8]) -> Result<Image, String> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|e| format!("WebP decode failed: {e}"))?;
        let (width, height) = decoder.dimensions();
        let mut pixels = vec![0; decoder.output_buffer_size().ok_or("WebP image is too large")?];
        decoder.read_image(&mut pixels).map_err(|e| format!("WebP decode failed: {e}"))?;
        let rgba = if decoder.has_alpha() { pixels } else {
            pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect()
        };
        let info = sk::ImageInfo::new((width as i32, height as i32), sk::ColorType::RGBA8888,
            sk::AlphaType::Unpremul, None);
        return sk::images::raster_from_data(&info, Data::new_copy(&rgba), width as usize * 4)
            .ok_or_else(|| "WebP raster creation failed".into());
    }
    Image::from_encoded(Data::new_copy(bytes)).ok_or_else(|| "image decode failed".into())
}

pub fn draw(
    input: &str,
    original: &str,
    output: &str,
    cache: &str,
    settings: &Value,
    data: &Value,
    smoothing: bool,
) -> Result<Value, String> {
    validate_output(Path::new(original), Path::new(output), Path::new(cache))?;
    let image = decode_image(&fs::read(input).map_err(|e| e.to_string())?)?;
    let (width, height) = (image.width(), image.height());
    let mut surface = sk::surfaces::raster_n32_premul((width, height))
        .ok_or("could not allocate drawing surface")?;
    let canvas = surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.draw_image(&image, (0, 0), None);
    let typeface = FontMgr::new()
        .new_from_data(
            Data::new_copy(include_bytes!("../../../Assets/Roboto-Bold.ttf")),
            None,
        )
        .ok_or("embedded font load failed")?;
    let mut paint = Paint::default();
    paint.set_anti_alias(smoothing);
    let sampling = if smoothing {
        sk::SamplingOptions::new(sk::FilterMode::Linear, sk::MipmapMode::Linear)
    } else {
        sk::SamplingOptions::default()
    };
    let mut overlays = Vec::new();
    let keys = [
        "community",
        "critic",
        "popcorn",
        "mal",
        "favorites",
        "parental",
    ];
    let icons = used_icons(data);
    for (index, group) in GROUPS.iter().enumerate() {
        let corner = alignment(settings, &format!("{group}IconAlignment"));
        if corner == 0 || corner > 4 || data[keys[index]].is_null() {
            continue;
        }
        let value = number(data, keys[index], 0.0) as f64;
        if index == 4 && value <= 0.0 {
            continue;
        }
        let name = match index {
            0 => "imdb",
            1 => {
                if value < 60.0 {
                    "splat"
                } else {
                    "tomato"
                }
            }
            2 => {
                if value < 60.0 {
                    "spilled"
                } else if value >= 90.0 && number(data, "popcorn_votes", 0.0) >= 500.0 {
                    "fresh"
                } else {
                    "popcorn"
                }
            }
            3 => "mal",
            4 => "heart",
            _ => text(data, "parental", ""),
        };
        let label = if index == 5 {
            None
        } else {
            Some(score_text(index, value, settings))
        };
        let icon = asset(name);
        if index == 5 && icon.is_none() {
            continue;
        }
        overlays.push(Overlay {
            group,
            image: icon,
            label,
            corner,
            priority: number(settings, &format!("{group}IconPriority"), index as f64),
            horizontal: flag(settings, &format!("{group}OverlayHorizontal"), true),
        });
    }
    overlays.sort_by(|a, b| a.priority.total_cmp(&b.priority));
    let min_dimension = width.min(height) as f32;
    let generic =
        ((min_dimension * number(settings, "IconSize", 10.0) / 100.0) as i32).clamp(8, 512) as f32;
    let spacing = (generic * number(settings, "IconSpacing", 12.5) / 100.0)
        .clamp(0.0, 64.0)
        .trunc();
    for corner in 1..=4 {
        let name = ["", "TopLeft", "TopRight", "BottomLeft", "BottomRight"][corner];
        let percentage = number(settings, &format!("{name}IconSize"), 0.0);
        let size = if percentage > 0.0 {
            ((min_dimension * percentage / 100.0) as i32).clamp(8, 512) as f32
        } else {
            generic
        };
        let padding = |axis: &str, dimension: i32| {
            let corner_value = number(settings, &format!("{name}{axis}Padding"), -1.0);
            let value = if corner_value >= 0.0 {
                corner_value
            } else {
                number(settings, &format!("EdgePadding{axis}"), -1.0)
            };
            if value >= 0.0 {
                (dimension as f32 * value / 100.0).clamp(0.0, 256.0).trunc()
            } else {
                (generic / 4.0).trunc().clamp(2.0, 64.0)
            }
        };
        let (hp, vp) = (padding("Horizontal", width), padding("Vertical", height));
        let mut font = Font::from_typeface(
            typeface.clone(),
            size * number(settings, "RatingFontSizeMultiplier", 0.75),
        );
        font.set_edging(if smoothing {
            sk::font::Edging::AntiAlias
        } else {
            sk::font::Edging::Alias
        });
        let mut vertical = 0.0;
        let mut horizontal = 0.0;
        let mut row_height: f32 = 0.0;
        let mut rows = 1;
        let items: Vec<_> = overlays.iter().filter(|o| o.corner == corner).collect();
        for overlay in items
            .iter()
            .filter(|o| o.horizontal)
            .chain(items.iter().filter(|o| !o.horizontal))
        {
            if !overlay.horizontal && row_height > 0.0 {
                vertical += row_height + spacing;
                horizontal = 0.0;
                row_height = 0.0;
            }
            let bounds = overlay
                .label
                .as_ref()
                .map(|label| font.measure_str(label, None).1)
                .unwrap_or_default();
            let icon_width = overlay
                .image
                .as_ref()
                .map(|i| (size * i.width() as f32 / i.height() as f32).round_ties_even())
                .unwrap_or(0.0);
            let shape = alignment_shape(settings, &format!("{}BackgroundShape", overlay.group));
            let score_width = if overlay.label.is_some() {
                bounds.width().ceil() + if shape > 0 { size * 0.4 } else { 0.0 }
            } else {
                0.0
            };
            let icon_gap = if overlay.label.is_some() && icon_width > 0.0 {
                (size / 10.0).trunc().max(1.0)
            } else {
                0.0
            };
            let total = icon_width + icon_gap + score_width;
            if overlay.horizontal
                && horizontal > 0.0
                && horizontal + total > (width as f32 - hp * 2.0).max(1.0)
            {
                if rows >= 2 {
                    continue;
                }
                vertical += row_height + spacing;
                horizontal = 0.0;
                row_height = 0.0;
                rows += 1;
            }
            let x = if corner == 2 || corner == 4 {
                width as f32 - hp - horizontal - total
            } else {
                hp + horizontal
            };
            let y = if corner >= 3 {
                height as f32 - vp - vertical - size
            } else {
                vp + vertical
            };
            if let Some(icon) = &overlay.image {
                canvas.draw_image_rect_with_sampling_options(
                    icon,
                    None,
                    Rect::from_xywh(x, y, icon_width, size),
                    sampling,
                    &paint,
                );
            }
            if let Some(label) = &overlay.label {
                let opacity = number(
                    settings,
                    &format!("{}BackgroundOpacity", overlay.group),
                    80.0,
                );
                let rect = Rect::from_xywh(x + icon_width + icon_gap, y, score_width, size);
                let mut bg = Paint::default();
                bg.set_anti_alias(true);
                if let Some(color) = parse_color(text(
                    settings,
                    &format!("{}BackgroundColor", overlay.group),
                    "#404040",
                )) {
                    bg.set_color(Color::from_argb(
                        (opacity * 2.55).clamp(0.0, 255.0) as u8,
                        color.r(),
                        color.g(),
                        color.b(),
                    ));
                    if shape == 2 {
                        canvas.draw_rect(rect, &bg);
                    } else if shape == 1 {
                        canvas.draw_circle(
                            rect.center(),
                            rect.width().min(rect.height()) / 2.0,
                            &bg,
                        );
                    }
                }
                let point = (
                    rect.left + (score_width - bounds.width().ceil()) / 2.0,
                    y + (size - bounds.height()) / 2.0 - bounds.top
                        + number(settings, "RatingTextVerticalOffset", 0.0),
                );
                let mut text_paint = Paint::default();
                text_paint.set_anti_alias(smoothing);
                if shape == 0 || opacity < 50.0 {
                    text_paint
                        .set_color(Color::BLACK)
                        .set_style(sk::paint::Style::StrokeAndFill)
                        .set_stroke_width((font.size() / 12.0).max(1.0));
                    canvas.draw_str(label, point, &font, &text_paint);
                }
                text_paint
                    .set_color(Color::WHITE)
                    .set_style(sk::paint::Style::Fill);
                canvas.draw_str(label, point, &font, &text_paint);
            }
            if overlay.horizontal {
                horizontal += total + spacing;
                row_height = row_height.max(size);
            } else {
                vertical += size + spacing;
            }
        }
    }
    let encoded = sk::png_encoder::encode_image(
        None,
        &surface.image_snapshot(),
        &sk::png_encoder::Options::default(),
    )
    .ok_or("lossless PNG encoding failed")?;
    let temp = format!(
        "{output}.ei-{}-{}.tmp",
        std::process::id(),
        crate::engine::now_millis()
    );
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(encoded.as_bytes())
            .map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        drop(file);
        validate_output(Path::new(original), Path::new(output), Path::new(cache))?;
        fs::rename(&temp, output).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(serde_json::json!({"width":width,"height":height,"bytes":encoded.len(),"icons":icons}))
}

pub fn font_revision() -> &'static str {
    static REVISION: OnceLock<String> = OnceLock::new();
    REVISION.get_or_init(|| {
        crate::engine::hash_bytes(include_bytes!("../../../Assets/Roboto-Bold.ttf"))
    })
}

fn alignment_shape(v: &Value, key: &str) -> u64 {
    v[key]
        .as_u64()
        .unwrap_or_else(|| match text(v, key, "None") {
            "Circle" => 1,
            "Square" => 2,
            _ => 0,
        })
}
fn parse_color(value: &str) -> Option<Color> {
    let hex = value.trim_start_matches('#');
    let n = u32::from_str_radix(hex, 16).ok()?;
    if hex.len() != 6 {
        return None;
    }
    Some(Color::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webp_decode_preserves_color_and_alpha() {
        for (pixels, color, expected) in [
            (vec![255, 0, 0, 0, 255, 0], image_webp::ColorType::Rgb8,
                vec![255, 0, 0, 255, 0, 255, 0, 255]),
            (vec![255, 0, 0, 255, 0, 255, 0, 128], image_webp::ColorType::Rgba8,
                vec![255, 0, 0, 255, 0, 255, 0, 128]),
        ] {
            let mut encoded = Vec::new();
            image_webp::WebPEncoder::new(&mut encoded).encode(&pixels, 2, 1, color).unwrap();
            let image = decode_image(&encoded).unwrap();
            assert_eq!((image.width(), image.height()), (2, 1));
            let info = sk::ImageInfo::new((2, 1), sk::ColorType::RGBA8888, sk::AlphaType::Unpremul, None);
            let mut rgba = vec![0u8; 8];
            assert!(image.read_pixels(&info, &mut rgba, 8, (0, 0), sk::image::CachingHint::Disallow));
            assert_eq!(rgba, expected);
        }
    }
}
