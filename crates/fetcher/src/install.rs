//! Detecting and installing the external helper programs.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use videleum_core::{Settings, tools_dir};

use crate::tool;
use crate::{FetchError, Result, http};

#[derive(Clone, Debug, Default)]
pub struct ToolStatus {
    pub ytdlp: Option<PathBuf>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg: Option<PathBuf>,
    pub ffmpeg_version: Option<String>,
}

impl ToolStatus {
    pub fn can_download_youtube(&self) -> bool {
        self.ytdlp.is_some()
    }

    pub fn can_merge_bilibili(&self) -> bool {
        self.ffmpeg.is_some()
    }
}

pub fn detect(settings: &Settings) -> ToolStatus {
    let ytdlp = tool::find_tool(settings.ytdlp_path.as_deref(), "yt-dlp");
    let ytdlp_version = ytdlp.as_deref().and_then(version_of);
    let ffmpeg = tool::find_tool(settings.ffmpeg_path.as_deref(), "ffmpeg");
    let ffmpeg_version = ffmpeg.as_deref().and_then(|path| {
        tool::run_capture(path, &["-version"])
            .ok()
            .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
    });
    ToolStatus {
        ytdlp,
        ytdlp_version,
        ffmpeg,
        ffmpeg_version,
    }
}

fn version_of(path: &Path) -> Option<String> {
    tool::run_capture(path, &["--version"])
        .ok()
        .and_then(|text| text.lines().next().map(|line| line.trim().to_string()))
}

/// Download the standalone yt-dlp binary into Videleum's tools folder. The
/// release assets are self-contained, so no Python installation is needed.
pub fn install_ytdlp(
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf> {
    let asset = if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        "yt-dlp_macos"
    } else {
        "yt-dlp"
    };
    let url = format!("https://github.com/yt-dlp/yt-dlp/releases/latest/download/{asset}");
    let folder = tools_dir();
    std::fs::create_dir_all(&folder)?;
    let destination = folder.join(asset);
    let temporary = folder.join(format!("{asset}.download"));
    progress(0, None);
    http::download_to(
        &http::agent(),
        &url,
        &[],
        &temporary,
        cancel,
        |downloaded, total, _| progress(downloaded, total),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))?;
    }
    if destination.exists() {
        let _ = std::fs::remove_file(&destination);
    }
    std::fs::rename(&temporary, &destination)?;
    if version_of(&destination).is_none() {
        let _ = std::fs::remove_file(&destination);
        return Err(FetchError::Tool(
            "The downloaded yt-dlp build couldn't run on this computer".to_string(),
        ));
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_tools_without_panicking() {
        let status = ToolStatus::default();
        assert!(!status.can_download_youtube());
        assert!(!status.can_merge_bilibili());
    }
}
