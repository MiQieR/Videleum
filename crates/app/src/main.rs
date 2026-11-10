//! Videleum: a native personal media archive.

mod app;
mod assets;
mod components;
mod theme;
mod views;

use gpui_kit::component::TitleBar;
use gpui_kit::{AppContext as _, WindowOptions, px, size};

use crate::app::VideleumApp;
use crate::assets::VideleumAssets;

fn main() {
    gpui_kit::application()
        .with_assets(VideleumAssets)
        .run(|cx| {
            gpui_kit::init(cx);

            let options = WindowOptions {
                window_min_size: Some(size(px(960.), px(640.))),
                ..TitleBar::window_options()
            };

            if let Err(error) = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| VideleumApp::new(window, cx))
            }) {
                eprintln!("Videleum could not open its window: {error}");
            }
        });
}
