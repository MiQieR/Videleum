//! Bilibili web API adapter.
//!
//! Bilibili is handled natively rather than through yt-dlp: the app needs DASH
//! stream selection, WBI request signing, danmaku XML and separate subtitle
//! tracks, all of which the public web API exposes directly.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use md5::{Digest, Md5};
use serde_json::Value;
use videleum_core::archive::sanitize_component;
use videleum_core::model::{MediaItem, MediaStatus, Subscription, now_timestamp};
use videleum_core::{Platform, Quality, SourceKind, SourceRef};

use crate::subscriptions::{FeedEntry, FeedPage};
use crate::tool;
use crate::{FetchError, Result, TransferProgress, http};

const REFERER: &str = "https://www.bilibili.com/";
const API_ROOT: &str = "https://api.bilibili.com";

/// The order in which the 64 halves of the WBI keys are interleaved.
const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

#[derive(Clone, Debug, Default)]
pub struct SubtitleTrack {
    pub lan: String,
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct VideoDetails {
    pub item: MediaItem,
    pub aid: u64,
    pub cid: u64,
    pub pic_url: String,
    pub avatar_url: Option<String>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}

#[derive(Clone, Debug, Default)]
pub struct Streams {
    pub video_url: String,
    pub video_backups: Vec<String>,
    pub audio_url: Option<String>,
    pub audio_backups: Vec<String>,
    pub quality_label: String,
    pub approx_bytes: u64,
}

pub struct BilibiliClient {
    agent: ureq::Agent,
    cookie: Option<String>,
    buvid3: Mutex<Option<String>>,
    mixin_key: Mutex<Option<String>>,
}

impl BilibiliClient {
    pub fn new(cookies: Option<&Path>) -> Self {
        Self {
            agent: http::agent(),
            cookie: cookies.and_then(|path| http::cookie_header(path, "bilibili.com")),
            buvid3: Mutex::new(None),
            mixin_key: Mutex::new(None),
        }
    }

    pub fn has_cookies(&self) -> bool {
        self.cookie.is_some()
    }

    fn cookie_header(&self) -> Option<String> {
        let mut parts: Vec<String> = self.cookie.clone().into_iter().collect();
        if let Ok(guard) = self.buvid3.lock()
            && let Some(buvid) = guard.as_deref()
            && !parts.iter().any(|part| part.starts_with("buvid3="))
        {
            parts.push(format!("buvid3={buvid}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        }
    }

    fn request_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            ("Referer".to_string(), REFERER.to_string()),
            ("Origin".to_string(), "https://www.bilibili.com".to_string()),
            (
                "Accept".to_string(),
                "application/json, text/plain, */*".to_string(),
            ),
        ];
        if let Some(cookie) = self.cookie_header() {
            headers.push(("Cookie".to_string(), cookie));
        }
        headers
    }

    fn api(&self, url: &str) -> Result<Value> {
        let headers = self.request_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let value = http::get_json(&self.agent, url, &refs)?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 {
            return Err(api_error(&value, code));
        }
        Ok(value.get("data").cloned().unwrap_or(Value::Null))
    }

    fn ensure_buvid(&self) {
        if self
            .cookie
            .as_deref()
            .is_some_and(|cookie| cookie.contains("buvid3="))
        {
            return;
        }
        if self
            .buvid3
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
        {
            return;
        }
        let url = format!("{API_ROOT}/x/frontend/finger/spi");
        let headers = [("Referer", REFERER)];
        if let Ok(value) = http::get_json(&self.agent, &url, &headers)
            && let Some(buvid) = value
                .get("data")
                .and_then(|data| data.get("b_3"))
                .and_then(Value::as_str)
            && let Ok(mut guard) = self.buvid3.lock()
        {
            *guard = Some(buvid.to_string());
        }
    }

    fn wbi_mixin_key(&self) -> Result<String> {
        if let Some(key) = self
            .mixin_key
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or(None)
        {
            return Ok(key);
        }
        let url = format!("{API_ROOT}/x/web-interface/nav");
        let value = self.api(&url)?;
        let image = value
            .get("wbi_img")
            .and_then(|image| image.get("img_url"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                FetchError::Api("Bilibili didn't return the signing keys. Try again.".to_string())
            })?;
        let sub = value
            .get("wbi_img")
            .and_then(|image| image.get("sub_url"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                FetchError::Api("Bilibili didn't return the signing keys. Try again.".to_string())
            })?;
        let raw = format!("{}{}", file_stem(image), file_stem(sub));
        let key: String = MIXIN_KEY_ENC_TAB
            .iter()
            .take(32)
            .filter_map(|index| raw.chars().nth(*index))
            .collect();
        if key.len() != 32 {
            return Err(FetchError::Api(
                "Bilibili returned an unexpected signing key".to_string(),
            ));
        }
        if let Ok(mut guard) = self.mixin_key.lock() {
            *guard = Some(key.clone());
        }
        Ok(key)
    }

    fn signed_url(&self, base: &str, params: &[(&str, String)]) -> Result<String> {
        self.ensure_buvid();
        let mixin = self.wbi_mixin_key()?;
        let mut pairs: Vec<(String, String)> = params
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect();
        pairs.push(("wts".to_string(), now_timestamp().to_string()));
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        let query = pairs
            .iter()
            .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
            .collect::<Vec<_>>()
            .join("&");
        let mut hasher = Md5::new();
        hasher.update(format!("{query}{mixin}").as_bytes());
        let digest = hasher.finalize();
        let mut signature = String::with_capacity(32);
        for byte in digest {
            signature.push_str(&format!("{byte:02x}"));
        }
        Ok(format!("{base}?{query}&w_rid={signature}"))
    }

    /// Read the video page, tags and subtitle index. Comments are fetched
    /// separately so adding a link to the library stays quick.
    pub fn fetch_video(&self, source: &SourceRef) -> Result<VideoDetails> {
        if source.platform != Platform::Bilibili {
            return Err(FetchError::Unsupported(
                "This link isn't a Bilibili video".to_string(),
            ));
        }
        let is_bv = source.id.starts_with("BV");
        let view_url = if is_bv {
            format!(
                "{API_ROOT}/x/web-interface/view?bvid={}",
                encode_component(&source.id)
            )
        } else {
            format!(
                "{API_ROOT}/x/web-interface/view?aid={}",
                encode_component(&source.id)
            )
        };
        let data = self.api(&view_url)?;
        let bvid = text(&data, "bvid");
        let aid = data.get("aid").and_then(Value::as_u64).unwrap_or(0);
        let pages = data
            .get("pages")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let page_index = source.page.unwrap_or(1).max(1) as usize;
        let page = pages.get(page_index.saturating_sub(1));
        let cid = page
            .and_then(|page| page.get("cid"))
            .and_then(Value::as_u64)
            .or_else(|| data.get("cid").and_then(Value::as_u64))
            .unwrap_or(0);
        if cid == 0 {
            return Err(FetchError::Parse(
                "Bilibili didn't return a playable part for this video".to_string(),
            ));
        }

        let base_title = text(&data, "title");
        let title = match page {
            Some(page) if pages.len() > 1 => {
                let part = text(page, "part");
                if part.is_empty() {
                    format!("{base_title} - P{page_index}")
                } else {
                    format!("{base_title} - P{page_index} {part}")
                }
            }
            _ => base_title.clone(),
        };
        let source_id = if pages.len() > 1 {
            format!("{bvid}-p{page_index}")
        } else {
            bvid.clone()
        };
        let source_url = if pages.len() > 1 {
            format!("https://www.bilibili.com/video/{bvid}?p={page_index}")
        } else {
            format!("https://www.bilibili.com/video/{bvid}")
        };
        let duration_secs = page
            .and_then(|page| page.get("duration"))
            .and_then(Value::as_u64)
            .or_else(|| data.get("duration").and_then(Value::as_u64))
            .map(|seconds| seconds.min(u32::MAX as u64) as u32)
            .filter(|seconds| *seconds > 0);
        let owner = data.get("owner").cloned().unwrap_or(Value::Null);
        let stat = data.get("stat").cloned().unwrap_or(Value::Null);
        let picture = text(&data, "pic");
        let cover_url = http::absolute_media_url(&picture);
        let avatar_url = {
            let face = text(&owner, "face");
            (!face.is_empty()).then(|| http::absolute_media_url(&face))
        };

        let tags = self.fetch_tags(&bvid).unwrap_or_default();

        let mut item = MediaItem {
            platform: Platform::Bilibili,
            source_id,
            source_url,
            title,
            creator: text(&owner, "name"),
            creator_id: owner
                .get("mid")
                .and_then(Value::as_u64)
                .map(|mid| mid.to_string()),
            description: text(&data, "desc"),
            published_at: data
                .get("pubdate")
                .and_then(Value::as_i64)
                .filter(|timestamp| *timestamp > 0),
            duration_secs,
            tags,
            view_count: stat.get("view").and_then(Value::as_u64),
            like_count: stat.get("like").and_then(Value::as_u64),
            comment_count: stat.get("reply").and_then(Value::as_u64),
            added_at: now_timestamp(),
            status: MediaStatus::Pending,
            ..Default::default()
        };
        if item.creator.is_empty() {
            item.creator = "Unknown creator".to_string();
        }

        let subtitle_tracks = self.fetch_subtitle_tracks(&bvid, cid).unwrap_or_default();

        Ok(VideoDetails {
            item,
            aid,
            cid,
            pic_url: cover_url,
            avatar_url,
            subtitle_tracks,
        })
    }

    fn fetch_tags(&self, bvid: &str) -> Result<Vec<String>> {
        if bvid.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{API_ROOT}/x/tag/archive/tags?bvid={}",
            encode_component(bvid)
        );
        let data = self.api(&url)?;
        Ok(data
            .as_array()
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.get("tag_name").and_then(Value::as_str))
                    .map(str::to_string)
                    .take(50)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn fetch_subtitle_tracks(&self, bvid: &str, cid: u64) -> Result<Vec<SubtitleTrack>> {
        let url = format!(
            "{API_ROOT}/x/player/v2?bvid={}&cid={cid}",
            encode_component(bvid)
        );
        let data = self.api(&url)?;
        let subtitles = data
            .get("subtitle")
            .and_then(|subtitle| subtitle.get("subtitles"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(subtitles
            .iter()
            .filter_map(|track| {
                let url = track.get("subtitle_url").and_then(Value::as_str)?;
                if url.is_empty() {
                    return None;
                }
                Some(SubtitleTrack {
                    lan: track
                        .get("lan")
                        .and_then(Value::as_str)
                        .unwrap_or("und")
                        .to_string(),
                    label: track
                        .get("lan_doc")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    url: http::absolute_media_url(url),
                })
            })
            .take(6)
            .collect())
    }

    /// High-quality comments written into the NFO. This endpoint changes more
    /// often than the rest, so failures are reported and ignored by callers.
    pub fn fetch_comments(&self, aid: u64, limit: usize) -> Result<Vec<videleum_core::Comment>> {
        if limit == 0 || aid == 0 {
            return Ok(Vec::new());
        }
        let signed = self.signed_url(
            &format!("{API_ROOT}/x/v2/reply/wbi/main"),
            &[
                ("oid", aid.to_string()),
                ("type", "1".to_string()),
                ("mode", "3".to_string()),
                ("ps", limit.to_string()),
                ("next", "0".to_string()),
            ],
        );
        let data = match signed.and_then(|url| self.api(&url)) {
            Ok(data) => data,
            Err(_) => {
                let url = format!(
                    "{API_ROOT}/x/v2/reply?type=1&oid={aid}&sort=1&ps={}&pn=1",
                    limit
                );
                self.api(&url)?
            }
        };
        let mut comments = Vec::new();
        collect_replies(
            data.get("top_replies").and_then(Value::as_array),
            limit,
            &mut comments,
        );
        if comments.len() < limit {
            collect_replies(
                data.get("replies").and_then(Value::as_array),
                limit,
                &mut comments,
            );
        }
        comments.truncate(limit);
        Ok(comments)
    }

    /// Fetch and decompress the XML danmaku for a part.
    pub fn fetch_danmaku(&self, cid: u64) -> Result<Vec<u8>> {
        let url = format!("https://comment.bilibili.com/{cid}.xml");
        let headers = self.request_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let bytes = http::get_bytes(&self.agent, &url, &refs, 64 * 1024 * 1024)?;
        Ok(decompress_xml(&bytes))
    }

    /// Download a cover or avatar. Bilibili's image hosts require the same
    /// referer as the media CDN.
    pub fn download_image(&self, url: &str, destination: &Path, cancel: &AtomicBool) -> Result<()> {
        let headers = self.request_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        http::download_to(&self.agent, url, &refs, destination, cancel, |_, _, _| {})?;
        Ok(())
    }

    pub fn fetch_streams(&self, details: &VideoDetails, quality: Quality) -> Result<Streams> {
        let bvid = details.item.source_id.split("-p").next().unwrap_or("");
        let qn = quality.bilibili_qn().to_string();
        let params = vec![
            ("bvid", bvid.to_string()),
            ("cid", details.cid.to_string()),
            ("qn", qn),
            ("fnver", "0".to_string()),
            ("fnval", "4048".to_string()),
            ("fourk", "1".to_string()),
        ];
        let signed = self.signed_url(&format!("{API_ROOT}/x/player/wbi/playurl"), &params);
        let data = match signed.and_then(|url| self.api(&url)) {
            Ok(data) => data,
            Err(_) => {
                let url = format!(
                    "{API_ROOT}/x/player/playurl?bvid={}&cid={}&qn={}&fnver=0&fnval=4048&fourk=1",
                    encode_component(bvid),
                    details.cid,
                    quality.bilibili_qn()
                );
                self.api(&url)?
            }
        };
        select_streams(&data, quality)
    }

    /// Download selected streams into `destination` and produce one playable
    /// MP4. DASH video and audio arrive separately and are merged with ffmpeg.
    #[allow(clippy::too_many_arguments)]
    pub fn download_streams(
        &self,
        streams: &Streams,
        destination: &Path,
        ffmpeg: Option<&Path>,
        cancel: &AtomicBool,
        mut progress: impl FnMut(TransferProgress),
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(destination)?;
        let headers = self.request_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let video_path = destination.join("video.m4s");
        let audio_path = destination.join("audio.m4s");

        let mut downloaded = 0u64;
        let mut totals: Option<u64> = None;
        let read_video_total = std::sync::Arc::new(std::sync::Mutex::new(0u64));
        let video_total_slot = read_video_total.clone();
        download_first(
            &self.agent,
            &streams.video_url,
            &streams.video_backups,
            &refs,
            &video_path,
            cancel,
            |done, total, speed| {
                downloaded = done;
                if let Some(total) = total {
                    if let Ok(mut slot) = video_total_slot.lock() {
                        *slot = total;
                    }
                    totals = Some(total);
                }
                progress(TransferProgress {
                    downloaded,
                    total: totals,
                    speed: Some(speed).filter(|speed| *speed > 0),
                    eta: None,
                });
            },
        )?;
        let video_total = *read_video_total
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let video_done = downloaded;

        if let Some(audio_url) = &streams.audio_url {
            if cancel.load(Ordering::Relaxed) {
                return Err(FetchError::Canceled);
            }
            let mut audio_total = None;
            download_first(
                &self.agent,
                audio_url,
                &streams.audio_backups,
                &refs,
                &audio_path,
                cancel,
                |done, total, speed| {
                    if let Some(total) = total {
                        audio_total = Some(total);
                    }
                    let combined_total = match (video_total, audio_total) {
                        (0, None) => None,
                        (0, Some(total)) => Some(total),
                        (video, None) => Some(video + done),
                        (video, Some(total)) => Some(video + total),
                    };
                    progress(TransferProgress {
                        downloaded: video_done + done,
                        total: combined_total,
                        speed: Some(speed).filter(|speed| *speed > 0),
                        eta: None,
                    });
                },
            )?;
        }

        let output = destination.join("video.mp4");
        let ffmpeg = ffmpeg.ok_or_else(|| {
            FetchError::Tool(
                "Bilibili downloads need FFmpeg to combine the video and audio tracks. Set the FFmpeg path in Settings."
                    .to_string(),
            )
        })?;
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Canceled);
        }
        let mut command = tool::command(ffmpeg);
        command.args(["-hide_banner", "-loglevel", "error", "-y"]);
        command.arg("-i").arg(&video_path);
        if audio_path.exists() {
            command.arg("-i").arg(&audio_path);
            command.args(["-map", "0:v:0", "-map", "1:a:0"]);
        }
        command.args(["-c", "copy", "-movflags", "+faststart"]);
        command.arg(&output);
        let status = command
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| FetchError::Tool(format!("Couldn't run FFmpeg: {error}")))?;
        if !status.status.success() {
            let detail = String::from_utf8_lossy(&status.stderr);
            return Err(FetchError::Tool(format!(
                "FFmpeg couldn't combine the Bilibili streams: {}",
                tool::last_lines(&detail, 5)
            )));
        }
        let _ = std::fs::remove_file(&video_path);
        let _ = std::fs::remove_file(&audio_path);
        Ok(output)
    }

    pub fn download_subtitles(
        &self,
        details: &VideoDetails,
        destination: &Path,
    ) -> Result<Vec<PathBuf>> {
        if details.subtitle_tracks.is_empty() {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(destination)?;
        let headers = self.request_headers();
        let refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let mut paths = Vec::new();
        for (index, track) in details.subtitle_tracks.iter().enumerate() {
            let bytes = match http::get_bytes(&self.agent, &track.url, &refs, 8 * 1024 * 1024) {
                Ok(bytes) => bytes,
                Err(_) => continue,
            };
            let value: Value = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let srt = subtitle_json_to_srt(&value);
            if srt.trim().is_empty() {
                continue;
            }
            let language = if track.lan.trim().is_empty() {
                format!("track{index}")
            } else {
                sanitize_component(&track.lan)
            };
            let path = destination.join(format!("subtitle.{language}.srt"));
            std::fs::write(&path, srt)?;
            paths.push(path);
        }
        Ok(paths)
    }

    pub fn fetch_feed(&self, subscription: &Subscription, limit: usize) -> Result<FeedPage> {
        let limit = limit.clamp(1, 60);
        match subscription.kind {
            SourceKind::Favorites => self.fetch_favorites_feed(&subscription.source_id, limit),
            SourceKind::Playlist => self.fetch_playlist_feed(subscription, limit),
            _ => self.fetch_channel_feed(&subscription.source_id, limit),
        }
    }

    fn fetch_channel_feed(&self, mid: &str, limit: usize) -> Result<FeedPage> {
        self.ensure_buvid();
        let url = self.signed_url(
            &format!("{API_ROOT}/x/space/wbi/arc/search"),
            &[
                ("mid", mid.to_string()),
                ("ps", limit.to_string()),
                ("pn", "1".to_string()),
                ("order", "pubdate".to_string()),
                ("platform", "web".to_string()),
                ("web_location", "1550101".to_string()),
            ],
        )?;
        let data = self.api(&url)?;
        let entries = data
            .get("list")
            .and_then(|list| list.get("vlist"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(feed_entry_from_search)
                    .take(limit)
                    .collect()
            })
            .unwrap_or_default();
        let card = self.fetch_card(mid).ok();
        Ok(FeedPage {
            title: card
                .as_ref()
                .and_then(|card| card.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| subscription_title(mid)),
            avatar_url: card
                .as_ref()
                .and_then(|card| card.get("face"))
                .and_then(Value::as_str)
                .filter(|face| !face.is_empty())
                .map(http::absolute_media_url),
            entries,
        })
    }

    fn fetch_card(&self, mid: &str) -> Result<Value> {
        let url = format!(
            "{API_ROOT}/x/web-interface/card?mid={}",
            encode_component(mid)
        );
        let data = self.api(&url)?;
        Ok(data.get("card").cloned().unwrap_or(Value::Null))
    }

    fn fetch_favorites_feed(&self, media_id: &str, limit: usize) -> Result<FeedPage> {
        self.ensure_buvid();
        let url = format!(
            "{API_ROOT}/x/v3/fav/resource/list?media_id={}&pn=1&ps={}&order=mtime&type=0&platform=web",
            encode_component(media_id),
            limit
        );
        let data = self.api(&url)?;
        let info = data.get("info").cloned().unwrap_or(Value::Null);
        let upper = info.get("upper").cloned().unwrap_or(Value::Null);
        let entries = data
            .get("medias")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|media| {
                        let bvid = media.get("bvid").and_then(Value::as_str)?;
                        if bvid.is_empty() {
                            return None;
                        }
                        Some(FeedEntry {
                            platform: Platform::Bilibili,
                            source_id: bvid.to_string(),
                            url: format!("https://www.bilibili.com/video/{bvid}"),
                            title: media
                                .get("title")
                                .and_then(Value::as_str)
                                .unwrap_or("Untitled")
                                .to_string(),
                            creator: media
                                .get("upper")
                                .and_then(|upper| upper.get("name"))
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            duration_secs: media
                                .get("duration")
                                .and_then(Value::as_u64)
                                .map(|seconds| seconds.min(u32::MAX as u64) as u32),
                            thumbnail: media
                                .get("cover")
                                .and_then(Value::as_str)
                                .map(http::absolute_media_url),
                            published_at: media.get("pubtime").and_then(Value::as_i64),
                        })
                    })
                    .take(limit)
                    .collect()
            })
            .unwrap_or_default();
        Ok(FeedPage {
            title: info
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Bilibili favorites")
                .to_string(),
            avatar_url: upper
                .get("face")
                .and_then(Value::as_str)
                .filter(|face| !face.is_empty())
                .map(http::absolute_media_url),
            entries,
        })
    }

    fn fetch_playlist_feed(&self, subscription: &Subscription, limit: usize) -> Result<FeedPage> {
        let mid = mid_from_url(&subscription.source_url);
        if let Some(mid) = mid {
            let url = format!(
                "{API_ROOT}/x/polymer/web-space/seasons_archives_list?mid={mid}&season_id={}&sort_reverse=false&page_num=1&page_size={limit}",
                encode_component(&subscription.source_id)
            );
            if let Ok(data) = self.api(&url) {
                let entries = data
                    .get("archives")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                let bvid = item.get("bvid").and_then(Value::as_str)?;
                                Some(FeedEntry {
                                    platform: Platform::Bilibili,
                                    source_id: bvid.to_string(),
                                    url: format!("https://www.bilibili.com/video/{bvid}"),
                                    title: item
                                        .get("title")
                                        .and_then(Value::as_str)
                                        .unwrap_or("Untitled")
                                        .to_string(),
                                    creator: subscription.title.clone(),
                                    duration_secs: item
                                        .get("duration")
                                        .and_then(Value::as_u64)
                                        .map(|seconds| seconds.min(u32::MAX as u64) as u32),
                                    thumbnail: item
                                        .get("pic")
                                        .and_then(Value::as_str)
                                        .map(http::absolute_media_url),
                                    published_at: item.get("pubdate").and_then(Value::as_i64),
                                })
                            })
                            .take(limit)
                            .collect()
                    })
                    .unwrap_or_default();
                return Ok(FeedPage {
                    title: subscription.title.clone(),
                    avatar_url: None,
                    entries,
                });
            }
        }
        Err(FetchError::Unsupported(
            "Bilibili collections are only supported for links that include the creator's space"
                .to_string(),
        ))
    }
}

fn subscription_title(mid: &str) -> String {
    format!("UP {mid}")
}

fn collect_replies(
    items: Option<&Vec<Value>>,
    limit: usize,
    comments: &mut Vec<videleum_core::Comment>,
) {
    let Some(items) = items else {
        return;
    };
    for reply in items {
        if comments.len() >= limit {
            break;
        }
        let message = reply
            .get("content")
            .and_then(|content| content.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if message.is_empty() {
            continue;
        }
        comments.push(videleum_core::Comment {
            author: reply
                .get("member")
                .and_then(|member| member.get("uname"))
                .and_then(Value::as_str)
                .unwrap_or("Unknown")
                .to_string(),
            text: message.to_string(),
            likes: reply.get("like").and_then(Value::as_u64),
            created_at: reply.get("ctime").and_then(Value::as_i64),
        });
    }
}

fn mid_from_url(url: &str) -> Option<&str> {
    let rest = url.split("space.bilibili.com/").nth(1)?;
    let mid = rest.split(['/', '?']).next()?;
    mid.chars().all(|c| c.is_ascii_digit()).then_some(mid)
}

fn feed_entry_from_search(value: &Value) -> Option<FeedEntry> {
    let bvid = value.get("bvid").and_then(Value::as_str)?;
    if bvid.is_empty() {
        return None;
    }
    Some(FeedEntry {
        platform: Platform::Bilibili,
        source_id: bvid.to_string(),
        url: format!("https://www.bilibili.com/video/{bvid}"),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("Untitled")
            .to_string(),
        creator: value
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        duration_secs: value
            .get("length")
            .and_then(Value::as_str)
            .and_then(parse_length),
        thumbnail: value
            .get("pic")
            .and_then(Value::as_str)
            .map(http::absolute_media_url),
        published_at: value.get("created").and_then(Value::as_i64),
    })
}

fn parse_length(value: &str) -> Option<u32> {
    let mut total = 0u32;
    let mut parts = value.split(':').rev();
    for (index, part) in parts.by_ref().enumerate() {
        let number: u32 = part.trim().parse().ok()?;
        total += number * 60u32.pow(index as u32);
    }
    (total > 0).then_some(total)
}

fn select_streams(data: &Value, quality: Quality) -> Result<Streams> {
    if let Some(dash) = data.get("dash") {
        let max_qn = quality.bilibili_qn();
        let mut videos = dash
            .get("video")
            .and_then(Value::as_array)
            .map(|items| items.to_vec())
            .unwrap_or_default();
        if videos.is_empty() {
            return Err(FetchError::Api(
                "Bilibili didn't return a playable video stream".to_string(),
            ));
        }
        videos.retain(|video| {
            video
                .get("id")
                .and_then(Value::as_u64)
                .is_some_and(|id| id <= max_qn as u64)
        });
        if videos.is_empty() {
            return Err(FetchError::Api(
                "This video isn't available at the selected quality".to_string(),
            ));
        }
        let best_id = videos
            .iter()
            .filter_map(|video| video.get("id").and_then(Value::as_u64))
            .max()
            .unwrap_or(0);
        // AVC (7) plays everywhere; HEVC (12) and AV1 (13) are fallbacks.
        for codec in [7u64, 12, 13] {
            let mut candidates: Vec<&Value> = videos
                .iter()
                .filter(|video| {
                    video.get("id").and_then(Value::as_u64) == Some(best_id)
                        && video.get("codecid").and_then(Value::as_u64) == Some(codec)
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            candidates.sort_by_key(|video| {
                std::cmp::Reverse(video.get("bandwidth").and_then(Value::as_u64).unwrap_or(0))
            });
            let Some(video) = candidates.first() else {
                continue;
            };
            let Some(video_url) = stream_url(video) else {
                continue;
            };
            let video_backups = stream_backups(video);
            let mut audios = dash
                .get("audio")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            audios.sort_by_key(|audio| {
                std::cmp::Reverse(audio.get("bandwidth").and_then(Value::as_u64).unwrap_or(0))
            });
            let audio = audios.first();
            let quality_label = quality_label(best_id as u32);
            return Ok(Streams {
                video_url,
                video_backups,
                audio_url: audio.and_then(stream_url),
                audio_backups: audio.map(stream_backups).unwrap_or_default(),
                quality_label,
                approx_bytes: video.get("bandwidth").and_then(Value::as_u64).unwrap_or(0),
            });
        }
        Err(FetchError::Api(
            "Bilibili didn't return a usable video stream".to_string(),
        ))
    } else if let Some(durl) = data
        .get("durl")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    {
        let video_url = stream_url(durl).ok_or_else(|| {
            FetchError::Api("Bilibili didn't return a playable stream".to_string())
        })?;
        Ok(Streams {
            video_url,
            video_backups: stream_backups(durl),
            audio_url: None,
            audio_backups: Vec::new(),
            quality_label: quality_label(
                data.get("quality").and_then(Value::as_u64).unwrap_or(0) as u32
            ),
            approx_bytes: durl.get("size").and_then(Value::as_u64).unwrap_or(0),
        })
    } else {
        Err(FetchError::Api(
            "Bilibili didn't return a playable stream for this video".to_string(),
        ))
    }
}

fn stream_url(value: &Value) -> Option<String> {
    value
        .get("baseUrl")
        .or_else(|| value.get("base_url"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(http::absolute_media_url)
}

fn stream_backups(value: &Value) -> Vec<String> {
    value
        .get("backupUrl")
        .or_else(|| value.get("backup_url"))
        .and_then(Value::as_array)
        .map(|urls| {
            urls.iter()
                .filter_map(Value::as_str)
                .filter(|url| !url.is_empty())
                .map(http::absolute_media_url)
                .collect()
        })
        .unwrap_or_default()
}

fn quality_label(qn: u32) -> String {
    match qn {
        125..=127 => "8K".to_string(),
        120 => "4K".to_string(),
        116 => "1440p".to_string(),
        112 => "1080p+".to_string(),
        80 => "1080p".to_string(),
        74 => "720p60".to_string(),
        64 => "720p".to_string(),
        32 => "480p".to_string(),
        16 => "360p".to_string(),
        other => format!("{other}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn download_first(
    agent: &ureq::Agent,
    url: &str,
    backups: &[String],
    headers: &[(&str, &str)],
    destination: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>, u64),
) -> Result<()> {
    let mut candidates = vec![url.to_string()];
    candidates.extend(backups.iter().cloned());
    let mut last_error = None;
    for candidate in candidates {
        if cancel.load(Ordering::Relaxed) {
            return Err(FetchError::Canceled);
        }
        match http::download_to(
            agent,
            &candidate,
            headers,
            destination,
            cancel,
            &mut progress,
        ) {
            Ok(_) => return Ok(()),
            Err(FetchError::Canceled) => return Err(FetchError::Canceled),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| FetchError::Network("No download server responded".into())))
}

/// Bilibili sometimes serves zlib/deflate even when a plain XML body is
/// expected. Try the common encodings before giving up.
fn decompress_xml(bytes: &[u8]) -> Vec<u8> {
    let trimmed = bytes.trim_ascii_start();
    if trimmed.starts_with(b"<") {
        return bytes.to_vec();
    }
    use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
    for decoder in 0..3 {
        let mut output = Vec::new();
        let result = match decoder {
            0 => ZlibDecoder::new(trimmed).read_to_end(&mut output),
            1 => DeflateDecoder::new(trimmed).read_to_end(&mut output),
            _ => GzDecoder::new(trimmed).read_to_end(&mut output),
        };
        if let Ok(read) = result
            && read > 0
            && output.trim_ascii_start().starts_with(b"<")
        {
            return output;
        }
    }
    bytes.to_vec()
}

fn subtitle_json_to_srt(value: &Value) -> String {
    let body = value
        .get("body")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut output = String::with_capacity(body.len() * 100);
    for (index, line) in body.iter().enumerate() {
        let from = line.get("from").and_then(Value::as_f64).unwrap_or(0.0);
        let to = line.get("to").and_then(Value::as_f64).unwrap_or(0.0);
        let content = line
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if content.is_empty() || to <= from {
            continue;
        }
        output.push_str(&(index + 1).to_string());
        output.push('\n');
        output.push_str(&format!(
            "{} --> {}\n{}\n\n",
            srt_time(from),
            srt_time(to),
            content
        ));
    }
    output
}

fn srt_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let hours = (seconds / 3600.0).floor() as u32;
    let minutes = ((seconds % 3600.0) / 60.0).floor() as u32;
    let whole = (seconds % 60.0).floor() as u32;
    let millis = ((seconds - seconds.floor()) * 1000.0).round() as u32;
    let (whole, millis) = if millis >= 1000 {
        (whole + 1, 0)
    } else {
        (whole, millis)
    };
    format!("{hours:02}:{minutes:02}:{whole:02},{millis:03}")
}

fn api_error(value: &Value, code: i64) -> FetchError {
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("Unknown error");
    let friendly = match code {
        -101 => "Sign in to Bilibili in Settings to view this content.",
        -352 | -412 => "Bilibili rejected the request. Add fresh Bilibili cookies in Settings.",
        -403 => "This video is restricted and can't be downloaded.",
        -404 => "This Bilibili video doesn't exist or was deleted.",
        -10403 => "This video is region-locked and can't be downloaded here.",
        62002 => "This video is private.",
        62004 => "This video is only available to channel members.",
        _ => return FetchError::Api(format!("Bilibili: {message} ({code})")),
    };
    FetchError::Api(friendly.to_string())
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn file_stem(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .to_string()
}

fn encode_component(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            other => output.push_str(&format!("%{other:02X}")),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_wbi_components() {
        assert_eq!(encode_component("hello world"), "hello%20world");
        assert_eq!(encode_component("中文"), "%E4%B8%AD%E6%96%87");
        assert_eq!(encode_component("a+b&c"), "a%2Bb%26c");
    }

    #[test]
    fn selects_the_highest_allowed_avc_stream() {
        let value: Value = serde_json::from_str(
            r#"{
                "dash": {
                    "video": [
                        {"id": 120, "codecid": 7, "bandwidth": 9000, "baseUrl": "https://cdn/4k-avc.m4s"},
                        {"id": 80, "codecid": 7, "bandwidth": 4000, "baseUrl": "https://cdn/1080-avc.m4s"},
                        {"id": 80, "codecid": 12, "bandwidth": 3000, "baseUrl": "https://cdn/1080-hevc.m4s"},
                        {"id": 64, "codecid": 7, "bandwidth": 2000, "baseUrl": "https://cdn/720-avc.m4s"}
                    ],
                    "audio": [
                        {"id": 30280, "bandwidth": 192000, "baseUrl": "https://cdn/a192.m4s"},
                        {"id": 30216, "bandwidth": 64000, "baseUrl": "https://cdn/a64.m4s"}
                    ]
                }
            }"#,
        )
        .unwrap();
        let streams = select_streams(&value, Quality::P1080).unwrap();
        assert_eq!(streams.video_url, "https://cdn/1080-avc.m4s");
        assert_eq!(streams.audio_url.as_deref(), Some("https://cdn/a192.m4s"));
        assert_eq!(streams.quality_label, "1080p");

        let best = select_streams(&value, Quality::Best).unwrap();
        assert_eq!(best.video_url, "https://cdn/4k-avc.m4s");
    }

    #[test]
    fn converts_subtitle_json_to_srt() {
        let value: Value = serde_json::from_str(
            r#"{"body":[
                {"from":1.5,"to":3.25,"content":"Hello"},
                {"from":4.0,"to":5.0,"content":"World"}
            ]}"#,
        )
        .unwrap();
        let srt = subtitle_json_to_srt(&value);
        assert!(srt.contains("00:00:01,500 --> 00:00:03,250"));
        assert!(srt.contains("Hello"));
        assert!(srt.contains("00:00:04,000 --> 00:00:05,000"));
    }

    #[test]
    fn decompresses_deflated_danmaku() {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        use std::io::Write;
        let xml = b"<?xml version=\"1.0\"?><i><d p=\"1,1,25,0,0,0,0,1\">hi</d></i>";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(xml).unwrap();
        let compressed = encoder.finish().unwrap();
        assert_eq!(decompress_xml(&compressed), xml);
        assert_eq!(decompress_xml(xml), xml);
    }

    #[test]
    fn extracts_space_id_from_urls() {
        assert_eq!(
            mid_from_url("https://space.bilibili.com/2267573/lists/123"),
            Some("2267573")
        );
        assert_eq!(mid_from_url("https://www.bilibili.com/video/BV1"), None);
    }

    #[test]
    fn parses_search_lengths() {
        assert_eq!(parse_length("01:02"), Some(62));
        assert_eq!(parse_length("1:02:03"), Some(3723));
        assert_eq!(parse_length("0:00"), None);
    }
}
