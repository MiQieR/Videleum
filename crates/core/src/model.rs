//! Core records shared by storage, the fetchers and the desktop UI.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::platform::{Platform, SourceKind};

/// A comment captured in the NFO file for offline reference.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Comment {
    pub author: String,
    pub text: String,
    pub likes: Option<u64>,
    /// Unix timestamp in seconds.
    pub created_at: Option<i64>,
}

/// Availability of the archived copy on disk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    /// Metadata was fetched and a download is queued or running.
    Pending,
    #[default]
    Ready,
    /// The database entry exists but the video file is gone.
    Missing,
    Failed,
}

impl MediaStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Failed => "failed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "missing" => Self::Missing,
            "failed" => Self::Failed,
            _ => Self::Ready,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Downloading",
            Self::Ready => "Ready",
            Self::Missing => "File missing",
            Self::Failed => "Failed",
        }
    }
}

/// One archived video and its sidecar files.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: i64,
    pub platform: Platform,
    pub source_id: String,
    pub source_url: String,
    pub title: String,
    pub creator: String,
    pub creator_id: Option<String>,
    pub description: String,
    pub published_at: Option<i64>,
    pub duration_secs: Option<u32>,
    pub tags: Vec<String>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub comments: Vec<Comment>,
    pub cover_path: Option<PathBuf>,
    pub avatar_path: Option<PathBuf>,
    pub video_path: Option<PathBuf>,
    pub nfo_path: Option<PathBuf>,
    pub subtitle_paths: Vec<PathBuf>,
    pub danmaku_path: Option<PathBuf>,
    pub folder_path: PathBuf,
    pub file_size: u64,
    pub status: MediaStatus,
    pub watched: bool,
    pub favorite: bool,
    pub added_at: i64,
    pub updated_at: i64,
}

impl MediaItem {
    pub fn key(&self) -> (Platform, &str) {
        (self.platform, self.source_id.as_str())
    }

    pub fn published_label(&self) -> String {
        self.published_at
            .map(format_date)
            .unwrap_or_else(|| "Unknown date".to_string())
    }

    pub fn duration_label(&self) -> String {
        self.duration_secs.map(format_duration).unwrap_or_default()
    }

    pub fn has_video(&self) -> bool {
        self.video_path.as_ref().is_some_and(|path| path.exists())
    }
}

/// A channel, playlist or favorites list followed for new videos.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Subscription {
    pub id: i64,
    pub platform: Platform,
    pub kind: SourceKind,
    pub source_id: String,
    pub source_url: String,
    pub title: String,
    pub avatar_path: Option<PathBuf>,
    pub auto_download: bool,
    pub refresh_interval_minutes: u32,
    pub last_checked_at: Option<i64>,
    pub last_status: String,
    pub created_at: i64,
}

/// A queued or historical download.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    #[default]
    Queued,
    Fetching,
    Downloading,
    Processing,
    Completed,
    Failed,
    Canceled,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Fetching => "fetching",
            Self::Downloading => "downloading",
            Self::Processing => "processing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Self {
        match value {
            "fetching" => Self::Fetching,
            "downloading" => Self::Downloading,
            "processing" => Self::Processing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "canceled" => Self::Canceled,
            _ => Self::Queued,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "Queued",
            Self::Fetching => "Fetching details",
            Self::Downloading => "Downloading",
            Self::Processing => "Organizing files",
            Self::Completed => "Completed",
            Self::Failed => "Failed",
            Self::Canceled => "Canceled",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Fetching | Self::Downloading | Self::Processing
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled)
    }
}

/// A download job as shown in the queue and persisted across launches.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DownloadJob {
    pub id: i64,
    pub media_id: Option<i64>,
    pub source_url: String,
    pub platform: Platform,
    pub source_id: String,
    pub title: String,
    pub creator: String,
    pub cover_path: Option<PathBuf>,
    pub state: DownloadState,
    /// Zero to one.
    pub progress: f32,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub speed_bps: Option<u64>,
    pub eta_secs: Option<u64>,
    pub stage: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DownloadJob {
    pub fn display_title(&self) -> String {
        if self.title.trim().is_empty() {
            self.source_url.clone()
        } else {
            self.title.clone()
        }
    }

    pub fn progress_label(&self) -> String {
        if self.state == DownloadState::Downloading {
            format!("{:.0}%", self.progress * 100.0)
        } else {
            self.state.label().to_string()
        }
    }
}

/// Aggregate library numbers for the sidebar and settings.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct LibraryStats {
    pub item_count: usize,
    pub total_bytes: u64,
    pub platform_counts: [usize; 3],
    pub missing_count: usize,
}

pub fn format_duration(seconds: u32) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

pub fn format_speed(bytes_per_second: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_second))
}

pub fn format_seconds_short(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

pub fn format_date(timestamp: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|| "Unknown date".to_string())
}

pub fn format_relative_time(timestamp: i64, now: i64) -> String {
    let delta = (now - timestamp).max(0);
    match delta {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m ago", delta / 60),
        3600..=86_399 => format!("{}h ago", delta / 3600),
        86_400..=604_799 => format!("{}d ago", delta / 86_400),
        _ => format_date(timestamp),
    }
}

pub fn now_timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}
