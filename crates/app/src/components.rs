//! Small presentational building blocks shared by the application views.
//!
//! Components here are stateless: they take already-resolved data and return
//! elements. Interaction is always wired by the owning view.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, tag::Tag};
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _,
    div, px,
};

use videleum_core::Platform;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
}

/// A page header with a title, optional supporting line and right-aligned
/// actions. It owns the standard page inset so every view lines up.
pub fn page_header(
    title: impl Into<SharedString>,
    subtitle: Option<SharedString>,
    actions: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    let mut heading = div().flex().flex_col().gap_1();
    heading = heading.child(
        div()
            .text_2xl()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.into()),
    );
    if let Some(subtitle) = subtitle {
        heading = heading.child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(subtitle),
        );
    }

    div()
        .flex_shrink_0()
        .px_6()
        .pt_5()
        .pb_4()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap_6()
                .child(heading)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_shrink_0()
                        .children(actions),
                ),
        )
        .into_any_element()
}

/// The scrollable region under a page header.
pub fn page_body() -> gpui_kit::Div {
    div().flex().flex_col().flex_1().min_h_0().w_full()
}

pub fn section_heading(
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .w_full();
    let mut label = div().flex().items_baseline().gap_2();
    label = label.child(
        div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.into()),
    );
    if let Some(detail) = detail {
        label = label.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(detail),
        );
    }
    row = row.child(label);
    if let Some(action) = action {
        row = row.child(action);
    }
    row.into_any_element()
}

/// A bordered surface for grouped settings or metadata.
pub fn panel(children: impl IntoIterator<Item = AnyElement>, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap_4()
        .p_5()
        .rounded(cx.theme().radius_lg)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .children(children)
        .into_any_element()
}

/// A label/description pair with a control on the trailing edge.
pub fn setting_row(
    label: impl IntoElement,
    description: Option<SharedString>,
    control: AnyElement,
    cx: &App,
) -> AnyElement {
    let mut text = div().flex().flex_col().gap_1().flex_1().min_w_0();
    text = text.child(div().text_sm().child(label));
    if let Some(description) = description {
        text = text.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description),
        );
    }
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_6()
        .w_full()
        .child(text)
        .child(div().flex_shrink_0().child(control))
        .into_any_element()
}

fn tone_colors(tone: Tone, cx: &App) -> (Hsla, Hsla, Hsla) {
    let theme = cx.theme();
    match tone {
        Tone::Neutral => (
            theme.muted_foreground,
            theme.background.opacity(0.0),
            theme.border,
        ),
        Tone::Accent => (
            theme.primary,
            theme.primary.opacity(0.12),
            theme.primary.opacity(0.3),
        ),
        Tone::Success => (
            theme.success,
            theme.success.opacity(0.12),
            theme.success.opacity(0.3),
        ),
        Tone::Warning => (
            theme.warning,
            theme.warning.opacity(0.14),
            theme.warning.opacity(0.32),
        ),
        Tone::Danger => (
            theme.danger,
            theme.danger.opacity(0.12),
            theme.danger.opacity(0.3),
        ),
    }
}

/// A small status chip.
pub fn tone_tag(label: impl Into<SharedString>, tone: Tone, cx: &App) -> AnyElement {
    let (foreground, background, border) = tone_colors(tone, cx);
    Tag::custom(background, foreground, border)
        .rounded_full()
        .child(label.into())
        .into_any_element()
}

pub fn platform_tag(platform: Platform, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = match platform {
        Platform::YouTube => theme.red,
        Platform::Bilibili => theme.cyan,
        Platform::Twitch => theme.magenta,
    };
    Tag::custom(color.opacity(0.12), color, color.opacity(0.3))
        .rounded_full()
        .child(platform.label())
        .into_any_element()
}

/// A quiet informational strip used for startup or library problems.
pub fn banner(
    icon: IconName,
    message: impl Into<SharedString>,
    tone: Tone,
    cx: &App,
) -> AnyElement {
    let (foreground, background, border) = tone_colors(tone, cx);
    div()
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .px_4()
        .py_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(border)
        .bg(background)
        .text_color(foreground)
        .child(Icon::new(icon).with_size(Size::Medium))
        .child(div().text_sm().flex_1().min_w_0().child(message.into()))
        .into_any_element()
}

/// A centered empty state with an optional primary action.
pub fn empty_state(
    icon: IconName,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let mut content = div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .max_w(px(420.))
        .text_center()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(56.))
                .rounded_full()
                .bg(cx.theme().secondary)
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(icon).with_size(Size::Large)),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
        );
    if let Some(action) = action {
        content = content.child(div().mt_2().child(action));
    }

    div()
        .flex()
        .flex_1()
        .w_full()
        .min_h_0()
        .items_center()
        .justify_center()
        .p_8()
        .child(content)
        .into_any_element()
}

/// A label with a value, stacked for metadata lists.
pub fn spec(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label.into()),
        )
        .child(div().text_sm().truncate().child(value.into()))
        .into_any_element()
}

/// A muted file row with a value and trailing action.
pub fn file_row(
    icon: IconName,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap_3()
        .w_full()
        .py_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded(cx.theme().radius)
                .bg(cx.theme().background)
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(icon).with_size(Size::Small)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().child(label.into()))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(value.into()),
                ),
        );
    if let Some(action) = action {
        row = row.child(action);
    }
    row.into_any_element()
}

pub fn muted(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

/// A compact human-readable count, e.g. `1.2M`.
pub fn compact_count(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}K", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// A full-width divider that matches the current theme.
pub fn divider(cx: &App) -> AnyElement {
    div()
        .h(px(1.))
        .w_full()
        .bg(cx.theme().border)
        .into_any_element()
}
