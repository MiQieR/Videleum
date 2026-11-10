//! A single video: hero, actions, metadata, files and captured comments.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, Size, WindowExt as _,
    avatar::Avatar,
    button::{Button, ButtonVariant, ButtonVariants as _},
    scroll::ScrollableElement as _,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, IntoElement, ObjectFit, ParentElement as _, Styled as _, StyledImage as _,
    Window, div, img, px,
};

use videleum_core::model::{Comment, MediaItem, MediaStatus};
use videleum_core::{format_bytes, format_date, format_relative_time, now_timestamp};

use crate::app::VideleumApp;
use crate::components::{self, Tone};

pub fn render(
    app: &mut VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let Some(item) = app.selected_media().cloned() else {
        return div()
            .flex()
            .flex_col()
            .size_full()
            .child(components::page_header("Library", None, Vec::new(), cx))
            .child(components::empty_state(
                IconName::Clapperboard,
                "Video not found",
                "It may have been removed from the library.",
                None,
                cx,
            ))
            .into_any_element();
    };

    let back = Button::new("detail-back")
        .ghost()
        .icon(IconName::ArrowLeft)
        .label("Library")
        .on_click(cx.listener(|this, _, _, cx| this.close_detail(cx)));

    let source_url = item.source_url.clone();
    let open_original = Button::new("detail-original")
        .outline()
        .icon(IconName::ExternalLink)
        .label("Open original")
        .on_click(move |_, _, cx| cx.open_url(&source_url));

    let top_bar = div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_3()
        .px_6()
        .py_3()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(back)
        .child(div().flex_1())
        .child(open_original);

    let mut body = div()
        .flex()
        .flex_col()
        .gap_6()
        .flex_1()
        .min_h_0()
        .w_full()
        .p_6()
        .overflow_y_scrollbar();

    body = body.child(hero(&item, cx));

    if !item.tags.is_empty() {
        body = body.child(
            div().flex().flex_wrap().gap_2().children(
                item.tags
                    .iter()
                    .map(|tag| components::tone_tag(tag.clone(), Tone::Neutral, cx)),
            ),
        );
    }

    body = body.child(description_panel(&item, cx));
    body = body.child(files_panel(&item, cx));
    if !item.comments.is_empty() {
        body = body.child(comments_panel(&item, cx));
    }
    body = body.child(details_panel(&item, cx));

    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(top_bar)
        .child(body)
        .into_any_element()
}

fn hero(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let id = item.id;
    let cover = item
        .cover_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| {
            img(path.clone())
                .object_fit(ObjectFit::Cover)
                .size_full()
                .into_any_element()
        })
        .unwrap_or_else(|| {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size_full()
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Clapperboard).with_size(Size::Large))
                .into_any_element()
        });

    let cover_box = div()
        .w(px(460.))
        .max_w_full()
        .aspect_ratio(16.0 / 9.0)
        .rounded(cx.theme().radius_lg)
        .overflow_hidden()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .child(cover);

    let mut creator_row = div().flex().items_center().gap_2();
    creator_row = creator_row.child(
        Avatar::new().name(item.creator.clone()).when_some(
            item.avatar_path
                .as_ref()
                .filter(|path| path.exists())
                .cloned(),
            |avatar, path| avatar.src(path),
        ),
    );
    creator_row = creator_row
        .child(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .child(if item.creator.is_empty() {
                    "Unknown creator".to_string()
                } else {
                    item.creator.clone()
                }),
        )
        .child(components::platform_tag(item.platform, cx));
    if item.status != MediaStatus::Ready {
        creator_row = creator_row.child(components::tone_tag(
            item.status.label(),
            if item.status == MediaStatus::Missing {
                Tone::Warning
            } else {
                Tone::Neutral
            },
            cx,
        ));
    }

    let mut meta = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_3()
        .text_xs()
        .text_color(cx.theme().muted_foreground);
    meta = meta.child(meta_item(IconName::Calendar, item.published_label()));
    if item.duration_secs.is_some() {
        meta = meta.child(meta_item(IconName::Clock, item.duration_label()));
    }
    if let Some(views) = item.view_count {
        meta = meta.child(meta_item(
            IconName::Eye,
            format!("{} views", components::compact_count(views)),
        ));
    }
    if let Some(likes) = item.like_count {
        meta = meta.child(meta_item(
            IconName::ThumbsUp,
            components::compact_count(likes),
        ));
    }

    let has_video = item.has_video();
    let video_path = item.video_path.clone();
    let fallback_url = item.source_url.clone();
    let play = Button::new("detail-play")
        .primary()
        .icon(IconName::Play)
        .label(if has_video { "Play" } else { "Open source" })
        .on_click(
            move |_, _, cx| match video_path.as_ref().filter(|path| path.exists()) {
                Some(path) => cx.open_with_system(path),
                None => cx.open_url(&fallback_url),
            },
        );

    let favorite = Button::new("detail-favorite")
        .outline()
        .icon(if item.favorite {
            IconName::Heart
        } else {
            IconName::HeartOff
        })
        .label(if item.favorite {
            "Favorited"
        } else {
            "Favorite"
        })
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_favorite(id, cx)));

    let watched = Button::new("detail-watched")
        .outline()
        .icon(if item.watched {
            IconName::CircleCheck
        } else {
            IconName::Circle
        })
        .label(if item.watched {
            "Watched"
        } else {
            "Mark watched"
        })
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_watched(id, cx)));

    let folder = item.folder_path.clone();
    let reveal = Button::new("detail-reveal")
        .outline()
        .icon(IconName::FolderOpen)
        .tooltip("Show in file manager")
        .on_click(move |_, _, cx| cx.reveal_path(&folder));

    let entity = cx.entity();
    let remove = Button::new("detail-remove")
        .ghost()
        .icon(IconName::Trash)
        .tooltip("Remove from library")
        .on_click(move |_, window, cx| {
            let entity = entity.clone();
            window.open_alert_dialog(cx, move |alert, _, _cx| {
                let entity = entity.clone();
                alert
                    .title("Remove from library?")
                    .description(
                        "The catalogue entry is removed. Files in the library folder stay on disk.",
                    )
                    .confirm()
                    .ok_text("Remove")
                    .ok_variant(ButtonVariant::Danger)
                    .on_ok(move |_, _, cx| {
                        entity.update(cx, |app, cx| app.delete_media(id, cx));
                        true
                    })
            });
        });

    let details = div()
        .flex()
        .flex_col()
        .gap_4()
        .flex_1()
        .min_w(px(280.))
        .child(creator_row)
        .child(
            div()
                .text_2xl()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(item.title.clone()),
        )
        .child(meta)
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .pt_2()
                .child(play)
                .child(favorite)
                .child(watched)
                .child(reveal)
                .child(remove),
        );

    div()
        .flex()
        .flex_wrap()
        .items_start()
        .gap_6()
        .w_full()
        .child(cover_box)
        .child(details)
        .into_any_element()
}

fn meta_item(icon: IconName, text: impl Into<gpui_kit::SharedString>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).with_size(Size::XSmall))
        .child(div().child(text.into()))
        .into_any_element()
}

fn description_panel(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let description = if item.description.trim().is_empty() {
        "No description was captured for this video.".to_string()
    } else {
        item.description.clone()
    };
    components::panel(
        vec![
            components::section_heading("Description", None, None, cx),
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description)
                .into_any_element(),
        ],
        cx,
    )
}

fn files_panel(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let mut rows = vec![components::section_heading("Files", None, None, cx)];

    if let Some(path) = item.video_path.as_ref().filter(|path| path.exists()) {
        rows.push(file_row_with_actions(
            IconName::Clapperboard,
            "Video",
            path,
            "Open",
            cx,
        ));
    } else {
        rows.push(components::file_row(
            IconName::VideoOff,
            "Video",
            "The video file is missing from the archive folder",
            None,
            cx,
        ));
    }

    if let Some(path) = item.cover_path.as_ref().filter(|path| path.exists()) {
        rows.push(file_row_with_actions(
            IconName::Image,
            "Poster",
            path,
            "Open",
            cx,
        ));
    }
    if let Some(path) = item.avatar_path.as_ref().filter(|path| path.exists()) {
        rows.push(file_row_with_actions(
            IconName::UserRound,
            "Creator avatar",
            path,
            "Open",
            cx,
        ));
    }
    if let Some(path) = item.nfo_path.as_ref().filter(|path| path.exists()) {
        rows.push(file_row_with_actions(
            IconName::FileText,
            "Metadata",
            path,
            "Open",
            cx,
        ));
    }
    if !item.subtitle_paths.is_empty() {
        let names = item
            .subtitle_paths
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .collect::<Vec<_>>()
            .join(" · ");
        rows.push(components::file_row(
            IconName::Captions,
            "Subtitles",
            names,
            None,
            cx,
        ));
    }
    if let Some(path) = item.danmaku_path.as_ref().filter(|path| path.exists()) {
        rows.push(file_row_with_actions(
            IconName::MessageSquare,
            "Danmaku",
            path,
            "Open",
            cx,
        ));
    }

    components::panel(rows, cx)
}

fn file_row_with_actions(
    icon: IconName,
    label: &'static str,
    path: &Path,
    open_label: &'static str,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let open_path = path.to_path_buf();
    let reveal_path = path.to_path_buf();
    let actions = div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            Button::new(gpui_kit::SharedString::from(format!("open-{label}")))
                .ghost()
                .label(open_label)
                .on_click(move |_, _, cx| cx.open_with_system(&open_path)),
        )
        .child(
            Button::new(gpui_kit::SharedString::from(format!("reveal-{label}")))
                .ghost()
                .icon(IconName::FolderOpen)
                .tooltip("Show in file manager")
                .on_click(move |_, _, cx| cx.reveal_path(&reveal_path)),
        );
    components::file_row(
        icon,
        label,
        path.to_string_lossy().to_string(),
        Some(actions.into_any_element()),
        cx,
    )
}

fn comments_panel(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let now = now_timestamp();
    let mut rows = vec![components::section_heading(
        "Comments",
        Some(format!("{} captured", item.comments.len()).into()),
        None,
        cx,
    )];
    for comment in item.comments.iter().take(30) {
        rows.push(comment_row(comment, now, cx));
    }
    components::panel(rows, cx)
}

fn comment_row(comment: &Comment, now: i64, cx: &mut Context<VideleumApp>) -> AnyElement {
    let initial = comment
        .author
        .trim()
        .chars()
        .next()
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let mut header = div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.))
                .rounded_full()
                .bg(cx.theme().secondary)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(initial),
        )
        .child(
            div()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .child(comment.author.clone()),
        );
    if let Some(created_at) = comment.created_at {
        header = header.child(components::muted(format_relative_time(created_at, now), cx));
    }
    if let Some(likes) = comment.likes {
        header = header.child(components::muted(
            format!("{} likes", components::compact_count(likes)),
            cx,
        ));
    }

    div()
        .flex()
        .flex_col()
        .gap_2()
        .py_2()
        .child(header)
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(comment.text.clone()),
        )
        .into_any_element()
}

fn details_panel(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let username = item
        .creator_id
        .clone()
        .unwrap_or_else(|| "Unknown".to_string());
    components::panel(
        vec![
            components::section_heading("Details", None, None, cx),
            div()
                .grid()
                .grid_cols(2)
                .gap_4()
                .w_full()
                .child(components::spec("Platform", item.platform.label(), cx))
                .child(components::spec("Source ID", item.source_id.clone(), cx))
                .child(components::spec("Creator ID", username, cx))
                .child(components::spec("Published", item.published_label(), cx))
                .child(components::spec("Added", format_date(item.added_at), cx))
                .child(components::spec("Size", format_bytes(item.file_size), cx))
                .child(components::spec(
                    "Folder",
                    item.folder_path.to_string_lossy().to_string(),
                    cx,
                ))
                .child(components::spec("Source URL", item.source_url.clone(), cx))
                .into_any_element(),
        ],
        cx,
    )
}
