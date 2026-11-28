//! Table column definitions, width/visibility persistence, sorting helpers, and
//! 200-item cross-page selection cap for the Positions workspace.
//!
//! Mirrors `position-table-columns.ts`, `decision-table-columns.ts`, and
//! `position-browser-api.ts` while persisting preferences in
//! [`crate::local_state::LocalUiState::positions`].

use crate::local_state::{LocalUiState, PositionsUiPrefs};
use neondojo::position_browser::{PositionBrowserDirection, PositionBrowserSort};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use uuid::Uuid;

/// Hard selection ceiling across pages in Saved Positions and Decision Search.
pub const POSITION_SELECTION_LIMIT: usize = 200;

/// Maximum resizable column width in logical pixels (matches `LocalUiState::sanitize`).
pub const MAX_COLUMN_WIDTH_PX: u32 = 2400;

/// Default visible data columns for Saved Positions (`shared/configuration-options.json`).
pub const DEFAULT_SAVED_VISIBLE_COLUMNS: &[SavedDataColumn] = &[
    SavedDataColumn::Origin,
    SavedDataColumn::Game,
    SavedDataColumn::Ply,
    SavedDataColumn::Player,
    SavedDataColumn::Dice,
    SavedDataColumn::Score,
    SavedDataColumn::Loss,
];

/// Every column rendered in the Saved Positions table (fixed + 13 data/toggleable columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SavedPositionColumn {
    Select,
    Origin,
    Database,
    Game,
    Ply,
    Player,
    Opponent,
    Dice,
    Score,
    Cube,
    PlayedMove,
    Loss,
    Decks,
    Notes,
    Actions,
}

/// The 12 user-pickable Saved Positions data columns (excluding `PlayedMove`,
/// which is controlled by the dedicated `show_played` toggle, matching web parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SavedDataColumn {
    Origin,
    Database,
    Game,
    Ply,
    Player,
    Opponent,
    Dice,
    Score,
    Cube,
    Loss,
    Decks,
    Notes,
}

impl SavedDataColumn {
    pub const ALL: &'static [Self] = &[
        Self::Origin,
        Self::Database,
        Self::Game,
        Self::Ply,
        Self::Player,
        Self::Opponent,
        Self::Dice,
        Self::Score,
        Self::Cube,
        Self::Loss,
        Self::Decks,
        Self::Notes,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Origin => "origin",
            Self::Database => "database",
            Self::Game => "game",
            Self::Ply => "ply",
            Self::Player => "player",
            Self::Opponent => "opponent",
            Self::Dice => "dice",
            Self::Score => "score",
            Self::Cube => "cube",
            Self::Loss => "loss",
            Self::Decks => "decks",
            Self::Notes => "notes",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.id() == id)
    }

    pub const fn to_column(self) -> SavedPositionColumn {
        match self {
            Self::Origin => SavedPositionColumn::Origin,
            Self::Database => SavedPositionColumn::Database,
            Self::Game => SavedPositionColumn::Game,
            Self::Ply => SavedPositionColumn::Ply,
            Self::Player => SavedPositionColumn::Player,
            Self::Opponent => SavedPositionColumn::Opponent,
            Self::Dice => SavedPositionColumn::Dice,
            Self::Score => SavedPositionColumn::Score,
            Self::Cube => SavedPositionColumn::Cube,
            Self::Loss => SavedPositionColumn::Loss,
            Self::Decks => SavedPositionColumn::Decks,
            Self::Notes => SavedPositionColumn::Notes,
        }
    }
}

/// Static column geometry specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnSpec {
    pub id: &'static str,
    pub min_width: u32,
    pub default_width: u32,
}

impl SavedPositionColumn {
    /// Complete ordered list of all 15 columns (selection + 13 content columns + actions).
    pub const ALL: &'static [Self] = &[
        Self::Select,
        Self::Origin,
        Self::Database,
        Self::Game,
        Self::Ply,
        Self::Player,
        Self::Opponent,
        Self::Dice,
        Self::Score,
        Self::Cube,
        Self::PlayedMove,
        Self::Loss,
        Self::Decks,
        Self::Notes,
        Self::Actions,
    ];

    /// The 13 sortable / hideable / resizable content columns.
    pub const CONTENT_COLUMNS: &'static [Self] = &[
        Self::Origin,
        Self::Database,
        Self::Game,
        Self::Ply,
        Self::Player,
        Self::Opponent,
        Self::Dice,
        Self::Score,
        Self::Cube,
        Self::PlayedMove,
        Self::Loss,
        Self::Decks,
        Self::Notes,
    ];

    pub const fn spec(self) -> ColumnSpec {
        match self {
            Self::Select => ColumnSpec {
                id: "select",
                min_width: 32,
                default_width: 36,
            },
            Self::Origin => ColumnSpec {
                id: "origin",
                min_width: 112,
                default_width: 228,
            },
            Self::Database => ColumnSpec {
                id: "database",
                min_width: 70,
                default_width: 130,
            },
            Self::Game => ColumnSpec {
                id: "game",
                min_width: 44,
                default_width: 84,
            },
            Self::Ply => ColumnSpec {
                id: "ply",
                min_width: 44,
                default_width: 84,
            },
            Self::Player => ColumnSpec {
                id: "player",
                min_width: 70,
                default_width: 156,
            },
            Self::Opponent => ColumnSpec {
                id: "opponent",
                min_width: 70,
                default_width: 144,
            },
            Self::Dice => ColumnSpec {
                id: "dice",
                min_width: 44,
                default_width: 76,
            },
            Self::Score => ColumnSpec {
                id: "score",
                min_width: 60,
                default_width: 100,
            },
            Self::Cube => ColumnSpec {
                id: "cube",
                min_width: 44,
                default_width: 86,
            },
            Self::PlayedMove => ColumnSpec {
                id: "played_move",
                min_width: 80,
                default_width: 140,
            },
            Self::Loss => ColumnSpec {
                id: "loss",
                min_width: 60,
                default_width: 132,
            },
            Self::Decks => ColumnSpec {
                id: "decks",
                min_width: 80,
                default_width: 144,
            },
            Self::Notes => ColumnSpec {
                id: "notes",
                min_width: 120,
                default_width: 200,
            },
            Self::Actions => ColumnSpec {
                id: "actions",
                min_width: 60,
                default_width: 72,
            },
        }
    }

    pub const fn id(self) -> &'static str {
        self.spec().id
    }

    /// Sort keys exposed by this column header.
    pub const fn sort_keys(self) -> &'static [PositionBrowserSort] {
        match self {
            Self::Origin => &[PositionBrowserSort::MatchId, PositionBrowserSort::Match],
            Self::Database => &[PositionBrowserSort::Database],
            Self::Game => &[PositionBrowserSort::Game],
            Self::Ply => &[PositionBrowserSort::Ply],
            Self::Player => &[PositionBrowserSort::Player],
            Self::Opponent => &[PositionBrowserSort::Opponent],
            Self::Dice => &[PositionBrowserSort::Dice],
            Self::Score => &[PositionBrowserSort::Score],
            Self::Cube => &[PositionBrowserSort::Cube],
            Self::PlayedMove => &[PositionBrowserSort::PlayedMove],
            Self::Loss => &[PositionBrowserSort::Loss],
            Self::Decks => &[PositionBrowserSort::Decks],
            Self::Notes => &[PositionBrowserSort::Tags, PositionBrowserSort::Comment],
            Self::Select | Self::Actions => &[],
        }
    }

    pub const fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::Game | Self::Ply | Self::Dice | Self::Score | Self::Cube | Self::Loss
        )
    }
}

/// Resolves the effective sort direction for a [`PositionBrowserSort`]:
/// Loss defaults to descending; every other sort defaults to ascending.
pub const fn effective_sort_direction(
    sort: PositionBrowserSort,
    direction: Option<PositionBrowserDirection>,
) -> PositionBrowserDirection {
    match direction {
        Some(dir) => dir,
        None => match sort {
            PositionBrowserSort::Loss => PositionBrowserDirection::Desc,
            _ => PositionBrowserDirection::Asc,
        },
    }
}

/// Computes the next sort direction when clicking a sort heading.
pub fn toggled_sort_direction(
    current_sort: PositionBrowserSort,
    current_direction: Option<PositionBrowserDirection>,
    clicked_sort: PositionBrowserSort,
) -> PositionBrowserDirection {
    let dir = effective_sort_direction(current_sort, current_direction);
    if current_sort == clicked_sort && matches!(dir, PositionBrowserDirection::Asc) {
        PositionBrowserDirection::Desc
    } else {
        PositionBrowserDirection::Asc
    }
}

/// Runtime table layout state for the Saved Positions 13-column table, backed
/// by [`PositionsUiPrefs`] inside [`LocalUiState`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTableLayout {
    pub visible_columns: Vec<SavedDataColumn>,
    pub column_widths: BTreeMap<String, u32>,
    pub show_played: bool,
}

impl Default for SavedTableLayout {
    fn default() -> Self {
        Self {
            visible_columns: DEFAULT_SAVED_VISIBLE_COLUMNS.to_vec(),
            column_widths: BTreeMap::new(),
            show_played: false,
        }
    }
}

impl SavedTableLayout {
    /// Restores sanitized Saved Positions column preferences from [`PositionsUiPrefs`].
    pub fn from_prefs(prefs: &PositionsUiPrefs) -> Self {
        let mut seen = HashSet::new();
        let mut visible = Vec::new();
        for id in &prefs.visible_columns {
            if let Some(col) = SavedDataColumn::from_id(id)
                && seen.insert(col)
            {
                visible.push(col);
            }
        }
        // Preserve canonical column ordering.
        let visible_ordered: Vec<SavedDataColumn> = if visible.is_empty() {
            DEFAULT_SAVED_VISIBLE_COLUMNS.to_vec()
        } else {
            SavedDataColumn::ALL
                .iter()
                .copied()
                .filter(|col| seen.contains(col))
                .collect()
        };

        let mut widths = BTreeMap::new();
        for col in SavedPositionColumn::ALL {
            let spec = col.spec();
            if let Some(&raw) = prefs.column_widths.get(spec.id) {
                widths.insert(
                    spec.id.to_string(),
                    raw.clamp(spec.min_width, MAX_COLUMN_WIDTH_PX),
                );
            }
        }

        Self {
            visible_columns: visible_ordered,
            column_widths: widths,
            show_played: prefs.show_played,
        }
    }

    /// Exports preferences in [`PositionsUiPrefs`] wire format for [`LocalUiState`].
    pub fn to_prefs(&self) -> PositionsUiPrefs {
        PositionsUiPrefs {
            visible_columns: self
                .visible_columns
                .iter()
                .map(|c| c.id().to_string())
                .collect(),
            column_widths: self.column_widths.clone(),
            show_played: self.show_played,
        }
    }

    /// Persists these column preferences into `state.positions` and writes atomically
    /// when `path` is provided.
    pub fn save_to_local_state(
        &self,
        state: &mut LocalUiState,
        path: Option<&Path>,
    ) -> anyhow::Result<()> {
        state.positions = self.to_prefs();
        if let Some(path) = path {
            state.save_atomic(path)?;
        }
        Ok(())
    }

    /// Ordered active columns rendered in the Saved Positions table.
    pub fn active_columns(&self) -> Vec<SavedPositionColumn> {
        SavedPositionColumn::ALL
            .iter()
            .copied()
            .filter(|&col| match col {
                SavedPositionColumn::Select | SavedPositionColumn::Actions => true,
                SavedPositionColumn::PlayedMove => self.show_played,
                other => SavedDataColumn::from_id(other.id())
                    .is_some_and(|dc| self.visible_columns.contains(&dc)),
            })
            .collect()
    }

    /// Returns whether `sort` belongs to any currently visible column.
    pub fn is_sort_visible(&self, sort: PositionBrowserSort) -> bool {
        self.active_columns()
            .into_iter()
            .any(|col| col.sort_keys().contains(&sort))
    }

    /// Returns the effective width (in px) for `column`.
    pub fn width_for(&self, column: SavedPositionColumn) -> u32 {
        let spec = column.spec();
        self.column_widths
            .get(spec.id)
            .copied()
            .unwrap_or(spec.default_width)
            .clamp(spec.min_width, MAX_COLUMN_WIDTH_PX)
    }

    /// Sets a custom width for `column`, clamped to `[spec.min_width, MAX_COLUMN_WIDTH_PX]`.
    pub fn set_width(&mut self, column: SavedPositionColumn, width: u32) {
        let spec = column.spec();
        let clamped = width.clamp(spec.min_width, MAX_COLUMN_WIDTH_PX);
        self.column_widths.insert(spec.id.to_string(), clamped);
    }

    /// Adjusts `column`'s width by signed delta pixels.
    pub fn resize_by(&mut self, column: SavedPositionColumn, delta: i32) {
        let current = self.width_for(column) as i64;
        let next = (current + i64::from(delta)).max(0) as u32;
        self.set_width(column, next);
    }

    /// Returns `true` when any column has a custom width override.
    pub fn has_custom_widths(&self) -> bool {
        !self.column_widths.is_empty()
    }

    /// Clears all custom column widths back to defaults.
    pub fn reset_widths(&mut self) {
        self.column_widths.clear();
    }

    /// Toggles visibility of a data column. Never allows hiding the last visible column.
    pub fn toggle_column(&mut self, column: SavedDataColumn, visible: bool) -> bool {
        let next: Vec<SavedDataColumn> = SavedDataColumn::ALL
            .iter()
            .copied()
            .filter(|&c| {
                if c == column {
                    visible
                } else {
                    self.visible_columns.contains(&c)
                }
            })
            .collect();
        if next.is_empty() || next == self.visible_columns {
            return false;
        }
        self.visible_columns = next;
        true
    }

    /// Shows all 12 pickable data columns.
    pub fn show_all_columns(&mut self) {
        self.visible_columns = SavedDataColumn::ALL.to_vec();
    }

    /// Resets visible columns to the default 7 columns.
    pub fn reset_visible_columns(&mut self) {
        self.visible_columns = DEFAULT_SAVED_VISIBLE_COLUMNS.to_vec();
    }
}

/// Cross-page selection toggle enforcing [`POSITION_SELECTION_LIMIT`] (200 items).
///
/// Mirrors `togglePositionSelection` in `position-browser-api.ts`:
/// - When `checked` is `true`, appends each unseen ID in `ids` while total < 200.
/// - When `checked` is `false`, removes every ID present in `ids`.
pub fn toggle_position_selection(
    current: &[Uuid],
    ids: &[Uuid],
    checked: bool,
    limit: usize,
) -> Vec<Uuid> {
    if checked {
        let mut out = current.to_vec();
        let mut present: HashSet<Uuid> = out.iter().copied().collect();
        for &id in ids {
            if out.len() >= limit {
                break;
            }
            if present.insert(id) {
                out.push(id);
            }
        }
        out
    } else {
        let remove: HashSet<Uuid> = ids.iter().copied().collect();
        current
            .iter()
            .copied()
            .filter(|id| !remove.contains(id))
            .collect()
    }
}

// ── All Analyzed Decisions Table Columns ─────────────────────────────────────

/// Columns of the All Analyzed Decisions table (`decision-table-columns.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionTableColumn {
    Match,
    Move,
    Kind,
    Score,
    Cube,
    Played,
    Best,
    Loss,
    Source,
}

impl DecisionTableColumn {
    pub const ALL: &'static [Self] = &[
        Self::Match,
        Self::Move,
        Self::Kind,
        Self::Score,
        Self::Cube,
        Self::Played,
        Self::Best,
        Self::Loss,
        Self::Source,
    ];

    pub const fn spec(self) -> ColumnSpec {
        match self {
            Self::Match => ColumnSpec {
                id: "match",
                min_width: 90,
                default_width: 190,
            },
            Self::Move => ColumnSpec {
                id: "move",
                min_width: 60,
                default_width: 100,
            },
            Self::Kind => ColumnSpec {
                id: "kind",
                min_width: 80,
                default_width: 136,
            },
            Self::Score => ColumnSpec {
                id: "score",
                min_width: 70,
                default_width: 136,
            },
            Self::Cube => ColumnSpec {
                id: "cube",
                min_width: 44,
                default_width: 84,
            },
            Self::Played => ColumnSpec {
                id: "played",
                min_width: 60,
                default_width: 112,
            },
            Self::Best => ColumnSpec {
                id: "best",
                min_width: 60,
                default_width: 112,
            },
            Self::Loss => ColumnSpec {
                id: "loss",
                min_width: 60,
                default_width: 96,
            },
            Self::Source => ColumnSpec {
                id: "source",
                min_width: 90,
                default_width: 180,
            },
        }
    }

    pub const fn id(self) -> &'static str {
        self.spec().id
    }

    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Move | Self::Score | Self::Cube | Self::Loss)
    }
}

/// Column visibility and widths for the Decision Search table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTableLayout {
    pub visible_columns: Vec<DecisionTableColumn>,
    pub column_widths: BTreeMap<String, u32>,
}

impl Default for DecisionTableLayout {
    fn default() -> Self {
        Self {
            visible_columns: DecisionTableColumn::ALL.to_vec(),
            column_widths: BTreeMap::new(),
        }
    }
}

impl DecisionTableLayout {
    pub fn width_for(&self, column: DecisionTableColumn) -> u32 {
        let spec = column.spec();
        self.column_widths
            .get(spec.id)
            .copied()
            .unwrap_or(spec.default_width)
            .clamp(spec.min_width, MAX_COLUMN_WIDTH_PX)
    }

    pub fn set_width(&mut self, column: DecisionTableColumn, width: u32) {
        let spec = column.spec();
        self.column_widths.insert(
            spec.id.to_string(),
            width.clamp(spec.min_width, MAX_COLUMN_WIDTH_PX),
        );
    }

    pub fn resize_by(&mut self, column: DecisionTableColumn, delta: i32) {
        let current = self.width_for(column) as i64;
        let next = (current + i64::from(delta)).max(0) as u32;
        self.set_width(column, next);
    }

    pub fn has_custom_widths(&self) -> bool {
        !self.column_widths.is_empty()
    }

    pub fn reset_widths(&mut self) {
        self.column_widths.clear();
    }

    pub fn toggle_column(&mut self, column: DecisionTableColumn, visible: bool) -> bool {
        let next: Vec<DecisionTableColumn> = DecisionTableColumn::ALL
            .iter()
            .copied()
            .filter(|&c| {
                if c == column {
                    visible
                } else {
                    self.visible_columns.contains(&c)
                }
            })
            .collect();
        if next.is_empty() || next == self.visible_columns {
            return false;
        }
        self.visible_columns = next;
        true
    }

    pub fn show_all(&mut self) {
        self.visible_columns = DecisionTableColumn::ALL.to_vec();
    }
}
