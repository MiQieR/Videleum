//! Bilibili danmaku XML → ASS subtitle conversion.
//!
//! This is a small, predictable renderer rather than a full danmaku engine:
//! scrolling, top and bottom comments are placed on collision-free lanes so
//! the resulting file opens cleanly in VLC, mpv or any ASS-capable player.

use std::fmt::Write as _;
use std::io::Cursor;

use quick_xml::Reader;
use quick_xml::events::Event;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct Danmaku {
    /// Seconds from the start of the video.
    pub time: f64,
    /// Bilibili mode: 1–3 scrolling, 4 bottom, 5 top, 6 reverse, 7 advanced.
    pub mode: u8,
    pub size: u32,
    pub color: u32,
    pub text: String,
}

#[derive(Debug, Error)]
pub enum DanmakuError {
    #[error("Couldn't read the danmaku file: {0}")]
    Xml(String),
}

/// Parse Bilibili's XML danmaku format.
pub fn parse_xml(xml: &[u8]) -> Result<Vec<Danmaku>, DanmakuError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut comments = Vec::new();
    let mut in_comment = false;
    let mut attributes = String::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                if event.name().as_ref() == b"d" {
                    in_comment = true;
                    text.clear();
                    attributes.clear();
                    for attribute in event.attributes().flatten() {
                        if attribute.key.as_ref() == b"p" {
                            attributes = String::from_utf8_lossy(&attribute.value).into_owned();
                        }
                    }
                }
            }
            Ok(Event::Text(event)) if in_comment => {
                if let Ok(value) = event.decode() {
                    text.push_str(&value);
                }
            }
            Ok(Event::GeneralRef(event)) if in_comment => {
                if let Ok(Some(character)) = event.resolve_char_ref() {
                    text.push(character);
                } else if let Ok(name) = event.decode() {
                    match name.as_ref() {
                        "amp" => text.push('&'),
                        "lt" => text.push('<'),
                        "gt" => text.push('>'),
                        "quot" => text.push('"'),
                        "apos" => text.push('\''),
                        _ => {}
                    }
                }
            }
            Ok(Event::CData(event)) if in_comment => {
                text.push_str(&String::from_utf8_lossy(&event));
            }
            Ok(Event::End(event)) => {
                if event.name().as_ref() == b"d" && in_comment {
                    in_comment = false;
                    if let Some(comment) = parse_attributes(&attributes, &text) {
                        comments.push(comment);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(DanmakuError::Xml(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    comments.sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok(comments)
}

fn parse_attributes(attributes: &str, text: &str) -> Option<Danmaku> {
    let mut fields = attributes.split(',');
    let time = fields.next()?.parse::<f64>().ok()?;
    let mode = fields.next()?.parse::<u8>().ok()?;
    let size = fields
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(25);
    let color = fields
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0xFFFFFF);
    let text = text.trim().to_string();
    if text.is_empty() || !time.is_finite() || time < 0.0 {
        return None;
    }
    Some(Danmaku {
        time,
        mode,
        size,
        color,
        text,
    })
}

#[derive(Clone, Debug)]
pub struct AssOptions {
    pub width: u32,
    pub height: u32,
    pub font: String,
    /// How long a scrolling comment stays on screen, before width correction.
    pub scroll_seconds: f64,
    pub font_scale: f64,
    pub opacity: f64,
}

impl Default for AssOptions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            font: default_font().to_string(),
            scroll_seconds: 7.5,
            font_scale: 1.2,
            opacity: 1.0,
        }
    }
}

fn default_font() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "PingFang SC"
    }
    #[cfg(target_os = "windows")]
    {
        "Microsoft YaHei"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Noto Sans CJK SC"
    }
}

/// Convert comments into a complete ASS document.
pub fn to_ass(comments: &[Danmaku], options: &AssOptions) -> String {
    let width = options.width.max(320) as f64;
    let height = options.height.max(240) as f64;
    let mut output = String::with_capacity(comments.len() * 120 + 1024);

    let _ = writeln!(
        output,
        "[Script Info]\n; Generated by Videleum\nScriptType: v4.00+\nPlayResX: {}\nPlayResY: {}\nWrapStyle: 2\nScaledBorderAndShadow: yes\nYCbCr Matrix: TV.709\n",
        options.width, options.height
    );
    let _ = writeln!(
        output,
        "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding"
    );
    let alpha = ((1.0 - options.opacity.clamp(0.0, 1.0)) * 255.0).round() as u32;
    for (name, alignment) in [
        ("Videleum Danmaku", 7),
        ("Videleum Top", 8),
        ("Videleum Bottom", 2),
    ] {
        let _ = writeln!(
            output,
            "Style: {name},{font},40,&H{alpha:02X}FFFFFF,&H{alpha:02X}FFFFFF,&H{alpha:02X}101010,&H{alpha:02X}000000,0,0,0,0,100,100,0,0,1,1.4,0.8,{alignment},20,20,20,1",
            font = escape_style_field(&options.font),
        );
    }
    output.push_str("\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");

    let mut scroll_free = vec![0.0_f64; 48];
    let mut top_free = vec![0.0_f64; 24];
    let mut bottom_free = vec![0.0_f64; 24];
    let base_line = 44.0;

    for comment in comments {
        if comment.text.trim().is_empty() || comment.time > 86_400.0 {
            continue;
        }
        let font_size = (comment.size as f64 * options.font_scale)
            .clamp(22.0, 64.0)
            .min(height / 12.0);
        let line_height = font_size * 1.35;
        let color = ass_color(comment.color, alpha);
        let text = escape_text(&comment.text);
        let estimated_width = estimate_width(&comment.text, font_size);
        let style_font_size = 40.0;

        match comment.mode {
            1 | 2 | 3 | 6 => {
                let lane_count = ((height - 40.0) / line_height).floor().max(1.0) as usize;
                let lane = pick_lane(&scroll_free, lane_count, comment.time);
                let y = 20.0 + lane as f64 * line_height + line_height;
                let speed = (width + estimated_width) / options.scroll_seconds.max(3.0);
                let duration = ((width + estimated_width) / speed).clamp(4.0, 12.0);
                let start = comment.time;
                let end = start + duration;
                let (from, to) = if comment.mode == 6 {
                    (-estimated_width, width)
                } else {
                    (width, -estimated_width)
                };
                let _ = writeln!(
                    output,
                    "Dialogue: 0,{},{},Videleum Danmaku,,0,0,0,,{{\\move({:.1},{:.1},{:.1},{:.1})\\fs{:.1}\\c{}}} {}",
                    ass_time(start),
                    ass_time(end),
                    from,
                    y,
                    to,
                    y,
                    font_size / style_font_size * 40.0,
                    color,
                    text
                );
                let lane_index = lane.min(scroll_free.len().saturating_sub(1));
                scroll_free[lane_index] = end + 0.05;
            }
            4 | 5 => {
                let is_top = comment.mode == 5;
                let lanes = if is_top {
                    &mut top_free
                } else {
                    &mut bottom_free
                };
                let lane_count = ((height - 80.0) / line_height).floor().max(1.0) as usize;
                let lane = pick_lane(lanes, lane_count, comment.time);
                let duration = 4.5;
                let start = comment.time;
                let end = start + duration;
                let y = if is_top {
                    20.0 + lane as f64 * line_height + line_height
                } else {
                    height - 40.0 - lane as f64 * line_height
                };
                let style = if is_top {
                    "Videleum Top"
                } else {
                    "Videleum Bottom"
                };
                let _ = writeln!(
                    output,
                    "Dialogue: 0,{},{},{},,0,0,0,,{{\\pos({:.1},{:.1})\\fs{:.1}\\c{}}} {}",
                    ass_time(start),
                    ass_time(end),
                    style,
                    width / 2.0,
                    y,
                    font_size / style_font_size * 40.0,
                    color,
                    text
                );
                let lane_index = lane.min(lanes.len().saturating_sub(1));
                lanes[lane_index] = end + 0.05;
            }
            // Advanced (7), code (8) and BAS (9) comments are not plain text.
            _ => {}
        }
        let _ = base_line;
    }
    output
}

fn pick_lane(free_at: &[f64], lane_count: usize, time: f64) -> usize {
    let usable = lane_count.max(1).min(free_at.len());
    let mut best = 0;
    let mut best_free = f64::INFINITY;
    for (index, free) in free_at.iter().take(usable).enumerate() {
        if *free <= time {
            return index;
        }
        if *free < best_free {
            best_free = *free;
            best = index;
        }
    }
    best
}

fn estimate_width(text: &str, font_size: f64) -> f64 {
    let units: f64 = text
        .chars()
        .map(|character| {
            if character.is_ascii() {
                if character == ' ' { 0.32 } else { 0.56 }
            } else {
                1.0
            }
        })
        .sum();
    (units * font_size).max(40.0)
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\r' => {}
            '\n' => escaped.push_str("\\N"),
            '{' => escaped.push('｛'),
            '}' => escaped.push('｝'),
            '\\' => escaped.push('＼'),
            character => escaped.push(character),
        }
    }
    escaped
}

fn escape_style_field(value: &str) -> String {
    value.replace(',', " ")
}

fn ass_color(rgb: u32, alpha: u32) -> String {
    let red = (rgb >> 16) & 0xFF;
    let green = (rgb >> 8) & 0xFF;
    let blue = rgb & 0xFF;
    format!("&H{alpha:02X}{blue:02X}{green:02X}{red:02X}")
}

fn ass_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let whole = (seconds % 60.0).floor() as u32;
    let centiseconds = ((seconds - seconds.floor()) * 100.0).round() as u32;
    let (whole, centiseconds) = if centiseconds >= 100 {
        (whole + 1, 0)
    } else {
        (whole, centiseconds)
    };
    format!("{hours}:{minutes:02}:{whole:02}.{centiseconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<i>
 <d p="1.234,1,25,16777215,1700000000,0,abc,1">Hello &amp; welcome</d>
 <d p="2.000,5,25,16711680,0,0,abc,2">Top comment</d>
 <d p="3.000,4,25,255,0,0,abc,3">Bottom comment</d>
 <d p="4.000,7,25,255,0,0,abc,4">{"advance":true}</d>
</i>"#;

    #[test]
    fn parses_comments_and_entities() {
        let comments = parse_xml(XML.as_bytes()).unwrap();
        assert_eq!(comments.len(), 4);
        assert_eq!(comments[0].text, "Hello & welcome");
        assert_eq!(comments[1].mode, 5);
        assert_eq!(comments[2].color, 255);
    }

    #[test]
    fn writes_valid_ass_dialogues() {
        let comments = parse_xml(XML.as_bytes()).unwrap();
        let ass = to_ass(&comments, &AssOptions::default());
        assert!(ass.contains("[Script Info]"));
        assert!(ass.contains("PlayResX: 1920"));
        assert!(ass.contains("Dialogue: 0,0:00:01.23"));
        assert!(ass.contains("\\move(1920.0"));
        assert!(ass.contains("\\pos(960.0"));
        // Advanced comment is intentionally skipped.
        assert!(!ass.contains("advance"));
        assert_eq!(ass.matches("[Events]").count(), 1);
    }

    #[test]
    fn formats_times() {
        assert_eq!(ass_time(0.0), "0:00:00.00");
        assert_eq!(ass_time(61.239), "0:01:01.24");
        assert_eq!(ass_time(3661.5), "1:01:01.50");
    }
}
