//! NeonDojo design system: 6 color palettes, semantic error severities,
//! sRGB `color-mix` derivation, typography scale, tabular numerals, and
//! `gpui-kit` theme projection.
//!
//! Every hex constant mirrors `web/src/styles/ui-appearance.css` and
//! `web/src/styles/typography.css` 1:1. Board artwork colors remain completely
//! decoupled from interface palettes.

use gpui_kit::{
    App, FontFeatures, FontWeight, Global, Hsla, Pixels, SharedString, Styled, WindowAppearance,
    component::theme::{Theme, ThemeMode as GpuiThemeMode},
    px, rgb, rgba,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Exact typography scale matching `web/src/styles/typography.css` and `docs/typography.md`.
pub const FONT_SIZE_PAGE: f32 = 24.0;
pub const FONT_SIZE_DIALOG: f32 = 20.0;
pub const FONT_SIZE_QUESTION: f32 = 18.0;
pub const FONT_SIZE_SUBJECT: f32 = 16.0;
pub const FONT_SIZE_PANEL: f32 = 14.0;
pub const FONT_SIZE_BODY: f32 = 13.0;
pub const FONT_SIZE_CONTROL: f32 = 13.0;
pub const FONT_SIZE_LABEL: f32 = 12.0;
pub const FONT_SIZE_TABLE: f32 = 12.0;
pub const FONT_SIZE_TABLE_HEADING: f32 = 11.0;
pub const FONT_SIZE_META: f32 = 11.0;
pub const FONT_SIZE_MICRO: f32 = 10.0;

/// Minimum hit target for compact icon buttons (`docs/design/action-icons-proposal.md`).
pub const COMPACT_ICON_BUTTON_SIZE: f32 = 44.0;
/// Dense board navigation icon button height in desktop Analysis.
pub const DENSE_BOARD_NAV_BUTTON_SIZE: f32 = 26.0;
/// Minimum hit target for dense table selection checkboxes.
pub const TABLE_CHECKBOX_TARGET_SIZE: f32 = 28.0;

/// Shared system font stacks (macOS resolves `.SystemUIFont` to SF Pro).
pub const FONT_UI_FAMILY: &str = ".SystemUIFont";
#[cfg(target_os = "macos")]
pub const FONT_MONO_FAMILY: &str = "SF Mono";
#[cfg(not(target_os = "macos"))]
pub const FONT_MONO_FAMILY: &str = "Menlo";

/// User-selectable interface palette family.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NeonPaletteId {
    #[default]
    Graphite,
    Club,
    Paper,
}

impl NeonPaletteId {
    pub const ALL: &'static [Self] = &[Self::Graphite, Self::Club, Self::Paper];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Graphite => "graphite",
            Self::Club => "club",
            Self::Paper => "paper",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Graphite => "Graphite",
            Self::Club => "Club",
            Self::Paper => "Paper",
        }
    }

    pub fn parse_lossy(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "club" => Self::Club,
            "paper" => Self::Paper,
            _ => Self::Graphite,
        }
    }
}

/// User display mode preference (`light`, `dark`, or `system`).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    pub const ALL: &'static [Self] = &[Self::Light, Self::Dark, Self::System];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    pub fn parse_lossy(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::System,
        }
    }

    pub const fn resolve(self, system_dark: bool) -> ResolvedMode {
        match self {
            Self::Light => ResolvedMode::Light,
            Self::Dark => ResolvedMode::Dark,
            Self::System => {
                if system_dark {
                    ResolvedMode::Dark
                } else {
                    ResolvedMode::Light
                }
            }
        }
    }
}

/// Concrete light or dark mode after resolving `ThemeMode::System`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolvedMode {
    #[default]
    Light,
    Dark,
}

impl ResolvedMode {
    pub const ALL: &'static [Self] = &[Self::Light, Self::Dark];

    pub const fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    pub fn from_window_appearance(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

/// Move / cube error severity grades matching `base.css` and `desktop-analysis.css`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeverityLevel {
    None,
    Best,
    Doubtful,
    Bad,
    Blunder,
    VeryBad,
}

/// Exact sRGB hex triplet used both for GPUI color construction and WCAG
/// contrast auditing without floating-point HSL round-trip drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbHex(pub u32);

impl RgbHex {
    pub const fn channels(self) -> (u8, u8, u8) {
        (
            ((self.0 >> 16) & 0xff) as u8,
            ((self.0 >> 8) & 0xff) as u8,
            (self.0 & 0xff) as u8,
        )
    }

    pub fn to_hsla(self) -> Hsla {
        rgb(self.0).into()
    }

    /// sRGB linear interpolation matching CSS `color-mix(in srgb, self weight, other)`.
    pub fn mix_srgb(self, weight: f32, other: Self) -> Self {
        let w = weight.clamp(0.0, 1.0);
        let (r1, g1, b1) = self.channels();
        let (r2, g2, b2) = other.channels();
        let blend = |a: u8, b: u8| -> u8 {
            ((a as f32 * w) + (b as f32 * (1.0 - w)))
                .round()
                .clamp(0.0, 255.0) as u8
        };
        let r = blend(r1, r2) as u32;
        let g = blend(g1, g2) as u32;
        let b = blend(b1, b2) as u32;
        Self((r << 16) | (g << 8) | b)
    }

    /// WCAG 2.1 relative luminance (`Y`).
    pub fn relative_luminance(self) -> f64 {
        let (r, g, b) = self.channels();
        let linear = |channel: u8| {
            let c = f64::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    /// WCAG 2.1 contrast ratio between two opaque colors (`1.0 ..= 21.0`).
    pub fn contrast_ratio(self, other: Self) -> f64 {
        let a = self.relative_luminance();
        let b = other.relative_luminance();
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}

/// Complete resolved NeonDojo color palette.
///
/// Passed explicitly (`&NeonPalette`) to scoped previews such as
/// `Settings → Appearance` so an unapplied draft palette renders accurately
/// inside an active window using a different global palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeonPalette {
    pub id: NeonPaletteId,
    pub mode: ResolvedMode,

    // Raw sRGB hexes retained for exact WCAG contrast checks.
    pub bg_hex: RgbHex,
    pub surface_hex: RgbHex,
    pub surface_alt_hex: RgbHex,
    pub text_hex: RgbHex,
    pub muted_hex: RgbHex,
    pub border_hex: RgbHex,
    pub control_border_hex: RgbHex,
    pub accent_hex: RgbHex,
    pub selected_hex: RgbHex,

    // Palette-specific tokens
    pub bg: Hsla,
    pub surface: Hsla,
    pub surface_alt: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub border: Hsla,
    pub control_border: Hsla,
    pub accent: Hsla,
    pub selected: Hsla,

    // Mode-scoped semantic tokens (stable across palettes)
    pub accent_text: Hsla,
    pub success_text: Hsla,
    pub success_bg: Hsla,
    pub warning_text: Hsla,
    pub warning_bg: Hsla,
    pub bad_text: Hsla,
    pub bad_bg: Hsla,
    pub danger_text: Hsla,
    pub danger_bg: Hsla,
    pub danger_button: Hsla,
    pub danger_button_text: Hsla,
    pub overlay: Hsla,
    pub shadow: Hsla,

    // Derived sRGB color-mix tokens
    pub accent_hover: Hsla,
    pub border_strong: Hsla,
    pub danger_border: Hsla,
    pub compact_disabled: Hsla,
}

impl NeonPalette {
    /// Resolves one of the 6 approved NeonDojo palettes (`web/src/styles/ui-appearance.css`).
    pub fn resolve(id: NeonPaletteId, mode: ResolvedMode) -> Self {
        let (
            bg_hex,
            surface_hex,
            surface_alt_hex,
            text_hex,
            muted_hex,
            border_hex,
            control_border_hex,
            accent_hex,
            selected_hex,
        ) = match (id, mode) {
            (NeonPaletteId::Graphite, ResolvedMode::Light) => (
                RgbHex(0xeef1f5),
                RgbHex(0xffffff),
                RgbHex(0xf5f7fa),
                RgbHex(0x202833),
                RgbHex(0x596779),
                RgbHex(0xdce2ea),
                RgbHex(0x7c899b),
                RgbHex(0x315bc5),
                RgbHex(0xeaf0ff),
            ),
            (NeonPaletteId::Graphite, ResolvedMode::Dark) => (
                RgbHex(0x15191f),
                RgbHex(0x1e242d),
                RgbHex(0x252d38),
                RgbHex(0xe5eaf2),
                RgbHex(0xa9b5c6),
                RgbHex(0x35404f),
                RgbHex(0x6b798d),
                RgbHex(0x98b6ff),
                RgbHex(0x2c3c59),
            ),
            (NeonPaletteId::Club, ResolvedMode::Light) => (
                RgbHex(0xedf2ee),
                RgbHex(0xfbfdf9),
                RgbHex(0xf0f5ee),
                RgbHex(0x24352e),
                RgbHex(0x536b5d),
                RgbHex(0xd6e1d5),
                RgbHex(0x768b7c),
                RgbHex(0x30664c),
                RgbHex(0xe2efe3),
            ),
            (NeonPaletteId::Club, ResolvedMode::Dark) => (
                RgbHex(0x151e1a),
                RgbHex(0x1d2a23),
                RgbHex(0x25342b),
                RgbHex(0xe4ebdf),
                RgbHex(0xacbcac),
                RgbHex(0x3c4e40),
                RgbHex(0x698071),
                RgbHex(0xacce9a),
                RgbHex(0x314634),
            ),
            (NeonPaletteId::Paper, ResolvedMode::Light) => (
                RgbHex(0xf2eee7),
                RgbHex(0xfffcf5),
                RgbHex(0xf5efe6),
                RgbHex(0x362d2e),
                RgbHex(0x726563),
                RgbHex(0xe0d6ca),
                RgbHex(0x91837c),
                RgbHex(0x78528e),
                RgbHex(0xefe5f1),
            ),
            (NeonPaletteId::Paper, ResolvedMode::Dark) => (
                RgbHex(0x211d1d),
                RgbHex(0x2b2525),
                RgbHex(0x352d2e),
                RgbHex(0xeee5da),
                RgbHex(0xc1afa9),
                RgbHex(0x514342),
                RgbHex(0x867674),
                RgbHex(0xd0aae6),
                RgbHex(0x453248),
            ),
        };

        let (
            accent_text_hex,
            success_text_hex,
            success_bg_hex,
            warning_text_hex,
            warning_bg_hex,
            bad_text_hex,
            bad_bg_hex,
            danger_text_hex,
            danger_bg_hex,
            overlay,
            shadow,
        ) = match mode {
            ResolvedMode::Light => (
                RgbHex(0xffffff),
                RgbHex(0x25633c),
                RgbHex(0xe3f2e7),
                RgbHex(0x805d13),
                RgbHex(0xfff0cb),
                RgbHex(0x94451f),
                RgbHex(0xffe6d1),
                RgbHex(0xa13428),
                RgbHex(0xf8ddd8),
                rgba(0x10182099).into(),
                rgba(0x10182024).into(),
            ),
            ResolvedMode::Dark => (
                RgbHex(0x15191f),
                RgbHex(0x99d9ac),
                RgbHex(0x20392a),
                RgbHex(0xedcc7a),
                RgbHex(0x40351d),
                RgbHex(0xffc294),
                RgbHex(0x482f20),
                RgbHex(0xffb4a7),
                RgbHex(0x452a29),
                rgba(0x080c12b3).into(),
                rgba(0x00000066).into(),
            ),
        };

        let danger_button_hex = RgbHex(0xa13428);
        let danger_button_text_hex = RgbHex(0xffffff);

        let accent_hover_hex = accent_hex.mix_srgb(0.88, text_hex);
        let border_strong_hex = muted_hex.mix_srgb(0.55, border_hex);
        let danger_border_hex = danger_text_hex.mix_srgb(0.45, control_border_hex);
        let compact_disabled_hex = muted_hex.mix_srgb(0.85, surface_hex);

        Self {
            id,
            mode,
            bg_hex,
            surface_hex,
            surface_alt_hex,
            text_hex,
            muted_hex,
            border_hex,
            control_border_hex,
            accent_hex,
            selected_hex,
            bg: bg_hex.to_hsla(),
            surface: surface_hex.to_hsla(),
            surface_alt: surface_alt_hex.to_hsla(),
            text: text_hex.to_hsla(),
            muted: muted_hex.to_hsla(),
            border: border_hex.to_hsla(),
            control_border: control_border_hex.to_hsla(),
            accent: accent_hex.to_hsla(),
            selected: selected_hex.to_hsla(),
            accent_text: accent_text_hex.to_hsla(),
            success_text: success_text_hex.to_hsla(),
            success_bg: success_bg_hex.to_hsla(),
            warning_text: warning_text_hex.to_hsla(),
            warning_bg: warning_bg_hex.to_hsla(),
            bad_text: bad_text_hex.to_hsla(),
            bad_bg: bad_bg_hex.to_hsla(),
            danger_text: danger_text_hex.to_hsla(),
            danger_bg: danger_bg_hex.to_hsla(),
            danger_button: danger_button_hex.to_hsla(),
            danger_button_text: danger_button_text_hex.to_hsla(),
            overlay,
            shadow,
            accent_hover: accent_hover_hex.to_hsla(),
            border_strong: border_strong_hex.to_hsla(),
            danger_border: danger_border_hex.to_hsla(),
            compact_disabled: compact_disabled_hex.to_hsla(),
        }
    }

    /// Returns `(foreground, background)` for an analysis severity grade.
    pub fn severity_colors(&self, severity: SeverityLevel) -> (Hsla, Hsla) {
        match severity {
            SeverityLevel::None => (self.muted, self.surface),
            SeverityLevel::Best => (self.success_text, self.success_bg),
            SeverityLevel::Doubtful => (self.warning_text, self.warning_bg),
            SeverityLevel::Bad => (self.bad_text, self.bad_bg),
            SeverityLevel::Blunder | SeverityLevel::VeryBad => (self.danger_text, self.danger_bg),
        }
    }

    /// Projects this palette onto `gpui-kit`'s global `Theme` so standard
    /// components (`Button`, `Input`, `Dialog`, `Popover`, `Table`, `Tooltip`,
    /// scrollbars, and resize handles) inherit NeonDojo's exact colors.
    pub fn apply_to_gpui_theme(&self, cx: &mut App) {
        let gpui_mode = match self.mode {
            ResolvedMode::Light => GpuiThemeMode::Light,
            ResolvedMode::Dark => GpuiThemeMode::Dark,
        };
        if Theme::global(cx).mode != gpui_mode {
            Theme::change(gpui_mode, None, cx);
        }

        let p = *self;
        Theme::update(cx, |theme| {
            theme.font_family = SharedString::from(FONT_UI_FAMILY);
            theme.font_size = px(FONT_SIZE_BODY);
            theme.mono_font_family = SharedString::from(FONT_MONO_FAMILY);
            theme.mono_font_size = px(FONT_SIZE_CONTROL);
            theme.radius = px(6.0);
            theme.radius_lg = px(10.0);

            let c = &mut theme.colors;
            c.background = p.bg;
            c.foreground = p.text;
            c.muted = p.surface_alt;
            c.muted_foreground = p.muted;
            c.border = p.border;
            c.input = p.control_border;
            c.ring = p.accent;
            c.caret = p.accent;
            c.selection = p.selected;
            c.drag_border = p.accent;
            c.drop_target = p.selected;

            c.primary = p.accent;
            c.primary_foreground = p.accent_text;
            c.primary_hover = p.accent_hover;
            c.primary_active = p.accent_hover;

            c.secondary = p.surface;
            c.secondary_foreground = p.text;
            c.secondary_hover = p.selected;
            c.secondary_active = p.selected;

            c.accent = p.selected;
            c.accent_foreground = p.text;

            c.button = p.surface;
            c.button_foreground = p.text;
            c.button_hover = p.selected;
            c.button_active = p.selected;
            c.button_primary = p.accent;
            c.button_primary_foreground = p.accent_text;
            c.button_primary_hover = p.accent_hover;
            c.button_primary_active = p.accent_hover;
            c.button_secondary = p.surface_alt;
            c.button_secondary_foreground = p.text;
            c.button_secondary_hover = p.selected;
            c.button_secondary_active = p.selected;
            c.button_danger = p.danger_button;
            c.button_danger_foreground = p.danger_button_text;
            c.button_danger_hover = p.danger_button;
            c.button_danger_active = p.danger_button;

            c.success = p.success_bg;
            c.success_foreground = p.success_text;
            c.success_hover = p.success_bg;
            c.success_active = p.success_bg;
            c.warning = p.warning_bg;
            c.warning_foreground = p.warning_text;
            c.warning_hover = p.warning_bg;
            c.warning_active = p.warning_bg;
            c.danger = p.danger_bg;
            c.danger_foreground = p.danger_text;
            c.danger_hover = p.danger_bg;
            c.danger_active = p.danger_bg;

            c.popover = p.surface;
            c.popover_foreground = p.text;
            c.overlay = p.overlay;

            c.link = p.accent;
            c.link_hover = p.accent_hover;
            c.link_active = p.accent_hover;

            c.list = p.surface;
            c.list_head = p.surface_alt;
            c.list_even = p.surface;
            c.list_hover = p.surface_alt;
            c.list_active = p.selected;
            c.list_active_border = p.accent;
            c.list_highlight_foreground = p.accent;

            c.table = p.surface;
            c.table_head = p.surface_alt;
            c.table_head_foreground = p.muted;
            c.table_foot = p.surface_alt;
            c.table_foot_foreground = p.muted;
            c.table_even = p.surface;
            c.table_hover = p.surface_alt;
            c.table_active = p.selected;
            c.table_active_border = p.accent;
            c.table_row_border = p.border;

            c.sidebar = p.surface_alt;
            c.sidebar_foreground = p.text;
            c.sidebar_border = p.border;
            c.sidebar_accent = p.selected;
            c.sidebar_accent_foreground = p.accent;
            c.sidebar_primary = p.accent;
            c.sidebar_primary_foreground = p.accent_text;

            c.tab = p.surface;
            c.tab_bar = p.surface;
            c.tab_bar_segmented = p.surface_alt;
            c.tab_foreground = p.muted;
            c.tab_active = p.surface;
            c.tab_active_foreground = p.accent;

            c.title_bar = p.surface;
            c.title_bar_border = p.border;
            c.status_bar = p.bg;
            c.status_bar_border = p.border;
            c.progress_bar = p.accent;
            c.group_box = p.surface;
            c.group_box_foreground = p.text;
            c.description_list_label = p.surface_alt;
            c.description_list_label_foreground = p.muted;
        });
    }
}

/// Global NeonDojo appearance state stored in GPUI `App`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeonTheme {
    pub palette_id: NeonPaletteId,
    pub mode: ThemeMode,
    pub resolved_mode: ResolvedMode,
    pub palette: NeonPalette,
}

impl Global for NeonTheme {}

impl NeonTheme {
    /// Initializes `NeonTheme` and projects it onto `gpui-kit`'s `Theme`.
    pub fn init(palette_id: NeonPaletteId, mode: ThemeMode, system_dark: bool, cx: &mut App) {
        let resolved_mode = mode.resolve(system_dark);
        let palette = NeonPalette::resolve(palette_id, resolved_mode);
        cx.set_global(Self {
            palette_id,
            mode,
            resolved_mode,
            palette,
        });
        palette.apply_to_gpui_theme(cx);
    }

    /// Updates palette/mode and syncs `gpui-kit`'s global `Theme` + windows.
    pub fn set(palette_id: NeonPaletteId, mode: ThemeMode, system_dark: bool, cx: &mut App) {
        Self::init(palette_id, mode, system_dark, cx);
    }

    /// Re-resolves the palette when macOS changes Light/Dark appearance.
    pub fn sync_system_appearance(appearance: WindowAppearance, cx: &mut App) {
        let Some(current) = cx.try_global::<Self>().copied() else {
            return;
        };
        if current.mode != ThemeMode::System {
            return;
        }
        let system_dark = ResolvedMode::from_window_appearance(appearance).is_dark();
        if current.resolved_mode.is_dark() != system_dark {
            Self::set(current.palette_id, current.mode, system_dark, cx);
        }
    }

    /// Returns the active [`NeonPalette`] (or Graphite Light if uninitialized).
    pub fn current_palette(cx: &App) -> NeonPalette {
        cx.try_global::<Self>()
            .map(|t| t.palette)
            .unwrap_or_else(|| NeonPalette::resolve(NeonPaletteId::Graphite, ResolvedMode::Light))
    }
}

/// Returns OpenType `FontFeatures` enabling tabular lining numerals (`tnum=1`).
pub fn tabular_nums_features() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".to_string(), 1)]))
}

/// Fluent typography helpers mirroring `web/src/styles/typography.css`.
pub trait NeonTypographyExt: Styled + Sized {
    fn text_page(self) -> Self {
        self.text_size(px(FONT_SIZE_PAGE))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(FONT_SIZE_PAGE * 1.2))
    }

    fn text_dialog(self) -> Self {
        self.text_size(px(FONT_SIZE_DIALOG))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(FONT_SIZE_DIALOG * 1.25))
    }

    fn text_question(self) -> Self {
        self.text_size(px(FONT_SIZE_QUESTION))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(FONT_SIZE_QUESTION * 1.3))
    }

    fn text_subject(self) -> Self {
        self.text_size(px(FONT_SIZE_SUBJECT))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(FONT_SIZE_SUBJECT * 1.35))
    }

    fn text_panel(self) -> Self {
        self.text_size(px(FONT_SIZE_PANEL))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(FONT_SIZE_PANEL * 1.4))
    }

    fn text_body(self) -> Self {
        self.text_size(px(FONT_SIZE_BODY))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(FONT_SIZE_BODY * 1.5))
    }

    fn text_control(self) -> Self {
        self.text_size(px(FONT_SIZE_CONTROL))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(FONT_SIZE_CONTROL * 1.5))
    }

    fn text_label(self) -> Self {
        self.text_size(px(FONT_SIZE_LABEL))
            .font_weight(FontWeight::MEDIUM)
            .line_height(px(FONT_SIZE_LABEL * 1.4))
    }

    fn text_table(self) -> Self {
        self.text_size(px(FONT_SIZE_TABLE))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(20.0))
            .tabular_nums()
    }

    fn text_table_heading(self) -> Self {
        self.text_size(px(FONT_SIZE_TABLE_HEADING))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(px(16.0))
    }

    fn text_meta(self) -> Self {
        self.text_size(px(FONT_SIZE_META))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(FONT_SIZE_META * 1.45))
    }

    fn text_micro(self) -> Self {
        self.text_size(px(FONT_SIZE_MICRO))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(FONT_SIZE_MICRO * 1.4))
    }

    fn mono_font(self) -> Self {
        self.font_family(FONT_MONO_FAMILY)
    }

    fn tabular_nums(self) -> Self {
        self.font_features(tabular_nums_features())
    }
}

impl<T: Styled + Sized> NeonTypographyExt for T {}

/// Returns the logical pixel typography size for a named CSS role.
pub const fn role_font_size(role: &str) -> Pixels {
    match role.as_bytes() {
        b"page" => px(FONT_SIZE_PAGE),
        b"dialog" => px(FONT_SIZE_DIALOG),
        b"question" => px(FONT_SIZE_QUESTION),
        b"subject" => px(FONT_SIZE_SUBJECT),
        b"panel" => px(FONT_SIZE_PANEL),
        b"body" | b"control" => px(FONT_SIZE_BODY),
        b"label" | b"table" => px(FONT_SIZE_TABLE),
        b"table-heading" | b"meta" => px(FONT_SIZE_META),
        _ => px(FONT_SIZE_MICRO),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_six_palettes_match_exact_hex_tokens() {
        let expected = [
            (
                NeonPaletteId::Graphite,
                ResolvedMode::Light,
                0xeef1f5,
                0xffffff,
                0x315bc5,
            ),
            (
                NeonPaletteId::Graphite,
                ResolvedMode::Dark,
                0x15191f,
                0x1e242d,
                0x98b6ff,
            ),
            (
                NeonPaletteId::Club,
                ResolvedMode::Light,
                0xedf2ee,
                0xfbfdf9,
                0x30664c,
            ),
            (
                NeonPaletteId::Club,
                ResolvedMode::Dark,
                0x151e1a,
                0x1d2a23,
                0xacce9a,
            ),
            (
                NeonPaletteId::Paper,
                ResolvedMode::Light,
                0xf2eee7,
                0xfffcf5,
                0x78528e,
            ),
            (
                NeonPaletteId::Paper,
                ResolvedMode::Dark,
                0x211d1d,
                0x2b2525,
                0xd0aae6,
            ),
        ];

        for (id, mode, bg, surface, accent) in expected {
            let p = NeonPalette::resolve(id, mode);
            assert_eq!(p.bg_hex, RgbHex(bg), "{id:?} {mode:?} bg");
            assert_eq!(p.surface_hex, RgbHex(surface), "{id:?} {mode:?} surface");
            assert_eq!(p.accent_hex, RgbHex(accent), "{id:?} {mode:?} accent");
            assert_ne!(
                p.severity_colors(SeverityLevel::Doubtful),
                p.severity_colors(SeverityLevel::Bad)
            );
            assert_ne!(
                p.severity_colors(SeverityLevel::Bad),
                p.severity_colors(SeverityLevel::Blunder)
            );
        }
    }

    /// Ports `web/src/styles/ui-appearance-styles.test.ts` WCAG 2.1 SC 1.4.11 check.
    #[test]
    fn control_border_meets_wcag_3_to_1_across_all_six_palettes() {
        for &id in NeonPaletteId::ALL {
            for &mode in ResolvedMode::ALL {
                let p = NeonPalette::resolve(id, mode);
                for (name, target) in [
                    ("surface", p.surface_hex),
                    ("surface_alt", p.surface_alt_hex),
                    ("bg", p.bg_hex),
                ] {
                    let ratio = p.control_border_hex.contrast_ratio(target);
                    assert!(
                        ratio >= 3.0,
                        "{id:?} {mode:?} control_border vs {name} contrast {ratio:.2} < 3.0"
                    );
                }
            }
        }
    }

    #[test]
    fn system_mode_resolves_to_host_appearance() {
        assert_eq!(ThemeMode::System.resolve(false), ResolvedMode::Light);
        assert_eq!(ThemeMode::System.resolve(true), ResolvedMode::Dark);
        assert_eq!(ThemeMode::Light.resolve(true), ResolvedMode::Light);
        assert_eq!(ThemeMode::Dark.resolve(false), ResolvedMode::Dark);
    }
}
