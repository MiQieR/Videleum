//! SQLite catalogue stored inside the library folder.
//!
//! Keeping the database next to the media makes the whole archive portable.
//! All calls are synchronous and cheap; the fetchers run them from their own
//! worker threads, while the UI batches reloads after coherent events.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

use crate::archive::{ArchiveLayout, discover_nfos, read_nfo};
use crate::model::{
    Comment, DownloadJob, DownloadState, LibraryStats, MediaItem, MediaStatus, Subscription,
    now_timestamp,
};
use crate::platform::{Platform, SourceKind};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Couldn't open the library catalogue: {0}")]
    Open(String),
    #[error("Couldn't read the library catalogue: {0}")]
    Query(String),
    #[error("Couldn't save to the library catalogue: {0}")]
    Write(String),
}

/// The catalogue handle. Cloning it shares the same connection.
#[derive(Clone)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
    library_root: PathBuf,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScanSummary {
    pub imported: usize,
    pub updated: usize,
    pub skipped: usize,
}

impl Store {
    pub fn open(library_root: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let library_root = library_root.into();
        let data_dir = library_root.join(".videleum");
        std::fs::create_dir_all(&data_dir).map_err(|error| StoreError::Open(error.to_string()))?;
        let database_path = data_dir.join("videleum.db");
        let connection = Connection::open(&database_path)
            .map_err(|error| StoreError::Open(error.to_string()))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| StoreError::Open(error.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| StoreError::Open(error.to_string()))?;
        let store = Self {
            connection: Arc::new(Mutex::new(connection)),
            library_root,
        };
        store.migrate()?;
        store.recover_interrupted_downloads()?;
        Ok(store)
    }

    pub fn library_root(&self) -> &Path {
        &self.library_root
    }

    pub fn database_path(&self) -> PathBuf {
        self.library_root.join(".videleum").join("videleum.db")
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS media (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    platform TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    source_url TEXT NOT NULL DEFAULT '',
                    title TEXT NOT NULL,
                    creator TEXT NOT NULL DEFAULT '',
                    creator_id TEXT,
                    description TEXT NOT NULL DEFAULT '',
                    published_at INTEGER,
                    duration_secs INTEGER,
                    tags_json TEXT NOT NULL DEFAULT '[]',
                    view_count INTEGER,
                    like_count INTEGER,
                    comment_count INTEGER,
                    comments_json TEXT NOT NULL DEFAULT '[]',
                    cover_path TEXT,
                    avatar_path TEXT,
                    video_path TEXT,
                    nfo_path TEXT,
                    subtitle_paths_json TEXT NOT NULL DEFAULT '[]',
                    danmaku_path TEXT,
                    folder_path TEXT NOT NULL DEFAULT '',
                    file_size INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'ready',
                    watched INTEGER NOT NULL DEFAULT 0,
                    favorite INTEGER NOT NULL DEFAULT 0,
                    added_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    UNIQUE(platform, source_id)
                );
                CREATE INDEX IF NOT EXISTS media_published ON media(published_at DESC);
                CREATE INDEX IF NOT EXISTS media_added ON media(added_at DESC);
                CREATE INDEX IF NOT EXISTS media_creator ON media(creator);

                CREATE TABLE IF NOT EXISTS subscriptions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    platform TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    source_url TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    avatar_path TEXT,
                    auto_download INTEGER NOT NULL DEFAULT 1,
                    refresh_interval_minutes INTEGER NOT NULL DEFAULT 60,
                    last_checked_at INTEGER,
                    last_status TEXT NOT NULL DEFAULT '',
                    created_at INTEGER NOT NULL,
                    UNIQUE(platform, source_id)
                );

                CREATE TABLE IF NOT EXISTS downloads (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    media_id INTEGER,
                    source_url TEXT NOT NULL,
                    platform TEXT NOT NULL,
                    source_id TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    creator TEXT NOT NULL DEFAULT '',
                    cover_path TEXT,
                    state TEXT NOT NULL DEFAULT 'queued',
                    progress REAL NOT NULL DEFAULT 0,
                    downloaded_bytes INTEGER NOT NULL DEFAULT 0,
                    total_bytes INTEGER,
                    speed_bps INTEGER,
                    eta_secs INTEGER,
                    stage TEXT NOT NULL DEFAULT '',
                    error TEXT,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS downloads_state ON downloads(state, created_at DESC);
                PRAGMA user_version = 1;
                "#,
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    fn recover_interrupted_downloads(&self) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE downloads SET state = 'failed', error = 'Interrupted when Videleum closed', updated_at = ?1
                 WHERE state IN ('queued', 'fetching', 'downloading', 'processing')",
                params![now_timestamp()],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn upsert_media(&self, item: &MediaItem) -> Result<i64, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        let now = now_timestamp();
        let added_at = if item.added_at == 0 {
            now
        } else {
            item.added_at
        };
        let tags = serde_json::to_string(&item.tags).unwrap_or_else(|_| "[]".into());
        let comments = serde_json::to_string(&item.comments).unwrap_or_else(|_| "[]".into());
        let subtitles = serde_json::to_string(&item.subtitle_paths).unwrap_or_else(|_| "[]".into());
        let id: i64 = connection
            .query_row(
                r#"
                INSERT INTO media (
                    platform, source_id, source_url, title, creator, creator_id, description,
                    published_at, duration_secs, tags_json, view_count, like_count, comment_count,
                    comments_json, cover_path, avatar_path, video_path, nfo_path,
                    subtitle_paths_json, danmaku_path, folder_path, file_size, status, watched,
                    favorite, added_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                          ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
                ON CONFLICT(platform, source_id) DO UPDATE SET
                    source_url = excluded.source_url,
                    title = excluded.title,
                    creator = excluded.creator,
                    creator_id = excluded.creator_id,
                    description = excluded.description,
                    published_at = excluded.published_at,
                    duration_secs = excluded.duration_secs,
                    tags_json = excluded.tags_json,
                    view_count = excluded.view_count,
                    like_count = excluded.like_count,
                    comment_count = excluded.comment_count,
                    comments_json = excluded.comments_json,
                    cover_path = COALESCE(excluded.cover_path, media.cover_path),
                    avatar_path = COALESCE(excluded.avatar_path, media.avatar_path),
                    video_path = COALESCE(excluded.video_path, media.video_path),
                    nfo_path = COALESCE(excluded.nfo_path, media.nfo_path),
                    subtitle_paths_json = excluded.subtitle_paths_json,
                    danmaku_path = COALESCE(excluded.danmaku_path, media.danmaku_path),
                    folder_path = excluded.folder_path,
                    file_size = excluded.file_size,
                    status = excluded.status,
                    watched = media.watched,
                    favorite = media.favorite,
                    updated_at = excluded.updated_at
                RETURNING id
                "#,
                params![
                    item.platform.as_str(),
                    item.source_id,
                    item.source_url,
                    item.title,
                    item.creator,
                    item.creator_id,
                    item.description,
                    item.published_at,
                    item.duration_secs,
                    tags,
                    item.view_count,
                    item.like_count,
                    item.comment_count,
                    comments,
                    path_to_string(item.cover_path.as_deref()),
                    path_to_string(item.avatar_path.as_deref()),
                    path_to_string(item.video_path.as_deref()),
                    path_to_string(item.nfo_path.as_deref()),
                    subtitles,
                    path_to_string(item.danmaku_path.as_deref()),
                    item.folder_path.to_string_lossy(),
                    item.file_size,
                    item.status.as_str(),
                    item.watched,
                    item.favorite,
                    added_at,
                    now,
                ],
                |row| row.get(0),
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(id)
    }

    pub fn list_media(&self) -> Result<Vec<MediaItem>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        let mut statement = connection
            .prepare("SELECT * FROM media ORDER BY COALESCE(published_at, added_at) DESC, id DESC")
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let rows = statement
            .query_map([], media_from_row)
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|error| StoreError::Query(error.to_string()))?);
        }
        Ok(items)
    }

    pub fn get_media(&self, id: i64) -> Result<Option<MediaItem>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        connection
            .query_row(
                "SELECT * FROM media WHERE id = ?1",
                params![id],
                media_from_row,
            )
            .optional()
            .map_err(|error| StoreError::Query(error.to_string()))
    }

    pub fn find_media(
        &self,
        platform: Platform,
        source_id: &str,
    ) -> Result<Option<MediaItem>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        connection
            .query_row(
                "SELECT * FROM media WHERE platform = ?1 AND source_id = ?2",
                params![platform.as_str(), source_id],
                media_from_row,
            )
            .optional()
            .map_err(|error| StoreError::Query(error.to_string()))
    }

    pub fn set_media_status(&self, id: i64, status: MediaStatus) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE media SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now_timestamp(), id],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn set_watched(&self, id: i64, watched: bool) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE media SET watched = ?1, updated_at = ?2 WHERE id = ?3",
                params![watched, now_timestamp(), id],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE media SET favorite = ?1, updated_at = ?2 WHERE id = ?3",
                params![favorite, now_timestamp(), id],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn delete_media(&self, id: i64) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute("DELETE FROM media WHERE id = ?1", params![id])
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn stats(&self) -> Result<LibraryStats, StoreError> {
        let items = self.list_media()?;
        let mut stats = LibraryStats {
            item_count: items.len(),
            ..Default::default()
        };
        for item in items {
            stats.total_bytes += item.file_size;
            if item.status == MediaStatus::Missing {
                stats.missing_count += 1;
            }
            let index = match item.platform {
                Platform::YouTube => 0,
                Platform::Bilibili => 1,
                Platform::Twitch => 2,
            };
            stats.platform_counts[index] += 1;
        }
        Ok(stats)
    }

    /// Import all Videleum NFO files in the library. Existing records keep
    /// watched/favorite state; files present on disk win over stale paths.
    pub fn scan_archive(&self, _layout: &ArchiveLayout) -> Result<ScanSummary, StoreError> {
        let mut summary = ScanSummary::default();
        for nfo in discover_nfos(&self.library_root) {
            let mut item = match read_nfo(&nfo) {
                Ok(item) => item,
                Err(_) => {
                    summary.skipped += 1;
                    continue;
                }
            };
            match self.find_media(item.platform, &item.source_id) {
                Ok(Some(existing)) => {
                    item.id = existing.id;
                    item.watched = existing.watched;
                    item.favorite = existing.favorite;
                    item.added_at = existing.added_at;
                    self.upsert_media(&item)?;
                    summary.updated += 1;
                }
                Ok(None) => {
                    self.upsert_media(&item)?;
                    summary.imported += 1;
                }
                Err(_) => summary.skipped += 1,
            }
        }
        self.refresh_missing()?;
        Ok(summary)
    }

    pub fn refresh_missing(&self) -> Result<(), StoreError> {
        let items = self.list_media()?;
        for mut item in items {
            let missing = item.video_path.as_ref().is_none_or(|path| !path.exists());
            let desired = if missing {
                MediaStatus::Missing
            } else {
                MediaStatus::Ready
            };
            if item.status != desired {
                item.status = desired;
                self.set_media_status(item.id, desired)?;
            }
            if !missing
                && let Some(video) = &item.video_path
                && let Ok(metadata) = std::fs::metadata(video)
                && metadata.len() != item.file_size
            {
                item.file_size = metadata.len();
                self.upsert_media(&item)?;
            }
        }
        Ok(())
    }

    // ---- Subscriptions -------------------------------------------------

    pub fn upsert_subscription(&self, subscription: &Subscription) -> Result<i64, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        let created_at = if subscription.created_at == 0 {
            now_timestamp()
        } else {
            subscription.created_at
        };
        let id: i64 = connection
            .query_row(
                r#"
                INSERT INTO subscriptions (
                    platform, kind, source_id, source_url, title, avatar_path,
                    auto_download, refresh_interval_minutes, last_checked_at,
                    last_status, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(platform, source_id) DO UPDATE SET
                    kind = excluded.kind,
                    source_url = excluded.source_url,
                    title = excluded.title,
                    avatar_path = COALESCE(excluded.avatar_path, subscriptions.avatar_path),
                    auto_download = excluded.auto_download,
                    refresh_interval_minutes = excluded.refresh_interval_minutes,
                    last_checked_at = excluded.last_checked_at,
                    last_status = excluded.last_status
                RETURNING id
                "#,
                params![
                    subscription.platform.as_str(),
                    subscription.kind.as_str(),
                    subscription.source_id,
                    subscription.source_url,
                    subscription.title,
                    path_to_string(subscription.avatar_path.as_deref()),
                    subscription.auto_download,
                    subscription.refresh_interval_minutes,
                    subscription.last_checked_at,
                    subscription.last_status,
                    created_at,
                ],
                |row| row.get(0),
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(id)
    }

    pub fn list_subscriptions(&self) -> Result<Vec<Subscription>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        let mut statement = connection
            .prepare("SELECT * FROM subscriptions ORDER BY created_at ASC")
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let rows = statement
            .query_map([], subscription_from_row)
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let mut subscriptions = Vec::new();
        for row in rows {
            subscriptions.push(row.map_err(|error| StoreError::Query(error.to_string()))?);
        }
        Ok(subscriptions)
    }

    pub fn find_subscription(
        &self,
        platform: Platform,
        source_id: &str,
    ) -> Result<Option<Subscription>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        connection
            .query_row(
                "SELECT * FROM subscriptions WHERE platform = ?1 AND source_id = ?2",
                params![platform.as_str(), source_id],
                subscription_from_row,
            )
            .optional()
            .map_err(|error| StoreError::Query(error.to_string()))
    }

    pub fn set_subscription_checked(
        &self,
        id: i64,
        status: &str,
        checked_at: i64,
    ) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE subscriptions SET last_checked_at = ?1, last_status = ?2 WHERE id = ?3",
                params![checked_at, status, id],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn set_subscription_auto_download(&self, id: i64, enabled: bool) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "UPDATE subscriptions SET auto_download = ?1 WHERE id = ?2",
                params![enabled, id],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn delete_subscription(&self, id: i64) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute("DELETE FROM subscriptions WHERE id = ?1", params![id])
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    // ---- Downloads -----------------------------------------------------

    pub fn save_job(&self, job: &DownloadJob) -> Result<i64, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        let now = now_timestamp();
        if job.id > 0 {
            connection
                .execute(
                    r#"
                    UPDATE downloads SET
                        media_id = ?1, source_url = ?2, platform = ?3, source_id = ?4,
                        title = ?5, creator = ?6, cover_path = ?7, state = ?8,
                        progress = ?9, downloaded_bytes = ?10, total_bytes = ?11,
                        speed_bps = ?12, eta_secs = ?13, stage = ?14, error = ?15,
                        updated_at = ?16
                    WHERE id = ?17
                    "#,
                    params![
                        job.media_id,
                        job.source_url,
                        job.platform.as_str(),
                        job.source_id,
                        job.title,
                        job.creator,
                        path_to_string(job.cover_path.as_deref()),
                        job.state.as_str(),
                        job.progress,
                        job.downloaded_bytes,
                        job.total_bytes,
                        job.speed_bps,
                        job.eta_secs,
                        job.stage,
                        job.error,
                        now,
                        job.id,
                    ],
                )
                .map_err(|error| StoreError::Write(error.to_string()))?;
            Ok(job.id)
        } else {
            let created_at = if job.created_at == 0 {
                now
            } else {
                job.created_at
            };
            let id: i64 = connection
                .query_row(
                    r#"
                    INSERT INTO downloads (
                        media_id, source_url, platform, source_id, title, creator, cover_path,
                        state, progress, downloaded_bytes, total_bytes, speed_bps, eta_secs,
                        stage, error, created_at, updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                    RETURNING id
                    "#,
                    params![
                        job.media_id,
                        job.source_url,
                        job.platform.as_str(),
                        job.source_id,
                        job.title,
                        job.creator,
                        path_to_string(job.cover_path.as_deref()),
                        job.state.as_str(),
                        job.progress,
                        job.downloaded_bytes,
                        job.total_bytes,
                        job.speed_bps,
                        job.eta_secs,
                        job.stage,
                        job.error,
                        created_at,
                        now,
                    ],
                    |row| row.get(0),
                )
                .map_err(|error| StoreError::Write(error.to_string()))?;
            Ok(id)
        }
    }

    pub fn list_jobs(&self, limit: usize) -> Result<Vec<DownloadJob>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Query("database lock poisoned".into()))?;
        let mut statement = connection
            .prepare("SELECT * FROM downloads ORDER BY created_at DESC LIMIT ?1")
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let rows = statement
            .query_map(params![limit as i64], job_from_row)
            .map_err(|error| StoreError::Query(error.to_string()))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(row.map_err(|error| StoreError::Query(error.to_string()))?);
        }
        Ok(jobs)
    }

    pub fn delete_job(&self, id: i64) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute("DELETE FROM downloads WHERE id = ?1", params![id])
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }

    pub fn clear_finished_jobs(&self) -> Result<(), StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::Write("database lock poisoned".into()))?;
        connection
            .execute(
                "DELETE FROM downloads WHERE state IN ('completed', 'failed', 'canceled')",
                [],
            )
            .map_err(|error| StoreError::Write(error.to_string()))?;
        Ok(())
    }
}

fn path_to_string(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.to_string_lossy().into_owned())
}

fn string_to_path(value: Option<String>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

fn media_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaItem> {
    let platform: String = row.get("platform")?;
    let tags: String = row.get("tags_json")?;
    let comments: String = row.get("comments_json")?;
    let subtitles: String = row.get("subtitle_paths_json")?;
    let status: String = row.get("status")?;
    Ok(MediaItem {
        id: row.get("id")?,
        platform: Platform::from_str(&platform).unwrap_or_default(),
        source_id: row.get("source_id")?,
        source_url: row.get("source_url")?,
        title: row.get("title")?,
        creator: row.get("creator")?,
        creator_id: row.get("creator_id")?,
        description: row.get("description")?,
        published_at: row.get("published_at")?,
        duration_secs: row.get("duration_secs")?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        view_count: row.get("view_count")?,
        like_count: row.get("like_count")?,
        comment_count: row.get("comment_count")?,
        comments: serde_json::from_str::<Vec<Comment>>(&comments).unwrap_or_default(),
        cover_path: string_to_path(row.get("cover_path")?),
        avatar_path: string_to_path(row.get("avatar_path")?),
        video_path: string_to_path(row.get("video_path")?),
        nfo_path: string_to_path(row.get("nfo_path")?),
        subtitle_paths: serde_json::from_str::<Vec<PathBuf>>(&subtitles).unwrap_or_default(),
        danmaku_path: string_to_path(row.get("danmaku_path")?),
        folder_path: PathBuf::from(row.get::<_, String>("folder_path")?),
        file_size: row.get::<_, i64>("file_size")?.max(0) as u64,
        status: MediaStatus::from_str(&status),
        watched: row.get("watched")?,
        favorite: row.get("favorite")?,
        added_at: row.get("added_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn subscription_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Subscription> {
    let platform: String = row.get("platform")?;
    let kind: String = row.get("kind")?;
    Ok(Subscription {
        id: row.get("id")?,
        platform: Platform::from_str(&platform).unwrap_or_default(),
        kind: match kind.as_str() {
            "channel" => SourceKind::Channel,
            "playlist" => SourceKind::Playlist,
            "favorites" => SourceKind::Favorites,
            _ => SourceKind::Channel,
        },
        source_id: row.get("source_id")?,
        source_url: row.get("source_url")?,
        title: row.get("title")?,
        avatar_path: string_to_path(row.get("avatar_path")?),
        auto_download: row.get("auto_download")?,
        refresh_interval_minutes: row.get::<_, i64>("refresh_interval_minutes")?.max(15) as u32,
        last_checked_at: row.get("last_checked_at")?,
        last_status: row.get("last_status")?,
        created_at: row.get("created_at")?,
    })
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DownloadJob> {
    let platform: String = row.get("platform")?;
    let state: String = row.get("state")?;
    Ok(DownloadJob {
        id: row.get("id")?,
        media_id: row.get("media_id")?,
        source_url: row.get("source_url")?,
        platform: Platform::from_str(&platform).unwrap_or_default(),
        source_id: row.get("source_id")?,
        title: row.get("title")?,
        creator: row.get("creator")?,
        cover_path: string_to_path(row.get("cover_path")?),
        state: DownloadState::from_str(&state),
        progress: row.get::<_, f64>("progress")? as f32,
        downloaded_bytes: row.get::<_, i64>("downloaded_bytes")?.max(0) as u64,
        total_bytes: row
            .get::<_, Option<i64>>("total_bytes")?
            .map(|value| value.max(0) as u64),
        speed_bps: row
            .get::<_, Option<i64>>("speed_bps")?
            .map(|value| value.max(0) as u64),
        eta_secs: row
            .get::<_, Option<i64>>("eta_secs")?
            .map(|value| value.max(0) as u64),
        stage: row.get("stage")?,
        error: row.get("error")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MediaItem;

    fn temp_root(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("videleum-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn media_round_trip_and_single_identity() {
        let root = temp_root("media");
        let store = Store::open(&root).unwrap();
        let item = MediaItem {
            platform: Platform::YouTube,
            source_id: "abc123".into(),
            title: "First title".into(),
            creator: "Creator".into(),
            tags: vec!["one".into()],
            duration_secs: Some(60),
            added_at: 100,
            ..Default::default()
        };
        let id = store.upsert_media(&item).unwrap();
        assert!(id > 0);

        let mut update = item.clone();
        update.title = "Updated title".into();
        update.tags.push("two".into());
        let same_id = store.upsert_media(&update).unwrap();
        assert_eq!(id, same_id);

        let items = store.list_media().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Updated title");
        assert_eq!(items[0].tags, vec!["one", "two"]);
        assert_eq!(store.stats().unwrap().platform_counts[0], 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn downloads_are_persisted_and_recovered() {
        let root = temp_root("jobs");
        let store = Store::open(&root).unwrap();
        let job = DownloadJob {
            source_url: "https://youtu.be/x".into(),
            platform: Platform::YouTube,
            source_id: "x".into(),
            state: DownloadState::Downloading,
            progress: 0.4,
            ..Default::default()
        };
        let id = store.save_job(&job).unwrap();
        let jobs = store.list_jobs(10).unwrap();
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].state, DownloadState::Downloading);

        drop(store);
        let store = Store::open(&root).unwrap();
        let jobs = store.list_jobs(10).unwrap();
        assert_eq!(jobs[0].state, DownloadState::Failed);
        assert!(jobs[0].error.as_deref().unwrap().contains("Interrupted"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn subscriptions_are_unique_per_source() {
        let root = temp_root("subscriptions");
        let store = Store::open(&root).unwrap();
        let subscription = Subscription {
            platform: Platform::Bilibili,
            kind: SourceKind::Channel,
            source_id: "123".into(),
            source_url: "https://space.bilibili.com/123/video".into(),
            title: "UP".into(),
            auto_download: true,
            refresh_interval_minutes: 60,
            created_at: 1,
            ..Default::default()
        };
        let id = store.upsert_subscription(&subscription).unwrap();
        assert_eq!(store.upsert_subscription(&subscription).unwrap(), id);
        assert_eq!(store.list_subscriptions().unwrap().len(), 1);
        store.set_subscription_auto_download(id, false).unwrap();
        assert!(!store.list_subscriptions().unwrap()[0].auto_download);
        let _ = std::fs::remove_dir_all(&root);
    }
}
