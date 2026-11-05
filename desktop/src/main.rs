#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui_kit::{
    App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, WindowBounds, WindowOptions,
    component::{
        Disableable,
        button::Button,
        input::{Input, InputState},
    },
    div, px, rgb, rgba, size,
};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

const DEFAULT_BACKEND_PORT: u16 = 5551;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NavPage {
    Overview,
    PosterWall,
    VideoSources,
    Downloads,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PlatformFilter {
    All,
    YouTube,
    Bilibili,
    Twitch,
}

impl PlatformFilter {
    fn as_str(&self) -> &'static str {
        match self {
            PlatformFilter::All => "all",
            PlatformFilter::YouTube => "youtube",
            PlatformFilter::Bilibili => "bilibili",
            PlatformFilter::Twitch => "twitch",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct VideoItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    video_path: Option<String>,
    #[serde(default)]
    video_filename: Option<String>,
    #[serde(default)]
    thumbnail_path: Option<String>,
    #[serde(default)]
    duration: Option<serde_json::Value>,
    #[serde(default)]
    file_size: Option<serde_json::Value>,
    #[serde(default)]
    progress: Option<f64>,
    #[serde(default)]
    part_number: Option<u32>,
    #[serde(default)]
    total_parts: Option<u32>,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
}

impl VideoItem {
    fn canonical_platform(&self) -> &'static str {
        let raw = self
            .platform
            .as_deref()
            .unwrap_or(self.source.as_str())
            .trim()
            .to_lowercase();
        if raw.contains("bili") {
            "bilibili"
        } else if raw.contains("twitch") {
            "twitch"
        } else if raw.contains("youtube") || raw == "yt" {
            "youtube"
        } else {
            "other"
        }
    }

    fn platform_display_name(&self) -> &'static str {
        match self.canonical_platform() {
            "youtube" => "YouTube",
            "bilibili" => "Bilibili",
            "twitch" => "Twitch",
            _ => "Media",
        }
    }

    fn platform_badge_color(&self) -> u32 {
        match self.canonical_platform() {
            // YouTube brand red (#FF0000)
            "youtube" => 0xFF0000,
            // Bilibili logo blue (#00AEEC)
            "bilibili" => 0x00AEEC,
            // Twitch logo purple (#9146FF)
            "twitch" => 0x9146FF,
            _ => 0x475569,
        }
    }

    fn duration_seconds(&self) -> f64 {
        match &self.duration {
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
            Some(serde_json::Value::String(s)) => {
                if let Ok(sec) = s.parse::<f64>() {
                    return sec;
                }
                let parts: Vec<&str> = s.split(':').collect();
                match parts.len() {
                    2 => {
                        let m = parts[0].parse::<f64>().unwrap_or(0.0);
                        let sec = parts[1].parse::<f64>().unwrap_or(0.0);
                        m * 60.0 + sec
                    }
                    3 => {
                        let h = parts[0].parse::<f64>().unwrap_or(0.0);
                        let m = parts[1].parse::<f64>().unwrap_or(0.0);
                        let sec = parts[2].parse::<f64>().unwrap_or(0.0);
                        h * 3600.0 + m * 60.0 + sec
                    }
                    _ => 0.0,
                }
            }
            _ => 0.0,
        }
    }

    fn formatted_duration(&self) -> String {
        let total = self.duration_seconds() as u64;
        if total == 0 {
            return "--:--".to_string();
        }
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m:02}:{s:02}")
        }
    }

    fn playback_percent(&self) -> f32 {
        let dur = self.duration_seconds();
        let prog = self.progress.unwrap_or(0.0);
        if dur > 0.0 && prog > 0.0 {
            ((prog / dur) * 100.0).clamp(0.0, 100.0) as f32
        } else {
            0.0
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadTaskItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    progress: Option<f64>,
    #[serde(default)]
    speed: Option<String>,
    #[serde(default)]
    total_size: Option<String>,
    #[serde(default)]
    downloaded_size: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    platform: Option<String>,
    #[serde(default)]
    type_field: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadHistoryItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadStatusResponse {
    #[serde(default)]
    active_downloads: Vec<DownloadTaskItem>,
    #[serde(default)]
    queued_downloads: Vec<DownloadTaskItem>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    author_url: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    interval: u64,
    #[serde(default)]
    download_count: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoragePathResponse {
    #[serde(default)]
    uploads_dir: String,
    #[serde(default)]
    videos_dir: String,
    #[serde(default)]
    images_dir: String,
    #[serde(default)]
    subtitles_dir: String,
    #[serde(default)]
    custom_uploads_dir: Option<String>,
    #[serde(default)]
    default_uploads_dir: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CookieCheckResponse {
    #[serde(default)]
    exists: bool,
    #[serde(default)]
    youtube_exists: Option<bool>,
    #[serde(default)]
    bilibili_exists: Option<bool>,
}

#[derive(Clone, Debug, Default)]
struct SharedBackendState {
    backend_online: bool,
    status_banner: Option<(bool, String)>, // (is_error, message)
    videos: Vec<VideoItem>,
    active_downloads: Vec<DownloadTaskItem>,
    queued_downloads: Vec<DownloadTaskItem>,
    history_downloads: Vec<DownloadHistoryItem>,
    subscriptions: Vec<SubscriptionItem>,
    storage_info: StoragePathResponse,
    youtube_cookies_exist: bool,
    bilibili_cookies_exist: bool,
}

enum WorkerCommand {
    RefreshAll,
    SubmitDownload { url: String, audio_only: bool },
    CancelDownload { id: String },
    ClearHistory,
    DeleteVideo { id: String },
    PlayVideo { video: VideoItem },
    RevealVideoInFinder { video: VideoItem },
    OpenStorageFolder,
    UpdateStoragePath { path: String },
    PickAndUploadCookie { platform: String },
    DeleteCookie { platform: String },
    AddSubscription { url: String, interval_minutes: u64 },
    DeleteSubscription { id: String },
}

fn default_user_storage_root() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join("Movies").join("Videleum");
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(profile) = env::var("USERPROFILE") {
            return PathBuf::from(profile).join("Videos").join("Videleum");
        }
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("uploads")
}

fn default_user_data_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Videleum")
                .join("data");
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(appdata) = env::var("APPDATA") {
            return PathBuf::from(appdata).join("Videleum").join("data");
        }
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("backend")
        .join("data")
}

fn find_backend_entry(root: &Path) -> Option<PathBuf> {
    let candidates = [
        root.join("dist/src/server.js"),
        root.join("dist/server.js"),
        root.join("backend/dist/src/server.js"),
        root.join("backend/dist/server.js"),
    ];
    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

/// Resolves the bundled or workspace `app` directory containing the compiled backend
fn resolve_app_bundle_dir() -> Option<PathBuf> {
    if let Ok(exe) = env::current_exe() {
        if let Some(macos_dir) = exe.parent() {
            let resources_app = macos_dir.join("../Resources/app");
            if find_backend_entry(&resources_app).is_some() {
                return resources_app.canonicalize().ok().or(Some(resources_app));
            }
            if find_backend_entry(macos_dir).is_some() {
                return Some(macos_dir.to_path_buf());
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        if find_backend_entry(&cwd).is_some() || cwd.join("backend/src/server.ts").exists() {
            return Some(cwd);
        }
        if let Some(parent) = cwd.parent() {
            if find_backend_entry(parent).is_some()
                || parent.join("backend/src/server.ts").exists()
            {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

/// Migrate any videos or database files trapped inside legacy bundle paths
/// into the user-visible ~/Movies/Videleum directory.
fn migrate_legacy_bundle_media(app_dir: &Path, user_uploads_dir: &Path, user_data_dir: &Path) {
    let _ = fs::create_dir_all(user_uploads_dir.join("videos"));
    let _ = fs::create_dir_all(user_uploads_dir.join("images"));
    let _ = fs::create_dir_all(user_uploads_dir.join("subtitles"));
    let _ = fs::create_dir_all(user_uploads_dir.join("avatars"));
    let _ = fs::create_dir_all(user_data_dir);

    let mut legacy_upload_dirs = vec![app_dir.join("uploads")];
    if cfg!(target_os = "macos") {
        if let Ok(home) = env::var("HOME") {
            legacy_upload_dirs.push(
                PathBuf::from(home.clone())
                    .join("Library/Application Support/Videleum/uploads"),
            );
            legacy_upload_dirs.push(
                PathBuf::from(home)
                    .join("Library/Application Support/MyMedia/uploads"),
            );
        }
    }

    for legacy_uploads in legacy_upload_dirs {
        if legacy_uploads.exists() && legacy_uploads != user_uploads_dir {
            for sub in ["videos", "images", "subtitles", "avatars"] {
                let src_dir = legacy_uploads.join(sub);
                let dst_dir = user_uploads_dir.join(sub);
                if let Ok(entries) = fs::read_dir(&src_dir) {
                    for entry in entries.flatten() {
                        let src_path = entry.path();
                        if src_path.is_file() {
                            if let Some(name) = src_path.file_name() {
                                let dst_path = dst_dir.join(name);
                                if !dst_path.exists() {
                                    let _ = fs::copy(&src_path, &dst_path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let legacy_data = app_dir.join("data");
    if legacy_data.exists() && legacy_data != user_data_dir {
        for file_name in [
            "videleum.db",
            "cookies.txt",
            "youtube_cookies.txt",
            "bilibili_cookies.txt",
        ] {
            let src = legacy_data.join(file_name);
            let dst = user_data_dir.join(file_name);
            if src.exists() && !dst.exists() {
                let _ = fs::copy(&src, &dst);
            }
        }
    }
}

fn is_backend_reachable(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(350),
    )
    .is_ok()
}

fn spawn_backend_if_needed(port: u16) -> Option<Child> {
    if is_backend_reachable(port) {
        return None;
    }

    let app_dir = resolve_app_bundle_dir()?;
    let is_inside_app_bundle = app_dir.to_string_lossy().contains(".app/Contents/Resources");

    let uploads_dir = if is_inside_app_bundle {
        default_user_storage_root()
    } else {
        let dev_uploads = app_dir.join("backend").join("uploads");
        if dev_uploads.exists() {
            dev_uploads
        } else {
            default_user_storage_root()
        }
    };

    let data_dir = if is_inside_app_bundle {
        default_user_data_dir()
    } else {
        let dev_data = app_dir.join("backend").join("data");
        if dev_data.exists() {
            dev_data
        } else {
            default_user_data_dir()
        }
    };

    if is_inside_app_bundle {
        migrate_legacy_bundle_media(&app_dir, &uploads_dir, &data_dir);
    }

    let bundled_node_macos = app_dir.join("runtime/bin/node");
    let bundled_node_macos_alt = app_dir.join("runtime/node/bin/node");
    let bundled_node_win = app_dir.join("runtime/node.exe");
    let bundled_node_win_alt = app_dir.join("runtime/node/node.exe");
    let node_bin = if bundled_node_macos.exists() {
        bundled_node_macos
    } else if bundled_node_macos_alt.exists() {
        bundled_node_macos_alt
    } else if bundled_node_win.exists() {
        bundled_node_win
    } else if bundled_node_win_alt.exists() {
        bundled_node_win_alt
    } else {
        PathBuf::from("node")
    };

    let server_js = find_backend_entry(&app_dir)?;

    let mut path_env = env::var("PATH").unwrap_or_default();
    if cfg!(target_os = "macos") {
        for extra in ["/opt/homebrew/bin", "/usr/local/bin"] {
            if !path_env.split(':').any(|p| p == extra) {
                path_env = format!("{extra}:{path_env}");
            }
        }
    }

    let mut cmd = Command::new(node_bin);
    cmd.arg(&server_js)
        .current_dir(&app_dir)
        .env("PORT", port.to_string())
        .env("NODE_ENV", "production")
        .env("VIDELEUM_DESKTOP", "1")
        .env("UPLOADS_DIR", &uploads_dir)
        .env("DATA_DIR", &data_dir)
        .env("PATH", path_env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn().ok()
}

fn pick_cookie_file_native(title: &str) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        let script = format!(
            "POSIX path of (choose file with prompt \"{}\" of type {{\"txt\", \" public.plain-text\"}})",
            title.replace('"', "\\\"")
        );
        let output = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .output()
            .ok()?;
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
    } else if cfg!(target_os = "windows") {
        let ps_script = "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Filter = 'Cookies (*.txt)|*.txt|All Files (*.*)|*.*'; if ($f.ShowDialog() -eq 'OK') { Write-Output $f.FileName }";
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", ps_script])
            .output()
            .ok()?;
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path_str.is_empty() {
                return Some(PathBuf::from(path_str));
            }
        }
    }
    None
}

fn open_path_in_os(target: &Path, reveal: bool) {
    if cfg!(target_os = "macos") {
        let mut cmd = Command::new("/usr/bin/open");
        if reveal {
            cmd.arg("-R");
        }
        let _ = cmd.arg(target).spawn();
    } else if cfg!(target_os = "windows") {
        let mut cmd = Command::new("explorer.exe");
        if reveal {
            cmd.arg(format!("/select,{}", target.display()));
        } else {
            cmd.arg(target);
        }
        let _ = cmd.spawn();
    }
}

fn format_localized_download_error(raw_error: &str) -> String {
    let lower = raw_error.to_lowercase();
    if lower.contains("requires authentication cookies to pass bot verification")
        || lower.contains("sign in to confirm you")
    {
        return "下载 YouTube 视频需要提供登录 Cookie 以通过机器人验证，请前往「系统设置 -> Cookie 设置」上传 YouTube Cookie (youtube_cookies.txt)。".to_string();
    }
    if lower.contains("http 403") && lower.contains("youtube") {
        return "YouTube 拒绝了下载 (HTTP 403)。您的 YouTube Cookie 可能已过期，请在「系统设置 -> Cookie 设置」中重新上传。".to_string();
    }
    raw_error.to_string()
}

fn start_backend_worker(
    port: u16,
    shared: Arc<Mutex<SharedBackendState>>,
) -> mpsc::Sender<WorkerCommand> {
    let (tx, rx) = mpsc::channel::<WorkerCommand>();

    thread::spawn(move || {
        let mut child_guard = spawn_backend_if_needed(port);
        let base_url = format!("http://127.0.0.1:{port}/api");
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        let refresh_snapshot = |client: &reqwest::blocking::Client,
                                shared: &Arc<Mutex<SharedBackendState>>| {
            let online = is_backend_reachable(port);
            if !online {
                if let Ok(mut guard) = shared.lock() {
                    guard.backend_online = false;
                }
                return;
            }

            let videos: Vec<VideoItem> = client
                .get(format!("{base_url}/videos"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_default();

            let dl_status: DownloadStatusResponse = client
                .get(format!("{base_url}/download-status"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_default();

            let history_downloads: Vec<DownloadHistoryItem> = client
                .get(format!("{base_url}/downloads/history"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_default();

            let subscriptions: Vec<SubscriptionItem> = client
                .get(format!("{base_url}/subscriptions"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_default();

            let storage_info: StoragePathResponse = client
                .get(format!("{base_url}/settings/storage-path"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_else(|_| {
                    let root = default_user_storage_root();
                    StoragePathResponse {
                        uploads_dir: root.to_string_lossy().to_string(),
                        videos_dir: root.join("videos").to_string_lossy().to_string(),
                        images_dir: root.join("images").to_string_lossy().to_string(),
                        subtitles_dir: root.join("subtitles").to_string_lossy().to_string(),
                        custom_uploads_dir: None,
                        default_uploads_dir: root.to_string_lossy().to_string(),
                    }
                });

            let cookie_check: CookieCheckResponse = client
                .get(format!("{base_url}/settings/check-cookies?platform=all"))
                .send()
                .and_then(|r| r.json())
                .unwrap_or_default();

            if let Ok(mut guard) = shared.lock() {
                guard.backend_online = true;
                guard.videos = videos;
                guard.active_downloads = dl_status.active_downloads;
                guard.queued_downloads = dl_status.queued_downloads;
                guard.history_downloads = history_downloads;
                guard.subscriptions = subscriptions;
                guard.storage_info = storage_info;
                guard.youtube_cookies_exist = cookie_check
                    .youtube_exists
                    .unwrap_or(cookie_check.exists);
                guard.bilibili_cookies_exist = cookie_check.bilibili_exists.unwrap_or(false);
            }
        };

        // Initial poll
        refresh_snapshot(&client, &shared);
        let mut last_poll = Instant::now();

        loop {
            match rx.recv_timeout(Duration::from_millis(600)) {
                Ok(cmd) => {
                    match cmd {
                        WorkerCommand::RefreshAll => {
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::SubmitDownload { url, audio_only } => {
                            let body = serde_json::json!({
                                "youtubeUrl": url,
                                "audioOnly": audio_only,
                            });
                            match client.post(format!("{base_url}/download")).json(&body).send() {
                                Ok(resp) if resp.status().is_success() => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner = Some((
                                            false,
                                            format!("已加入下载队列：{url}"),
                                        ));
                                    }
                                }
                                Ok(resp) => {
                                    let err_txt = resp.text().unwrap_or_default();
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner = Some((
                                            true,
                                            format!(
                                                "添加下载失败：{}",
                                                format_localized_download_error(&err_txt)
                                            ),
                                        ));
                                    }
                                }
                                Err(err) => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((true, format!("请求后端失败：{err}")));
                                    }
                                }
                            }
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::CancelDownload { id } => {
                            let _ = client
                                .post(format!("{base_url}/downloads/cancel/{id}"))
                                .send();
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::ClearHistory => {
                            let _ = client.delete(format!("{base_url}/downloads/history")).send();
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::DeleteVideo { id } => {
                            let _ = client.delete(format!("{base_url}/videos/{id}")).send();
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::PlayVideo { video } => {
                            let videos_dir = shared
                                .lock()
                                .ok()
                                .map(|g| PathBuf::from(&g.storage_info.videos_dir))
                                .unwrap_or_else(|| default_user_storage_root().join("videos"));
                            let candidate = if let Some(vp) = &video.video_path {
                                let rel = vp.trim_start_matches("/videos/").trim_start_matches('/');
                                videos_dir.join(rel)
                            } else if let Some(vf) = &video.video_filename {
                                videos_dir.join(vf)
                            } else {
                                videos_dir
                            };
                            open_path_in_os(&candidate, false);
                        }
                        WorkerCommand::RevealVideoInFinder { video } => {
                            let videos_dir = shared
                                .lock()
                                .ok()
                                .map(|g| PathBuf::from(&g.storage_info.videos_dir))
                                .unwrap_or_else(|| default_user_storage_root().join("videos"));
                            let candidate = if let Some(vp) = &video.video_path {
                                let rel = vp.trim_start_matches("/videos/").trim_start_matches('/');
                                videos_dir.join(rel)
                            } else if let Some(vf) = &video.video_filename {
                                videos_dir.join(vf)
                            } else {
                                videos_dir.clone()
                            };
                            if candidate.exists() {
                                open_path_in_os(&candidate, true);
                            } else {
                                open_path_in_os(&videos_dir, false);
                            }
                        }
                        WorkerCommand::OpenStorageFolder => {
                            let videos_dir = shared
                                .lock()
                                .ok()
                                .map(|g| PathBuf::from(&g.storage_info.videos_dir))
                                .unwrap_or_else(|| default_user_storage_root().join("videos"));
                            let _ = fs::create_dir_all(&videos_dir);
                            open_path_in_os(&videos_dir, false);
                        }
                        WorkerCommand::UpdateStoragePath { path } => {
                            let body = serde_json::json!({ "path": path });
                            match client
                                .post(format!("{base_url}/settings/storage-path"))
                                .json(&body)
                                .send()
                            {
                                Ok(resp) if resp.status().is_success() => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner = Some((
                                            false,
                                            "视频下载存储目录已成功更新并立即生效！".to_string(),
                                        ));
                                    }
                                }
                                Ok(resp) => {
                                    let msg = resp.text().unwrap_or_default();
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((true, format!("更新存储目录失败：{msg}")));
                                    }
                                }
                                Err(e) => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((true, format!("更新存储目录请求异常：{e}")));
                                    }
                                }
                            }
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::PickAndUploadCookie { platform } => {
                            let label = if platform == "bilibili" {
                                "请选择 Bilibili cookies.txt 文件"
                            } else {
                                "请选择 YouTube cookies.txt 文件"
                            };
                            if let Some(chosen_path) = pick_cookie_file_native(label) {
                                match reqwest::blocking::multipart::Form::new()
                                    .text("platform", platform.clone())
                                    .file("file", &chosen_path)
                                {
                                    Ok(form) => {
                                        match client
                                            .post(format!(
                                                "{base_url}/settings/upload-cookies?platform={platform}"
                                            ))
                                            .multipart(form)
                                            .send()
                                        {
                                            Ok(resp) if resp.status().is_success() => {
                                                if let Ok(mut guard) = shared.lock() {
                                                    guard.status_banner = Some((
                                                        false,
                                                        format!(
                                                            "{} Cookie 上传成功！",
                                                            if platform == "bilibili" {
                                                                "Bilibili"
                                                            } else {
                                                                "YouTube"
                                                            }
                                                        ),
                                                    ));
                                                }
                                            }
                                            Ok(resp) => {
                                                let txt = resp.text().unwrap_or_default();
                                                if let Ok(mut guard) = shared.lock() {
                                                    guard.status_banner = Some((
                                                        true,
                                                        format!("Cookie 上传失败：{txt}"),
                                                    ));
                                                }
                                            }
                                            Err(err) => {
                                                if let Ok(mut guard) = shared.lock() {
                                                    guard.status_banner = Some((
                                                        true,
                                                        format!("Cookie 上传请求错误：{err}"),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        if let Ok(mut guard) = shared.lock() {
                                            guard.status_banner = Some((
                                                true,
                                                format!("读取 Cookie 文件失败：{err}"),
                                            ));
                                        }
                                    }
                                }
                                refresh_snapshot(&client, &shared);
                            }
                        }
                        WorkerCommand::DeleteCookie { platform } => {
                            let body = serde_json::json!({ "platform": platform });
                            let _ = client
                                .post(format!(
                                    "{base_url}/settings/delete-cookies?platform={platform}"
                                ))
                                .json(&body)
                                .send();
                            if let Ok(mut guard) = shared.lock() {
                                guard.status_banner = Some((
                                    false,
                                    format!(
                                        "已删除 {} Cookie 文件。",
                                        if platform == "bilibili" {
                                            "Bilibili"
                                        } else {
                                            "YouTube"
                                        }
                                    ),
                                ));
                            }
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::AddSubscription {
                            url,
                            interval_minutes,
                        } => {
                            let body = serde_json::json!({
                                "url": url,
                                "interval": interval_minutes,
                            });
                            match client
                                .post(format!("{base_url}/subscriptions"))
                                .json(&body)
                                .send()
                            {
                                Ok(resp) if resp.status().is_success() => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((false, format!("已成功添加订阅源：{url}")));
                                    }
                                }
                                Ok(resp) => {
                                    let txt = resp.text().unwrap_or_default();
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((true, format!("添加订阅源失败：{txt}")));
                                    }
                                }
                                Err(err) => {
                                    if let Ok(mut guard) = shared.lock() {
                                        guard.status_banner =
                                            Some((true, format!("订阅请求失败：{err}")));
                                    }
                                }
                            }
                            refresh_snapshot(&client, &shared);
                        }
                        WorkerCommand::DeleteSubscription { id } => {
                            let _ = client
                                .delete(format!("{base_url}/subscriptions/{id}"))
                                .send();
                            refresh_snapshot(&client, &shared);
                        }
                    }
                    last_poll = Instant::now();
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if last_poll.elapsed() >= Duration::from_millis(1200) {
                        if child_guard.is_none() && !is_backend_reachable(port) {
                            child_guard = spawn_backend_if_needed(port);
                        }
                        refresh_snapshot(&client, &shared);
                        last_poll = Instant::now();
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if let Some(mut child) = child_guard.take() {
                        let _ = child.kill();
                    }
                    break;
                }
            }
        }
    });

    tx
}

struct VideleumDesktopView {
    nav_page: NavPage,
    platform_filter: PlatformFilter,
    shared: Arc<Mutex<SharedBackendState>>,
    snapshot: SharedBackendState,
    worker_tx: mpsc::Sender<WorkerCommand>,
    url_input: Entity<InputState>,
    search_input: Entity<InputState>,
    subscription_input: Entity<InputState>,
    storage_path_input: Entity<InputState>,
    storage_input_initialized: bool,
}

impl VideleumDesktopView {
    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        shared: Arc<Mutex<SharedBackendState>>,
        worker_tx: mpsc::Sender<WorkerCommand>,
    ) -> Self {
        let url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("粘贴 YouTube / Bilibili / Twitch 视频、合集或收藏夹链接...")
        });
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("搜索视频标题或 UP 主 / 作者..."));
        let subscription_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入 Bilibili 空间 / 收藏夹、YouTube 频道 / 列表或 Twitch 频道 URL...")
        });
        let default_storage_str = default_user_storage_root().to_string_lossy().to_string();
        let storage_path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("输入自定义媒体存储根目录，例如 ~/Movies/Videleum")
                .default_value(default_storage_str)
        });

        let initial_snapshot = shared.lock().map(|g| g.clone()).unwrap_or_default();

        // Periodic UI refresh task using GPUI's native background executor timer
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(800))
                    .await;
                let keep_going = this
                    .update(cx, |view, cx| {
                        view.sync_from_shared();
                        cx.notify();
                    })
                    .is_ok();
                if !keep_going {
                    break;
                }
            }
        })
        .detach();

        Self {
            nav_page: NavPage::PosterWall,
            platform_filter: PlatformFilter::All,
            shared,
            snapshot: initial_snapshot,
            worker_tx,
            url_input,
            search_input,
            subscription_input,
            storage_path_input,
            storage_input_initialized: false,
        }
    }

    fn sync_from_shared(&mut self) {
        if let Ok(guard) = self.shared.lock() {
            self.snapshot = guard.clone();
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_count =
            self.snapshot.active_downloads.len() + self.snapshot.queued_downloads.len();
        let video_count = self.snapshot.videos.len();
        let sub_count = self.snapshot.subscriptions.len();
        let backend_online = self.snapshot.backend_online;

        let nav_items: [(NavPage, &str, String); 5] = [
            (NavPage::Overview, "概览看板", String::new()),
            (
                NavPage::PosterWall,
                "媒体库海报墙",
                format!("{video_count}"),
            ),
            (
                NavPage::VideoSources,
                "视频源与订阅",
                format!("{sub_count}"),
            ),
            (
                NavPage::Downloads,
                "下载任务中心",
                format!("{active_count}"),
            ),
            (NavPage::Settings, "系统设置", String::new()),
        ];

        let mut nav_list = div().flex().flex_col().gap_1().px_3().mt_2();
        for (idx, (page, label, badge_text)) in nav_items.into_iter().enumerate() {
            let is_active = self.nav_page == page;
            let row_bg = if is_active {
                rgb(0x1E293B)
            } else {
                rgb(0x0F172A)
            };
            let text_col = if is_active {
                rgb(0x38BDF8)
            } else {
                rgb(0xCBD5E1)
            };

            let mut item = div()
                .id(("nav-item", idx))
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .bg(row_bg)
                .text_color(text_col)
                .text_sm()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.nav_page = page;
                    cx.notify();
                }))
                .child(label.to_string());

            if !badge_text.is_empty() {
                item = item.child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .bg(if is_active {
                            rgb(0x0284C7)
                        } else {
                            rgb(0x334155)
                        })
                        .text_xs()
                        .text_color(rgb(0xFFFFFF))
                        .child(badge_text),
                );
            }
            nav_list = nav_list.child(item);
        }

        div()
            .w(px(236.))
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .bg(rgb(0x0F172A))
            .border_r_1()
            .border_color(rgb(0x1E293B))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        // Top brand header (bili-sync style)
                        div()
                            .px_4()
                            .py_4()
                            .border_b_1()
                            .border_color(rgb(0x1E293B))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .w(px(28.))
                                            .h(px(28.))
                                            .rounded_md()
                                            .bg(rgb(0x00AEEC))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_xs()
                                            .text_color(rgb(0xFFFFFF))
                                            .child("▶"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_base()
                                                    .text_color(rgb(0xF8FAFC))
                                                    .child("Videleum"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x64748B))
                                                    .child("跨平台流媒体归档中心"),
                                            ),
                                    ),
                            ),
                    )
                    .child(nav_list),
            )
            .child(
                // Sidebar bottom status indicator (no intrusive copyright banner here)
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w(px(8.)).h(px(8.)).rounded_full().bg(if backend_online {
                                rgb(0x22C55E)
                            } else {
                                rgb(0xF59E0B)
                            }))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .child(if backend_online {
                                        "核心采集引擎已就绪"
                                    } else {
                                        "正在连接核心采集引擎..."
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x64748B))
                            .child(format!(
                                "YouTube Cookie: {} · Bilibili: {}",
                                if self.snapshot.youtube_cookies_exist {
                                    "已就绪"
                                } else {
                                    "未配置"
                                },
                                if self.snapshot.bilibili_cookies_exist {
                                    "已就绪"
                                } else {
                                    "免登录"
                                }
                            )),
                    ),
            )
    }

    fn render_top_download_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .px_6()
            .py_3()
            .bg(rgb(0x111827))
            .border_b_1()
            .border_color(rgb(0x1E293B))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.url_input).id("global-url-input")),
                    )
                    .child(
                        Button::new("submit-video-dl")
                            .label("下载视频")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let url = this.url_input.read(cx).value().trim().to_string();
                                if !url.is_empty() {
                                    let _ = this.worker_tx.send(WorkerCommand::SubmitDownload {
                                        url,
                                        audio_only: false,
                                    });
                                    this.url_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                }
                            })),
                    )
                    .child(
                        Button::new("submit-audio-dl")
                            .label("仅下载音频")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let url = this.url_input.read(cx).value().trim().to_string();
                                if !url.is_empty() {
                                    let _ = this.worker_tx.send(WorkerCommand::SubmitDownload {
                                        url,
                                        audio_only: true,
                                    });
                                    this.url_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("open-videos-folder-top")
                            .label("打开视频文件夹")
                            .on_click(cx.listener(|this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::OpenStorageFolder);
                            })),
                    )
                    .child(
                        Button::new("refresh-all-top")
                            .label("刷新")
                            .on_click(cx.listener(|this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::RefreshAll);
                            })),
                    ),
            )
    }

    fn render_status_banner(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some((is_err, msg)) = &self.snapshot.status_banner {
            let bg = if *is_err {
                rgb(0x451A1A)
            } else {
                rgb(0x0F291E)
            };
            let border = if *is_err {
                rgb(0xDC2626)
            } else {
                rgb(0x16A34A)
            };
            let text_col = if *is_err {
                rgb(0xFCA5A5)
            } else {
                rgb(0x86EFAC)
            };

            div()
                .mx_6()
                .mt_3()
                .px_4()
                .py_2()
                .rounded_md()
                .bg(bg)
                .border_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().text_color(text_col).child(msg.clone()))
                .child(
                    div()
                        .id("dismiss-banner")
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(0x94A3B8))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Ok(mut guard) = this.shared.lock() {
                                guard.status_banner = None;
                            }
                            this.snapshot.status_banner = None;
                            cx.notify();
                        }))
                        .child("关闭 ✕"),
                )
        } else {
            div()
        }
    }

    fn render_video_card(
        &self,
        idx: usize,
        video: &VideoItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let badge_label = video.platform_display_name();
        let badge_bg = rgb(video.platform_badge_color());
        let duration_label = video.formatted_duration();
        let progress_pct = video.playback_percent();
        let title_str = if video.title.is_empty() {
            "未命名视频".to_string()
        } else {
            video.title.clone()
        };
        let author_str = if video.author.is_empty() {
            "未知作者".to_string()
        } else {
            video.author.clone()
        };

        let video_for_play = video.clone();
        let video_for_reveal = video.clone();
        let video_id_for_del = video.id.clone();

        // Thumbnail header with platform badge, duration chip, and bottom playback progress bar
        let mut thumb = div()
            .w_full()
            .h(px(142.))
            .rounded_t_lg()
            .bg(rgb(0x1E293B))
            .flex()
            .flex_col()
            .justify_between()
            .p_2()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(if video.media_type.as_deref() == Some("audio") {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(rgba(0x000000B0))
                            .text_xs()
                            .text_color(rgb(0xFFFFFF))
                            .child("♪ 音频")
                    } else if let (Some(p), Some(tot)) = (video.part_number, video.total_parts) {
                        if tot > 1 {
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(rgba(0x000000B0))
                                .text_xs()
                                .text_color(rgb(0xFFFFFF))
                                .child(format!("P{p}/{tot}"))
                        } else {
                            div()
                        }
                    } else {
                        div()
                    })
                    .child(
                        // Issue 6: Canonical Uppercase Platform Badge with Brand Colors:
                        // YouTube (#FF0000), Bilibili (#00AEEC), Twitch (#9146FF)
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(badge_bg)
                            .text_xs()
                            .text_color(rgb(0xFFFFFF))
                            .child(badge_label),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x94A3B8))
                            .child("▶ 点击播放本地高清媒体"),
                    )
                    .child(
                        div()
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgba(0x000000B8))
                            .text_xs()
                            .text_color(rgb(0xFFFFFF))
                            .child(duration_label),
                    ),
            );

        if progress_pct > 0.0 {
            thumb = thumb.child(
                div()
                    .w_full()
                    .h(px(4.))
                    .bg(rgb(0x334155))
                    .rounded_full()
                    .child(
                        div()
                            .w(px((progress_pct / 100.0) * 240.0))
                            .h_full()
                            .bg(rgb(0xEF4444)),
                    ),
            );
        }

        div()
            .w(px(268.))
            .rounded_lg()
            .bg(rgb(0x111827))
            .border_1()
            .border_color(rgb(0x1E293B))
            .flex()
            .flex_col()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id(("play-thumb", idx))
                            .cursor_pointer()
                            .on_click({
                                let v = video_for_play.clone();
                                cx.listener(move |this, _, _, _| {
                                    let _ = this
                                        .worker_tx
                                        .send(WorkerCommand::PlayVideo { video: v.clone() });
                                })
                            })
                            .child(thumb),
                    )
                    .child(
                        div()
                            .p_3()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xF8FAFC))
                                    .child(title_str),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .child(format!("UP / 作者：{author_str}")),
                            ),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .pb_3()
                    .pt_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .id(("btn-play-video", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x0284C7))
                            .text_xs()
                            .text_color(rgb(0xFFFFFF))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::PlayVideo {
                                    video: video_for_play.clone(),
                                });
                            }))
                            .child("▶ 播放"),
                    )
                    .child(
                        div()
                            .id(("btn-reveal-video", idx))
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x1E293B))
                            .text_xs()
                            .text_color(rgb(0xCBD5E1))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::RevealVideoInFinder {
                                    video: video_for_reveal.clone(),
                                });
                            }))
                            .child("📂 定位文件"),
                    )
                    .child(
                        div()
                            .id(("btn-del-video", idx))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x3F1D1D))
                            .text_xs()
                            .text_color(rgb(0xFCA5A5))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::DeleteVideo {
                                    id: video_id_for_del.clone(),
                                });
                            }))
                            .child("删除"),
                    ),
            )
    }

    fn render_overview_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = self.snapshot.videos.len();
        let yt_count = self
            .snapshot
            .videos
            .iter()
            .filter(|v| v.canonical_platform() == "youtube")
            .count();
        let bili_count = self
            .snapshot
            .videos
            .iter()
            .filter(|v| v.canonical_platform() == "bilibili")
            .count();
        let twitch_count = self
            .snapshot
            .videos
            .iter()
            .filter(|v| v.canonical_platform() == "twitch")
            .count();

        let stat_cards = [
            ("总馆藏媒体", format!("{total}"), 0x38BDF8u32),
            ("YouTube 馆藏", format!("{yt_count}"), 0xFF0000u32),
            ("Bilibili 馆藏", format!("{bili_count}"), 0x00AEECu32),
            ("Twitch 馆藏", format!("{twitch_count}"), 0x9146FFu32),
        ];

        let mut cards_row = div().flex().flex_wrap().gap_4();
        for (title, val, color_hex) in stat_cards {
            cards_row = cards_row.child(
                div()
                    .w(px(210.))
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_xs().text_color(rgb(0x94A3B8)).child(title))
                    .child(div().text_2xl().text_color(rgb(color_hex)).child(val)),
            );
        }

        let mut recent_grid = div().flex().flex_wrap().gap_4();
        for (idx, video) in self.snapshot.videos.iter().take(8).enumerate() {
            recent_grid = recent_grid.child(self.render_video_card(idx, video, cx));
        }

        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(rgb(0xF8FAFC))
                                    .child("bili-sync 风格概览看板"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .child(format!(
                                        "当前视频保存目录：{}",
                                        self.snapshot.storage_info.videos_dir
                                    )),
                            ),
                    ),
            )
            .child(cards_row)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_base()
                            .text_color(rgb(0xF8FAFC))
                            .child("最近归档媒体"),
                    )
                    .child(if self.snapshot.videos.is_empty() {
                        div()
                            .p_8()
                            .rounded_lg()
                            .bg(rgb(0x111827))
                            .border_1()
                            .border_color(rgb(0x1E293B))
                            .text_sm()
                            .text_color(rgb(0x94A3B8))
                            .child("暂无已归档视频，请在上方输入框粘贴 YouTube / Bilibili / Twitch 链接开始下载。")
                    } else {
                        recent_grid
                    }),
            )
    }

    fn render_poster_wall_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let search_kw = self
            .search_input
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let active_platform = self.platform_filter;

        let filtered: Vec<VideoItem> = self
            .snapshot
            .videos
            .iter()
            .filter(|v| {
                let platform_ok = match active_platform {
                    PlatformFilter::All => true,
                    _ => v.canonical_platform() == active_platform.as_str(),
                };
                if !platform_ok {
                    return false;
                }
                if search_kw.is_empty() {
                    return true;
                }
                v.title.to_lowercase().contains(&search_kw)
                    || v.author.to_lowercase().contains(&search_kw)
            })
            .cloned()
            .collect();

        let pills: [(PlatformFilter, &str, u32); 4] = [
            (PlatformFilter::All, "全部平台", 0x0284C7),
            (PlatformFilter::YouTube, "YouTube", 0xFF0000),
            (PlatformFilter::Bilibili, "Bilibili", 0x00AEEC),
            (PlatformFilter::Twitch, "Twitch", 0x9146FF),
        ];

        let mut pill_row = div().flex().items_center().gap_2();
        for (idx, (pf, label, brand_hex)) in pills.into_iter().enumerate() {
            let selected = self.platform_filter == pf;
            pill_row = pill_row.child(
                div()
                    .id(("platform-pill", idx))
                    .px_3()
                    .py_1p5()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(if selected {
                        rgb(brand_hex)
                    } else {
                        rgb(0x1E293B)
                    })
                    .text_xs()
                    .text_color(rgb(0xFFFFFF))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.platform_filter = pf;
                        cx.notify();
                    }))
                    .child(label),
            );
        }

        let mut grid = div().flex().flex_wrap().gap_4();
        for (idx, video) in filtered.iter().enumerate() {
            grid = grid.child(self.render_video_card(idx + 100, video, cx));
        }

        // Note: Issue 5 - No footer copyright on the Poster Wall (Home) page!
        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(pill_row)
                    .child(
                        div()
                            .w(px(320.))
                            .child(Input::new(&self.search_input).id("poster-search-input")),
                    ),
            )
            .child(if filtered.is_empty() {
                div()
                    .p_10()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .text_sm()
                    .text_color(rgb(0x94A3B8))
                    .child("当前筛选条件下暂无视频。")
            } else {
                grid
            })
    }

    fn render_video_sources_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sub_list = div().flex().flex_col().gap_3();
        for (idx, sub) in self.snapshot.subscriptions.iter().enumerate() {
            let sub_id = sub.id.clone();
            let badge_color = if sub.platform.to_lowercase().contains("bili") {
                0x00AEECu32
            } else if sub.platform.to_lowercase().contains("twitch") {
                0x9146FFu32
            } else {
                0xFF0000u32
            };
            sub_list = sub_list.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .bg(rgb(badge_color))
                                            .text_xs()
                                            .text_color(rgb(0xFFFFFF))
                                            .child(sub.platform.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(0xF8FAFC))
                                            .child(sub.author.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .child(format!(
                                        "{} · 每 {} 分钟轮询 · 已归档 {} 个视频",
                                        sub.author_url, sub.interval, sub.download_count
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .id(("del-sub", idx))
                            .px_3()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x3F1D1D))
                            .text_xs()
                            .text_color(rgb(0xFCA5A5))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::DeleteSubscription {
                                    id: sub_id.clone(),
                                });
                            }))
                            .child("取消订阅"),
                    ),
            );
        }

        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .text_color(rgb(0xF8FAFC))
                            .child("视频源与自动订阅 (bili-sync 同步中心)"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x94A3B8))
                            .child("支持 Bilibili UP 主投稿 / 收藏夹 / 稍后再看、YouTube 频道 / 播放列表以及 Twitch 频道 VOD 自动轮询归档。"),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.subscription_input).id("sub-url-input")),
                    )
                    .child(
                        Button::new("add-subscription-btn")
                            .label("添加定时订阅 (60 分钟)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                let url = this
                                    .subscription_input
                                    .read(cx)
                                    .value()
                                    .trim()
                                    .to_string();
                                if !url.is_empty() {
                                    let _ = this.worker_tx.send(WorkerCommand::AddSubscription {
                                        url,
                                        interval_minutes: 60,
                                    });
                                    this.subscription_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                }
                            })),
                    ),
            )
            .child(if self.snapshot.subscriptions.is_empty() {
                div()
                    .p_8()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .text_sm()
                    .text_color(rgb(0x94A3B8))
                    .child("暂未配置自动订阅源。")
            } else {
                sub_list
            })
    }

    fn render_downloads_page(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut active_list = div().flex().flex_col().gap_3();
        for (idx, task) in self.snapshot.active_downloads.iter().enumerate() {
            let pct = task.progress.unwrap_or(0.0).clamp(0.0, 100.0);
            let speed = task.speed.clone().unwrap_or_else(|| "0 B/s".to_string());
            let dl_size = task
                .downloaded_size
                .clone()
                .unwrap_or_else(|| "0 B".to_string());
            let tot_size = task.total_size.clone().unwrap_or_else(|| "?".to_string());
            let cancel_id = task.id.clone();

            active_list = active_list.child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x0284C7))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xF8FAFC))
                                    .child(task.title.clone()),
                            )
                            .child(
                                div()
                                    .id(("cancel-dl", idx))
                                    .px_2p5()
                                    .py_1()
                                    .rounded_md()
                                    .bg(rgb(0x3F1D1D))
                                    .text_xs()
                                    .text_color(rgb(0xFCA5A5))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        let _ = this.worker_tx.send(
                                            WorkerCommand::CancelDownload {
                                                id: cancel_id.clone(),
                                            },
                                        );
                                    }))
                                    .child("取消"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(8.))
                            .rounded_full()
                            .bg(rgb(0x1E293B))
                            .child(
                                div()
                                    .w(px((pct as f32 / 100.0) * 520.0))
                                    .h_full()
                                    .rounded_full()
                                    .bg(rgb(0x00AEEC)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x94A3B8))
                            .child(format!(
                                "进度：{pct:.1}% · 速度：{speed} · 大小：{dl_size} / {tot_size}"
                            )),
                    ),
            );
        }

        let mut history_list = div().flex().flex_col().gap_2();
        for item in self.snapshot.history_downloads.iter().take(15) {
            let is_failed = item.status == "failed";
            let err_msg = item
                .error
                .as_deref()
                .map(format_localized_download_error)
                .unwrap_or_default();

            history_list = history_list.child(
                div()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(if is_failed {
                        rgb(0x7F1D1D)
                    } else {
                        rgb(0x1E293B)
                    })
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(0xF8FAFC))
                                    .child(item.title.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if is_failed {
                                        rgb(0xFCA5A5)
                                    } else {
                                        rgb(0x86EFAC)
                                    })
                                    .child(if is_failed { "失败" } else { "已完成" }),
                            ),
                    )
                    .child(if !err_msg.is_empty() {
                        div().text_xs().text_color(rgb(0xFCD34D)).child(err_msg)
                    } else {
                        div()
                    }),
            );
        }

        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(rgb(0xF8FAFC))
                                    .child("下载任务与历史记录"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x94A3B8))
                                    .child("Bilibili 视频使用原生 WBI 签名与 DASH 流高速下载并实时反馈进度；YouTube / Twitch 使用内置引擎。"),
                            ),
                    )
                    .child(
                        Button::new("clear-dl-history")
                            .label("清空历史记录")
                            .on_click(cx.listener(|this, _, _, _| {
                                let _ = this.worker_tx.send(WorkerCommand::ClearHistory);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x38BDF8))
                            .child(format!(
                                "正在下载 ({}) · 排队中 ({})",
                                self.snapshot.active_downloads.len(),
                                self.snapshot.queued_downloads.len()
                            )),
                    )
                    .child(if self.snapshot.active_downloads.is_empty() {
                        div()
                            .p_4()
                            .rounded_lg()
                            .bg(rgb(0x111827))
                            .border_1()
                            .border_color(rgb(0x1E293B))
                            .text_xs()
                            .text_color(rgb(0x94A3B8))
                            .child("当前没有正在进行的下载任务。")
                    } else {
                        active_list
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xCBD5E1))
                            .child("最近任务记录"),
                    )
                    .child(history_list),
            )
    }

    fn render_settings_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        if !self.storage_input_initialized && !self.snapshot.storage_info.uploads_dir.is_empty() {
            let current_dir = self
                .snapshot
                .storage_info
                .custom_uploads_dir
                .clone()
                .unwrap_or_else(|| self.snapshot.storage_info.uploads_dir.clone());
            self.storage_path_input.update(cx, |input, cx| {
                input.set_value(current_dir, window, cx);
            });
            self.storage_input_initialized = true;
        }

        let yt_ok = self.snapshot.youtube_cookies_exist;
        let bili_ok = self.snapshot.bilibili_cookies_exist;

        div()
            .p_6()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                // 1. Storage Directory Configuration Card
                div()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(0xF8FAFC))
                            .child("媒体下载存储目录设置"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x38BDF8))
                            .child(format!(
                                "当前视频文件实际存储位置：{}",
                                self.snapshot.storage_info.videos_dir
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .flex_1()
                                    .child(Input::new(&self.storage_path_input).id("storage-path-input")),
                            )
                            .child(
                                Button::new("apply-storage-path")
                                    .label("保存并切换目录")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let path = this
                                            .storage_path_input
                                            .read(cx)
                                            .value()
                                            .trim()
                                            .to_string();
                                        let _ = this
                                            .worker_tx
                                            .send(WorkerCommand::UpdateStoragePath { path });
                                    })),
                            )
                            .child(
                                Button::new("open-storage-dir-btn")
                                    .label("在 Finder / 资源管理器中打开")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        let _ =
                                            this.worker_tx.send(WorkerCommand::OpenStorageFolder);
                                    })),
                            ),
                    ),
            )
            .child(
                // 2. Separate YouTube & Bilibili Cookies Configuration (Issue 3)
                div()
                    .p_5()
                    .rounded_lg()
                    .bg(rgb(0x111827))
                    .border_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(0xF8FAFC))
                            .child("安全与访问 — 平台独立 Cookie 设置"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x94A3B8))
                            .child("YouTube 与 Bilibili 的 Cookie 完全独立存储与管理（支持使用浏览器扩展 Get cookies.txt LOCALLY 导出后分别上传）。"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_4()
                            // YouTube Cookie Card
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(320.))
                                    .p_4()
                                    .rounded_md()
                                    .bg(rgb(0x0F172A))
                                    .border_1()
                                    .border_color(rgb(0x1E293B))
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_md()
                                                    .bg(rgb(0xFF0000))
                                                    .text_xs()
                                                    .text_color(rgb(0xFFFFFF))
                                                    .child("YouTube"),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0xF8FAFC))
                                                    .child("YouTube Cookie (youtube_cookies.txt)"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if yt_ok {
                                                rgb(0x86EFAC)
                                            } else {
                                                rgb(0xFCD34D)
                                            })
                                            .child(if yt_ok {
                                                "✓ 已配置 youtube_cookies.txt（可通过 YouTube 机器人验证）"
                                            } else {
                                                "⚠ 未上传 youtube_cookies.txt（下载 YouTube 视频若提示机器人验证，请在此上传）"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Button::new("upload-yt-cookie")
                                                    .label("选择并上传 YouTube Cookie")
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        let _ = this.worker_tx.send(
                                                            WorkerCommand::PickAndUploadCookie {
                                                                platform: "youtube".to_string(),
                                                            },
                                                        );
                                                    })),
                                            )
                                            .child(
                                                Button::new("del-yt-cookie")
                                                    .label("删除")
                                                    .disabled(!yt_ok)
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        let _ = this.worker_tx.send(
                                                            WorkerCommand::DeleteCookie {
                                                                platform: "youtube".to_string(),
                                                            },
                                                        );
                                                    })),
                                            ),
                                    ),
                            )
                            // Bilibili Cookie Card
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(320.))
                                    .p_4()
                                    .rounded_md()
                                    .bg(rgb(0x0F172A))
                                    .border_1()
                                    .border_color(rgb(0x1E293B))
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_md()
                                                    .bg(rgb(0x00AEEC))
                                                    .text_xs()
                                                    .text_color(rgb(0xFFFFFF))
                                                    .child("Bilibili"),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(rgb(0xF8FAFC))
                                                    .child("Bilibili Cookie (bilibili_cookies.txt)"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if bili_ok {
                                                rgb(0x86EFAC)
                                            } else {
                                                rgb(0x94A3B8)
                                            })
                                            .child(if bili_ok {
                                                "✓ 已配置 bilibili_cookies.txt（已解锁 1080P+/4K 高码率流与收藏夹同步）"
                                            } else {
                                                "ℹ 未上传 bilibili_cookies.txt（当前使用原生 WBI 免登录接口下载，上传后可解锁大会员高画质）"
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                Button::new("upload-bili-cookie")
                                                    .label("选择并上传 Bilibili Cookie")
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        let _ = this.worker_tx.send(
                                                            WorkerCommand::PickAndUploadCookie {
                                                                platform: "bilibili".to_string(),
                                                            },
                                                        );
                                                    })),
                                            )
                                            .child(
                                                Button::new("del-bili-cookie")
                                                    .label("删除")
                                                    .disabled(!bili_ok)
                                                    .on_click(cx.listener(|this, _, _, _| {
                                                        let _ = this.worker_tx.send(
                                                            WorkerCommand::DeleteCookie {
                                                                platform: "bilibili".to_string(),
                                                            },
                                                        );
                                                    })),
                                            ),
                                    ),
                            ),
                    ),
            )
            .child(
                // Issue 5 & 7: Copyright footer shown ONLY on Settings page
                div()
                    .mt_4()
                    .pt_4()
                    .border_t_1()
                    .border_color(rgb(0x1E293B))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(rgb(0x64748B))
                    .child("Videleum Desktop (GPUI + GPUI Kit Native UI) · v0.1.0")
                    .child("跨平台流媒体采集与本地归档中心"),
            )
    }
}

impl Render for VideleumDesktopView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match self.nav_page {
            NavPage::Overview => self.render_overview_page(cx).into_any_element(),
            NavPage::PosterWall => self.render_poster_wall_page(cx).into_any_element(),
            NavPage::VideoSources => self.render_video_sources_page(cx).into_any_element(),
            NavPage::Downloads => self.render_downloads_page(cx).into_any_element(),
            NavPage::Settings => self.render_settings_page(window, cx).into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .bg(rgb(0x0B0F19))
            .text_color(rgb(0xF8FAFC))
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(self.render_top_download_bar(cx))
                    .child(self.render_status_banner(cx))
                    .child(
                        div()
                            .id("main-scroll-container")
                            .flex_1()
                            .overflow_y_scroll()
                            .child(content),
                    ),
            )
    }
}

fn main() {
    let port = env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(DEFAULT_BACKEND_PORT);

    let shared = Arc::new(Mutex::new(SharedBackendState::default()));
    let worker_tx = start_backend_worker(port, Arc::clone(&shared));

    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);

        let bounds = Bounds::centered(None, size(px(1360.), px(860.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        let shared_clone = Arc::clone(&shared);
        let tx_clone = worker_tx.clone();
        gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| VideleumDesktopView::new(window, cx, shared_clone, tx_clone))
        })
        .expect("Failed to open Videleum GPUI window");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_badge_display_names_and_brand_colors_match_specification() {
        let yt = VideoItem {
            platform: Some("youtube".to_string()),
            ..Default::default()
        };
        assert_eq!(yt.platform_display_name(), "YouTube");
        assert_eq!(yt.platform_badge_color(), 0xFF0000);

        let bili = VideoItem {
            platform: Some("bilibili".to_string()),
            ..Default::default()
        };
        assert_eq!(bili.platform_display_name(), "Bilibili");
        assert_eq!(bili.platform_badge_color(), 0x00AEEC);

        let twitch = VideoItem {
            platform: Some("twitch".to_string()),
            ..Default::default()
        };
        assert_eq!(twitch.platform_display_name(), "Twitch");
        assert_eq!(twitch.platform_badge_color(), 0x9146FF);
    }

    #[test]
    fn duration_and_playback_progress_formatting_work() {
        let item = VideoItem {
            duration: Some(serde_json::json!(125)),
            progress: Some(62.5),
            ..Default::default()
        };
        assert_eq!(item.formatted_duration(), "02:05");
        assert!((item.playback_percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn default_user_storage_root_uses_visible_videleum_folder() {
        let path = default_user_storage_root();
        let rendered = path.to_string_lossy();
        assert!(rendered.contains("Videleum") || rendered.ends_with("uploads"));
    }
}
