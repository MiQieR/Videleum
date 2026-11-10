//! Platform adapters that turn links into archived media.
//!
//! YouTube and Twitch go through `yt-dlp`; Bilibili uses its public web API so
//! the app can keep login cookies, pick DASH streams and fetch danmaku without
//! shelling out to a tool that does not understand the platform well.
//!
//! Everything here is blocking. The desktop app runs these calls on worker
//! threads so GPUI never waits on the network.

use thiserror::Error;

pub mod bilibili;
pub mod download;
pub mod http;
pub mod install;
pub mod organize;
pub mod subscriptions;
pub mod tool;
pub mod ytdlp;

pub use download::{DownloadRequest, JobEvent};
pub use install::{ToolStatus, install_ytdlp};
pub use subscriptions::{FeedEntry, FeedPage, fetch_feed};

/// Everything that can go wrong while fetching or archiving media.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("Couldn't reach the network: {0}")]
    Network(String),
    #[error("{0}")]
    Api(String),
    #[error("Couldn't understand the response: {0}")]
    Parse(String),
    #[error("Couldn't use files on this computer: {0}")]
    Io(String),
    #[error("{0}")]
    Tool(String),
    #[error("The download was canceled")]
    Canceled,
    #[error("{0}")]
    Unsupported(String),
}

impl FetchError {
    pub fn is_canceled(&self) -> bool {
        matches!(self, Self::Canceled)
    }

    pub fn io(error: impl std::fmt::Display) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<std::io::Error> for FetchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<videleum_core::archive::ArchiveError> for FetchError {
    fn from(error: videleum_core::archive::ArchiveError) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<videleum_core::StoreError> for FetchError {
    fn from(error: videleum_core::StoreError) -> Self {
        Self::Io(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, FetchError>;

/// Byte progress shared by the yt-dlp and Bilibili download paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed: Option<u64>,
    pub eta: Option<u64>,
}

impl TransferProgress {
    pub fn fraction(self) -> f32 {
        match self.total {
            Some(total) if total > 0 => {
                (self.downloaded as f64 / total as f64).clamp(0.0, 1.0) as f32
            }
            _ => 0.0,
        }
    }
}

/// `true` when the platform needs a login cookie for most content.
pub fn platform_needs_login(platform: videleum_core::Platform) -> bool {
    matches!(platform, videleum_core::Platform::Bilibili)
}
