//! The poster wall: browse, search, filter and open the library.

use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, Sizable as _,
    button::{Button, ButtonGroup, ButtonVariants as _},
    input::Input,
    scroll::ScrollableElement as _,
};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window, div, img, px,
};

use videleum_core::{MediaItem, MediaStatus, Platform};

use crate::app::{AddIntent, Collection, SortKey, VideleumApp};
use crate::components::{self, Tone};

pub fn render(
    app: &mut VideleumApp,
    window: &mut Window,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let filtered = app.filtered_media();
    let shown = filtered.len();
    let total = app.media.len();
    let subtitle = if total == 0 {
        "Your personal video archive".to_string()
    } else if shown == total {
        format!("{} {}", total, nouns(total))
    } else {
        format!("{shown} of {total} {}", nouns(total))
    };

    let add = Button::new("library-add")
        .primary()
        .icon(IconName::Plus)
        .label("Add media")
        .on_click(cx.listener(|this, _, window, cx| {
            this.open_add_media(AddIntent::Auto, window, cx);
        }));
    let refresh = Button::new("library-refresh")
        .outline()
        .icon(IconName::RefreshCw)
        .tooltip("Scan the library folder for new or missing files")
        .on_click(cx.listener(|this, _, window, cx| this.rescan(window, cx)));

    let header = components::page_header(
        "Library",
        Some(subtitle.into()),
        vec![refresh.into_any_element(), add.into_any_element()],
        cx,
    );

    let has_filters = app.has_filters();
    let toolbar = render_toolbar(app, has_filters, cx);

    let mut body = components::page_body().gap_4().p_6().overflow_y_scrollbar();

    if let Some(error) = app.startup_error.as_deref() {
        body = body.child(components::banner(
            IconName::CircleAlert,
            error.to_string(),
            Tone::Warning,
            cx,
        ));
    }
    if let Some(error) = app.library_error.as_deref() {
        body = body.child(components::banner(
            IconName::CircleAlert,
            format!("The library catalogue couldn't be read: {error}"),
            Tone::Danger,
            cx,
        ));
    }

    if total == 0 {
        let action = Button::new("library-empty-add")
            .primary()
            .icon(IconName::Plus)
            .label("Add your first video")
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_add_media(AddIntent::Auto, window, cx);
            }));
        body = body.child(components::empty_state(
            IconName::Clapperboard,
            "Your library is empty",
            "Paste a YouTube, Bilibili, or Twitch link to download a video, or follow a creator to build the archive over time.",
            Some(action.into_any_element()),
            cx,
        ));
    } else if shown == 0 {
        let action = Button::new("library-empty-clear")
            .outline()
            .label("Clear filters")
            .on_click(cx.listener(|this, _, window, cx| this.clear_filters(window, cx)));
        body = body.child(components::empty_state(
            IconName::Search,
            "No videos match",
            "Try a different search, platform, or collection filter.",
            Some(action.into_any_element()),
            cx,
        ));
    } else {
        let columns = columns_for(window);
        let cards = filtered
            .iter()
            .map(|item| poster_card(item, cx))
            .collect::<Vec<_>>();
        body = body.child(
            div()
                .grid()
                .grid_cols(columns)
                .gap_4()
                .w_full()
                .children(cards),
        );
    }

    div()
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .child(header)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .px_6()
                .pt_4()
                .child(toolbar),
        )
        .child(body)
        .into_any_element()
}

fn nouns(count: usize) -> &'static str {
    if count == 1 { "video" } else { "videos" }
}

fn render_toolbar(
    app: &VideleumApp,
    has_filters: bool,
    cx: &mut Context<VideleumApp>,
) -> AnyElement {
    let search = Input::new(&app.search)
        .cleanable(true)
        .prefix(
            Icon::new(IconName::Search)
                .with_size(gpui_kit::component::Size::Small)
                .text_color(cx.theme().muted_foreground),
        )
        .w(px(300.));

    let selected_platform = app
        .platform_filter
        .map(|platform| {
            Platform::ALL
                .iter()
                .position(|candidate| *candidate == platform)
                .map(|position| position + 1)
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let platforms = ButtonGroup::new("library-platforms")
        .compact()
        .outline()
        .child(
            Button::new("platform-all")
                .label("All")
                .selected(selected_platform == 0),
        )
        .child(
            Button::new("platform-youtube")
                .label("YouTube")
                .selected(selected_platform == 1),
        )
        .child(
            Button::new("platform-bilibili")
                .label("Bilibili")
                .selected(selected_platform == 2),
        )
        .child(
            Button::new("platform-twitch")
                .label("Twitch")
                .selected(selected_platform == 3),
        )
        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
            let platform = clicks
                .first()
                .and_then(|index| Platform::ALL.get(index.saturating_sub(1)).copied());
            this.set_platform_filter(platform, cx);
        }));

    let collection_index = match app.collection {
        Collection::All => 0,
        Collection::Favorites => 1,
        Collection::Unwatched => 2,
    };
    let collections = ButtonGroup::new("library-collections")
        .compact()
        .outline()
        .child(
            Button::new("collection-all")
                .label("All")
                .selected(collection_index == 0),
        )
        .child(
            Button::new("collection-favorites")
                .icon(IconName::Heart)
                .label("Favorites")
                .selected(collection_index == 1),
        )
        .child(
            Button::new("collection-unwatched")
                .label("Unwatched")
                .selected(collection_index == 2),
        )
        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
            let collection = match clicks.first().copied().unwrap_or(0) {
                1 => Collection::Favorites,
                2 => Collection::Unwatched,
                _ => Collection::All,
            };
            this.set_collection(collection, cx);
        }));

    let sort_index = match app.sort {
        SortKey::Recent => 0,
        SortKey::Published => 1,
        SortKey::Title => 2,
        SortKey::Size => 3,
    };
    let sorts = ButtonGroup::new("library-sorts")
        .compact()
        .outline()
        .child(
            Button::new("sort-added")
                .label("Added")
                .selected(sort_index == 0),
        )
        .child(
            Button::new("sort-published")
                .label("Published")
                .selected(sort_index == 1),
        )
        .child(
            Button::new("sort-title")
                .label("Title")
                .selected(sort_index == 2),
        )
        .child(
            Button::new("sort-size")
                .label("Size")
                .selected(sort_index == 3),
        )
        .on_click(cx.listener(|this, clicks: &Vec<usize>, _, cx| {
            let sort = match clicks.first().copied().unwrap_or(0) {
                1 => SortKey::Published,
                2 => SortKey::Title,
                3 => SortKey::Size,
                _ => SortKey::Recent,
            };
            this.set_sort(sort, cx);
        }));

    let mut filters = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .child(platforms)
        .child(div().h(px(20.)).w(px(1.)).bg(cx.theme().border).mx_1())
        .child(collections);
    if has_filters {
        filters = filters.child(
            Button::new("library-clear")
                .ghost()
                .icon(IconName::X)
                .label("Clear")
                .on_click(cx.listener(|this, _, window, cx| this.clear_filters(window, cx))),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap_3()
        .w_full()
        .child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_3()
                .w_full()
                .child(search)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .flex_1()
                        .justify_end()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Sort"),
                        )
                        .child(sorts),
                ),
        )
        .child(filters)
        .into_any_element()
}

fn poster_card(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let id = item.id;
    let hover = cx.theme().secondary;
    let cover = cover_element(item, cx);

    let mut title_row = div().flex().items_start().gap_2().w_full();
    title_row = title_row
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .line_clamp(2)
                .child(item.title.clone()),
        )
        .child(components::platform_tag(item.platform, cx));

    let meta = div()
        .flex()
        .items_center()
        .gap_2()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(item.creator.clone()),
        )
        .child(item.published_label());

    let mut card = div()
        .id(("poster", id as usize))
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .min_w_0()
        .p_2()
        .rounded(cx.theme().radius_lg)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(cx.listener(move |this, _, _, cx| this.open_media(id, cx)))
        .child(cover)
        .child(title_row)
        .child(meta);

    if !item.tags.is_empty() {
        let tags = item
            .tags
            .iter()
            .take(3)
            .map(|tag| components::tone_tag(tag.clone(), Tone::Neutral, cx))
            .collect::<Vec<_>>();
        card = card.child(div().flex().flex_wrap().gap_1().w_full().children(tags));
    }

    if item.status == MediaStatus::Missing {
        card = card.child(div().w_full().child(components::tone_tag(
            "File missing",
            Tone::Warning,
            cx,
        )));
    }

    card.into_any_element()
}

fn cover_element(item: &MediaItem, cx: &mut Context<VideleumApp>) -> AnyElement {
    let image = item
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
                .child(
                    Icon::new(IconName::Clapperboard).with_size(gpui_kit::component::Size::Large),
                )
                .into_any_element()
        });

    let mut overlays = div()
        .flex()
        .items_center()
        .justify_between()
        .absolute()
        .top_2()
        .left_2()
        .right_2();

    overlays = overlays.child(components::platform_tag(item.platform, cx));
    if item.favorite {
        overlays = overlays.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.))
                .rounded_full()
                .bg(cx.theme().background.opacity(0.9))
                .text_color(cx.theme().primary)
                .child(Icon::new(IconName::Heart).with_size(gpui_kit::component::Size::Small)),
        );
    }

    let duration = item.duration_secs.map(|seconds| {
        div()
            .absolute()
            .bottom_2()
            .right_2()
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(cx.theme().background.opacity(0.88))
            .text_xs()
            .text_color(cx.theme().foreground)
            .child(videleum_core::format_duration(seconds))
    });

    div()
        .relative()
        .w_full()
        .aspect_ratio(16.0 / 9.0)
        .rounded(cx.theme().radius)
        .overflow_hidden()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .child(image)
        .child(overlays)
        .children(duration)
        .into_any_element()
}

fn columns_for(window: &Window) -> u16 {
    let viewport = window.viewport_size().width.as_f32();
    let content = (viewport - 255.0 - 48.0).max(320.0);
    (((content + 16.0) / 244.0).floor() as u16).clamp(2, 6)
}
