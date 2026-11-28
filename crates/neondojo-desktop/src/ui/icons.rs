//! Bundled Lucide Outline action icons (`assets/icons/*.svg`) and brand logo.
//!
//! Covers all 61 semantic icons specified in `docs/design/action-icons-proposal.md`.

use crate::assets::{LOGO_PATH, NeonAssets};
use gpui_kit::{
    AssetSource, Hsla, Img, ObjectFit, RenderImage, Styled, StyledImage, Svg, img, px, svg,
};
use std::sync::{Arc, LazyLock};

/// Default square icon dimension for desktop action controls (`20px`).
pub const DEFAULT_ACTION_ICON_SIZE: f32 = 20.0;

/// Semantic identifier for every bundled NeonDojo action icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionIconName {
    Add,
    Analysis,
    Back,
    Bookmark,
    Cancel,
    Cards,
    Clear,
    Close,
    Collapse,
    Confirm,
    Copy,
    Database,
    Deck,
    Delete,
    Dice,
    Double,
    Drop,
    Duplicates,
    Edit,
    Expand,
    Export,
    Filter,
    First,
    Grip,
    Help,
    Hint,
    ImportFolder,
    Import,
    Last,
    Link,
    Loading,
    Merge,
    More,
    Move,
    NewGame,
    Next,
    Notes,
    Open,
    Pass,
    Pause,
    Play,
    Refresh,
    Reset,
    Resign,
    Resume,
    Roll,
    Save,
    Search,
    SelectAll,
    Settings,
    Similar,
    Solution,
    SortDown,
    SortUp,
    SystemDatabase,
    Tags,
    Take,
    Unbookmark,
    Undo,
    View,
    Warning,
}

impl ActionIconName {
    /// Complete catalogue of all 61 embedded action icons.
    pub const ALL: &'static [Self] = &[
        Self::Add,
        Self::Analysis,
        Self::Back,
        Self::Bookmark,
        Self::Cancel,
        Self::Cards,
        Self::Clear,
        Self::Close,
        Self::Collapse,
        Self::Confirm,
        Self::Copy,
        Self::Database,
        Self::Deck,
        Self::Delete,
        Self::Dice,
        Self::Double,
        Self::Drop,
        Self::Duplicates,
        Self::Edit,
        Self::Expand,
        Self::Export,
        Self::Filter,
        Self::First,
        Self::Grip,
        Self::Help,
        Self::Hint,
        Self::ImportFolder,
        Self::Import,
        Self::Last,
        Self::Link,
        Self::Loading,
        Self::Merge,
        Self::More,
        Self::Move,
        Self::NewGame,
        Self::Next,
        Self::Notes,
        Self::Open,
        Self::Pass,
        Self::Pause,
        Self::Play,
        Self::Refresh,
        Self::Reset,
        Self::Resign,
        Self::Resume,
        Self::Roll,
        Self::Save,
        Self::Search,
        Self::SelectAll,
        Self::Settings,
        Self::Similar,
        Self::Solution,
        Self::SortDown,
        Self::SortUp,
        Self::SystemDatabase,
        Self::Tags,
        Self::Take,
        Self::Unbookmark,
        Self::Undo,
        Self::View,
        Self::Warning,
    ];

    /// Canonical kebab-case icon name matching `assets/icons/<name>.svg`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Analysis => "analysis",
            Self::Back => "back",
            Self::Bookmark => "bookmark",
            Self::Cancel => "cancel",
            Self::Cards => "cards",
            Self::Clear => "clear",
            Self::Close => "close",
            Self::Collapse => "collapse",
            Self::Confirm => "confirm",
            Self::Copy => "copy",
            Self::Database => "database",
            Self::Deck => "deck",
            Self::Delete => "delete",
            Self::Dice => "dice",
            Self::Double => "double",
            Self::Drop => "drop",
            Self::Duplicates => "duplicates",
            Self::Edit => "edit",
            Self::Expand => "expand",
            Self::Export => "export",
            Self::Filter => "filter",
            Self::First => "first",
            Self::Grip => "grip",
            Self::Help => "help",
            Self::Hint => "hint",
            Self::ImportFolder => "import-folder",
            Self::Import => "import",
            Self::Last => "last",
            Self::Link => "link",
            Self::Loading => "loading",
            Self::Merge => "merge",
            Self::More => "more",
            Self::Move => "move",
            Self::NewGame => "new-game",
            Self::Next => "next",
            Self::Notes => "notes",
            Self::Open => "open",
            Self::Pass => "pass",
            Self::Pause => "pause",
            Self::Play => "play",
            Self::Refresh => "refresh",
            Self::Reset => "reset",
            Self::Resign => "resign",
            Self::Resume => "resume",
            Self::Roll => "roll",
            Self::Save => "save",
            Self::Search => "search",
            Self::SelectAll => "select-all",
            Self::Settings => "settings",
            Self::Similar => "similar",
            Self::Solution => "solution",
            Self::SortDown => "sort-down",
            Self::SortUp => "sort-up",
            Self::SystemDatabase => "system-database",
            Self::Tags => "tags",
            Self::Take => "take",
            Self::Unbookmark => "unbookmark",
            Self::Undo => "undo",
            Self::View => "view",
            Self::Warning => "warning",
        }
    }

    /// Embedded asset path served by [`crate::assets::NeonAssets`].
    pub const fn asset_path(self) -> &'static str {
        match self {
            Self::Add => "icons/add.svg",
            Self::Analysis => "icons/analysis.svg",
            Self::Back => "icons/back.svg",
            Self::Bookmark => "icons/bookmark.svg",
            Self::Cancel => "icons/cancel.svg",
            Self::Cards => "icons/cards.svg",
            Self::Clear => "icons/clear.svg",
            Self::Close => "icons/close.svg",
            Self::Collapse => "icons/collapse.svg",
            Self::Confirm => "icons/confirm.svg",
            Self::Copy => "icons/copy.svg",
            Self::Database => "icons/database.svg",
            Self::Deck => "icons/deck.svg",
            Self::Delete => "icons/delete.svg",
            Self::Dice => "icons/dice.svg",
            Self::Double => "icons/double.svg",
            Self::Drop => "icons/drop.svg",
            Self::Duplicates => "icons/duplicates.svg",
            Self::Edit => "icons/edit.svg",
            Self::Expand => "icons/expand.svg",
            Self::Export => "icons/export.svg",
            Self::Filter => "icons/filter.svg",
            Self::First => "icons/first.svg",
            Self::Grip => "icons/grip.svg",
            Self::Help => "icons/help.svg",
            Self::Hint => "icons/hint.svg",
            Self::ImportFolder => "icons/import-folder.svg",
            Self::Import => "icons/import.svg",
            Self::Last => "icons/last.svg",
            Self::Link => "icons/link.svg",
            Self::Loading => "icons/loading.svg",
            Self::Merge => "icons/merge.svg",
            Self::More => "icons/more.svg",
            Self::Move => "icons/move.svg",
            Self::NewGame => "icons/new-game.svg",
            Self::Next => "icons/next.svg",
            Self::Notes => "icons/notes.svg",
            Self::Open => "icons/open.svg",
            Self::Pass => "icons/pass.svg",
            Self::Pause => "icons/pause.svg",
            Self::Play => "icons/play.svg",
            Self::Refresh => "icons/refresh.svg",
            Self::Reset => "icons/reset.svg",
            Self::Resign => "icons/resign.svg",
            Self::Resume => "icons/resume.svg",
            Self::Roll => "icons/roll.svg",
            Self::Save => "icons/save.svg",
            Self::Search => "icons/search.svg",
            Self::SelectAll => "icons/select-all.svg",
            Self::Settings => "icons/settings.svg",
            Self::Similar => "icons/similar.svg",
            Self::Solution => "icons/solution.svg",
            Self::SortDown => "icons/sort-down.svg",
            Self::SortUp => "icons/sort-up.svg",
            Self::SystemDatabase => "icons/system-database.svg",
            Self::Tags => "icons/tags.svg",
            Self::Take => "icons/take.svg",
            Self::Unbookmark => "icons/unbookmark.svg",
            Self::Undo => "icons/undo.svg",
            Self::View => "icons/view.svg",
            Self::Warning => "icons/warning.svg",
        }
    }
}

/// Renders an embedded Lucide outline icon with explicit square size and tint.
pub fn action_icon(name: ActionIconName, size_px: f32, color: Hsla) -> Svg {
    svg()
        .path(name.asset_path())
        .size(px(size_px))
        .text_color(color)
        .flex_shrink_0()
}

const BRAND_LOGO_RASTER_PX: u32 = 256;

static BRAND_LOGO_IMAGE: LazyLock<Arc<RenderImage>> = LazyLock::new(|| {
    let bytes = NeonAssets
        .load(LOGO_PATH)
        .expect("read embedded NeonDojo logo asset")
        .expect("neondojo-logo.svg must be embedded");
    neondojo_board::rasterize_surface_svg(&bytes, BRAND_LOGO_RASTER_PX, BRAND_LOGO_RASTER_PX)
        .expect("rasterize NeonDojo brand logo SVG")
});

/// Returns the cached full-color GPU texture for the NeonDojo brand mark.
pub fn brand_logo_image() -> Arc<RenderImage> {
    Arc::clone(&BRAND_LOGO_IMAGE)
}

/// Renders the embedded text-free NeonDojo brand mark in full color.
///
/// GPUI's `svg()` element paints monochrome alpha masks tinted by `text_color`,
/// which skips rendering without an explicit text color and cannot display a
/// multi-color SVG with gradients and glow filters. Rasterizing via `resvg`
/// into a cached `RenderImage` and painting with `img()` preserves the full
/// brand artwork at Retina DPI.
pub fn brand_logo(size_px: f32) -> Img {
    img(brand_logo_image())
        .size(px(size_px))
        .object_fit(ObjectFit::Contain)
        .flex_shrink_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_logo_rasterizes_full_color_bgra_artwork() {
        let image = brand_logo_image();
        assert_eq!(image.frame_count(), 1);
        let size = image.size(0);
        assert_eq!(size.width.0, BRAND_LOGO_RASTER_PX as i32);
        assert_eq!(size.height.0, BRAND_LOGO_RASTER_PX as i32);

        let bytes = image.as_bytes(0).expect("rendered logo frame bytes");
        // Corner (0, 0) lies outside the rx=56 rounded tile and must be transparent.
        assert_eq!(bytes[3], 0, "top-left corner should be transparent");

        // Verify the rasterized BGRA frame preserves both the cyan torii (#3df2e0)
        // and pink rising sun checker (#ff4fa3) rather than a monochrome mask.
        let mut saw_cyan = false;
        let mut saw_pink = false;
        for px in bytes.as_chunks::<4>().0 {
            let (b, g, r, a) = (px[0], px[1], px[2], px[3]);
            if a > 200 && g > 180 && b > 180 && r < 120 {
                saw_cyan = true;
            }
            if a > 200 && r > 200 && b > 120 && g < 130 {
                saw_pink = true;
            }
        }
        assert!(saw_cyan, "expected cyan torii pixels in rasterized logo");
        assert!(saw_pink, "expected pink sun pixels in rasterized logo");
    }
}
