//! yt-dlp adapter used for YouTube and Twitch.
//!
//! The tool is treated as an implementation detail: Videleum reads its JSON
//! output, parses the machine-readable progress template and finds the merged
//! file in a private temporary folder.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;
use videleum_core::archive::is_video_file;
use videleum_core::model::Comment;
use videleum_core::{Platform, Quality, Settings};

use crate::tool;
use crate::{FetchError, Result, TransferProgress};

const PROGRESS_PREFIX: &str = "VIDELEUM|";
const METADATA_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Default)]
pub struct YtMetadata {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub uploader_id: Option<String>,
    pub description: String,
    pub timestamp: Option<i64>,
    pub duration: Option<u32>,
    pub tags: Vec<String>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
}

#[derive(Clone, Debug, Default)]
pub struct YtEntry {
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: Option<u32>,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct YtFeed {
    pub title: String,
    pub avatar_url: Option<String>,
    pub entries: Vec<YtEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct YtDownload {
    pub video: PathBuf,
    pub subtitles: Vec<PathBuf>,
    pub size: u64,
}

#[derive(Clone, Debug)]
pub struct YtOptions {
    pub subtitles: bool,
    pub merge_mp4: bool,
}

impl Default for YtOptions {
    fn default() -> Self {
        Self {
            subtitles: true,
            merge_mp4: true,
        }
    }
}

pub struct YtDlp {
    path: PathBuf,
    ffmpeg: Option<PathBuf>,
}

impl YtDlp {
    pub fn discover(settings: &Settings) -> Result<Self> {
        let path = tool::find_tool(settings.ytdlp_path.as_deref(), "yt-dlp").ok_or_else(|| {
            FetchError::Tool(
                "Videleum needs yt-dlp for YouTube and Twitch downloads. Install it from Settings."
                    .to_string(),
            )
        })?;
        let ffmpeg = tool::find_tool(settings.ffmpeg_path.as_deref(), "ffmpeg");
        Ok(Self { path, ffmpeg })
    }

    pub fn new(path: PathBuf, ffmpeg: Option<PathBuf>) -> Self {
        Self { path, ffmpeg }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn ffmpeg(&self) -> Option<&Path> {
        self.ffmpeg.as_deref()
    }

    pub fn version(&self) -> Option<String> {
        tool::run_capture(&self.path, &["--version"])
            .ok()
            .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
            .filter(|line| !line.is_empty())
    }

    fn base(&self) -> Command {
        let mut command = tool::command(&self.path);
        command.args([
            "--ignore-config",
            "--no-color",
            "--no-warnings",
            "--no-playlist",
        ]);
        command.env("PYTHONIOENCODING", "utf-8");
        command
    }

    fn add_cookies(command: &mut Command, cookies: Option<&Path>) {
        if let Some(path) = cookies.filter(|path| path.exists()) {
            command.arg("--cookies").arg(path);
        }
    }

    pub fn fetch_metadata(
        &self,
        url: &str,
        cookies: Option<&Path>,
        cancel: &AtomicBool,
    ) -> Result<YtMetadata> {
        let mut command = self.base();
        command
            .args(["--dump-single-json", "--skip-download"])
            .arg(url);
        Self::add_cookies(&mut command, cookies);
        let output = run_collect(command, cancel, METADATA_TIMEOUT)?;
        let value: Value = serde_json::from_str(&output)
            .map_err(|error| FetchError::Parse(format!("yt-dlp returned invalid JSON: {error}")))?;
        let value = first_entry(value);
        Ok(metadata_from_value(&value, url))
    }

    /// Fetch comments for a video. This is deliberately a second call: asking
    /// for comments while probing metadata makes every add-to-library slow.
    pub fn fetch_comments(
        &self,
        url: &str,
        cookies: Option<&Path>,
        limit: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<Comment>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut command = self.base();
        command
            .args([
                "--dump-single-json",
                "--skip-download",
                "--write-comments",
                "--extractor-args",
            ])
            .arg(format!("youtube:max_comments={limit}"))
            .arg(url);
        Self::add_cookies(&mut command, cookies);
        let output = run_collect(command, cancel, METADATA_TIMEOUT)?;
        let value: Value = serde_json::from_str(&output)
            .map_err(|error| FetchError::Parse(format!("yt-dlp returned invalid JSON: {error}")))?;
        let mut comments = Vec::new();
        if let Some(items) = value.get("comments").and_then(Value::as_array) {
            for item in items {
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("html").and_then(Value::as_str))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    continue;
                }
                comments.push(Comment {
                    author: item
                        .get("author")
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown")
                        .to_string(),
                    text,
                    likes: item.get("like_count").and_then(Value::as_u64),
                    created_at: item.get("timestamp").and_then(Value::as_i64),
                });
                if comments.len() >= limit {
                    break;
                }
            }
        }
        Ok(comments)
    }

    pub fn fetch_feed(
        &self,
        url: &str,
        limit: usize,
        cookies: Option<&Path>,
        cancel: &AtomicBool,
    ) -> Result<YtFeed> {
        let mut command = self.base();
        command
            .args([
                "--flat-playlist",
                "--dump-single-json",
                "--skip-download",
                "--playlist-end",
            ])
            .arg(limit.max(1).to_string())
            .arg(url);
        Self::add_cookies(&mut command, cookies);
        let output = run_collect(command, cancel, METADATA_TIMEOUT)?;
        let value: Value = serde_json::from_str(&output)
            .map_err(|error| FetchError::Parse(format!("yt-dlp returned invalid JSON: {error}")))?;
        let platform = platform_from_url(url);
        let title = value
            .get("title")
            .or_else(|| value.get("channel"))
            .or_else(|| value.get("uploader"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let avatar_url = value
            .get("thumbnail")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| largest_thumbnail(&value));
        let mut entries = Vec::new();
        if let Some(items) = value.get("entries").and_then(Value::as_array) {
            for item in items {
                if let Some(entry) = entry_from_value(item, platform) {
                    entries.push(entry);
                }
            }
        }
        Ok(YtFeed {
            title,
            avatar_url,
            entries,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn download(
        &self,
        url: &str,
        destination: &Path,
        format: &str,
        cookies: Option<&Path>,
        options: &YtOptions,
        cancel: &AtomicBool,
        mut progress: impl FnMut(TransferProgress),
    ) -> Result<YtDownload> {
        std::fs::create_dir_all(destination)?;
        let mut command = self.base();
        command
            .args(["--newline", "-f", format, "-o"])
            .arg(destination.join("media.%(ext)s"))
            .args(["--progress-template", "download:VIDELEUM|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s"])
            .args(["--retries", "3", "--fragment-retries", "5", "--concurrent-fragments", "4"])
            .args(["--no-part", "--no-keep-fragments"]);
        if options.merge_mp4 {
            command.args(["--merge-output-format", "mp4"]);
        }
        if options.subtitles {
            command.args([
                "--write-subs",
                "--write-auto-subs",
                "--sub-langs",
                "en.*,zh-Hans,zh-CN,zh.*",
                "--sub-format",
                "srt/vtt/best",
            ]);
            if self.ffmpeg.is_some() {
                command.args(["--convert-subs", "srt"]);
            }
        }
        command.arg(url);
        Self::add_cookies(&mut command, cookies);
        if let Some(ffmpeg) = &self.ffmpeg {
            command.args(["--ffmpeg-location"]).arg(ffmpeg);
        }

        run_download(command, cancel, &mut progress)?;
        collect_download(destination)
    }
}

fn platform_from_url(url: &str) -> Platform {
    if url.contains("twitch.tv") {
        Platform::Twitch
    } else {
        Platform::YouTube
    }
}

fn first_entry(value: Value) -> Value {
    if value.get("_type").and_then(Value::as_str) == Some("playlist")
        && let Some(entries) = value.get("entries").and_then(Value::as_array)
        && let Some(first) = entries.first()
        && first.is_object()
    {
        return first.clone();
    }
    value
}

fn metadata_from_value(value: &Value, requested_url: &str) -> YtMetadata {
    let id = value
        .get("id")
        .map(|id| match id {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let title = value
        .get("title")
        .or_else(|| value.get("fulltitle"))
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let uploader = value
        .get("uploader")
        .or_else(|| value.get("channel"))
        .or_else(|| value.get("creator"))
        .and_then(Value::as_str)
        .unwrap_or("Unknown creator")
        .to_string();
    let uploader_id = value
        .get("channel_id")
        .or_else(|| value.get("uploader_id"))
        .or_else(|| value.get("creator_id"))
        .and_then(value_as_string);
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_i64)
        .or_else(|| value.get("release_timestamp").and_then(Value::as_i64))
        .or_else(|| {
            value
                .get("upload_date")
                .and_then(Value::as_str)
                .and_then(parse_upload_date)
        });
    let duration = value
        .get("duration")
        .and_then(Value::as_f64)
        .map(|seconds| seconds.max(0.0).round() as u32)
        .filter(|seconds| *seconds > 0);
    let tags = value
        .get("tags")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .take(50)
                .collect()
        })
        .unwrap_or_default();
    let thumbnail = value
        .get("thumbnail")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| largest_thumbnail(value));
    let webpage_url = value
        .get("webpage_url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| requested_url.to_string());
    YtMetadata {
        id,
        title,
        uploader,
        uploader_id,
        description,
        timestamp,
        duration,
        tags,
        view_count: value.get("view_count").and_then(Value::as_u64),
        like_count: value.get("like_count").and_then(Value::as_u64),
        comment_count: value.get("comment_count").and_then(Value::as_u64),
        thumbnail,
        webpage_url,
    }
}

fn entry_from_value(value: &Value, platform: Platform) -> Option<YtEntry> {
    let id = value
        .get("id")
        .map(|id| match id {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .filter(|id| !id.is_empty())?;
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled")
        .to_string();
    let url = value
        .get("webpage_url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("url")
                .and_then(Value::as_str)
                .filter(|url| url.starts_with("http"))
                .map(str::to_string)
        })
        .unwrap_or_else(|| match platform {
            Platform::Twitch => format!("https://www.twitch.tv/videos/{id}"),
            _ => format!("https://www.youtube.com/watch?v={id}"),
        });
    let duration = value
        .get("duration")
        .and_then(Value::as_f64)
        .map(|seconds| seconds.max(0.0).round() as u32)
        .filter(|seconds| *seconds > 0);
    Some(YtEntry {
        id,
        title,
        url,
        duration,
        thumbnail: value
            .get("thumbnail")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| largest_thumbnail(value)),
    })
}

fn largest_thumbnail(value: &Value) -> Option<String> {
    let thumbnails = value.get("thumbnails")?.as_array()?;
    let mut best: Option<(&str, u64)> = None;
    for thumbnail in thumbnails {
        let Some(url) = thumbnail.get("url").and_then(Value::as_str) else {
            continue;
        };
        let score = thumbnail
            .get("preference")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0) as u64
            * 10
            + thumbnail.get("width").and_then(Value::as_u64).unwrap_or(0)
                * thumbnail.get("height").and_then(Value::as_u64).unwrap_or(0)
                / 1000;
        if best.is_none_or(|(_, current)| score >= current) {
            best = Some((url, score));
        }
    }
    best.map(|(url, _)| url.to_string())
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_upload_date(value: &str) -> Option<i64> {
    use chrono::NaiveDate;
    let date = NaiveDate::parse_from_str(value.trim(), "%Y%m%d").ok()?;
    date.and_hms_opt(12, 0, 0)
        .map(|date| date.and_utc().timestamp())
}

/// Drive a child process, streaming stdout/stderr, honoring cancellation.
fn run_collect(mut command: Command, cancel: &AtomicBool, timeout: Duration) -> Result<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| FetchError::Tool(format!("Couldn't start yt-dlp: {error}")))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (sender, receiver) = mpsc::channel::<(bool, String)>();
    if let Some(stdout) = stdout {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = sender.send((false, line));
            }
        });
    }
    if let Some(stderr) = stderr {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = sender.send((true, line));
            }
        });
    }
    drop(sender);

    let started = Instant::now();
    let mut output = String::new();
    let mut errors: VecDeque<String> = VecDeque::new();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchError::Canceled);
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchError::Tool(
                "yt-dlp took too long to respond. Check the link or your connection.".to_string(),
            ));
        }
        match receiver.recv_timeout(Duration::from_millis(120)) {
            Ok((is_error, line)) => {
                if is_error {
                    push_tail(&mut errors, &line);
                } else {
                    output.push_str(&line);
                    output.push('\n');
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break child
                    .wait()
                    .map_err(|error| FetchError::Tool(error.to_string()))?;
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| FetchError::Tool(error.to_string()))?
        {
            // Drain anything the readers already queued.
            while let Ok((is_error, line)) = receiver.try_recv() {
                if is_error {
                    push_tail(&mut errors, &line);
                } else {
                    output.push_str(&line);
                    output.push('\n');
                }
            }
            break status;
        }
    };
    if !status.success() {
        let detail = errors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let detail = tool::last_lines(&detail, 6);
        return Err(FetchError::Tool(if detail.is_empty() {
            "yt-dlp couldn't read this link".to_string()
        } else {
            detail
        }));
    }
    Ok(output)
}

fn push_tail(tail: &mut VecDeque<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    tail.push_back(line.to_string());
    while tail.len() > 24 {
        tail.pop_front();
    }
}

fn run_download(
    mut command: Command,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(TransferProgress),
) -> Result<()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| FetchError::Tool(format!("Couldn't start yt-dlp: {error}")))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (sender, receiver) = mpsc::channel::<String>();
    if let Some(stdout) = stdout {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = sender.send(line);
            }
        });
    }
    if let Some(stderr) = stderr {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let _ = sender.send(line);
            }
        });
    }
    drop(sender);

    let mut errors: VecDeque<String> = VecDeque::new();
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FetchError::Canceled);
        }
        match receiver.recv_timeout(Duration::from_millis(160)) {
            Ok(line) => {
                if let Some(value) = parse_progress_line(&line) {
                    progress(value);
                } else if !line.trim().is_empty() {
                    push_tail(&mut errors, &line);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break child
                    .wait()
                    .map_err(|error| FetchError::Tool(error.to_string()))?;
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| FetchError::Tool(error.to_string()))?
        {
            break status;
        }
    };
    if !status.success() {
        let detail = errors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let detail = tool::last_lines(&detail, 8);
        return Err(FetchError::Tool(if detail.is_empty() {
            "yt-dlp couldn't download this video".to_string()
        } else {
            detail
        }));
    }
    Ok(())
}

pub fn parse_progress_line(line: &str) -> Option<TransferProgress> {
    let payload = line.trim().strip_prefix(PROGRESS_PREFIX)?;
    let fields: Vec<&str> = payload.split('|').collect();
    if fields.len() < 5 {
        return None;
    }
    let number = |value: &str| {
        let value = value.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("NA")
            || value.eq_ignore_ascii_case("none")
        {
            None
        } else {
            value.parse::<f64>().ok().map(|value| value.max(0.0) as u64)
        }
    };
    let downloaded = number(fields[0])?;
    let total = number(fields[1]).or_else(|| number(fields[2]));
    Some(TransferProgress {
        downloaded,
        total,
        speed: number(fields[3]),
        eta: number(fields[4]),
    })
}

fn collect_download(directory: &Path) -> Result<YtDownload> {
    let mut video: Option<(PathBuf, u64)> = None;
    let mut subtitles = Vec::new();
    let entries = std::fs::read_dir(directory)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "srt" | "ass" | "vtt") {
            subtitles.push(path);
            continue;
        }
        if is_video_file(&path) {
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            if video.as_ref().is_none_or(|(_, current)| size > *current) {
                video = Some((path, size));
            }
        }
    }
    subtitles.sort();
    let Some((video, size)) = video else {
        return Err(FetchError::Tool(
            "The download finished but no video file was produced".to_string(),
        ));
    };
    Ok(YtDownload {
        video,
        subtitles,
        size,
    })
}

/// The quality format expression, with a preference for widely playable MP4.
pub fn format_for(quality: Quality) -> String {
    let limit = match quality {
        Quality::Best => return "bv*+ba/b".to_string(),
        Quality::P2160 => "height<=2160",
        Quality::P1440 => "height<=1440",
        Quality::P1080 => "height<=1080",
        Quality::P720 => "height<=720",
    };
    format!("bv*[{limit}]+ba/b[{limit}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_machine_progress() {
        let progress = parse_progress_line("VIDELEUM|1048576|8388608|NA|524288|14").unwrap();
        assert_eq!(progress.downloaded, 1_048_576);
        assert_eq!(progress.total, Some(8_388_608));
        assert_eq!(progress.speed, Some(524_288));
        assert_eq!(progress.eta, Some(14));
        assert!((progress.fraction() - 0.125).abs() < 0.0001);

        let fallback = parse_progress_line("VIDELEUM|10|NA|100|NA|NA").unwrap();
        assert_eq!(fallback.total, Some(100));
        assert_eq!(fallback.speed, None);

        assert!(parse_progress_line("[download] 12%").is_none());
        assert!(parse_progress_line("VIDELEUM|NA|NA|NA|NA|NA").is_none());
    }

    #[test]
    fn metadata_is_normalized() {
        let value: Value = serde_json::from_str(
            r#"{
                "id": "dQw4w9WgXcQ",
                "title": "A video",
                "uploader": "Someone",
                "channel_id": "UC123",
                "description": "Hello",
                "upload_date": "20240131",
                "duration": 212.4,
                "tags": ["a", "b"],
                "view_count": 10,
                "thumbnails": [
                    {"url": "https://example.com/small.jpg", "width": 120, "height": 90},
                    {"url": "https://example.com/large.jpg", "width": 1280, "height": 720}
                ]
            }"#,
        )
        .unwrap();
        let metadata = metadata_from_value(&value, "https://youtu.be/dQw4w9WgXcQ");
        assert_eq!(metadata.id, "dQw4w9WgXcQ");
        assert_eq!(metadata.uploader_id.as_deref(), Some("UC123"));
        assert_eq!(metadata.duration, Some(212));
        assert_eq!(
            metadata.thumbnail.as_deref(),
            Some("https://example.com/large.jpg")
        );
        assert_eq!(metadata.timestamp, Some(1_706_702_400));
        assert_eq!(metadata.webpage_url, "https://youtu.be/dQw4w9WgXcQ");
    }

    #[test]
    fn feed_entries_get_platform_urls() {
        let value: Value =
            serde_json::from_str(r#"{"id":"abc123","title":"Next","url":"abc123"}"#).unwrap();
        let entry = entry_from_value(&value, Platform::YouTube).unwrap();
        assert_eq!(entry.url, "https://www.youtube.com/watch?v=abc123");

        let value: Value = serde_json::from_str(r#"{"id":"99","title":"VOD"}"#).unwrap();
        let entry = entry_from_value(&value, Platform::Twitch).unwrap();
        assert_eq!(entry.url, "https://www.twitch.tv/videos/99");
    }
}
