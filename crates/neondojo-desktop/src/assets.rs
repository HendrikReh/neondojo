//! Embedded SVG asset source for NeonDojo desktop UI.
//!
//! Serves NeonDojo's 61 bundled Lucide Outline action icons (`icons/*.svg`) and
//! brand mark (`neondojo-logo.svg`), falling back to `gpui_kit::assets::Assets`
//! for built-in component chrome.

use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::collections::BTreeSet;

/// Asset path for the text-free NeonDojo brand mark.
pub const LOGO_PATH: &str = "neondojo-logo.svg";

#[derive(RustEmbed)]
#[folder = "assets/"]
#[include = "*.svg"]
#[include = "icons/*.svg"]
struct NeonEmbeddedAssets;

/// Composite [`AssetSource`] serving NeonDojo's embedded SVGs first and
/// delegating remaining lookups to [`gpui_kit::assets::Assets`].
#[derive(Clone, Copy, Debug, Default)]
pub struct NeonAssets;

impl AssetSource for NeonAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(file) = NeonEmbeddedAssets::get(path) {
            return Ok(Some(file.data));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut entries = BTreeSet::<String>::new();
        for item in NeonEmbeddedAssets::iter() {
            if item.starts_with(path) {
                entries.insert(item.into_owned());
            }
        }
        if let Ok(kit_entries) = gpui_kit::assets::Assets.list(path) {
            for item in kit_entries {
                entries.insert(item.to_string());
            }
        }
        Ok(entries.into_iter().map(SharedString::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::icons::ActionIconName;

    #[test]
    fn loads_brand_logo_and_all_61_action_icons() {
        let assets = NeonAssets;
        let logo = assets
            .load(LOGO_PATH)
            .expect("logo load must not error")
            .expect("neondojo-logo.svg must be embedded");
        assert!(!logo.is_empty());
        assert!(std::str::from_utf8(&logo).unwrap().contains("<svg"));

        assert_eq!(ActionIconName::ALL.len(), 61);
        for &icon in ActionIconName::ALL {
            let path = icon.asset_path();
            let bytes = assets
                .load(path)
                .expect("icon load must not error")
                .unwrap_or_else(|| panic!("missing embedded SVG asset: {path}"));
            let svg = std::str::from_utf8(&bytes).expect("icon SVG must be valid UTF-8");
            assert!(svg.contains("<svg"), "invalid SVG for {path}");
        }

        let listed = assets.list("icons/").expect("list icons/");
        assert!(listed.len() >= 61);
    }
}
