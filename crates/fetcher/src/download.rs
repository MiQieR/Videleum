//! End-to-end download orchestration and job reporting.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use videleum_core::archive::{ArchiveLayout, sanitize_component};
use videleum_core::model::{DownloadJob, DownloadState, MediaItem, MediaStatus, now_timestamp};
use videleum_core::{Platform, Settings, SourceKind, SourceRef, Store, parse_source};

use crate::bilibili::{BilibiliClient, VideoDetails};
use crate::organize::{ArchiveInput, archive_media};
use crate::tool;
use crate::ytdlp::{YtDlp, YtOptions};
use crate::{FetchError, Result, TransferProgress, http};

#[derive(Clone, Debug)]
pub struct DownloadRequest {
    pub url: String,
    pub source: Option<SourceRef>,
}

impl DownloadRequest {
    pub fn from_url(url: &str) -> Result<Self> {
        let source =
            parse_source(url).map_err(|error| FetchError::Unsupported(error.to_string()))?;
        Ok(Self {
            url: source.canonical_url.clone(),
            source: Some(source),
        })
    }
}

#[derive(Clone, Debug)]
pub enum JobEvent {
    Updated(DownloadJob),
    Completed { job: DownloadJob, media_id: i64 },
    Failed { job: DownloadJob, message: String },
    Canceled(DownloadJob),
}

/// Persist a queued download so it survives a restart and can be shown in the
/// downloads view immediately.
pub fn enqueue(store: &Store, source: &SourceRef) -> Result<DownloadJob> {
    if !matches!(source.kind, SourceKind::Video | SourceKind::Redirect) {
        return Err(FetchError::Unsupported(
            "Only video links can be downloaded. Follow channels and playlists instead."
                .to_string(),
        ));
    }
    let now = now_timestamp();
    let job = DownloadJob {
        source_url: source.canonical_url.clone(),
        platform: source.platform,
        source_id: source.id.clone(),
        state: DownloadState::Queued,
        stage: "Waiting".to_string(),
        created_at: now,
        updated_at: now,
        ..Default::default()
    };
    let id = store.save_job(&job)?;
    Ok(DownloadJob { id, ..job })
}

/// Run one job to completion. Intended to be called from a worker thread with
/// the settings snapshot taken when the job was queued.
pub fn run_job(
    settings: Settings,
    store: Store,
    mut job: DownloadJob,
    cancel: Arc<AtomicBool>,
    emit: impl Fn(JobEvent),
) {
    let result = execute(&settings, &store, &mut job, cancel.as_ref(), &emit);
    let terminal = match result {
        Ok(archived) => {
            job.state = DownloadState::Completed;
            job.progress = 1.0;
            job.stage = String::new();
            job.error = None;
            job.media_id = Some(archived.id);
            job.cover_path = archived.cover_path.clone();
            let _ = store.save_job(&job);
            emit(JobEvent::Completed {
                job: job.clone(),
                media_id: archived.id,
            });
            return;
        }
        Err(FetchError::Canceled) => {
            job.state = DownloadState::Canceled;
            job.stage = String::new();
            job.error = None;
            JobEvent::Canceled(job.clone())
        }
        Err(error) => {
            let message = error.to_string();
            job.state = DownloadState::Failed;
            job.stage = String::new();
            job.error = Some(message.clone());
            mark_media_failed(&store, job.media_id);
            JobEvent::Failed {
                job: job.clone(),
                message,
            }
        }
    };
    let _ = store.save_job(&job);
    emit(terminal);
}

fn execute(
    settings: &Settings,
    store: &Store,
    job: &mut DownloadJob,
    cancel: &AtomicBool,
    emit: &dyn Fn(JobEvent),
) -> Result<MediaItem> {
    let mut source = parse_source(&job.source_url)
        .map_err(|error| FetchError::Unsupported(error.to_string()))?;
    if source.kind == SourceKind::Redirect {
        let resolved = http::resolve_redirect(&source.canonical_url)?;
        source =
            parse_source(&resolved).map_err(|error| FetchError::Unsupported(error.to_string()))?;
    }
    if source.kind != SourceKind::Video {
        return Err(FetchError::Unsupported(
            "This link points at a channel or playlist, not a single video".to_string(),
        ));
    }
    cancel_or(cancel)?;

    job.platform = source.platform;
    job.source_id = source.id.clone();
    job.source_url = source.canonical_url.clone();

    let layout = ArchiveLayout::new(
        settings.library_root.clone(),
        settings.naming_template.clone(),
    );
    let cookies = settings.cookies_for(source.platform);
    let bilibili = BilibiliClient::new(settings.cookies_for(Platform::Bilibili));
    let ytdlp = if source.platform == Platform::Bilibili {
        None
    } else {
        Some(YtDlp::discover(settings)?)
    };

    let job_id = job.id;
    let mut reporter = Reporter::new(store, job, emit);
    reporter.set_state(DownloadState::Fetching, "Reading video details");

    let mut item: MediaItem;
    let cover_url: Option<String>;
    let avatar_url: Option<String>;
    let mut details: Option<VideoDetails> = None;

    if source.platform == Platform::Bilibili {
        let fetched = bilibili.fetch_video(&source)?;
        item = fetched.item.clone();
        cover_url = Some(fetched.pic_url.clone()).filter(|url| !url.is_empty());
        avatar_url = fetched.avatar_url.clone();
        details = Some(fetched);
    } else {
        let tool = ytdlp
            .as_ref()
            .expect("yt-dlp is required for this platform");
        let metadata = tool.fetch_metadata(&source.canonical_url, cookies, cancel)?;
        item = MediaItem {
            platform: source.platform,
            source_id: source.id.clone(),
            source_url: source.canonical_url.clone(),
            title: metadata.title.clone(),
            creator: metadata.uploader.clone(),
            creator_id: metadata.uploader_id.clone(),
            description: metadata.description.clone(),
            published_at: metadata.timestamp,
            duration_secs: metadata.duration,
            tags: metadata.tags.clone(),
            view_count: metadata.view_count,
            like_count: metadata.like_count,
            comment_count: metadata.comment_count,
            status: MediaStatus::Pending,
            added_at: now_timestamp(),
            ..Default::default()
        };
        cover_url = metadata.thumbnail.clone();
        avatar_url = channel_avatar_url(
            source.platform,
            metadata.uploader_id.as_deref(),
            &metadata.uploader,
            cookies,
        );
    }
    if item.title.trim().is_empty() {
        item.title = "Untitled".to_string();
    }
    if item.creator.trim().is_empty() {
        item.creator = "Unknown creator".to_string();
    }

    if let Some(existing) = store.find_media(item.platform, &item.source_id)? {
        item.id = existing.id;
        item.watched = existing.watched;
        item.favorite = existing.favorite;
        item.added_at = existing.added_at;
        if item.creator_id.is_none() {
            item.creator_id = existing.creator_id;
        }
    }
    let media_id = store.upsert_media(&item)?;
    item.id = media_id;
    reporter.set_metadata(&item);

    if settings.fetch_comments && settings.comment_limit > 0 {
        reporter.set_state(DownloadState::Fetching, "Reading comments");
        let comments = match source.platform {
            Platform::Bilibili => bilibili
                .fetch_comments(
                    details.as_ref().map(|details| details.aid).unwrap_or(0),
                    settings.comment_limit,
                )
                .unwrap_or_default(),
            Platform::YouTube => ytdlp
                .as_ref()
                .expect("yt-dlp is required for YouTube")
                .fetch_comments(
                    &source.canonical_url,
                    cookies,
                    settings.comment_limit,
                    cancel,
                )
                .unwrap_or_default(),
            Platform::Twitch => Vec::new(),
        };
        cancel_or(cancel)?;
        item.comments = comments;
    }

    let work = WorkDir::new(store.library_root(), &source, job_id)?;
    let mut subtitles = Vec::new();
    let mut danmaku = None;
    let video_path: PathBuf;

    match source.platform {
        Platform::Bilibili => {
            let details = details
                .as_ref()
                .expect("Bilibili details are fetched before downloading");
            let streams = bilibili.fetch_streams(details, settings.quality)?;
            let stage = format!("Downloading {}", streams.quality_label);
            reporter.set_state(DownloadState::Downloading, &stage);
            let ffmpeg = tool::find_tool(settings.ffmpeg_path.as_deref(), "ffmpeg");
            video_path = bilibili.download_streams(
                &streams,
                &work.path,
                ffmpeg.as_deref(),
                cancel,
                |progress| reporter.progress(progress),
            )?;
            if settings.write_subtitles {
                subtitles = bilibili
                    .download_subtitles(details, &work.path)
                    .unwrap_or_default();
            }
            if settings.write_danmaku
                && let Ok(bytes) = bilibili.fetch_danmaku(details.cid)
            {
                let path = work.path.join("danmaku.xml");
                if std::fs::write(&path, bytes).is_ok() {
                    danmaku = Some(path);
                }
            }
        }
        _ => {
            reporter.set_state(DownloadState::Downloading, "Downloading video");
            let tool = ytdlp
                .as_ref()
                .expect("yt-dlp is required for this platform");
            let options = YtOptions {
                subtitles: settings.write_subtitles,
                merge_mp4: true,
            };
            let output = tool.download(
                &item.source_url,
                &work.path,
                settings.quality.ytdlp_format(),
                cookies,
                &options,
                cancel,
                |progress| reporter.progress(progress),
            )?;
            video_path = output.video;
            subtitles = output.subtitles;
        }
    }

    cancel_or(cancel)?;
    reporter.set_state(DownloadState::Processing, "Organizing files");

    let cover_source = work.path.join("cover.img");
    if let Some(url) = cover_url.as_deref() {
        let _ = download_artwork(&bilibili, source.platform, url, &cover_source, cancel);
    }
    let avatar_source = work.path.join("avatar.img");
    if let Some(url) = avatar_url.as_deref() {
        let _ = download_artwork(&bilibili, source.platform, url, &avatar_source, cancel);
    }
    reporter.set_state(DownloadState::Processing, "Writing metadata");

    let archived = archive_media(
        &layout,
        ArchiveInput {
            item: item.clone(),
            video_source: video_path,
            cover_source: cover_source.exists().then_some(cover_source),
            avatar_source: avatar_source.exists().then_some(avatar_source),
            subtitles,
            danmaku_xml: danmaku,
        },
    )?;
    let archived_id = store.upsert_media(&archived)?;
    let mut archived = archived;
    archived.id = archived_id;
    job.media_id = Some(archived_id);
    job.cover_path = archived.cover_path.clone();
    Ok(archived)
}

fn mark_media_failed(store: &Store, media_id: Option<i64>) {
    let Some(media_id) = media_id else {
        return;
    };
    let Ok(Some(item)) = store.get_media(media_id) else {
        return;
    };
    let has_video = item.video_path.as_ref().is_some_and(|path| path.exists());
    if !has_video {
        let _ = store.set_media_status(media_id, MediaStatus::Failed);
    }
}

fn cancel_or(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(FetchError::Canceled)
    } else {
        Ok(())
    }
}

fn download_artwork(
    bilibili: &BilibiliClient,
    platform: Platform,
    url: &str,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    if platform == Platform::Bilibili {
        bilibili.download_image(url, destination, cancel)
    } else {
        let agent = http::agent();
        http::download_to(&agent, url, &[], destination, cancel, |_, _, _| {})?;
        Ok(())
    }
}

/// Ask the channel page for its social preview image. This keeps creator
/// avatars archived without running a second metadata extraction.
fn channel_avatar_url(
    platform: Platform,
    uploader_id: Option<&str>,
    uploader: &str,
    cookies: Option<&Path>,
) -> Option<String> {
    let page = match platform {
        Platform::YouTube => {
            let id = uploader_id?;
            if id.starts_with("UC") {
                format!("https://www.youtube.com/channel/{id}")
            } else if id.starts_with('@') {
                format!("https://www.youtube.com/{id}")
            } else {
                return None;
            }
        }
        Platform::Twitch => {
            if uploader.trim().is_empty() {
                return None;
            }
            format!("https://www.twitch.tv/{uploader}")
        }
        _ => return None,
    };
    let domain = match platform {
        Platform::YouTube => "youtube.com",
        Platform::Twitch => "twitch.tv",
        Platform::Bilibili => "bilibili.com",
    };
    let cookie = cookies.and_then(|path| http::cookie_header(path, domain));
    let mut headers: Vec<(&str, &str)> = vec![("Accept-Language", "en-US,en;q=0.9")];
    if let Some(cookie) = cookie.as_deref() {
        headers.push(("Cookie", cookie));
    }
    let html = http::get_text(&http::agent(), &page, &headers, 4 * 1024 * 1024).ok()?;
    http::og_image(&html)
}

struct Reporter<'a> {
    store: &'a Store,
    job: &'a mut DownloadJob,
    emit: &'a dyn Fn(JobEvent),
    last: Instant,
}

impl<'a> Reporter<'a> {
    fn new(store: &'a Store, job: &'a mut DownloadJob, emit: &'a dyn Fn(JobEvent)) -> Self {
        Self {
            store,
            job,
            emit,
            last: Instant::now() - Duration::from_secs(1),
        }
    }

    fn set_metadata(&mut self, item: &MediaItem) {
        self.job.media_id = Some(item.id);
        self.job.title = item.title.clone();
        self.job.creator = item.creator.clone();
        self.job.platform = item.platform;
        self.job.source_id = item.source_id.clone();
        self.flush();
    }

    fn set_state(&mut self, state: DownloadState, stage: &str) {
        self.job.state = state;
        self.job.stage = stage.to_string();
        self.job.error = None;
        match state {
            DownloadState::Fetching => self.job.progress = self.job.progress.max(0.01),
            DownloadState::Downloading => self.job.progress = self.job.progress.max(0.02),
            DownloadState::Processing => self.job.progress = self.job.progress.max(0.97),
            _ => {}
        }
        self.flush();
    }

    fn progress(&mut self, progress: TransferProgress) {
        let fraction = progress.fraction();
        let mapped = 0.02 + fraction * 0.93;
        if mapped > self.job.progress {
            self.job.progress = mapped;
        }
        self.job.downloaded_bytes = progress.downloaded;
        self.job.total_bytes = progress.total;
        self.job.speed_bps = progress.speed;
        self.job.eta_secs = progress.eta;
        if self.last.elapsed() >= Duration::from_millis(300) {
            self.flush();
        }
    }

    fn flush(&mut self) {
        let _ = self.store.save_job(self.job);
        (self.emit)(JobEvent::Updated(self.job.clone()));
        self.last = Instant::now();
    }
}

/// Temporary download folder that always cleans itself up.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn new(root: &Path, source: &SourceRef, job_id: i64) -> Result<Self> {
        let path = root.join(".videleum").join("tmp").join(format!(
            "{}-{}-{}-{}",
            source.platform.as_str(),
            sanitize_component(&source.id),
            job_id,
            now_timestamp()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_video_links() {
        let root =
            std::env::temp_dir().join(format!("videleum-download-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = Store::open(&root).unwrap();
        let source = parse_source("https://www.youtube.com/@MKBHD").unwrap();
        let error = enqueue(&store, &source).unwrap_err();
        assert!(error.to_string().contains("Only video links"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn work_dirs_are_removed() {
        let root =
            std::env::temp_dir().join(format!("videleum-workdir-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = parse_source("https://www.youtube.com/watch?v=x").unwrap();
        let path = {
            let work = WorkDir::new(&root, &source, 1).unwrap();
            assert!(work.path.exists());
            work.path.clone()
        };
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
