//! Persistent application preferences.
//!
//! Settings live in the per-user application data directory, while the media
//! library (including its SQLite catalogue) lives wherever the user points it.
//! That keeps the archive portable: copy the library folder and its catalogue
//! travels with it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("Couldn't create the settings folder: {0}")]
    CreateFolder(String),
    #[error("Couldn't read settings: {0}")]
    Read(String),
    #[error("Couldn't save settings: {0}")]
    Write(String),
}

/// Follow the operating system appearance, or force one mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// The highest resolution Videleum should request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    #[default]
    Best,
    P2160,
    P1440,
    P1080,
    P720,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Best => "Best available",
            Self::P2160 => "2160p",
            Self::P1440 => "1440p",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
        }
    }

    /// The `yt-dlp` format expression for this preference.
    pub fn ytdlp_format(self) -> &'static str {
        match self {
            Self::Best => "bv*+ba/b",
            Self::P2160 => "bv*[height<=2160]+ba/b[height<=2160]",
            Self::P1440 => "bv*[height<=1440]+ba/b[height<=1440]",
            Self::P1080 => "bv*[height<=1080]+ba/b[height<=1080]",
            Self::P720 => "bv*[height<=720]+ba/b[height<=720]",
        }
    }

    /// The Bilibili quality number to request.
    pub fn bilibili_qn(self) -> u32 {
        match self {
            Self::Best => 127,
            Self::P2160 => 120,
            Self::P1440 => 116,
            Self::P1080 => 80,
            Self::P720 => 64,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub library_root: PathBuf,
    pub theme: ThemePreference,
    pub max_concurrent_downloads: usize,
    pub quality: Quality,
    pub naming_template: String,
    pub ytdlp_path: Option<PathBuf>,
    pub ffmpeg_path: Option<PathBuf>,
    pub youtube_cookies: Option<PathBuf>,
    pub bilibili_cookies: Option<PathBuf>,
    pub twitch_cookies: Option<PathBuf>,
    pub fetch_comments: bool,
    pub comment_limit: usize,
    pub write_subtitles: bool,
    pub write_danmaku: bool,
    pub auto_refresh_subscriptions: bool,
    pub refresh_interval_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            library_root: default_library_root(),
            theme: ThemePreference::System,
            max_concurrent_downloads: 2,
            quality: Quality::Best,
            naming_template: "{creator}/{title} [{id}]".to_string(),
            ytdlp_path: None,
            ffmpeg_path: None,
            youtube_cookies: None,
            bilibili_cookies: None,
            twitch_cookies: None,
            fetch_comments: true,
            comment_limit: 20,
            write_subtitles: true,
            write_danmaku: true,
            auto_refresh_subscriptions: true,
            refresh_interval_minutes: 60,
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self, SettingsError> {
        let path = settings_path();
        let mut settings = if path.exists() {
            let text = fs::read_to_string(&path)
                .map_err(|error| SettingsError::Read(error.to_string()))?;
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            Self::default()
        };

        if let Some(root) = std::env::var_os("VIDELEUM_LIBRARY") {
            settings.library_root = PathBuf::from(root);
        }
        settings.apply_environment_overrides();
        settings.normalize();
        Ok(settings)
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| SettingsError::CreateFolder(error.to_string()))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| SettingsError::Write(error.to_string()))?;
        let temporary = path.with_extension("json.tmp");
        {
            let mut file = fs::File::create(&temporary)
                .map_err(|error| SettingsError::Write(error.to_string()))?;
            file.write_all(text.as_bytes())
                .map_err(|error| SettingsError::Write(error.to_string()))?;
            file.sync_all()
                .map_err(|error| SettingsError::Write(error.to_string()))?;
        }
        fs::rename(&temporary, &path).map_err(|error| SettingsError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.max_concurrent_downloads = self.max_concurrent_downloads.clamp(1, 8);
        self.comment_limit = self.comment_limit.clamp(0, 100);
        self.refresh_interval_minutes = self.refresh_interval_minutes.clamp(15, 1440);
        if self.naming_template.trim().is_empty() {
            self.naming_template = Self::default().naming_template;
        }
    }

    /// Paths that are explicit test/automation overrides. Kept separate from
    /// the persisted file so secrets and machine-specific paths never get
    /// written by accident.
    fn apply_environment_overrides(&mut self) {
        if let Some(path) = std::env::var_os("VIDELEUM_YTDLP") {
            self.ytdlp_path = Some(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("VIDELEUM_FFMPEG") {
            self.ffmpeg_path = Some(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("VIDELEUM_YT_COOKIES") {
            self.youtube_cookies = Some(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("VIDELEUM_BILI_COOKIES") {
            self.bilibili_cookies = Some(PathBuf::from(path));
        }
        if let Some(path) = std::env::var_os("VIDELEUM_TWITCH_COOKIES") {
            self.twitch_cookies = Some(PathBuf::from(path));
        }
    }

    pub fn cookies_for(&self, platform: crate::platform::Platform) -> Option<&Path> {
        match platform {
            crate::platform::Platform::YouTube => self.youtube_cookies.as_deref(),
            crate::platform::Platform::Bilibili => self.bilibili_cookies.as_deref(),
            crate::platform::Platform::Twitch => self.twitch_cookies.as_deref(),
        }
        .filter(|path| path.exists())
    }
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Videleum")
}

pub fn tools_dir() -> PathBuf {
    app_data_dir().join("bin")
}

fn default_library_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    {
        home.join("Movies").join("Videleum")
    }
    #[cfg(target_os = "windows")]
    {
        home.join("Videos").join("Videleum")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        home.join("Videleum")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_untrusted_values() {
        let mut settings = Settings {
            max_concurrent_downloads: 99,
            comment_limit: 500,
            refresh_interval_minutes: 1,
            naming_template: "  ".into(),
            ..Default::default()
        };
        settings.normalize();
        assert_eq!(settings.max_concurrent_downloads, 8);
        assert_eq!(settings.comment_limit, 100);
        assert_eq!(settings.refresh_interval_minutes, 15);
        assert_eq!(
            settings.naming_template,
            Settings::default().naming_template
        );
    }
}
