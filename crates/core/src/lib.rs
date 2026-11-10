//! Videleum's platform-neutral media library core.
//!
//! This crate owns records, URL recognition, archive layout, NFO metadata,
//! danmaku conversion and the portable SQLite catalogue. It deliberately has
//! no UI or network dependencies.

pub mod archive;
pub mod danmaku;
pub mod model;
pub mod platform;
pub mod settings;
pub mod store;

pub use model::{
    Comment, DownloadJob, DownloadState, LibraryStats, MediaItem, MediaStatus, Subscription,
    format_bytes, format_date, format_duration, format_relative_time, format_seconds_short,
    format_speed, now_timestamp,
};
pub use platform::{ParseError, Platform, SourceKind, SourceRef, parse_source};
pub use settings::{Quality, Settings, ThemePreference, app_data_dir, settings_path, tools_dir};
pub use store::{ScanSummary, Store, StoreError};
