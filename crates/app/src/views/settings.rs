//! Preferences, helper tools, cookies and archive maintenance.

use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::Input,
    progress::Progress,
    scroll::ScrollableElement as _,
    switch::Switch,
};
use gpui_kit::{
    AnyElement, App, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};

use videleum_core::{Quality, ThemePreference, app_data_dir, settings_path, tools_dir};

use crate::app::{PathField, VideleumApp};
use crate::components::{self, Tone};

pub fn render(
    app: &mut VideleumApp,
    window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let header = components::page_header(
        "Settings",
        Some("Library, downloads, metadata and helper tools".into()),
        vec![
            Button::new("settings-rescan")
                .outline()
                .icon(IconName::Scan)
                .label("Rescan library")
                .on_click(cx.listener(|this, _, window, cx| this.rescan(window, cx)))
                .into_any_element(),
        ],
        cx,
    );

    let mut body = components::page_body().p_6().overflow_y_scrollbar();

    body = body.child(
        div()
            .flex()
            .flex_col()
            .gap_5()
            .w_full()
            .max_w(px(920.))
            .child(library_section(app, window, cx))
            .child(downloads_section(app, cx))
            .child(metadata_section(app, cx))
            .child(tools_section(app, window, cx))
            .child(cookies_section(app, window, cx))
            .child(appearance_section(app, window, cx))
            .child(subscriptions_section(app, cx))
            .child(about_section(app, cx)),
    );

    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(header)
        .child(body)
        .into_any_element()
}

fn section_heading(title: &'static str, detail: Option<&'static str>, cx: &App) -> AnyElement {
    let mut heading = div().flex().flex_col().gap_1().w_full().child(
        div()
            .text_base()
            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
            .child(title),
    );
    if let Some(detail) = detail {
        heading = heading.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(detail),
        );
    }
    heading.into_any_element()
}

fn library_section(
    app: &VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let root = app.settings.library_root.clone();
    let reveal_root = root.clone();
    let actions = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            Button::new("settings-library-reveal")
                .ghost()
                .icon(IconName::FolderOpen)
                .tooltip("Show in file manager")
                .on_click(move |_, _, cx| cx.reveal_path(&reveal_root)),
        )
        .child(
            Button::new("settings-library-change")
                .outline()
                .label("Change")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.pick_path(PathField::LibraryRoot, window, cx);
                })),
        );

    components::panel(
        vec![
            section_heading(
                "Library",
                Some("Archived videos and the portable catalogue live together."),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Library folder",
                Some(root.to_string_lossy().to_string().into()),
                actions.into_any_element(),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Catalogue",
                Some(
                    app.store
                        .database_path()
                        .to_string_lossy()
                        .to_string()
                        .into(),
                ),
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("videleum.db")
                    .into_any_element(),
                cx,
            ),
            components::banner(
                IconName::Info,
                "The library folder is portable: copy it to another drive and Videleum keeps your catalogue, posters, metadata and comments.",
                Tone::Neutral,
                cx,
            ),
        ],
        cx,
    )
}

fn downloads_section(app: &VideleumApp, cx: &mut Context<VideleumApp>) -> AnyElement {
    let quality_index = match app.settings.quality {
        Quality::Best => 0,
        Quality::P2160 => 1,
        Quality::P1440 => 2,
        Quality::P1080 => 3,
        Quality::P720 => 4,
    };
    let quality_group = ButtonGroup::new("settings-quality")
        .outline()
        .child(
            Button::new("quality-best")
                .label("Best")
                .selected(quality_index == 0),
        )
        .child(
            Button::new("quality-2160")
                .label("2160p")
                .selected(quality_index == 1),
        )
        .child(
            Button::new("quality-1440")
                .label("1440p")
                .selected(quality_index == 2),
        )
        .child(
            Button::new("quality-1080")
                .label("1080p")
                .selected(quality_index == 3),
        )
        .child(
            Button::new("quality-720")
                .label("720p")
                .selected(quality_index == 4),
        )
        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
            let quality = match clicks.first().copied().unwrap_or(0) {
                1 => Quality::P2160,
                2 => Quality::P1440,
                3 => Quality::P1080,
                4 => Quality::P720,
                _ => Quality::Best,
            };
            this.update_settings(cx, |settings| settings.quality = quality);
        }));

    let concurrency = app.settings.max_concurrent_downloads;
    let concurrency_group = ButtonGroup::new("settings-concurrency")
        .outline()
        .children((1..=4).map(|value| {
            Button::new(gpui_kit::SharedString::from(format!("concurrency-{value}")))
                .label(value.to_string())
                .selected(concurrency == value)
        }))
        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
            let value = clicks.first().map(|index| index + 1).unwrap_or(1);
            this.update_settings(cx, |settings| settings.max_concurrent_downloads = value);
        }));

    let naming = div()
        .w(px(420.))
        .flex()
        .flex_col()
        .gap_2()
        .child(Input::new(&app.naming))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(components::muted(
                    "Tokens: {creator}, {title}, {id}, {platform}, {date}, {tags}",
                    cx,
                ))
                .child(
                    Button::new("settings-naming-reset")
                        .ghost()
                        .label("Reset")
                        .on_click(cx.listener(|this, _, window, cx| {
                            let template = videleum_core::Settings::default().naming_template;
                            this.naming.update(cx, |state, cx| {
                                state.set_value(template.clone(), window, cx);
                            });
                            this.update_settings(cx, |settings| {
                                settings.naming_template = template;
                            });
                        })),
                ),
        );

    components::panel(
        vec![
            section_heading(
                "Downloads",
                Some("Quality, concurrency and archive naming."),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Preferred quality",
                Some("The highest resolution Videleum requests.".into()),
                quality_group.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Simultaneous downloads",
                Some("More parallel downloads can slow each other down.".into()),
                concurrency_group.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Folder and file naming",
                Some("Paths use your library folder as the root.".into()),
                naming.into_any_element(),
                cx,
            ),
        ],
        cx,
    )
}

fn metadata_section(app: &VideleumApp, cx: &mut Context<VideleumApp>) -> AnyElement {
    let comments = Switch::new("settings-comments")
        .checked(app.settings.fetch_comments)
        .on_change(cx.listener(|this, checked: &bool, _, cx| {
            this.update_settings(cx, |settings| settings.fetch_comments = *checked);
        }));
    let subtitles = Switch::new("settings-subtitles")
        .checked(app.settings.write_subtitles)
        .on_change(cx.listener(|this, checked: &bool, _, cx| {
            this.update_settings(cx, |settings| settings.write_subtitles = *checked);
        }));
    let danmaku = Switch::new("settings-danmaku")
        .checked(app.settings.write_danmaku)
        .on_change(cx.listener(|this, checked: &bool, _, cx| {
            this.update_settings(cx, |settings| settings.write_danmaku = *checked);
        }));

    let limit = app.settings.comment_limit;
    let limits = [0usize, 10, 25, 50, 100];
    let limit_group = ButtonGroup::new("settings-comment-limit")
        .outline()
        .children(limits.iter().map(|value| {
            Button::new(gpui_kit::SharedString::from(format!(
                "comment-limit-{value}"
            )))
            .label(if *value == 0 {
                "Off".to_string()
            } else {
                value.to_string()
            })
            .selected(limit == *value)
        }))
        .on_click(cx.listener(move |this, clicks: &Vec<usize>, _, cx| {
            let value = limits
                .get(clicks.first().copied().unwrap_or(0))
                .copied()
                .unwrap_or(0);
            this.update_settings(cx, |settings| settings.comment_limit = value);
        }));

    components::panel(
        vec![
            section_heading(
                "Metadata",
                Some("Sidecar files saved next to every video."),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Capture comments",
                Some("Save a readable copy of top comments in the NFO file.".into()),
                comments.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Write subtitles",
                Some("Keep available subtitle tracks in a subtitles folder.".into()),
                subtitles.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Convert danmaku",
                Some("Save Bilibili danmaku as ASS subtitles for playback.".into()),
                danmaku.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Comments to keep",
                Some("Zero turns comment capture off.".into()),
                limit_group.into_any_element(),
                cx,
            ),
        ],
        cx,
    )
}

fn tools_section(
    app: &VideleumApp,
    window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let mut rows = vec![
        section_heading(
            "Helper tools",
            Some("Videleum calls these programs to download and merge video."),
            cx,
        ),
        div()
            .w_full()
            .child(components::divider(cx))
            .into_any_element(),
    ];

    rows.push(tool_row(
        app,
        "yt-dlp",
        app.tools.ytdlp.as_deref(),
        app.tools.ytdlp_version.as_deref(),
        "Downloads YouTube, Twitch, and subscription videos.",
        PathField::Ytdlp,
        true,
        window,
        cx,
    ));
    rows.push(tool_row(
        app,
        "FFmpeg",
        app.tools.ffmpeg.as_deref(),
        app.tools.ffmpeg_version.as_deref(),
        "Merges Bilibili DASH audio and video and remuxes streams.",
        PathField::Ffmpeg,
        false,
        window,
        cx,
    ));

    if let Some((downloaded, total)) = app.tool_install_progress {
        let value = total
            .filter(|total| *total > 0)
            .map(|total| downloaded as f32 / total as f32 * 100.0)
            .unwrap_or(0.0);
        rows.push(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(components::muted("Installing yt-dlp…", cx))
                .child(
                    Progress::new("settings-ytdlp-install")
                        .value(value)
                        .loading(total.is_none())
                        .w_full(),
                )
                .into_any_element(),
        );
    }

    if let Some(error) = app.tool_install_error.as_deref() {
        rows.push(components::banner(
            IconName::CircleAlert,
            error.to_string(),
            Tone::Danger,
            cx,
        ));
    }

    components::panel(rows, cx)
}

#[allow(clippy::too_many_arguments)]
fn tool_row(
    app: &VideleumApp,
    label: &'static str,
    path: Option<&Path>,
    version: Option<&str>,
    description: &'static str,
    field: PathField,
    installable: bool,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let installed = path.is_some();
    let detail = match (path, version) {
        (Some(path), Some(version)) => format!("{version} · {}", path.display()),
        (Some(path), None) => path.display().to_string(),
        _ => description.to_string(),
    };

    let mut actions = div().flex().items_center().gap_2();
    if installable && !installed {
        actions = actions.child(
            Button::new(gpui_kit::SharedString::from(format!("install-{label}")))
                .primary()
                .icon(IconName::Download)
                .label("Install")
                .loading(app.tool_install_progress.is_some())
                .disabled(app.tool_install_progress.is_some())
                .on_click(cx.listener(|this, _, window, cx| this.install_ytdlp(window, cx))),
        );
    }
    actions = actions.child(
        Button::new(gpui_kit::SharedString::from(format!("choose-{label}")))
            .outline()
            .label(if installed { "Change" } else { "Choose" })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_path(field, window, cx);
            })),
    );
    if installed {
        actions = actions.child(
            Button::new(gpui_kit::SharedString::from(format!("clear-{label}")))
                .ghost()
                .icon(IconName::X)
                .tooltip("Clear custom path")
                .on_click(cx.listener(move |this, _, _, cx| this.clear_path(field, cx))),
        );
    }

    let status = if app.tools_loading {
        components::tone_tag("Checking…", Tone::Neutral, cx)
    } else if installed {
        components::tone_tag("Ready", Tone::Success, cx)
    } else {
        components::tone_tag("Not found", Tone::Warning, cx)
    };

    components::setting_row(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(label)
            .child(status)
            .into_any_element(),
        Some(detail.into()),
        actions.into_any_element(),
        cx,
    )
}

fn cookies_section(
    app: &VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    components::panel(
        vec![
            section_heading(
                "Cookies",
                Some("Needed for private, age-restricted, or member-only videos."),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            cookie_row(
                "YouTube",
                app.settings.youtube_cookies.as_ref(),
                PathField::YoutubeCookies,
                cx,
            ),
            cookie_row(
                "Bilibili",
                app.settings.bilibili_cookies.as_ref(),
                PathField::BilibiliCookies,
                cx,
            ),
            cookie_row(
                "Twitch",
                app.settings.twitch_cookies.as_ref(),
                PathField::TwitchCookies,
                cx,
            ),
            components::banner(
                IconName::Lock,
                "Cookie files stay on this computer and are only passed to the matching platform.",
                Tone::Neutral,
                cx,
            ),
        ],
        cx,
    )
}

fn cookie_row(
    platform: &'static str,
    path: Option<&PathBuf>,
    field: PathField,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let mut actions = div().flex().items_center().gap_2().child(
        Button::new(gpui_kit::SharedString::from(format!("cookie-{platform}")))
            .outline()
            .label(if path.is_some() { "Change" } else { "Choose" })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_path(field, window, cx);
            })),
    );
    if path.is_some() {
        actions = actions.child(
            Button::new(gpui_kit::SharedString::from(format!(
                "cookie-clear-{platform}"
            )))
            .ghost()
            .icon(IconName::X)
            .tooltip("Remove")
            .on_click(cx.listener(move |this, _, _, cx| this.clear_path(field, cx))),
        );
    }

    components::setting_row(
        platform,
        Some(
            path.map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| "Not set".to_string())
                .into(),
        ),
        actions.into_any_element(),
        cx,
    )
}

fn appearance_section(
    app: &VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let index = match app.settings.theme {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    };
    let themes = ButtonGroup::new("settings-theme")
        .outline()
        .child(
            Button::new("theme-system")
                .label("System")
                .selected(index == 0),
        )
        .child(
            Button::new("theme-light")
                .label("Light")
                .selected(index == 1),
        )
        .child(Button::new("theme-dark").label("Dark").selected(index == 2))
        .on_click(cx.listener(|this, clicks: &Vec<usize>, window, cx| {
            let preference = match clicks.first().copied().unwrap_or(0) {
                1 => ThemePreference::Light,
                2 => ThemePreference::Dark,
                _ => ThemePreference::System,
            };
            this.set_theme(preference, window, cx);
        }));

    components::panel(
        vec![
            section_heading("Appearance", Some("Match the system or pin a mode."), cx),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Theme",
                Some("Videleum's accent follows the brand artwork.".into()),
                themes.into_any_element(),
                cx,
            ),
        ],
        cx,
    )
}

fn subscriptions_section(app: &VideleumApp, cx: &mut Context<VideleumApp>) -> AnyElement {
    let auto_refresh = Switch::new("settings-auto-refresh")
        .checked(app.settings.auto_refresh_subscriptions)
        .on_change(cx.listener(|this, checked: &bool, _, cx| {
            this.update_settings(cx, |settings| {
                settings.auto_refresh_subscriptions = *checked
            });
        }));

    let intervals = [15u32, 30, 60, 180, 720, 1440];
    let current = app.settings.refresh_interval_minutes;
    let interval_group = ButtonGroup::new("settings-refresh-interval")
        .outline()
        .children(intervals.iter().map(|minutes| {
            Button::new(gpui_kit::SharedString::from(format!("refresh-{minutes}")))
                .label(interval_label(*minutes))
                .selected(current == *minutes)
        }))
        .on_click(cx.listener(move |this, clicks: &Vec<usize>, _, cx| {
            let minutes = intervals
                .get(clicks.first().copied().unwrap_or(0))
                .copied()
                .unwrap_or(30);
            this.update_settings(cx, |settings| {
                settings.refresh_interval_minutes = minutes;
            });
        }));

    components::panel(
        vec![
            section_heading(
                "Subscriptions",
                Some("Videleum checks followed creators while the app is open."),
                cx,
            ),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            components::setting_row(
                "Check automatically",
                Some("Skipped when a subscription is already being refreshed.".into()),
                auto_refresh.into_any_element(),
                cx,
            ),
            components::setting_row(
                "Check interval",
                Some("Each subscription follows this interval.".into()),
                interval_group.into_any_element(),
                cx,
            ),
        ],
        cx,
    )
}

fn interval_label(minutes: u32) -> String {
    if minutes >= 1440 {
        "24h".to_string()
    } else if minutes >= 60 && minutes.is_multiple_of(60) {
        format!("{}h", minutes / 60)
    } else {
        format!("{minutes}m")
    }
}

fn about_section(_app: &VideleumApp, cx: &mut Context<VideleumApp>) -> AnyElement {
    components::panel(
        vec![
            section_heading("About", Some("Videleum · personal media archive"), cx),
            div()
                .w_full()
                .child(components::divider(cx))
                .into_any_element(),
            div()
                .grid()
                .grid_cols(2)
                .gap_4()
                .w_full()
                .child(components::spec("Version", env!("CARGO_PKG_VERSION"), cx))
                .child(components::spec(
                    "Data folder",
                    app_data_dir().display().to_string(),
                    cx,
                ))
                .child(components::spec(
                    "Settings file",
                    settings_path().display().to_string(),
                    cx,
                ))
                .child(components::spec(
                    "Tools folder",
                    tools_dir().display().to_string(),
                    cx,
                ))
                .into_any_element(),
            components::muted(
                "Built with Rust and GPUI Kit. Downloads use yt-dlp for YouTube and Twitch, native Bilibili APIs, and FFmpeg for merging.",
                cx,
            ),
        ],
        cx,
    )
}
