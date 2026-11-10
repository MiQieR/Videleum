//! Turning downloaded files into a portable archive folder.
//!
//! Every video ends up in its own folder with a readable name, a poster, the
//! creator avatar, an NFO, subtitles and (for Bilibili) danmaku. Files are
//! moved, not copied, so temporary downloads leave nothing behind.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{ImageFormat, ImageReader};
use videleum_core::Platform;
use videleum_core::archive::{ArchiveLayout, ArchivePaths, write_nfo};
use videleum_core::danmaku::{AssOptions, parse_xml, to_ass};
use videleum_core::model::{MediaItem, MediaStatus, now_timestamp};

use crate::{FetchError, Result};

#[derive(Clone, Debug)]
pub struct ArchiveInput {
    pub item: MediaItem,
    pub video_source: PathBuf,
    pub cover_source: Option<PathBuf>,
    pub avatar_source: Option<PathBuf>,
    pub subtitles: Vec<PathBuf>,
    pub danmaku_xml: Option<PathBuf>,
}

/// Move a completed download into its archive folder and write all sidecars.
pub fn archive_media(layout: &ArchiveLayout, input: ArchiveInput) -> Result<MediaItem> {
    let mut item = input.item;
    let paths = layout.prepare(&item)?;

    let extension = input
        .video_source
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty())
        .unwrap_or("mp4");
    let video_path = paths.video(extension);
    move_file(&input.video_source, &video_path)?;
    item.file_size = fs::metadata(&video_path)
        .map(|meta| meta.len())
        .unwrap_or(0);
    item.video_path = Some(video_path);

    item.cover_path = match input.cover_source {
        Some(source) if source.exists() => convert_image(&source, &paths.poster)
            .ok()
            .map(|_| paths.poster.clone()),
        _ => None,
    };
    item.avatar_path = match input.avatar_source {
        Some(source) if source.exists() => convert_image(&source, &paths.avatar)
            .ok()
            .map(|_| paths.avatar.clone()),
        _ => None,
    };

    item.subtitle_paths = move_subtitles(&input.subtitles, &paths);

    if let Some(xml_source) = input.danmaku_xml.filter(|path| path.exists()) {
        item.danmaku_path = Some(archive_danmaku(&xml_source, &paths).unwrap_or_else(|_| {
            // Keep the XML even if the renderer rejected it.
            paths.danmaku_xml.clone()
        }));
        if item.danmaku_path.as_deref() == Some(paths.danmaku_xml.as_path()) {
            let _ = fs::copy(&xml_source, &paths.danmaku_xml);
        }
    }

    item.nfo_path = Some(paths.nfo.clone());
    item.folder_path = paths.folder.clone();
    item.status = MediaStatus::Ready;
    item.updated_at = now_timestamp();
    if item.added_at == 0 {
        item.added_at = item.updated_at;
    }
    write_nfo(&item, &paths)?;
    Ok(item)
}

fn move_subtitles(sources: &[PathBuf], paths: &ArchivePaths) -> Vec<PathBuf> {
    let mut moved = Vec::new();
    for source in sources {
        if !source.exists() {
            continue;
        }
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("srt")
            .to_ascii_lowercase();
        let language = language_hint(source).unwrap_or_else(|| "subtitle".to_string());
        let mut name = format!("{language}.{extension}");
        let mut destination = paths.subtitles_dir.join(&name);
        let mut index = 2;
        while destination.exists() {
            name = format!("{language}-{index}.{extension}");
            destination = paths.subtitles_dir.join(&name);
            index += 1;
        }
        if let Some(parent) = destination.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if move_file(source, &destination).is_ok() {
            moved.push(destination);
        }
    }
    moved
}

fn language_hint(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if !stem.contains('.') {
        return None;
    }
    let candidate = stem.rsplit('.').next()?.trim();
    let valid = (2..=12).contains(&candidate.len())
        && candidate
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    valid.then(|| candidate.to_string())
}

fn archive_danmaku(xml_source: &Path, paths: &ArchivePaths) -> Result<PathBuf> {
    let bytes = fs::read(xml_source)?;
    fs::write(&paths.danmaku_xml, &bytes)?;
    let comments = parse_xml(&bytes).map_err(|error| FetchError::Parse(error.to_string()))?;
    if comments.is_empty() {
        return Ok(paths.danmaku_xml.clone());
    }
    let ass = to_ass(&comments, &AssOptions::default());
    fs::write(&paths.danmaku_ass, ass)?;
    Ok(paths.danmaku_ass.clone())
}

/// Copy or move a file, falling back to copy+remove across volumes.
pub fn move_file(source: &Path, destination: &Path) -> Result<()> {
    if source == destination {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.exists() {
        let _ = fs::remove_file(destination);
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination)?;
            fs::remove_file(source)?;
            Ok(())
        }
    }
}

/// Decode an image and write a JPEG poster/avatar. JPEG inputs are copied
/// untouched so the original quality is preserved.
pub fn convert_image(source: &Path, destination: &Path) -> Result<()> {
    let bytes = fs::read(source)?;
    write_jpeg(&bytes, destination)
}

pub fn write_jpeg(bytes: &[u8], destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if image::guess_format(bytes).ok() == Some(ImageFormat::Jpeg) {
        fs::write(destination, bytes)?;
        return Ok(());
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| FetchError::Io(error.to_string()))?;
    let image = reader
        .decode()
        .map_err(|error| FetchError::Io(format!("Couldn't decode the artwork: {error}")))?;
    let rgb = image.to_rgb8();
    let mut file = fs::File::create(destination)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 92);
    encoder
        .encode_image(&rgb)
        .map_err(|error| FetchError::Io(format!("Couldn't write the artwork: {error}")))?;
    Ok(())
}

/// Store subscription avatars in the catalogue's cache, not in a media folder.
pub fn save_subscription_avatar(
    library_root: &Path,
    platform: Platform,
    source_id: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    let folder = library_root.join(".videleum").join("cache").join("avatars");
    fs::create_dir_all(&folder)?;
    let name = format!(
        "{}-{}.jpg",
        platform.as_str(),
        videleum_core::archive::sanitize_component(source_id)
    );
    let destination = folder.join(name);
    write_jpeg(bytes, &destination)?;
    Ok(destination)
}

/// Discover the video produced by a completed download into a temp folder.
pub fn find_video(directory: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(directory).ok()?;
    let mut best: Option<(PathBuf, u64)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || !videleum_core::archive::is_video_file(&path) {
            continue;
        }
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        if best.as_ref().is_none_or(|(_, current)| size > *current) {
            best = Some((path, size));
        }
    }
    best.map(|(path, _)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("videleum-organize-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn archives_video_and_sidecars() {
        let root = temporary("archive");
        let downloads = temporary("downloads");
        let video = downloads.join("media.mp4");
        fs::write(&video, b"fake video bytes").unwrap();
        let subtitle = downloads.join("media.en.srt");
        fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\nHi\n").unwrap();
        let danmaku = downloads.join("danmaku.xml");
        fs::write(
            &danmaku,
            br#"<?xml version="1.0"?><i><d p="1,1,25,0,0,0,0,1">hi</d></i>"#,
        )
        .unwrap();

        let layout = ArchiveLayout::new(&root, "{creator}/{title} [{id}]");
        let item = MediaItem {
            platform: Platform::Bilibili,
            source_id: "BV1xx".into(),
            title: "Test video".into(),
            creator: "Someone".into(),
            added_at: 10,
            ..Default::default()
        };
        let archived = archive_media(
            &layout,
            ArchiveInput {
                item,
                video_source: video,
                cover_source: None,
                avatar_source: None,
                subtitles: vec![subtitle],
                danmaku_xml: Some(danmaku),
            },
        )
        .unwrap();

        assert_eq!(archived.status, MediaStatus::Ready);
        assert_eq!(archived.file_size, 16);
        assert!(archived.video_path.as_ref().unwrap().exists());
        assert_eq!(archived.subtitle_paths.len(), 1);
        assert!(archived.subtitle_paths[0].ends_with("en.srt"));
        assert!(archived.danmaku_path.as_ref().unwrap().exists());
        assert!(archived.nfo_path.as_ref().unwrap().exists());
        assert!(
            fs::read_to_string(archived.nfo_path.as_ref().unwrap())
                .unwrap()
                .contains("BV1xx")
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&downloads);
    }

    #[test]
    fn reencodes_png_covers_to_jpeg() {
        let root = temporary("cover");
        let source = root.join("cover.png");
        let image = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 30, 90]));
        image.save(&source).unwrap();
        let destination = root.join("poster.jpg");
        convert_image(&source, &destination).unwrap();
        let bytes = fs::read(&destination).unwrap();
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn finds_language_suffixes() {
        assert_eq!(
            language_hint(Path::new("media.zh-Hans.srt")).as_deref(),
            Some("zh-Hans")
        );
        assert_eq!(
            language_hint(Path::new("subtitle.en.srt")).as_deref(),
            Some("en")
        );
        assert_eq!(language_hint(Path::new("media.srt")), None);
    }
}
