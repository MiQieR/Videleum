//! Archive layout, file naming and NFO metadata.
//!
//! The archive is deliberately readable outside Videleum. Each video gets a
//! folder with the media file, poster, creator avatar, NFO (Kodi-compatible
//! fields plus Videleum extensions), subtitles and Bilibili danmaku.

use std::fs;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;
use thiserror::Error;

use crate::model::{Comment, MediaItem, MediaStatus, format_bytes, now_timestamp};
use crate::platform::Platform;

pub const VIDEO_EXTENSIONS: [&str; 8] = ["mp4", "mkv", "webm", "flv", "mov", "m4v", "ts", "avi"];

pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

/// Resolve a naming template against a media record. Each path segment is
/// sanitized independently so users can move `{creator}` into a subfolder
/// without losing Windows filename safety.
#[derive(Clone, Debug)]
pub struct ArchiveLayout {
    pub root: PathBuf,
    pub template: String,
}

#[derive(Clone, Debug)]
pub struct ArchivePaths {
    pub folder: PathBuf,
    pub stem: String,
    pub nfo: PathBuf,
    pub poster: PathBuf,
    pub avatar: PathBuf,
    pub danmaku_xml: PathBuf,
    pub danmaku_ass: PathBuf,
    pub subtitles_dir: PathBuf,
}

impl ArchivePaths {
    pub fn video(&self, extension: &str) -> PathBuf {
        self.folder.join(format!(
            "{}.{}",
            self.stem,
            extension.trim_start_matches('.')
        ))
    }

    pub fn temporary_dir(&self) -> PathBuf {
        self.folder.join(".videleum-tmp")
    }
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("Couldn't create the archive folder: {0}")]
    CreateFolder(#[source] io::Error),
    #[error("Couldn't write metadata: {0}")]
    Write(#[source] io::Error),
    #[error("Couldn't read metadata: {0}")]
    Read(#[source] io::Error),
    #[error("Couldn't parse metadata: {0}")]
    Parse(String),
}

impl ArchiveLayout {
    pub fn new(root: impl Into<PathBuf>, template: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            template: template.into(),
        }
    }

    pub fn paths_for(&self, item: &MediaItem) -> ArchivePaths {
        let folder = self.folder_for(item);
        let stem = sanitize_component(&format!(
            "{} [{}]",
            truncate(&item.title, 90),
            truncate(&item.source_id, 24)
        ));
        let stem = if stem.is_empty() {
            "Untitled".to_string()
        } else {
            stem
        };
        ArchivePaths {
            nfo: folder.join(format!("{stem}.nfo")),
            poster: folder.join("poster.jpg"),
            avatar: folder.join("avatar.jpg"),
            danmaku_xml: folder.join("danmaku.xml"),
            danmaku_ass: folder.join("danmaku.ass"),
            subtitles_dir: folder.join("subtitles"),
            folder,
            stem,
        }
    }

    pub fn folder_for(&self, item: &MediaItem) -> PathBuf {
        let rendered = self.render_template(item);
        let mut path = self.root.clone();
        for segment in rendered.split(['/', '\\']) {
            let segment = sanitize_component(segment);
            if !segment.is_empty() {
                path.push(segment);
            }
        }
        // A flat template must not let videos share one folder. The source id
        // is already sanitized and unique within a platform.
        if path == self.root {
            path.push(format!(
                "{} [{}]",
                truncate(&item.title, 90),
                truncate(&item.source_id, 24)
            ));
        }
        path
    }

    fn render_template(&self, item: &MediaItem) -> String {
        let year = item
            .published_at
            .and_then(|timestamp| {
                use chrono::{DateTime, Datelike, Utc};
                DateTime::<Utc>::from_timestamp(timestamp, 0).map(|date| date.year().to_string())
            })
            .unwrap_or_default();
        let mut rendered = self.template.clone();
        let creator = sanitize_component(&item.creator);
        let title = sanitize_component(&item.title);
        let source_id = sanitize_component(&item.source_id);
        for (token, value) in [
            ("{creator}", creator.as_str()),
            ("{title}", title.as_str()),
            ("{id}", source_id.as_str()),
            ("{platform}", item.platform.as_str()),
            ("{year}", year.as_str()),
        ] {
            rendered = rendered.replace(token, value);
        }
        rendered
    }

    pub fn prepare(&self, item: &MediaItem) -> Result<ArchivePaths, ArchiveError> {
        let paths = self.paths_for(item);
        fs::create_dir_all(&paths.folder).map_err(ArchiveError::CreateFolder)?;
        if !paths.subtitles_dir.exists() {
            fs::create_dir_all(&paths.subtitles_dir).map_err(ArchiveError::CreateFolder)?;
        }
        Ok(paths)
    }
}

/// Make a single path component safe on Windows, macOS and Linux. This is
/// intentionally conservative: the archive should move between machines.
pub fn sanitize_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => output.push('_'),
            c if c.is_control() => output.push(' '),
            c => output.push(c),
        }
    }
    let mut output = output
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(['.', ' '])
        .to_string();
    if output.is_empty() {
        output.push_str("Untitled");
    }
    let upper = output.to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&upper.as_str()) {
        output.push('_');
    }
    truncate(&output, 120).to_string()
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    output.push('…');
    output
}

pub fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => escaped.push(' '),
            c => escaped.push(c),
        }
    }
    escaped
}

fn iso8601(timestamp: Option<i64>) -> String {
    use chrono::{DateTime, Utc};
    timestamp
        .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
        .map(|date| date.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// Write a Kodi-friendly NFO with Videleum's extra fields.
pub fn write_nfo(item: &MediaItem, paths: &ArchivePaths) -> Result<(), ArchiveError> {
    let mut xml = String::with_capacity(4096);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    xml.push_str("<movie>\n");
    push(&mut xml, "title", &item.title);
    push(&mut xml, "originaltitle", &item.title);
    push(&mut xml, "sorttitle", &item.title);
    push(&mut xml, "plot", &item.description);
    push(&mut xml, "outline", &summarize(&item.description));
    push(&mut xml, "studio", item.creator.as_str());
    push(&mut xml, "director", item.creator.as_str());
    push(&mut xml, "generator", "Videleum");
    push(&mut xml, "platform", item.platform.as_str());
    push(
        &mut xml,
        "creator_id",
        item.creator_id.as_deref().unwrap_or(""),
    );
    push(&mut xml, "source_url", &item.source_url);
    if let Some(duration) = item.duration_secs {
        push(&mut xml, "runtime", &(duration / 60).max(1).to_string());
    }
    if let Some(published) = item.published_at {
        use chrono::{DateTime, Datelike, Utc};
        if let Some(date) = DateTime::<Utc>::from_timestamp(published, 0) {
            push(&mut xml, "premiered", &date.format("%Y-%m-%d").to_string());
            push(&mut xml, "year", &date.year().to_string());
        }
        push(&mut xml, "aired", &iso8601(Some(published)));
    }
    push(&mut xml, "dateadded", &iso8601(Some(item.added_at)));
    push(&mut xml, "thumb", "poster.jpg");
    push(&mut xml, "fanart", "poster.jpg");
    for tag in &item.tags {
        push(&mut xml, "tag", tag);
    }
    push(&mut xml, "genre", item.platform.label());
    // The platform belongs in the type attribute, matching Kodi's convention.
    xml.push_str(&format!(
        "  <uniqueid type=\"{}\" default=\"true\">{}</uniqueid>\n",
        item.platform.as_str(),
        xml_escape(&item.source_id)
    ));
    xml.push_str("  <actor>\n");
    push_indented(&mut xml, 4, "name", &item.creator);
    push_indented(&mut xml, 4, "role", "Creator");
    if item.avatar_path.is_some() {
        push_indented(&mut xml, 4, "thumb", "avatar.jpg");
    }
    xml.push_str("  </actor>\n");
    if item.view_count.is_some() || item.like_count.is_some() || item.comment_count.is_some() {
        xml.push_str("  <videleum>\n");
        if let Some(value) = item.view_count {
            push_indented(&mut xml, 4, "view_count", &value.to_string());
        }
        if let Some(value) = item.like_count {
            push_indented(&mut xml, 4, "like_count", &value.to_string());
        }
        if let Some(value) = item.comment_count {
            push_indented(&mut xml, 4, "comment_count", &value.to_string());
        }
        push_indented(&mut xml, 4, "file_size", &item.file_size.to_string());
        xml.push_str("  </videleum>\n");
    }
    if !item.comments.is_empty() {
        xml.push_str("  <comments>\n");
        for comment in &item.comments {
            xml.push_str("    <comment>\n");
            push_indented(&mut xml, 6, "author", &comment.author);
            if let Some(likes) = comment.likes {
                push_indented(&mut xml, 6, "likes", &likes.to_string());
            }
            if let Some(created) = comment.created_at {
                push_indented(&mut xml, 6, "time", &created.to_string());
            }
            push_indented(&mut xml, 6, "text", &comment.text);
            xml.push_str("    </comment>\n");
        }
        xml.push_str("  </comments>\n");
    }
    if !item.subtitle_paths.is_empty() {
        xml.push_str("  <subtitles>\n");
        for subtitle in &item.subtitle_paths {
            if let Some(name) = subtitle.file_name().and_then(|name| name.to_str()) {
                xml.push_str(&format!(
                    "    <subtitle>subtitles/{}</subtitle>\n",
                    xml_escape(name)
                ));
            }
        }
        xml.push_str("  </subtitles>\n");
    }
    xml.push_str("</movie>\n");

    let mut file = fs::File::create(&paths.nfo).map_err(ArchiveError::Write)?;
    file.write_all(xml.as_bytes())
        .map_err(ArchiveError::Write)?;
    file.sync_all().map_err(ArchiveError::Write)?;
    Ok(())
}

fn push(xml: &mut String, tag: &str, value: &str) {
    push_indented(xml, 2, tag, value);
}

fn push_indented(xml: &mut String, indent: usize, tag: &str, value: &str) {
    let spaces = " ".repeat(indent);
    xml.push_str(&format!("{spaces}<{tag}>{}</{tag}>\n", xml_escape(value)));
}

fn summarize(description: &str) -> String {
    let single_line = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() > 240 {
        let mut short: String = single_line.chars().take(239).collect();
        short.push('…');
        short
    } else {
        single_line
    }
}

fn append_reference(output: &mut String, event: &quick_xml::events::BytesRef<'_>) {
    if let Ok(Some(character)) = event.resolve_char_ref() {
        output.push(character);
        return;
    }
    if let Ok(name) = event.decode() {
        match name.as_ref() {
            "amp" => output.push('&'),
            "lt" => output.push('<'),
            "gt" => output.push('>'),
            "quot" => output.push('"'),
            "apos" => output.push('\''),
            _ => {}
        }
    }
}

/// Import a Videleum NFO folder back into the catalogue.
pub fn read_nfo(path: &Path) -> Result<MediaItem, ArchiveError> {
    let file = fs::File::open(path).map_err(ArchiveError::Read)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut item = MediaItem {
        nfo_path: Some(path.to_path_buf()),
        folder_path: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };
    let mut current_comment: Option<Comment> = None;
    let mut unique_type = String::new();
    let mut subtitle_names: Vec<String> = Vec::new();
    let mut text_buffer = String::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                if name == "uniqueid" {
                    for attribute in event.attributes().flatten() {
                        if attribute.key.as_ref() == b"type" {
                            unique_type = String::from_utf8_lossy(&attribute.value).to_string();
                        }
                    }
                }
                text_buffer.clear();
                stack.push(name);
            }
            Ok(Event::Text(event)) => {
                if let Ok(text) = event.decode() {
                    text_buffer.push_str(&text);
                }
            }
            Ok(Event::GeneralRef(event)) => {
                append_reference(&mut text_buffer, &event);
            }
            Ok(Event::CData(event)) => {
                text_buffer.push_str(&String::from_utf8_lossy(&event));
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                let text = text_buffer.trim().to_string();
                let in_comment = stack.iter().any(|name| name == "comment");
                if !text.is_empty() {
                    if in_comment {
                        let comment = current_comment.get_or_insert_with(Comment::default);
                        match name.as_str() {
                            "author" => comment.author = text,
                            "text" => comment.text = text,
                            "likes" => comment.likes = text.parse().ok(),
                            "time" => comment.created_at = text.parse().ok(),
                            _ => {}
                        }
                    } else {
                        match name.as_str() {
                            "title" if item.title.is_empty() => item.title = text,
                            "plot" => item.description = text,
                            "studio" => item.creator = text,
                            "platform" => {
                                if let Some(platform) = Platform::from_str(&text) {
                                    item.platform = platform;
                                }
                            }
                            "creator_id" => item.creator_id = Some(text),
                            "source_url" => item.source_url = text,
                            "runtime" => {
                                item.duration_secs =
                                    text.parse::<u32>().ok().map(|minutes| minutes * 60)
                            }
                            "premiered" => {
                                use chrono::NaiveDate;
                                if let Ok(date) = NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
                                {
                                    item.published_at = date
                                        .and_hms_opt(0, 0, 0)
                                        .map(|value| value.and_utc().timestamp());
                                }
                            }
                            "aired" | "dateadded" => {
                                use chrono::DateTime;
                                if item.added_at == 0
                                    && name == "dateadded"
                                    && let Ok(date) = DateTime::parse_from_rfc3339(text.trim())
                                {
                                    item.added_at = date.timestamp();
                                }
                            }
                            "tag" => item.tags.push(text),
                            "uniqueid" => item.source_id = text,
                            "subtitle" => subtitle_names.push(text),
                            "view_count" => item.view_count = text.parse().ok(),
                            "like_count" => item.like_count = text.parse().ok(),
                            "comment_count" => item.comment_count = text.parse().ok(),
                            "file_size" => item.file_size = text.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
                if name == "comment"
                    && let Some(comment) = current_comment.take()
                {
                    item.comments.push(comment);
                }
                text_buffer.clear();
                stack.pop();
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Eof) => break,
            Err(error) => return Err(ArchiveError::Parse(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }

    if !unique_type.is_empty()
        && let Some(platform) = Platform::from_str(&unique_type)
    {
        item.platform = platform;
    }
    if item.source_id.is_empty() {
        // A Videleum archive always carries a uniqueid; without one this is
        // some other NFO dialect and is left for the user to import manually.
        return Err(ArchiveError::Parse("missing uniqueid".to_string()));
    }
    if item.title.is_empty() {
        item.title = item
            .folder_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_string();
    }
    if item.creator.is_empty() {
        item.creator = item
            .folder_path
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown creator")
            .to_string();
    }
    if item.added_at == 0 {
        item.added_at = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_else(now_timestamp);
    }

    let folder = item.folder_path.clone();
    if let Ok(entries) = fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_video_file(&path) {
                item.file_size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                item.video_path = Some(path);
                break;
            }
        }
    }
    let poster = folder.join("poster.jpg");
    if poster.exists() {
        item.cover_path = Some(poster);
    }
    let avatar = folder.join("avatar.jpg");
    if avatar.exists() {
        item.avatar_path = Some(avatar);
    }
    let danmaku = folder.join("danmaku.ass");
    if danmaku.exists() {
        item.danmaku_path = Some(danmaku);
    }
    for name in subtitle_names {
        let path = folder.join(name);
        if path.exists() {
            item.subtitle_paths.push(path);
        }
    }
    if item.subtitle_paths.is_empty() {
        let subtitles = folder.join("subtitles");
        if let Ok(entries) = fs::read_dir(subtitles) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(
                            extension.to_ascii_lowercase().as_str(),
                            "srt" | "ass" | "vtt"
                        )
                    })
                {
                    item.subtitle_paths.push(path);
                }
            }
        }
    }
    item.status = if item.video_path.is_some() {
        MediaStatus::Ready
    } else {
        MediaStatus::Missing
    };
    item.updated_at = now_timestamp();
    Ok(item)
}

/// Walk a library root and return every NFO in a media folder.
pub fn discover_nfos(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let media_root = root.join("media");
    let start = if media_root.exists() {
        media_root
    } else {
        root.to_path_buf()
    };
    walk(&start, 0, &mut found);
    found
}

fn walk(directory: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if name.starts_with('.') || name == "subtitles" {
                continue;
            }
            walk(&path, depth + 1, found);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("nfo"))
        {
            found.push(path);
        }
    }
}

pub fn human_size(item: &MediaItem) -> String {
    format_bytes(item.file_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Comment, MediaItem};
    use crate::platform::Platform;

    fn sample() -> MediaItem {
        MediaItem {
            platform: Platform::Bilibili,
            source_id: "BV1xx411c7mD".into(),
            source_url: "https://www.bilibili.com/video/BV1xx411c7mD".into(),
            title: "A/B: testing?".into(),
            creator: "UP 主".into(),
            description: "Line one\nLine two & more".into(),
            tags: vec!["tech".into(), "中文".into()],
            comments: vec![Comment {
                author: "alice".into(),
                text: "Nice <video>".into(),
                likes: Some(4),
                created_at: Some(1_700_000_000),
            }],
            published_at: Some(1_700_000_000),
            duration_secs: Some(125),
            view_count: Some(10),
            added_at: 1_700_000_000,
            ..Default::default()
        }
    }

    #[test]
    fn sanitizes_cross_platform_names() {
        assert_eq!(sanitize_component("a/b:c*?"), "a_b_c__");
        assert_eq!(sanitize_component("CON"), "CON_");
        assert_eq!(sanitize_component("  hello.  "), "hello");
        assert_eq!(sanitize_component("中文 标题"), "中文 标题");
    }

    #[test]
    fn renders_template_segments() {
        let layout = ArchiveLayout::new("/library", "{platform}/{creator}/{title} [{id}]");
        let paths = layout.paths_for(&sample());
        assert_eq!(
            paths.folder,
            PathBuf::from("/library/bilibili/UP 主/A_B_ testing_ [BV1xx411c7mD]")
        );
        assert!(paths.stem.contains("BV1xx411c7mD"));
    }

    #[test]
    fn nfo_round_trip_preserves_core_fields() {
        let directory =
            std::env::temp_dir().join(format!("videleum-nfo-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let layout = ArchiveLayout::new(&directory, "{creator}/{title} [{id}]");
        let mut item = sample();
        let paths = layout.prepare(&item).unwrap();
        let video = paths.video("mp4");
        fs::write(&video, b"fake video").unwrap();
        item.video_path = Some(video);
        item.file_size = 10;
        write_nfo(&item, &paths).unwrap();

        let read = read_nfo(&paths.nfo).unwrap();
        assert_eq!(read.title, item.title);
        assert_eq!(read.source_id, item.source_id);
        assert_eq!(read.platform, item.platform);
        assert_eq!(read.creator, item.creator);
        assert_eq!(read.tags, item.tags);
        assert_eq!(read.comments.len(), 1);
        assert_eq!(read.comments[0].text, "Nice <video>");
        assert_eq!(read.duration_secs, Some(120));
        assert_eq!(read.status, MediaStatus::Ready);
        let _ = fs::remove_dir_all(&directory);
    }
}
