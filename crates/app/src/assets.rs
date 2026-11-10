//! Application asset source: the default GPUI Kit icon set plus Videleum's
//! embedded brand artwork.

use std::borrow::Cow;

use gpui_kit::assets::AllAssets;
use gpui_kit::{AssetSource, Result, SharedString};

pub struct VideleumAssets;

const BRAND_ASSETS: [(&str, &[u8]); 2] = [
    (
        "branding/logo.svg",
        include_bytes!("../../../assets/branding/videleum_logo_transparent.svg"),
    ),
    (
        "branding/logo-dark.png",
        include_bytes!("../../../assets/branding/videleum_logo_darkmode.png"),
    ),
];

impl AssetSource for VideleumAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = BRAND_ASSETS.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = AllAssets.list(path)?;
        paths.extend(
            BRAND_ASSETS
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        Ok(paths)
    }
}
