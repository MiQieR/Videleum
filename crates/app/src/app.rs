//! The application entity: catalogue state, worker coordination and navigation.

use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogAction, DialogClose, DialogFooter},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    PathPromptOptions, Render, SharedString, Styled as _, Task, Window, div, rems,
};

use videleum_core::archive::ArchiveLayout;
use videleum_core::model::{DownloadState, LibraryStats, MediaItem, Subscription};
use videleum_core::{
    Platform, Settings, SourceKind, SourceRef, Store, ThemePreference, now_timestamp, parse_source,
};
use videleum_fetcher::download::{DownloadRequest, enqueue, run_job};
use videleum_fetcher::install::detect;
use videleum_fetcher::subscriptions::validate_subscription;
use videleum_fetcher::{JobEvent, ToolStatus, fetch_feed, install_ytdlp};

use crate::theme;
use crate::views;

const EVENT_TICK: Duration = Duration::from_millis(150);
const JOB_HISTORY_LIMIT: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Library,
    Downloads,
    Subscriptions,
    Settings,
}

impl View {
    pub fn label(self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Downloads => "Downloads",
            Self::Subscriptions => "Subscriptions",
            Self::Settings => "Settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Collection {
    All,
    Favorites,
    Unwatched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Recent,
    Published,
    Title,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddIntent {
    Auto,
    Video,
    Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathField {
    LibraryRoot,
    Ytdlp,
    Ffmpeg,
    YoutubeCookies,
    BilibiliCookies,
    TwitchCookies,
}

impl PathField {
    fn prompt(self) -> SharedString {
        match self {
            Self::LibraryRoot => "Choose a library folder",
            Self::Ytdlp => "Choose the yt-dlp executable",
            Self::Ffmpeg => "Choose the ffmpeg executable",
            Self::YoutubeCookies => "Choose a YouTube cookies file",
            Self::BilibiliCookies => "Choose a Bilibili cookies file",
            Self::TwitchCookies => "Choose a Twitch cookies file",
        }
        .into()
    }

    fn wants_directory(self) -> bool {
        self == Self::LibraryRoot
    }
}

/// Work completed on a background thread.
enum AppEvent {
    Job(JobEvent),
    Tools(Result<ToolStatus, String>),
    ToolProgress {
        downloaded: u64,
        total: Option<u64>,
    },
    ToolInstalled(Result<std::path::PathBuf, String>),
    SubscriptionRefreshed {
        id: i64,
        new_items: usize,
        queued: Vec<videleum_core::DownloadJob>,
        error: Option<String>,
    },
    Scanned(Result<String, String>),
}

struct RefreshOutcome {
    new_items: usize,
    queued: Vec<videleum_core::DownloadJob>,
}

pub struct VideleumApp {
    pub(crate) settings: Settings,
    pub(crate) store: Store,
    pub(crate) layout: ArchiveLayout,
    pub(crate) media: Vec<MediaItem>,
    pub(crate) jobs: Vec<videleum_core::DownloadJob>,
    pub(crate) subscriptions: Vec<Subscription>,
    pub(crate) stats: LibraryStats,
    pub(crate) view: View,
    pub(crate) selected: Option<i64>,

    pub(crate) query: String,
    pub(crate) search: Entity<InputState>,
    pub(crate) platform_filter: Option<Platform>,
    pub(crate) collection: Collection,
    pub(crate) sort: SortKey,

    pub(crate) library_error: Option<String>,
    pub(crate) startup_error: Option<String>,

    pub(crate) add_url: Entity<InputState>,
    pub(crate) naming: Entity<InputState>,
    pub(crate) add_intent: AddIntent,
    pub(crate) add_error: Option<String>,
    pub(crate) add_preview: Option<Result<SourceRef, String>>,
    pub(crate) add_existing: bool,

    pub(crate) tools: ToolStatus,
    pub(crate) tools_loading: bool,
    pub(crate) tool_install_progress: Option<(u64, Option<u64>)>,
    pub(crate) tool_install_error: Option<String>,

    pub(crate) checking_subscriptions: HashSet<i64>,
    pub(crate) subscription_errors: HashMap<i64, String>,
    pub(crate) refreshing_all: bool,

    pub(crate) pending_jobs: VecDeque<videleum_core::DownloadJob>,
    pub(crate) active_jobs: HashMap<i64, Arc<AtomicBool>>,

    events_tx: Sender<AppEvent>,
    events_rx: Receiver<AppEvent>,
    async_tasks: Vec<Task<()>>,
    _input_subscriptions: Vec<gpui_kit::Subscription>,
}

impl VideleumApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut settings = Settings::load().unwrap_or_default();
        settings.normalize();

        let mut startup_error = None;
        let store = match Store::open(&settings.library_root) {
            Ok(store) => store,
            Err(error) => {
                let fallback = videleum_core::app_data_dir().join("library");
                let _ = std::fs::create_dir_all(&fallback);
                let store = Store::open(&fallback).unwrap_or_else(|fallback_error| {
                    panic!(
                        "Videleum could not open a library at {} ({error}) or {} ({fallback_error})",
                        settings.library_root.display(),
                        fallback.display()
                    )
                });
                startup_error = Some(format!(
                    "Couldn't open the library at {}. Videleum switched to {}.",
                    settings.library_root.display(),
                    fallback.display()
                ));
                settings.library_root = fallback;
                store
            }
        };

        theme::apply(cx, settings.theme, window.appearance());

        let naming_value = settings.naming_template.clone();
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search titles, creators, and tags"));
        let add_url = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Paste a YouTube, Bilibili, or Twitch link")
        });
        let naming = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("{creator}/{title} [{id}]")
                .default_value(naming_value)
        });

        let mut input_subscriptions = Vec::new();
        input_subscriptions.push(
            cx.subscribe_in(&search, window, |this, state, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = state.read(cx).value().to_string();
                    cx.notify();
                }
            }),
        );
        input_subscriptions.push(
            cx.subscribe_in(&add_url, window, |this, state, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.update_add_preview(value, cx);
                }
            }),
        );
        input_subscriptions.push(
            cx.subscribe_in(&naming, window, |this, state, event, _, cx| match event {
                InputEvent::Change => {
                    this.settings.naming_template = state.read(cx).value().to_string();
                }
                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                    this.settings.normalize();
                    this.save_settings(cx);
                }
                InputEvent::Focus => {}
            }),
        );

        let (events_tx, events_rx) = mpsc::channel();
        let layout = ArchiveLayout::new(
            settings.library_root.clone(),
            settings.naming_template.clone(),
        );

        let mut app = Self {
            settings,
            store,
            layout,
            media: Vec::new(),
            jobs: Vec::new(),
            subscriptions: Vec::new(),
            stats: LibraryStats::default(),
            view: View::Library,
            selected: None,
            query: String::new(),
            search,
            platform_filter: None,
            collection: Collection::All,
            sort: SortKey::Recent,
            library_error: None,
            startup_error,
            add_url,
            naming,
            add_intent: AddIntent::Auto,
            add_error: None,
            add_preview: None,
            add_existing: false,
            tools: ToolStatus::default(),
            tools_loading: true,
            tool_install_progress: None,
            tool_install_error: None,
            checking_subscriptions: HashSet::new(),
            subscription_errors: HashMap::new(),
            refreshing_all: false,
            pending_jobs: VecDeque::new(),
            active_jobs: HashMap::new(),
            events_tx,
            events_rx,
            async_tasks: Vec::new(),
            _input_subscriptions: input_subscriptions,
        };

        app.reload_all(cx);
        app.start_event_ticker(window, cx);
        app.detect_tools(cx);
        app.maybe_auto_refresh(cx);
        app
    }

    // ---- Queries -------------------------------------------------------

    pub(crate) fn selected_media(&self) -> Option<&MediaItem> {
        let id = self.selected?;
        self.media.iter().find(|item| item.id == id)
    }

    pub(crate) fn filtered_media(&self) -> Vec<&MediaItem> {
        let query = self.query.trim().to_lowercase();
        let mut items: Vec<&MediaItem> = self
            .media
            .iter()
            .filter(|item| {
                if let Some(platform) = self.platform_filter
                    && item.platform != platform
                {
                    return false;
                }
                match self.collection {
                    Collection::All => {}
                    Collection::Favorites if !item.favorite => return false,
                    Collection::Unwatched if item.watched => return false,
                    _ => {}
                }
                if query.is_empty() {
                    return true;
                }
                item.title.to_lowercase().contains(&query)
                    || item.creator.to_lowercase().contains(&query)
                    || item
                        .tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&query))
            })
            .collect();

        match self.sort {
            SortKey::Recent => items.sort_by_key(|item| std::cmp::Reverse(item.added_at)),
            SortKey::Published => items
                .sort_by_key(|item| std::cmp::Reverse(item.published_at.unwrap_or(item.added_at))),
            SortKey::Title => {
                items.sort_by_key(|item| item.title.to_lowercase());
            }
            SortKey::Size => items.sort_by_key(|item| std::cmp::Reverse(item.file_size)),
        }
        items
    }

    pub(crate) fn active_job_count(&self) -> usize {
        self.jobs.iter().filter(|job| job.state.is_active()).count()
    }

    pub(crate) fn has_filters(&self) -> bool {
        !self.query.trim().is_empty()
            || self.platform_filter.is_some()
            || self.collection != Collection::All
    }

    // ---- Navigation ----------------------------------------------------

    pub(crate) fn navigate(&mut self, view: View, cx: &mut Context<Self>) {
        self.settings.normalize();
        self.save_settings(cx);
        self.view = view;
        self.selected = None;
        match view {
            View::Downloads => self.reload_jobs(),
            View::Subscriptions => self.reload_subscriptions(),
            View::Library | View::Settings => {}
        }
        cx.notify();
    }

    pub(crate) fn open_media(&mut self, id: i64, cx: &mut Context<Self>) {
        self.view = View::Library;
        self.selected = Some(id);
        cx.notify();
    }

    pub(crate) fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    pub(crate) fn set_platform_filter(
        &mut self,
        platform: Option<Platform>,
        cx: &mut Context<Self>,
    ) {
        self.platform_filter = platform;
        cx.notify();
    }

    pub(crate) fn set_collection(&mut self, collection: Collection, cx: &mut Context<Self>) {
        self.collection = collection;
        cx.notify();
    }

    pub(crate) fn set_sort(&mut self, sort: SortKey, cx: &mut Context<Self>) {
        self.sort = sort;
        cx.notify();
    }

    pub(crate) fn clear_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.platform_filter = None;
        self.collection = Collection::All;
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    // ---- Catalogue -----------------------------------------------------

    fn reload_all(&mut self, cx: &mut Context<Self>) {
        self.reload_library();
        self.reload_jobs();
        self.reload_subscriptions();
        cx.notify();
    }

    pub(crate) fn reload_library(&mut self) {
        match self.store.list_media() {
            Ok(media) => {
                self.media = media;
                self.library_error = None;
            }
            Err(error) => self.library_error = Some(error.to_string()),
        }
        match self.store.stats() {
            Ok(stats) => self.stats = stats,
            Err(_) => self.stats = LibraryStats::default(),
        }
        if let Some(id) = self.selected
            && !self.media.iter().any(|item| item.id == id)
        {
            self.selected = None;
        }
    }

    fn reload_jobs(&mut self) {
        if let Ok(jobs) = self.store.list_jobs(JOB_HISTORY_LIMIT) {
            self.jobs = jobs;
        }
    }

    fn reload_subscriptions(&mut self) {
        if let Ok(subscriptions) = self.store.list_subscriptions() {
            self.subscriptions = subscriptions;
        }
    }

    pub(crate) fn toggle_favorite(&mut self, id: i64, cx: &mut Context<Self>) {
        let value = self
            .media
            .iter()
            .find(|item| item.id == id)
            .map(|item| !item.favorite)
            .unwrap_or(true);
        if self.store.set_favorite(id, value).is_ok() {
            self.reload_library();
            cx.notify();
        }
    }

    pub(crate) fn toggle_watched(&mut self, id: i64, cx: &mut Context<Self>) {
        let value = self
            .media
            .iter()
            .find(|item| item.id == id)
            .map(|item| !item.watched)
            .unwrap_or(true);
        if self.store.set_watched(id, value).is_ok() {
            self.reload_library();
            cx.notify();
        }
    }

    pub(crate) fn delete_media(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.store.delete_media(id).is_ok() {
            if self.selected == Some(id) {
                self.selected = None;
            }
            self.reload_library();
            cx.notify();
        }
    }

    pub(crate) fn rescan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let layout = self.layout.clone();
        let tx = self.events_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("videleum-scan".into())
            .spawn(move || {
                let result = store
                    .scan_archive(&layout)
                    .map(|summary| {
                        format!(
                            "Imported {} and updated {} videos",
                            summary.imported, summary.updated
                        )
                    })
                    .map_err(|error| error.to_string());
                let _ = tx.send(AppEvent::Scanned(result));
            });
        if let Err(error) = spawned {
            window.push_notification(error.to_string(), cx);
        }
    }

    // ---- Settings ------------------------------------------------------

    pub(crate) fn save_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.settings.save() {
            let _ = error;
        }
        self.layout = ArchiveLayout::new(
            self.settings.library_root.clone(),
            self.settings.naming_template.clone(),
        );
        cx.notify();
    }

    pub(crate) fn update_settings(
        &mut self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut Settings),
    ) {
        update(&mut self.settings);
        self.settings.normalize();
        self.save_settings(cx);
    }

    pub(crate) fn set_theme(
        &mut self,
        preference: ThemePreference,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.theme = preference;
        self.save_settings(cx);
        theme::apply(cx, preference, window.appearance());
        cx.notify();
    }

    pub(crate) fn pick_path(
        &mut self,
        field: PathField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if field == PathField::LibraryRoot && !self.active_jobs.is_empty() {
            window.push_notification(
                "Wait for active downloads before changing the library folder",
                cx,
            );
            return;
        }
        let directories = field.wants_directory();
        let promise = cx.prompt_for_paths(PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some(field.prompt()),
        });
        let task = cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = promise.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |app, window, cx| {
                    app.apply_path(field, path, window, cx);
                });
            }
        });
        self.async_tasks.push(task);
    }

    fn apply_path(
        &mut self,
        field: PathField,
        path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match field {
            PathField::LibraryRoot => match Store::open(&path) {
                Ok(store) => {
                    self.store = store;
                    self.settings.library_root = path;
                    self.save_settings(cx);
                    self.selected = None;
                    self.reload_all(cx);
                    window.push_notification("Library folder changed", cx);
                }
                Err(error) => {
                    window.push_notification(
                        format!("Couldn't open that library folder: {error}"),
                        cx,
                    );
                }
            },
            PathField::Ytdlp => {
                self.settings.ytdlp_path = Some(path);
                self.save_settings(cx);
                self.detect_tools(cx);
            }
            PathField::Ffmpeg => {
                self.settings.ffmpeg_path = Some(path);
                self.save_settings(cx);
                self.detect_tools(cx);
            }
            PathField::YoutubeCookies => self.settings.youtube_cookies = Some(path),
            PathField::BilibiliCookies => self.settings.bilibili_cookies = Some(path),
            PathField::TwitchCookies => self.settings.twitch_cookies = Some(path),
        }
        if field != PathField::LibraryRoot {
            self.save_settings(cx);
        }
        cx.notify();
    }

    pub(crate) fn clear_path(&mut self, field: PathField, cx: &mut Context<Self>) {
        match field {
            PathField::LibraryRoot => {}
            PathField::Ytdlp => self.settings.ytdlp_path = None,
            PathField::Ffmpeg => self.settings.ffmpeg_path = None,
            PathField::YoutubeCookies => self.settings.youtube_cookies = None,
            PathField::BilibiliCookies => self.settings.bilibili_cookies = None,
            PathField::TwitchCookies => self.settings.twitch_cookies = None,
        }
        self.save_settings(cx);
        if matches!(field, PathField::Ytdlp | PathField::Ffmpeg) {
            self.detect_tools(cx);
        }
        cx.notify();
    }

    fn detect_tools(&mut self, cx: &mut Context<Self>) {
        self.tools_loading = true;
        let settings = self.settings.clone();
        let tx = self.events_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("videleum-tools".into())
            .spawn(move || {
                let status = std::panic::catch_unwind(|| detect(&settings))
                    .map_err(|_| "Couldn't inspect the helper tools".to_string());
                let _ = tx.send(AppEvent::Tools(status));
            });
        if spawned.is_err() {
            self.tools_loading = false;
        }
        cx.notify();
    }

    pub(crate) fn install_ytdlp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_install_progress.is_some() {
            return;
        }
        self.tool_install_error = None;
        self.tool_install_progress = Some((0, None));
        let tx = self.events_tx.clone();
        let progress_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("videleum-install-ytdlp".into())
            .spawn(move || {
                let cancel = AtomicBool::new(false);
                let mut last = Instant::now() - Duration::from_secs(1);
                let result = install_ytdlp(&cancel, |downloaded, total| {
                    if last.elapsed() >= Duration::from_millis(200) || downloaded == 0 {
                        last = Instant::now();
                        let _ = progress_tx.send(AppEvent::ToolProgress { downloaded, total });
                    }
                })
                .map_err(|error| error.to_string());
                let _ = tx.send(AppEvent::ToolInstalled(result));
            });
        if let Err(error) = spawned {
            self.tool_install_progress = None;
            self.tool_install_error = Some(error.to_string());
            window.push_notification("Couldn't start the yt-dlp installer", cx);
        }
        cx.notify();
    }

    // ---- Add media -----------------------------------------------------

    pub(crate) fn open_add_media(
        &mut self,
        intent: AddIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_intent = intent;
        self.add_error = None;
        self.add_preview = None;
        self.add_existing = false;
        self.add_url
            .update(cx, |state, cx| state.set_value("", window, cx));

        let input = self.add_url.clone();
        let entity = cx.entity();
        let builder_entity = entity.clone();
        let initial_focus = Rc::new(Cell::new(true));

        window.open_dialog(cx, move |dialog, window, cx| {
            if initial_focus.replace(false) {
                input.read(cx).focus_handle(cx).focus(window, cx);
            }
            let app = builder_entity.read(cx);
            let intent = app.add_intent;
            let preview = app.add_preview.as_ref();
            let existing = app.add_existing;

            let (title, promise) = match intent {
                AddIntent::Auto => (
                    "Add media",
                    "Download a video or follow a channel, playlist, or favorites list.",
                ),
                AddIntent::Video => ("Add video", "Download one video into your library."),
                AddIntent::Subscription => (
                    "Add subscription",
                    "Follow a channel, playlist, or favorites list for new videos.",
                ),
            };
            let (confirm_label, can_confirm) = confirm_state(preview, intent, existing);

            let mut body = div().flex().flex_col().gap_4();
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(promise),
            );
            body = body.child(Input::new(&input).cleanable(true));

            if let Some(error) = app.add_error.as_deref() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.to_string()),
                );
            } else {
                match preview {
                    Some(Ok(source)) => {
                        let platform = source.platform;
                        let kind = source.kind;
                        let handle = source.display_handle();
                        let mut info = div().flex().flex_col().gap_1();
                        info = info.child(
                            div()
                                .text_sm()
                                .font_weight(gpui_kit::FontWeight::MEDIUM)
                                .child(handle),
                        );
                        let mut meta = format!("{} · {}", platform.label(), kind.label());
                        if existing {
                            meta.push_str(" · already in your library");
                        }
                        info = info.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(meta),
                        );
                        body = body.child(
                            div()
                                .w_full()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .p_3()
                                .child(info),
                        );
                    }
                    Some(Err(error)) => {
                        body = body.child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(error.clone()),
                        );
                    }
                    None => {}
                }
            }

            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("YouTube, Bilibili, and Twitch links are supported."),
            );

            let dialog_entity = entity.clone();
            dialog.title(title).child(body).footer(
                DialogFooter::new()
                    .gap_2()
                    .child(
                        DialogClose::new()
                            .child(Button::new("add-cancel").outline().label("Cancel")),
                    )
                    .child(
                        DialogAction::new().child(
                            Button::new("add-confirm")
                                .primary()
                                .label(confirm_label)
                                .disabled(!can_confirm)
                                .on_click(move |_, window, cx| {
                                    let added = dialog_entity
                                        .update(cx, |app, cx| app.submit_add(window, cx));
                                    if added {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                    ),
            )
        });
    }

    fn update_add_preview(&mut self, value: String, cx: &mut Context<Self>) {
        self.add_error = None;
        self.add_existing = false;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            self.add_preview = None;
        } else {
            match parse_source(trimmed) {
                Ok(source) => {
                    self.add_existing = self
                        .store
                        .find_media(source.platform, &source.id)
                        .ok()
                        .flatten()
                        .is_some();
                    self.add_preview = Some(Ok(source));
                }
                Err(error) => self.add_preview = Some(Err(error.to_string())),
            }
        }
        cx.notify();
    }

    fn submit_add(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let value = self.add_url.read(cx).value().to_string();
        let source = match parse_source(value.trim()) {
            Ok(source) => source,
            Err(error) => {
                self.add_error = Some(error.to_string());
                cx.notify();
                return false;
            }
        };
        let is_subscription = source.is_subscription();
        if self.add_intent == AddIntent::Video && is_subscription {
            self.add_error = Some(
                "That link is a channel or playlist. Add it as a subscription instead.".into(),
            );
            cx.notify();
            return false;
        }
        if self.add_intent == AddIntent::Subscription && !is_subscription {
            self.add_error =
                Some("That link points at one video. Use Add media to download it.".into());
            cx.notify();
            return false;
        }
        if is_subscription {
            self.follow(source, window, cx)
        } else {
            self.start_download(source, window, cx)
        }
    }

    fn follow(&mut self, source: SourceRef, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let subscription = Subscription {
            platform: source.platform,
            kind: source.kind,
            source_id: source.id.clone(),
            source_url: source.canonical_url.clone(),
            title: source.display_handle(),
            auto_download: true,
            refresh_interval_minutes: self.settings.refresh_interval_minutes,
            created_at: now_timestamp(),
            ..Default::default()
        };
        if let Err(error) = validate_subscription(&subscription) {
            self.add_error = Some(error.to_string());
            cx.notify();
            return false;
        }
        match self.store.upsert_subscription(&subscription) {
            Ok(_) => {
                self.reload_subscriptions();
                self.view = View::Subscriptions;
                self.selected = None;
                window.push_notification(format!("Following {}", subscription.title), cx);
                if let Some(saved) = self
                    .subscriptions
                    .iter()
                    .find(|item| {
                        item.platform == subscription.platform
                            && item.source_id == subscription.source_id
                    })
                    .cloned()
                {
                    self.refresh_subscriptions(vec![saved], cx);
                }
                cx.notify();
                true
            }
            Err(error) => {
                self.add_error = Some(error.to_string());
                cx.notify();
                false
            }
        }
    }

    // ---- Downloads -----------------------------------------------------

    fn start_download(
        &mut self,
        source: SourceRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.jobs.iter().any(|job| {
            job.platform == source.platform && job.source_id == source.id && job.state.is_active()
        }) {
            window.push_notification("That video is already queued", cx);
            return false;
        }
        let request = DownloadRequest {
            url: source.canonical_url.clone(),
            source: Some(source),
        };
        let source = match request.source.clone() {
            Some(source) => source,
            None => return false,
        };
        match enqueue(&self.store, &source) {
            Ok(job) => {
                self.pending_jobs.push_back(job.clone());
                self.jobs.insert(0, job);
                self.pump_queue(cx);
                window.push_notification("Download queued", cx);
                cx.notify();
                true
            }
            Err(error) => {
                window.push_notification(error.to_string(), cx);
                false
            }
        }
    }

    fn pump_queue(&mut self, cx: &mut Context<Self>) {
        let limit = self.settings.max_concurrent_downloads.max(1);
        while self.active_jobs.len() < limit {
            let Some(job) = self.pending_jobs.pop_front() else {
                break;
            };
            let cancel = Arc::new(AtomicBool::new(false));
            self.active_jobs.insert(job.id, cancel.clone());

            let settings = self.settings.clone();
            let store = self.store.clone();
            let tx = self.events_tx.clone();
            let id = job.id;
            let mut worker_job = job.clone();
            worker_job.state = DownloadState::Queued;
            let spawned = std::thread::Builder::new()
                .name(format!("videleum-download-{id}"))
                .spawn(move || {
                    run_job(settings, store, worker_job, cancel, move |event| {
                        let _ = tx.send(AppEvent::Job(event));
                    });
                });

            if let Err(error) = spawned {
                self.active_jobs.remove(&id);
                let mut failed = job;
                failed.state = DownloadState::Failed;
                failed.stage = String::new();
                failed.error = Some(error.to_string());
                let _ = self.store.save_job(&failed);
                if let Some(slot) = self.jobs.iter_mut().find(|item| item.id == id) {
                    *slot = failed;
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn cancel_job(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Some(cancel) = self.active_jobs.get(&id) {
            cancel.store(true, Ordering::Relaxed);
            if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
                job.stage = "Canceling…".into();
            }
            cx.notify();
        }
    }

    pub(crate) fn retry_job(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(job) = self.jobs.iter().find(|job| job.id == id).cloned() else {
            return;
        };
        let source = match parse_source(&job.source_url) {
            Ok(source) => source,
            Err(error) => {
                window.push_notification(error.to_string(), cx);
                return;
            }
        };
        self.start_download(source, window, cx);
    }

    pub(crate) fn clear_finished_jobs(&mut self, cx: &mut Context<Self>) {
        if self.store.clear_finished_jobs().is_ok() {
            self.reload_jobs();
            cx.notify();
        }
    }

    pub(crate) fn delete_job(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.store.delete_job(id).is_ok() {
            self.jobs.retain(|job| job.id != id);
            cx.notify();
        }
    }

    // ---- Subscriptions -------------------------------------------------

    pub(crate) fn refresh_all_subscriptions(&mut self, cx: &mut Context<Self>) {
        if self.subscriptions.is_empty() || self.refreshing_all {
            return;
        }
        self.refresh_subscriptions(self.subscriptions.clone(), cx);
    }

    pub(crate) fn refresh_subscription(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.checking_subscriptions.contains(&id) {
            return;
        }
        if let Some(subscription) = self
            .subscriptions
            .iter()
            .find(|item| item.id == id)
            .cloned()
        {
            self.refresh_subscriptions(vec![subscription], cx);
        }
    }

    fn maybe_auto_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.settings.auto_refresh_subscriptions || self.subscriptions.is_empty() {
            return;
        }
        let now = now_timestamp();
        let due: Vec<Subscription> = self
            .subscriptions
            .iter()
            .filter(|subscription| {
                let interval = subscription.refresh_interval_minutes.max(15) as i64 * 60;
                subscription
                    .last_checked_at
                    .is_none_or(|checked| now - checked >= interval)
            })
            .cloned()
            .collect();
        if !due.is_empty() {
            self.refresh_subscriptions(due, cx);
        }
    }

    fn refresh_subscriptions(&mut self, subscriptions: Vec<Subscription>, cx: &mut Context<Self>) {
        if subscriptions.is_empty() {
            return;
        }
        for subscription in &subscriptions {
            self.checking_subscriptions.insert(subscription.id);
            self.subscription_errors.remove(&subscription.id);
        }
        self.refreshing_all = true;

        let settings = self.settings.clone();
        let store = self.store.clone();
        let tx = self.events_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("videleum-subscriptions".into())
            .spawn(move || {
                for subscription in subscriptions {
                    let result = refresh_subscription_feed(&settings, &store, &subscription, 15);
                    let event = match result {
                        Ok(outcome) => AppEvent::SubscriptionRefreshed {
                            id: subscription.id,
                            new_items: outcome.new_items,
                            queued: outcome.queued,
                            error: None,
                        },
                        Err(error) => AppEvent::SubscriptionRefreshed {
                            id: subscription.id,
                            new_items: 0,
                            queued: Vec::new(),
                            error: Some(error),
                        },
                    };
                    let _ = tx.send(event);
                }
            });
        if let Err(error) = spawned {
            self.checking_subscriptions.clear();
            self.refreshing_all = false;
            let _ = error;
        }
        cx.notify();
    }

    pub(crate) fn set_subscription_auto_download(
        &mut self,
        id: i64,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .store
            .set_subscription_auto_download(id, enabled)
            .is_ok()
        {
            if let Some(subscription) = self
                .subscriptions
                .iter_mut()
                .find(|subscription| subscription.id == id)
            {
                subscription.auto_download = enabled;
            }
            cx.notify();
        }
    }

    pub(crate) fn delete_subscription(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.store.delete_subscription(id).is_ok() {
            self.subscriptions.retain(|item| item.id != id);
            self.checking_subscriptions.remove(&id);
            self.subscription_errors.remove(&id);
            cx.notify();
        }
    }

    // ---- Worker events -------------------------------------------------

    fn start_event_ticker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(EVENT_TICK).await;
                let alive = this.update_in(cx, |app, window, cx| {
                    app.drain_events(window, cx);
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        self.async_tasks.push(task);
    }

    fn drain_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut changed = false;
        loop {
            match self.events_rx.try_recv() {
                Ok(event) => {
                    self.handle_event(event, window, cx);
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn handle_event(&mut self, event: AppEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            AppEvent::Job(event) => self.handle_job_event(event, window, cx),
            AppEvent::Tools(Ok(status)) => {
                self.tools = status;
                self.tools_loading = false;
            }
            AppEvent::Tools(Err(error)) => {
                self.tools_loading = false;
                self.tool_install_error = Some(error);
            }
            AppEvent::ToolProgress { downloaded, total } => {
                self.tool_install_progress = Some((downloaded, total));
            }
            AppEvent::ToolInstalled(result) => {
                self.tool_install_progress = None;
                match result {
                    Ok(path) => {
                        self.settings.ytdlp_path = Some(path);
                        self.save_settings(cx);
                        window.push_notification("yt-dlp installed", cx);
                        self.detect_tools(cx);
                    }
                    Err(error) => {
                        self.tool_install_error = Some(error.clone());
                        window.push_notification(format!("Couldn't install yt-dlp: {error}"), cx);
                    }
                }
            }
            AppEvent::SubscriptionRefreshed {
                id,
                new_items,
                queued,
                error,
            } => {
                self.checking_subscriptions.remove(&id);
                self.refreshing_all = !self.checking_subscriptions.is_empty();
                match error {
                    Some(error) => {
                        self.subscription_errors.insert(id, error);
                    }
                    None => {
                        self.subscription_errors.remove(&id);
                        for job in queued {
                            if !self.jobs.iter().any(|item| item.id == job.id) {
                                self.pending_jobs.push_back(job.clone());
                                self.jobs.insert(0, job);
                            }
                        }
                        self.pump_queue(cx);
                        self.reload_subscriptions();
                        if new_items > 0 {
                            let title = self
                                .subscriptions
                                .iter()
                                .find(|subscription| subscription.id == id)
                                .map(|subscription| subscription.title.clone())
                                .unwrap_or_default();
                            window.push_notification(
                                format!(
                                    "{} new {} from {}",
                                    new_items,
                                    if new_items == 1 { "video" } else { "videos" },
                                    title
                                ),
                                cx,
                            );
                        }
                    }
                }
                self.reload_subscriptions();
            }
            AppEvent::Scanned(result) => match result {
                Ok(message) => {
                    window.push_notification(message, cx);
                    self.reload_all(cx);
                }
                Err(error) => window.push_notification(error, cx),
            },
        }
    }

    fn handle_job_event(&mut self, event: JobEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            JobEvent::Updated(job) => {
                self.replace_job(job);
            }
            JobEvent::Completed { job, .. } => {
                self.active_jobs.remove(&job.id);
                let title = job.display_title();
                self.replace_job(job);
                self.reload_library();
                self.pump_queue(cx);
                window.push_notification(format!("Downloaded {title}"), cx);
            }
            JobEvent::Failed { job, message } => {
                self.active_jobs.remove(&job.id);
                self.replace_job(job);
                self.pump_queue(cx);
                window.push_notification(format!("Download failed: {message}"), cx);
            }
            JobEvent::Canceled(job) => {
                self.active_jobs.remove(&job.id);
                self.replace_job(job);
                self.pump_queue(cx);
            }
        }
    }

    fn replace_job(&mut self, job: videleum_core::DownloadJob) {
        if let Some(slot) = self.jobs.iter_mut().find(|item| item.id == job.id) {
            *slot = job;
        } else {
            self.jobs.insert(0, job);
        }
    }

    // ---- Render --------------------------------------------------------

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem};

        let active = self.active_job_count();
        let menu = SidebarMenu::new().children([
            SidebarMenuItem::new("Library")
                .icon(IconName::Library)
                .active(self.view == View::Library)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigate(View::Library, cx);
                })),
            SidebarMenuItem::new("Downloads")
                .icon(IconName::Download)
                .active(self.view == View::Downloads)
                .suffix(move |_, cx| {
                    if active == 0 {
                        return div().into_any_element();
                    }
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(active.to_string())
                        .into_any_element()
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigate(View::Downloads, cx);
                })),
            SidebarMenuItem::new("Subscriptions")
                .icon(IconName::Rss)
                .active(self.view == View::Subscriptions)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigate(View::Subscriptions, cx);
                })),
        ]);

        let settings = SidebarMenu::new().child(
            SidebarMenuItem::new("Settings")
                .icon(IconName::Settings)
                .active(self.view == View::Settings)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigate(View::Settings, cx);
                })),
        );

        let footer = div()
            .flex()
            .flex_col()
            .gap_1()
            .w_full()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.6))
                    .child("LIBRARY"),
            )
            .child(div().text_sm().child(format!(
                "{} videos · {}",
                self.stats.item_count,
                videleum_core::format_bytes(self.stats.total_bytes)
            )))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.6))
                    .child(self.settings.library_root.display().to_string()),
            );

        Sidebar::new("videleum-sidebar")
            .child(SidebarGroup::new("Browse").child(menu))
            .child(SidebarGroup::new("Videleum").child(settings))
            .footer(footer)
    }
}

impl Render for VideleumApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::TitleBar;

        let content = match self.view {
            View::Library => {
                if self.selected.is_some() {
                    views::detail::render(self, window, cx)
                } else {
                    views::library::render(self, window, cx)
                }
            }
            View::Downloads => views::downloads::render(self, window, cx),
            View::Subscriptions => views::subscriptions::render(self, window, cx),
            View::Settings => views::settings::render(self, window, cx),
        };

        let title_bar = TitleBar::new().child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    gpui_kit::svg()
                        .path("branding/logo.svg")
                        .size(rems(1.25))
                        .flex_shrink_0(),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                        .child("Videleum"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.view.label()),
                ),
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(title_bar)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(content),
                    ),
            )
    }
}

fn confirm_state(
    preview: Option<&Result<SourceRef, String>>,
    intent: AddIntent,
    existing: bool,
) -> (&'static str, bool) {
    match preview {
        Some(Ok(source)) if source.is_subscription() => match intent {
            AddIntent::Video => ("Follow", false),
            AddIntent::Subscription | AddIntent::Auto => ("Follow", true),
        },
        Some(Ok(_)) => match intent {
            AddIntent::Subscription => ("Download", false),
            AddIntent::Video | AddIntent::Auto if existing => ("Download again", true),
            AddIntent::Video | AddIntent::Auto => ("Download", true),
        },
        _ => ("Add", false),
    }
}

fn refresh_subscription_feed(
    settings: &Settings,
    store: &Store,
    subscription: &Subscription,
    limit: usize,
) -> Result<RefreshOutcome, String> {
    let cancel = AtomicBool::new(false);
    let page =
        fetch_feed(settings, subscription, limit, &cancel).map_err(|error| error.to_string())?;

    let known_from_media = store.list_media().unwrap_or_default();
    let known_from_jobs = store.list_jobs(JOB_HISTORY_LIMIT).unwrap_or_default();
    let mut known: HashSet<(Platform, String)> = HashSet::new();
    for item in &known_from_media {
        known.insert((item.platform, item.source_id.clone()));
    }
    for job in &known_from_jobs {
        known.insert((job.platform, job.source_id.clone()));
    }

    let mut new_items = 0usize;
    let mut queued = Vec::new();
    for entry in page.entries.iter().take(8) {
        let Ok(source) = parse_source(&entry.url) else {
            continue;
        };
        if source.kind != SourceKind::Video {
            continue;
        }
        let key = (source.platform, source.id.clone());
        if known.contains(&key) {
            continue;
        }
        new_items += 1;
        if subscription.auto_download
            && queued.len() < 5
            && let Ok(job) = enqueue(store, &source)
        {
            queued.push(job);
            known.insert(key);
        }
    }

    let status = if new_items == 0 {
        "Up to date".to_string()
    } else if subscription.auto_download {
        format!("{new_items} new · queued")
    } else {
        format!("{new_items} new")
    };

    let mut updated = subscription.clone();
    if !page.title.trim().is_empty() {
        updated.title = page.title;
    }
    updated.last_checked_at = Some(now_timestamp());
    updated.last_status = status;
    let _ = store.upsert_subscription(&updated);

    Ok(RefreshOutcome { new_items, queued })
}
