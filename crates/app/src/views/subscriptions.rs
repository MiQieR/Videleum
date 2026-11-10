//! Followed channels, playlists and favorites lists.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _,
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    scroll::ScrollableElement as _,
    switch::Switch,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div};

use videleum_core::model::format_relative_time;
use videleum_core::{Subscription, now_timestamp};

use crate::app::{AddIntent, VideleumApp};
use crate::components::{self, Tone};

pub fn render(
    app: &mut VideleumApp,
    _window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let now = now_timestamp();
    let count = app.subscriptions.len();
    let auto = app
        .subscriptions
        .iter()
        .filter(|subscription| subscription.auto_download)
        .count();
    let subtitle = if count == 0 {
        "Follow creators to collect new videos automatically".to_string()
    } else {
        format!(
            "{count} followed · {auto} auto-download · checks every {} minutes",
            app.settings.refresh_interval_minutes
        )
    };

    let refresh = Button::new("subscriptions-refresh")
        .outline()
        .icon(IconName::RefreshCw)
        .label("Refresh all")
        .loading(app.refreshing_all)
        .disabled(count == 0 || app.refreshing_all)
        .on_click(cx.listener(|this, _, _, cx| this.refresh_all_subscriptions(cx)));
    let add = Button::new("subscriptions-add")
        .primary()
        .icon(IconName::Plus)
        .label("Add subscription")
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_add_media(AddIntent::Subscription, window, cx);
        }));

    let header = components::page_header(
        "Subscriptions",
        Some(subtitle.into()),
        vec![refresh.into_any_element(), add.into_any_element()],
        cx,
    );

    let mut body = components::page_body().gap_4().p_6().overflow_y_scrollbar();

    if !app.settings.auto_refresh_subscriptions {
        body = body.child(components::banner(
            IconName::Info,
            "Automatic checks are off. Use Refresh all, or enable automatic checks in Settings.",
            Tone::Neutral,
            cx,
        ));
    }

    if count == 0 {
        body = body.child(components::empty_state(
            IconName::Rss,
            "No subscriptions yet",
            "Follow a YouTube channel, a Bilibili creator, a playlist, or a favorites list. Videleum remembers what it has already archived.",
            None,
            cx,
        ));
    } else {
        body = body.child(
            div().flex().flex_col().gap_3().w_full().children(
                app.subscriptions
                    .iter()
                    .map(|subscription| subscription_row(app, subscription, now, cx))
                    .collect::<Vec<_>>(),
            ),
        );
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

fn subscription_row(
    app: &VideleumApp,
    subscription: &Subscription,
    now: i64,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let id = subscription.id;
    let checking = app.checking_subscriptions.contains(&id);
    let error = app.subscription_errors.get(&id).cloned();

    let avatar = Avatar::new().name(subscription.title.clone()).when_some(
        subscription
            .avatar_path
            .as_ref()
            .filter(|path| path.exists())
            .cloned(),
        |avatar, path| avatar.src(path),
    );

    let last_checked = subscription
        .last_checked_at
        .map(|timestamp| format_relative_time(timestamp, now))
        .unwrap_or_else(|| "Never checked".to_string());

    let mut meta = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(components::platform_tag(subscription.platform, cx))
        .child(subscription.kind.label())
        .child(format!("· Checked {last_checked}"));
    if !subscription.last_status.is_empty() {
        meta = meta.child(format!("· {}", subscription.last_status));
    }

    let auto_download = Switch::new(gpui_kit::SharedString::from(format!(
        "subscription-auto-{id}"
    )))
    .checked(subscription.auto_download)
    .label("Auto-download")
    .on_change(cx.listener(move |this, checked: &bool, _, cx| {
        this.set_subscription_auto_download(id, *checked, cx);
    }));

    let check = Button::new(gpui_kit::SharedString::from(format!("check-{id}")))
        .outline()
        .icon(IconName::RefreshCw)
        .label("Check now")
        .loading(checking)
        .disabled(checking)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.refresh_subscription(id, cx);
        }));

    let delete = Button::new(gpui_kit::SharedString::from(format!("unfollow-{id}")))
        .ghost()
        .icon(IconName::Trash)
        .tooltip("Unfollow")
        .on_click(cx.listener(move |this, _, _, cx| {
            this.delete_subscription(id, cx);
        }));

    div()
        .flex()
        .flex_col()
        .gap_3()
        .w_full()
        .p_4()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(avatar)
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
                                .truncate()
                                .child(subscription.title.clone()),
                        )
                        .child(meta),
                )
                .child(auto_download)
                .child(check)
                .child(delete),
        )
        .when_some(error, |this, error| {
            this.child(components::banner(
                IconName::CircleAlert,
                error,
                Tone::Danger,
                cx,
            ))
        })
        .into_any_element()
}
