//! Recognition and normalization of supported media URLs.
//!
//! A [`SourceRef`] is intentionally platform-neutral: the app uses it to pick a
//! fetcher adapter, while the adapters own the platform-specific API calls.

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

/// A content platform supported by Videleum.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    #[default]
    YouTube,
    Bilibili,
    Twitch,
}

impl Platform {
    pub const ALL: [Self; 3] = [Self::YouTube, Self::Bilibili, Self::Twitch];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Bilibili => "bilibili",
            Self::Twitch => "twitch",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::Bilibili => "Bilibili",
            Self::Twitch => "Twitch",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "youtube" => Some(Self::YouTube),
            "bilibili" => Some(Self::Bilibili),
            "twitch" => Some(Self::Twitch),
            _ => None,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// What a link points at. The distinction changes the available action:
/// videos can be downloaded directly, while channels, playlists and favorites
/// belong in subscriptions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[default]
    Video,
    Channel,
    Playlist,
    Favorites,
    /// A shortened link that must be resolved with a network request.
    Redirect,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Channel => "channel",
            Self::Playlist => "playlist",
            Self::Favorites => "favorites",
            Self::Redirect => "redirect",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Channel => "Channel",
            Self::Playlist => "Playlist",
            Self::Favorites => "Favorites",
            Self::Redirect => "Link",
        }
    }
}

/// A parsed, canonical reference to content on a supported platform.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub platform: Platform,
    pub kind: SourceKind,
    pub id: String,
    pub canonical_url: String,
    /// Bilibili multi-part videos: the selected page, one-based.
    pub page: Option<u32>,
}

impl SourceRef {
    pub fn is_subscription(&self) -> bool {
        matches!(
            self.kind,
            SourceKind::Channel | SourceKind::Playlist | SourceKind::Favorites
        )
    }

    pub fn display_handle(&self) -> String {
        match self.platform {
            Platform::YouTube => match self.kind {
                SourceKind::Channel if self.id.starts_with('@') => self.id.clone(),
                SourceKind::Channel => format!("Channel {}", self.id),
                _ => self.id.clone(),
            },
            Platform::Bilibili => match self.kind {
                SourceKind::Channel => format!("UP {}", self.id),
                SourceKind::Favorites => format!("Favorites {}", self.id),
                _ => self.id.clone(),
            },
            Platform::Twitch => self.id.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Enter a supported YouTube, Bilibili or Twitch link")]
    Unsupported,
    #[error("The link is missing a video, channel or playlist identifier")]
    MissingIdentifier,
    #[error("Bilibili bangumi links are not supported yet")]
    Bangumi,
}

/// Parse a pasted link (with or without a scheme) into a canonical source.
pub fn parse_source(input: &str) -> Result<SourceRef, ParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ParseError::MissingIdentifier);
    }
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    let url = Url::parse(&with_scheme).map_err(|_| ParseError::Unsupported)?;
    let host = url
        .host_str()
        .ok_or(ParseError::Unsupported)?
        .trim_start_matches("www.")
        .trim_start_matches("m.")
        .to_ascii_lowercase();

    if host == "youtu.be" {
        let id = first_segment(&url).ok_or(ParseError::MissingIdentifier)?;
        return Ok(video(
            Platform::YouTube,
            id.to_string(),
            format!("https://www.youtube.com/watch?v={id}"),
        ));
    }

    if host == "youtube.com" || host == "music.youtube.com" {
        return parse_youtube(&url);
    }

    if host == "b23.tv" {
        return Ok(SourceRef {
            platform: Platform::Bilibili,
            kind: SourceKind::Redirect,
            id: first_segment(&url)
                .ok_or(ParseError::MissingIdentifier)?
                .to_string(),
            canonical_url: url.to_string(),
            page: None,
        });
    }

    if host == "bilibili.com" {
        return parse_bilibili(&url);
    }

    if host == "space.bilibili.com" {
        return parse_bilibili_space(&url);
    }

    if host == "clips.twitch.tv" {
        let id = first_segment(&url).ok_or(ParseError::MissingIdentifier)?;
        return Ok(video(
            Platform::Twitch,
            format!("clip:{id}"),
            format!("https://clips.twitch.tv/{id}"),
        ));
    }

    if host == "twitch.tv" {
        return parse_twitch(&url);
    }

    Err(ParseError::Unsupported)
}

fn first_segment(url: &Url) -> Option<&str> {
    url.path_segments()?.find(|segment| !segment.is_empty())
}

fn video(platform: Platform, id: String, canonical_url: String) -> SourceRef {
    SourceRef {
        platform,
        kind: SourceKind::Video,
        id,
        canonical_url,
        page: None,
    }
}

fn parse_youtube(url: &Url) -> Result<SourceRef, ParseError> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let first = segments.first().copied().unwrap_or("");

    match first {
        "watch" => {
            let id = query(url, "v").ok_or(ParseError::MissingIdentifier)?;
            Ok(video(
                Platform::YouTube,
                id.clone(),
                format!("https://www.youtube.com/watch?v={id}"),
            ))
        }
        "playlist" => {
            let id = query(url, "list").ok_or(ParseError::MissingIdentifier)?;
            Ok(SourceRef {
                platform: Platform::YouTube,
                kind: SourceKind::Playlist,
                id: id.clone(),
                canonical_url: format!("https://www.youtube.com/playlist?list={id}"),
                page: None,
            })
        }
        "shorts" | "live" | "embed" => {
            let id = segments.get(1).ok_or(ParseError::MissingIdentifier)?;
            Ok(video(
                Platform::YouTube,
                id.to_string(),
                format!("https://www.youtube.com/watch?v={id}"),
            ))
        }
        "channel" | "c" | "user" => {
            let id = segments.get(1).ok_or(ParseError::MissingIdentifier)?;
            let canonical = if first == "channel" {
                format!("https://www.youtube.com/channel/{id}")
            } else {
                format!("https://www.youtube.com/{first}/{id}")
            };
            Ok(SourceRef {
                platform: Platform::YouTube,
                kind: SourceKind::Channel,
                id: id.to_string(),
                canonical_url: canonical,
                page: None,
            })
        }
        handle if handle.starts_with('@') => Ok(SourceRef {
            platform: Platform::YouTube,
            kind: SourceKind::Channel,
            id: handle.to_string(),
            canonical_url: format!("https://www.youtube.com/{handle}"),
            page: None,
        }),
        _ => Err(ParseError::Unsupported),
    }
}

fn parse_bilibili(url: &Url) -> Result<SourceRef, ParseError> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let first = segments.first().copied().unwrap_or("");

    match first {
        "video" => {
            let raw = segments.get(1).ok_or(ParseError::MissingIdentifier)?;
            let (id, is_bv) =
                normalize_bilibili_video_id(raw).ok_or(ParseError::MissingIdentifier)?;
            let page = query(url, "p").and_then(|value| value.parse::<u32>().ok());
            let canonical = if is_bv {
                format!("https://www.bilibili.com/video/{id}")
            } else {
                format!("https://www.bilibili.com/video/av{id}")
            };
            Ok(SourceRef {
                platform: Platform::Bilibili,
                kind: SourceKind::Video,
                id,
                canonical_url: canonical,
                page,
            })
        }
        "medialist" | "list" => {
            let id = segments
                .iter()
                .find(|segment| segment.chars().all(|c| c.is_ascii_digit()))
                .ok_or(ParseError::MissingIdentifier)?;
            Ok(SourceRef {
                platform: Platform::Bilibili,
                kind: if first == "medialist" {
                    SourceKind::Playlist
                } else {
                    SourceKind::Favorites
                },
                id: id.to_string(),
                canonical_url: url.to_string(),
                page: None,
            })
        }
        "bangumi" => Err(ParseError::Bangumi),
        _ => Err(ParseError::Unsupported),
    }
}

fn parse_bilibili_space(url: &Url) -> Result<SourceRef, ParseError> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let mid = segments
        .first()
        .copied()
        .ok_or(ParseError::MissingIdentifier)?;
    if !mid.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseError::MissingIdentifier);
    }
    if segments.contains(&"favlist") {
        let fid = query(url, "fid").ok_or(ParseError::MissingIdentifier)?;
        return Ok(SourceRef {
            platform: Platform::Bilibili,
            kind: SourceKind::Favorites,
            id: fid.clone(),
            canonical_url: format!("https://space.bilibili.com/{mid}/favlist?fid={fid}"),
            page: None,
        });
    }
    if let Some(position) = segments.iter().position(|segment| *segment == "lists")
        && let Some(id) = segments.get(position + 1)
    {
        return Ok(SourceRef {
            platform: Platform::Bilibili,
            kind: SourceKind::Playlist,
            id: id.to_string(),
            canonical_url: url.to_string(),
            page: None,
        });
    }
    Ok(SourceRef {
        platform: Platform::Bilibili,
        kind: SourceKind::Channel,
        id: mid.to_string(),
        canonical_url: format!("https://space.bilibili.com/{mid}/video"),
        page: None,
    })
}

fn normalize_bilibili_video_id(raw: &str) -> Option<(String, bool)> {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("BV") {
        let id = format!("BV{rest}");
        return id
            .chars()
            .all(|c| c.is_ascii_alphanumeric())
            .then_some((id, true));
    }
    if let Some(rest) = raw.strip_prefix("av") {
        return rest
            .chars()
            .all(|c| c.is_ascii_digit())
            .then(|| (rest.to_string(), false));
    }
    None
}

fn parse_twitch(url: &Url) -> Result<SourceRef, ParseError> {
    let segments: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let first = segments.first().copied().unwrap_or("");
    match first {
        "videos" => {
            let id = segments.get(1).ok_or(ParseError::MissingIdentifier)?;
            Ok(video(
                Platform::Twitch,
                id.to_string(),
                format!("https://www.twitch.tv/videos/{id}"),
            ))
        }
        "clip" => {
            let id = segments.get(1).ok_or(ParseError::MissingIdentifier)?;
            Ok(video(
                Platform::Twitch,
                format!("clip:{id}"),
                format!("https://clips.twitch.tv/{id}"),
            ))
        }
        "directory" | "search" | "settings" | "subscriptions" | "inventory" => {
            Err(ParseError::Unsupported)
        }
        login => Ok(SourceRef {
            platform: Platform::Twitch,
            kind: SourceKind::Channel,
            id: login.to_ascii_lowercase(),
            canonical_url: format!("https://www.twitch.tv/{login}/videos"),
            page: None,
        }),
    }
}

fn query(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name.as_ref() == key)
        .map(|(_, value)| value.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_youtube_video_forms() {
        for input in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "youtube.com/watch?v=dQw4w9WgXcQ&t=10",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
        ] {
            let source = parse_source(input).unwrap();
            assert_eq!(source.platform, Platform::YouTube);
            assert_eq!(source.kind, SourceKind::Video);
            assert_eq!(source.id, "dQw4w9WgXcQ");
            assert_eq!(
                source.canonical_url,
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
            );
        }
    }

    #[test]
    fn parses_youtube_channel_and_playlist() {
        let channel = parse_source("https://www.youtube.com/@MKBHD").unwrap();
        assert_eq!(channel.kind, SourceKind::Channel);
        assert_eq!(channel.id, "@MKBHD");

        let playlist = parse_source("https://www.youtube.com/playlist?list=PL123").unwrap();
        assert_eq!(playlist.kind, SourceKind::Playlist);
        assert_eq!(playlist.id, "PL123");
    }

    #[test]
    fn parses_bilibili_links() {
        let video = parse_source("https://www.bilibili.com/video/BV1xx411c7mD?p=2").unwrap();
        assert_eq!(video.platform, Platform::Bilibili);
        assert_eq!(video.kind, SourceKind::Video);
        assert_eq!(video.id, "BV1xx411c7mD");
        assert_eq!(video.page, Some(2));

        let short = parse_source("https://b23.tv/abc123").unwrap();
        assert_eq!(short.kind, SourceKind::Redirect);

        let channel = parse_source("https://space.bilibili.com/2267573").unwrap();
        assert_eq!(channel.kind, SourceKind::Channel);
        assert_eq!(channel.id, "2267573");
    }

    #[test]
    fn parses_twitch_links() {
        let video = parse_source("https://www.twitch.tv/videos/123456").unwrap();
        assert_eq!(video.kind, SourceKind::Video);
        assert_eq!(video.id, "123456");

        let clip = parse_source("https://clips.twitch.tv/SomeSlug").unwrap();
        assert_eq!(clip.kind, SourceKind::Video);
        assert_eq!(clip.id, "clip:SomeSlug");

        let channel = parse_source("https://www.twitch.tv/someone").unwrap();
        assert_eq!(channel.kind, SourceKind::Channel);
        assert_eq!(channel.id, "someone");
    }

    #[test]
    fn rejects_unknown_links() {
        assert!(parse_source("https://example.com/video").is_err());
        assert!(parse_source(" ").is_err());
    }
}
