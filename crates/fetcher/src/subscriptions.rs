//! Subscription feed fetching for channels, playlists and favorites.

use std::sync::atomic::AtomicBool;

use videleum_core::{Platform, Settings, SourceKind, Subscription};

use crate::bilibili::BilibiliClient;
use crate::ytdlp::YtDlp;
use crate::{FetchError, Result};

#[derive(Clone, Debug, Default)]
pub struct FeedEntry {
    pub platform: Platform,
    pub source_id: String,
    pub url: String,
    pub title: String,
    pub creator: String,
    pub duration_secs: Option<u32>,
    pub thumbnail: Option<String>,
    pub published_at: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct FeedPage {
    pub title: String,
    pub avatar_url: Option<String>,
    pub entries: Vec<FeedEntry>,
}

/// Fetch the newest items for a subscription. The caller stores the checked
/// timestamp and decides which entries are new.
pub fn fetch_feed(
    settings: &Settings,
    subscription: &Subscription,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<FeedPage> {
    match subscription.platform {
        Platform::Bilibili => {
            let client = BilibiliClient::new(settings.cookies_for(Platform::Bilibili));
            client.fetch_feed(subscription, limit)
        }
        Platform::YouTube | Platform::Twitch => {
            let tool = YtDlp::discover(settings)?;
            let url = feed_url(subscription);
            let feed = tool.fetch_feed(
                &url,
                limit,
                settings.cookies_for(subscription.platform),
                cancel,
            )?;
            Ok(FeedPage {
                title: if feed.title.trim().is_empty() {
                    subscription.title.clone()
                } else {
                    feed.title
                },
                avatar_url: feed.avatar_url,
                entries: feed
                    .entries
                    .into_iter()
                    .map(|entry| FeedEntry {
                        platform: subscription.platform,
                        source_id: entry.id,
                        url: entry.url,
                        title: entry.title,
                        creator: subscription.title.clone(),
                        duration_secs: entry.duration,
                        thumbnail: entry.thumbnail,
                        published_at: None,
                    })
                    .collect(),
            })
        }
    }
}

fn feed_url(subscription: &Subscription) -> String {
    let mut url = subscription.source_url.clone();
    if subscription.platform == Platform::YouTube && subscription.kind == SourceKind::Channel {
        // Channel roots are ambiguous; ask yt-dlp for the uploads tab.
        let trimmed = url.trim_end_matches('/').to_string();
        if !trimmed.ends_with("/videos")
            && !trimmed.ends_with("/shorts")
            && !trimmed.ends_with("/streams")
        {
            url = format!("{trimmed}/videos");
        }
    }
    if url.trim().is_empty() {
        url = match subscription.platform {
            Platform::Twitch => {
                format!("https://www.twitch.tv/{}/videos", subscription.source_id)
            }
            _ => format!("https://www.youtube.com/channel/{}", subscription.source_id),
        };
    }
    url
}

/// Reject links that can't be followed as a feed.
pub fn validate_subscription(subscription: &Subscription) -> Result<()> {
    if !matches!(
        subscription.kind,
        SourceKind::Channel | SourceKind::Playlist | SourceKind::Favorites
    ) {
        return Err(FetchError::Unsupported(
            "Only channels, playlists and favorites can be followed".to_string(),
        ));
    }
    if subscription.source_id.trim().is_empty() {
        return Err(FetchError::Unsupported(
            "This link is missing a channel or playlist identifier".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(platform: Platform, kind: SourceKind, url: &str) -> Subscription {
        Subscription {
            platform,
            kind,
            source_id: "id".into(),
            source_url: url.into(),
            ..Default::default()
        }
    }

    #[test]
    fn asks_youtube_for_the_uploads_tab() {
        let item = subscription(
            Platform::YouTube,
            SourceKind::Channel,
            "https://www.youtube.com/@MKBHD",
        );
        assert_eq!(feed_url(&item), "https://www.youtube.com/@MKBHD/videos");

        let already = subscription(
            Platform::YouTube,
            SourceKind::Channel,
            "https://www.youtube.com/@MKBHD/videos",
        );
        assert_eq!(feed_url(&already), "https://www.youtube.com/@MKBHD/videos");
    }

    #[test]
    fn leaves_playlists_alone() {
        let item = subscription(
            Platform::YouTube,
            SourceKind::Playlist,
            "https://www.youtube.com/playlist?list=PL1",
        );
        assert_eq!(feed_url(&item), "https://www.youtube.com/playlist?list=PL1");
    }

    #[test]
    fn validates_followable_kinds() {
        let video = subscription(
            Platform::YouTube,
            SourceKind::Video,
            "https://www.youtube.com/watch?v=x",
        );
        assert!(validate_subscription(&video).is_err());
        let channel = subscription(
            Platform::Twitch,
            SourceKind::Channel,
            "https://www.twitch.tv/someone",
        );
        assert!(validate_subscription(&channel).is_ok());
    }
}
