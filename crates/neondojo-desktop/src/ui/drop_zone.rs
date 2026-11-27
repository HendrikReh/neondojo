//! Drag-and-drop match file & folder collector (`MatchDropZone.tsx`).
//!
//! Uses `walkdir` for recursive directory traversal, skips hidden dotfiles and
//! macOS AppleDouble (`._*`) entries, filters against the shared authoritative
//! `neondojo_rules::options::match_file_extensions()` list, and sorts paths in
//! natural numeric order (`Intl.Collator('en', { numeric: true })` parity).

use crate::i18n::current_locale;
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::ui::icons::{ActionIconName, action_icon};
use gpui_kit::{
    App, ClickEvent, ElementId, ExternalPaths, FontWeight, InteractiveElement, IntoElement,
    ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled, TestSupportExt,
    Window, base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo_i18n::I18n;
use neondojo_rules::options::match_file_extensions;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use walkdir::WalkDir;

type DropFilesHandler = Rc<dyn Fn(DroppedMatchBatch, &mut Window, &mut App) + 'static>;
type BrowseClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Result of collecting and filtering dropped files & folders.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DroppedMatchBatch {
    /// Accepted supported match files in deterministic natural sort order.
    pub accepted: Vec<PathBuf>,
    /// Regular non-hidden files whose extension is not a supported match format.
    pub skipped_unsupported: usize,
    /// Hidden files/folders (including `.DS_Store` and `._*` AppleDouble files) ignored.
    pub ignored_hidden: usize,
}

/// Returns true if a file or directory name starts with `.` (dotfiles, `.DS_Store`, `._*`).
pub fn is_hidden_segment(name: &OsStr) -> bool {
    name.to_str().is_some_and(|s| s.starts_with('.'))
}

/// Returns true if `path` has a supported match extension from `shared/configuration-options.json`.
pub fn is_supported_match_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let dotted = format!(".{}", ext.to_ascii_lowercase());
    match_file_extensions()
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&dotted))
}

/// Natural numeric-aware case-insensitive comparator matching
/// `Intl.Collator('en', { numeric: true, sensitivity: 'base' })`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.as_bytes().iter().peekable();
    let mut bi = b.as_bytes().iter().peekable();

    while let (Some(&&ac), Some(&&bc)) = (ai.peek(), bi.peek()) {
        if ac.is_ascii_digit() && bc.is_ascii_digit() {
            let mut a_digits = Vec::new();
            while let Some(&&d) = ai.peek() {
                if d.is_ascii_digit() {
                    a_digits.push(d);
                    ai.next();
                } else {
                    break;
                }
            }
            let mut b_digits = Vec::new();
            while let Some(&&d) = bi.peek() {
                if d.is_ascii_digit() {
                    b_digits.push(d);
                    bi.next();
                } else {
                    break;
                }
            }
            let a_trimmed = strip_leading_zeros(&a_digits);
            let b_trimmed = strip_leading_zeros(&b_digits);
            let ord = a_trimmed
                .len()
                .cmp(&b_trimmed.len())
                .then_with(|| a_trimmed.cmp(b_trimmed))
                .then_with(|| a_digits.len().cmp(&b_digits.len()));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            ai.next();
            bi.next();
            let ord = ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase());
            if ord != Ordering::Equal {
                return ord;
            }
        }
    }

    ai.peek().is_some().cmp(&bi.peek().is_some())
}

fn strip_leading_zeros(digits: &[u8]) -> &[u8] {
    let first_non_zero = digits
        .iter()
        .position(|&b| b != b'0')
        .unwrap_or(digits.len());
    &digits[first_non_zero..]
}

/// Recursively collects supported match files from dropped paths, skipping hidden
/// segments and sorting naturally.
pub fn collect_dropped_match_files(paths: &[PathBuf]) -> DroppedMatchBatch {
    let mut batch = DroppedMatchBatch::default();
    let mut seen = BTreeSet::<PathBuf>::new();

    for root in paths {
        if root.file_name().is_some_and(is_hidden_segment) {
            batch.ignored_hidden += 1;
            continue;
        }
        if root.is_file() {
            if is_supported_match_file(root) {
                if seen.insert(root.clone()) {
                    batch.accepted.push(root.clone());
                }
            } else {
                batch.skipped_unsupported += 1;
            }
            continue;
        }
        if !root.is_dir() {
            continue;
        }

        let mut iter = WalkDir::new(root).follow_links(false).into_iter();
        while let Some(entry_res) = iter.next() {
            let Ok(entry) = entry_res else {
                continue;
            };
            if entry.depth() > 0 && is_hidden_segment(entry.file_name()) {
                batch.ignored_hidden += 1;
                if entry.file_type().is_dir() {
                    iter.skip_current_dir();
                }
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            if is_supported_match_file(&path) {
                if seen.insert(path.clone()) {
                    batch.accepted.push(path);
                }
            } else {
                batch.skipped_unsupported += 1;
            }
        }
    }

    batch.accepted.sort_by(|a, b| {
        let a_str = a.to_string_lossy();
        let b_str = b.to_string_lossy();
        natural_cmp(&a_str, &b_str).then_with(|| a.cmp(b))
    });

    batch
}

/// Interactive desktop drag-and-drop match import target (`min-height: 330px`).
#[derive(IntoElement)]
pub struct MatchDropZone {
    id: ElementId,
    title: Option<SharedString>,
    subtitle: Option<SharedString>,
    busy: bool,
    palette: Option<NeonPalette>,
    on_drop_batch: Option<DropFilesHandler>,
    on_browse: Option<BrowseClickHandler>,
}

impl MatchDropZone {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: None,
            subtitle: None,
            busy: false,
            palette: None,
            on_drop_batch: None,
            on_browse: None,
        }
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub fn palette(mut self, palette: NeonPalette) -> Self {
        self.palette = Some(palette);
        self
    }

    pub fn on_drop_batch(
        mut self,
        handler: impl Fn(DroppedMatchBatch, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_drop_batch = Some(Rc::new(handler));
        self
    }

    pub fn on_browse(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_browse = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for MatchDropZone {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let locale = current_locale(cx);
        let p = self
            .palette
            .unwrap_or_else(|| NeonTheme::current_palette(cx));
        let extensions = match_file_extensions().join(" · ");
        let title = self
            .title
            .unwrap_or_else(|| SharedString::from(I18n::t(locale, "app", "app.dropZoneTitle")));
        let subtitle = self.subtitle.unwrap_or_else(|| {
            SharedString::from(I18n::t(locale, "workspace", "welcomeDescription"))
        });
        let busy = self.busy;
        let on_drop = self.on_drop_batch;
        let on_browse = self.on_browse;

        div()
            .id(self.id)
            .test_support()
            .v_flex()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .min_h(px(330.0))
            .w_full()
            .p(px(28.0))
            .rounded(px(10.0))
            .border_2()
            .border_dashed()
            .border_color(if busy { p.border } else { p.control_border })
            .bg(p.surface)
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style.border_color(p.accent).bg(p.selected)
            })
            .when(!busy, |this| {
                this.when_some(on_drop, |el, handler| {
                    el.on_drop::<ExternalPaths>(move |paths, window, cx| {
                        let batch = collect_dropped_match_files(paths.paths());
                        handler(batch, window, cx);
                    })
                })
                .when_some(on_browse, |el, handler| {
                    el.cursor_pointer()
                        .hover(move |s| s.border_color(p.accent).bg(p.surface_alt))
                        .on_click(move |event, window, cx| handler(event, window, cx))
                })
            })
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_center()
                    .w(px(52.0))
                    .h(px(52.0))
                    .rounded(px(12.0))
                    .bg(p.selected)
                    .child(action_icon(ActionIconName::Import, 24.0, p.accent)),
            )
            .child(
                div()
                    .text_subject()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(title),
            )
            .child(div().text_body().text_color(p.muted).child(subtitle))
            .child(
                div()
                    .text_meta()
                    .px(px(10.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .bg(p.surface_alt)
                    .border_1()
                    .border_color(p.border)
                    .text_color(p.muted)
                    .child(extensions),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn natural_sort_orders_numeric_segments_like_intl_collator() {
        let mut names = vec![
            "Round 10.mat",
            "round 2.mat",
            "Round 1.xg",
            "Round 03.sgf",
            "Round 20.bgf",
        ];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec![
                "Round 1.xg",
                "round 2.mat",
                "Round 03.sgf",
                "Round 10.mat",
                "Round 20.bgf",
            ]
        );
    }

    #[test]
    fn collects_supported_files_recursively_and_skips_hidden_and_appledouble() {
        let dir = tempdir().expect("create temp dir");
        let root = dir.path().join("Matches");
        let nested = root.join("tournament");
        let hidden_dir = root.join(".hidden");
        let trashes_dir = dir.path().join(".Trashes");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::create_dir_all(&trashes_dir).unwrap();

        fs::write(nested.join("match_10.mat"), b"mat").unwrap();
        fs::write(nested.join("match_2.XG"), b"xg").unwrap();
        fs::write(nested.join("match_1.ogxm"), b"ogxm").unwrap();
        fs::write(nested.join("._match_1.mat"), b"appledouble").unwrap();
        fs::write(nested.join(".DS_Store"), b"ds").unwrap();
        fs::write(nested.join("notes.pdf"), b"pdf").unwrap();
        fs::write(hidden_dir.join("secret.mat"), b"hidden").unwrap();
        fs::write(trashes_dir.join("trashed.mat"), b"trashed").unwrap();

        let batch = collect_dropped_match_files(&[root, trashes_dir]);
        let file_names: Vec<String> = batch
            .accepted
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            file_names,
            vec!["match_1.ogxm", "match_2.XG", "match_10.mat"]
        );
        assert_eq!(batch.skipped_unsupported, 1);
        assert_eq!(batch.ignored_hidden, 4);
    }
}
