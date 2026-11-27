//! Move severity visual tokens and localized labels (`move-severity.ts`).
//!
//! Strictly lives in the shared `review` layer below all workspaces so
//! Analysis, Playing, Positions, and Study share identical colors and labels.

use crate::theme::NeonPalette;
use gpui_kit::Hsla;
use neondojo::configuration::ErrorThresholds;
use neondojo::model::MoveSeverity;
use neondojo_i18n::{I18n, Locale};

/// Palette-aware styling and localization key for a [`MoveSeverity`] grade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeverityStyle {
    pub severity: MoveSeverity,
    pub fg: Hsla,
    pub bg: Hsla,
    pub label_key: &'static str,
    pub symbol: &'static str,
}

impl SeverityStyle {
    /// Resolves foreground/background tokens from `palette` for `severity`.
    pub fn resolve(severity: MoveSeverity, palette: &NeonPalette) -> Self {
        match severity {
            MoveSeverity::None => Self {
                severity,
                fg: palette.muted,
                bg: palette.surface,
                label_key: "severity.none",
                symbol: "",
            },
            MoveSeverity::Doubtful => Self {
                severity,
                fg: palette.warning_text,
                bg: palette.warning_bg,
                label_key: "severity.doubtful",
                symbol: "?!",
            },
            MoveSeverity::Bad => Self {
                severity,
                fg: palette.bad_text,
                bg: palette.bad_bg,
                label_key: "severity.bad",
                symbol: "?",
            },
            MoveSeverity::Blunder => Self {
                severity,
                fg: palette.danger_text,
                bg: palette.danger_bg,
                label_key: "severity.blunder",
                symbol: "??",
            },
            MoveSeverity::VeryBad => Self {
                severity,
                fg: palette.danger_text,
                bg: palette.danger_bg,
                label_key: "severity.very_bad",
                symbol: "???",
            },
        }
    }

    /// Returns the localized severity label (e.g. `"Questionable"`, `"Blunder"`, `"Patzer"`).
    pub fn label(&self, locale: Locale) -> &'static str {
        I18n::t(locale, "app", self.label_key)
    }

    pub fn is_error(&self) -> bool {
        self.severity != MoveSeverity::None
    }
}

/// Classifies normalized equity loss using the configured four strictly
/// increasing [`ErrorThresholds`].
pub fn classify_equity_loss(loss: f64, thresholds: ErrorThresholds) -> MoveSeverity {
    if !loss.is_finite() || loss < thresholds.questionable {
        MoveSeverity::None
    } else if loss >= thresholds.large_blunder {
        MoveSeverity::VeryBad
    } else if loss >= thresholds.blunder {
        MoveSeverity::Blunder
    } else if loss >= thresholds.error {
        MoveSeverity::Bad
    } else {
        MoveSeverity::Doubtful
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{NeonPaletteId, ResolvedMode};

    #[test]
    fn maps_every_severity_to_palette_tokens_and_localized_labels() {
        let palette = NeonPalette::resolve(NeonPaletteId::Graphite, ResolvedMode::Light);
        let thresholds = ErrorThresholds::default();

        assert_eq!(classify_equity_loss(0.0, thresholds), MoveSeverity::None);
        assert_eq!(
            classify_equity_loss(thresholds.questionable, thresholds),
            MoveSeverity::Doubtful
        );
        assert_eq!(
            classify_equity_loss(thresholds.error, thresholds),
            MoveSeverity::Bad
        );
        assert_eq!(
            classify_equity_loss(thresholds.blunder, thresholds),
            MoveSeverity::Blunder
        );
        assert_eq!(
            classify_equity_loss(thresholds.large_blunder, thresholds),
            MoveSeverity::VeryBad
        );

        let blunder = SeverityStyle::resolve(MoveSeverity::Blunder, &palette);
        assert_eq!(blunder.fg, palette.danger_text);
        assert_eq!(blunder.bg, palette.danger_bg);
        assert_eq!(blunder.label(Locale::En), "Blunder");
        assert_ne!(blunder.label(Locale::De), blunder.label(Locale::En));
    }
}
