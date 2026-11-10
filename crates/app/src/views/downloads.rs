//! Download queue, failures and history.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, Size,
    button::{Button, ButtonVariants as _},
    progress::Progress,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    AnyElement, Context, IntoElement, ObjectFit, ParentElement as _, Styled as _, StyledImage as _,
    Window, div, img, px,
};

use videleum_core::model::{
    format_bytes, format_relative_time, format_seconds_short, format_speed,
};
use videleum_core::{DownloadJob, DownloadState, now_timestamp};

use crate::app::{AddIntent, VideleumApp};
use crate::components::{self, Tone};

pub fn render(
    app: &mut VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let now = now_timestamp();
    let active: Vec<DownloadJob> = app
        .jobs
        .iter()
        .filter(|job| job.state.is_active())
        .cloned()
        .collect();
    let failed: Vec<DownloadJob> = app
        .jobs
        .iter()
        .filter(|job| job.state == DownloadState::Failed || job.state == DownloadState::Canceled)
        .cloned()
        .collect();
    let finished: Vec<DownloadJob> = app
        .jobs
        .iter()
        .filter(|job| job.state == DownloadState::Completed)
        .cloned()
        .collect();

    let subtitle = if active.is_empty() {
        "Nothing downloading right now".to_string()
    } else {
        format!(
            "{} active · {} queued",
            active
                .iter()
                .filter(|job| job.state != DownloadState::Queued)
                .count(),
            active
                .iter()
                .filter(|job| job.state == DownloadState::Queued)
                .count()
        )
    };

    let add = Button::new("downloads-add")
        .primary()
        .icon(IconName::Plus)
        .label("Add media")
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_add_media(AddIntent::Video, window, cx);
        }));
    let clear = Button::new("downloads-clear")
        .outline()
        .icon(IconName::Trash)
        .label("Clear completed")
        .disabled(finished.is_empty())
        .on_click(cx.listener(|this, _, _, cx| this.clear_finished_jobs(cx)));

    let header = components::page_header(
        "Downloads",
        Some(subtitle.into()),
        vec![clear.into_any_element(), add.into_any_element()],
        cx,
    );

    let mut body = components::page_body().gap_6().p_6().overflow_y_scrollbar();

    if active.is_empty() && failed.is_empty() && finished.is_empty() {
        let action = Button::new("downloads-empty-add")
            .primary()
            .icon(IconName::Plus)
            .label("Queue a download")
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_add_media(AddIntent::Video, window, cx);
            }));
        body = body.child(components::empty_state(
            IconName::Download,
            "No downloads yet",
            "Videos you add and new uploads from subscriptions will appear here while they download.",
            Some(action.into_any_element()),
            cx,
        ));
    } else {
        if !active.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .child(components::section_heading(
                        "Downloading",
                        Some(format!("{} in queue", active.len()).into()),
                        None,
                        cx,
                    ))
                    .children(
                        active
                            .iter()
                            .map(|job| active_row(app, job, cx))
                            .collect::<Vec<_>>(),
                    ),
            );
        }

        if !failed.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .child(components::section_heading(
                        "Needs attention",
                        Some(format!("{} failed", failed.len()).into()),
                        None,
                        cx,
                    ))
                    .children(
                        failed
                            .iter()
                            .map(|job| failed_row(job, cx))
                            .collect::<Vec<_>>(),
                    ),
            );
        }

        if !finished.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w_full()
                    .child(components::section_heading(
                        "History",
                        Some(format!("{} completed", finished.len()).into()),
                        None,
                        cx,
                    ))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .rounded(cx.theme().radius_lg)
                            .border_1()
                            .border_color(cx.theme().border)
                            .children(
                                finished
                                    .iter()
                                    .map(|job| finished_row(job, now, cx))
                                    .collect::<Vec<_>>(),
                            ),
                    ),
            );
        }
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(header)
        .child(body)
        .into_any_element()
}

fn active_row(app: &VideleumApp, job: &DownloadJob, cx: &mut Context<VideleumApp>) -> AnyElement {
    let id = job.id;
    let cancellable = app.active_jobs.contains_key(&id);
    let percentage = (job.progress * 100.0).clamp(0.0, 100.0);
    let thumbnail = job
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
                .child(Icon::new(IconName::Clapperboard).with_size(Size::Medium))
                .into_any_element()
        });

    let mut meta = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(components::platform_tag(job.platform, cx));
    if !job.creator.is_empty() {
        meta = meta.child(job.creator.clone());
    }
    if !job.stage.is_empty() {
        meta = meta.child(job.stage.clone());
    } else {
        meta = meta.child(job.state.label());
    }

    let mut stats = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground);
    if let Some(total) = job.total_bytes {
        stats = stats.child(format!(
            "{} of {}",
            format_bytes(job.downloaded_bytes),
            format_bytes(total)
        ));
    } else if job.downloaded_bytes > 0 {
        stats = stats.child(format_bytes(job.downloaded_bytes));
    }
    if let Some(speed) = job.speed_bps.filter(|speed| *speed > 0) {
        stats = stats.child(format_speed(speed));
    }
    if let Some(eta) = job.eta_secs.filter(|eta| *eta > 0) {
        stats = stats.child(format!("{} left", format_seconds_short(eta)));
    }

    let action = if cancellable {
        Button::new(gpui_kit::SharedString::from(format!("cancel-{id}")))
            .outline()
            .icon(IconName::X)
            .label("Cancel")
            .on_click(cx.listener(move |this, _, _, cx| this.cancel_job(id, cx)))
    } else {
        Button::new(gpui_kit::SharedString::from(format!("waiting-{id}")))
            .outline()
            .label("Waiting")
            .disabled(true)
    };

    div()
        .flex()
        .items_center()
        .gap_4()
        .w_full()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(
            div()
                .w(px(112.))
                .aspect_ratio(16.0 / 9.0)
                .rounded(cx.theme().radius)
                .overflow_hidden()
                .bg(cx.theme().background)
                .child(thumbnail),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui_kit::FontWeight::MEDIUM)
                                .truncate()
                                .child(job.display_title()),
                        )
                        .child(components::tone_tag(
                            format!("{percentage:.0}%"),
                            Tone::Accent,
                            cx,
                        )),
                )
                .child(meta)
                .child(
                    Progress::new(gpui_kit::SharedString::from(format!("progress-{id}")))
                        .value(percentage)
                        .w_full(),
                )
                .child(stats),
        )
        .child(action)
        .into_any_element()
}

fn failed_row(job: &DownloadJob, cx: &mut Context<VideleumApp>) -> AnyElement {
    let id = job.id;
    let error = job
        .error
        .clone()
        .unwrap_or_else(|| "The download stopped unexpectedly.".to_string());
    let retry = Button::new(gpui_kit::SharedString::from(format!("retry-{id}")))
        .outline()
        .icon(IconName::RotateCw)
        .label("Retry")
        .on_click(cx.listener(move |this, _, window, cx| this.retry_job(id, window, cx)));
    let delete = Button::new(gpui_kit::SharedString::from(format!("dismiss-{id}")))
        .ghost()
        .icon(IconName::Trash)
        .tooltip("Remove from history")
        .on_click(cx.listener(move |this, _, _, cx| this.delete_job(id, cx)));

    div()
        .flex()
        .items_start()
        .gap_4()
        .w_full()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().danger.opacity(0.3))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(32.))
                .rounded_full()
                .bg(cx.theme().danger.opacity(0.12))
                .text_color(cx.theme().danger)
                .child(Icon::new(IconName::TriangleAlert).with_size(Size::Small)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .child(job.display_title()),
                )
                .child(div().text_xs().text_color(cx.theme().danger).child(error))
                .child(components::muted(
                    if job.state == DownloadState::Canceled {
                        "Canceled".to_string()
                    } else {
                        "Failed".to_string()
                    },
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(retry)
                .child(delete),
        )
        .into_any_element()
}

fn finished_row(job: &DownloadJob, now: i64, cx: &mut Context<VideleumApp>) -> AnyElement {
    let id = job.id;
    let media_id = job.media_id;
    let title = job.display_title();
    let creator = job.creator.clone();
    let platform = job.platform;
    let updated = format_relative_time(job.updated_at, now);

    let mut row = div()
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded_full()
                .bg(cx.theme().success.opacity(0.12))
                .text_color(cx.theme().success)
                .child(Icon::new(IconName::CircleCheck).with_size(Size::Small)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().truncate().child(title))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(components::platform_tag(platform, cx))
                        .child(if creator.is_empty() {
                            "Unknown creator".to_string()
                        } else {
                            creator
                        })
                        .child(format!("· {updated}")),
                ),
        );

    if let Some(media_id) = media_id {
        row = row.child(
            Button::new(gpui_kit::SharedString::from(format!("open-media-{id}")))
                .ghost()
                .icon(IconName::Play)
                .tooltip("Open in library")
                .on_click(cx.listener(move |this, _, _, cx| this.open_media(media_id, cx))),
        );
    }
    row = row.child(
        Button::new(gpui_kit::SharedString::from(format!("delete-{id}")))
            .ghost()
            .icon(IconName::Trash)
            .tooltip("Remove from history")
            .on_click(cx.listener(move |this, _, _, cx| this.delete_job(id, cx))),
    );

    row.into_any_element()
}
