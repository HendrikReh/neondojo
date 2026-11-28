//! Checker evaluations as delivered by the selected catalog source.
use super::state::CompareEngineSlot;
use neondojo::{
    analysis_catalog::{AnalysisCatalogEntry, SelectedPositionAnalysis},
    engine::Candidate,
    imported_analysis::ImportedCandidate,
    model::Position,
};
use neondojo_i18n::{
    I18n, Locale, format_decimal, format_equity_digits, format_percent, format_ply,
};
use std::{cmp::Reverse, collections::HashMap};

#[derive(Debug, Clone)]
pub struct CheckerCandidate {
    pub rank: i32,
    /// Only available when all XG candidates have a finite equity in one known unit.
    pub xg_equity_rank: Option<i32>,
    pub notation: String,
    pub path: Option<String>,
    pub equity: Option<f64>,
    pub unit: String,
    pub depth: Option<String>,
    /// Cumulative win, win gammon+, win BG, loss, lose gammon+, lose BG.
    probabilities: [Option<f64>; 6],
}

const PROBABILITY_KEYS: [&str; 6] = [
    "win",
    "win_gammon",
    "win_backgammon",
    "lose",
    "lose_gammon",
    "lose_backgammon",
];

fn valid_probabilities(mut p: [Option<f64>; 6]) -> [Option<f64>; 6] {
    for value in &mut p {
        *value = value.filter(|v| (0.0..=1.0).contains(v));
    }
    p[3] = p[3].or_else(|| p[0].map(|win| 1.0 - win));
    p
}

impl CheckerCandidate {
    pub fn native(candidate: &Candidate, unit: &str) -> Self {
        Self {
            rank: candidate.rank,
            xg_equity_rank: None,
            notation: candidate.notation.clone(),
            path: candidate.path.clone(),
            equity: Some(candidate.equity).filter(|n| n.is_finite()),
            unit: unit.to_owned(),
            depth: candidate
                .plies
                .and_then(|n| u32::try_from(n).ok())
                .map(|n| format_ply(Locale::En, n)),
            probabilities: valid_probabilities(std::array::from_fn(|i| {
                candidate.probabilities.get(i).copied()
            })),
        }
    }

    fn imported(candidate: &ImportedCandidate) -> Self {
        let values = &candidate.probabilities;
        Self {
            rank: candidate.rank,
            xg_equity_rank: None,
            notation: candidate.notation.clone(),
            path: None,
            equity: candidate.equity.filter(|n| n.is_finite()),
            unit: candidate.equity_kind.clone(),
            depth: candidate.depth.clone().or_else(|| {
                candidate
                    .depth_plies
                    .and_then(|n| u32::try_from(n).ok())
                    .map(|n| format_ply(Locale::En, n))
            }),
            probabilities: valid_probabilities(std::array::from_fn(|i| {
                values
                    .get(PROBABILITY_KEYS[i])
                    .or_else(|| values.get(i))?
                    .as_f64()
            })),
        }
    }

    pub fn preview(&self) -> &str {
        self.path.as_deref().unwrap_or(&self.notation)
    }

    pub fn rank(&self) -> i32 {
        self.xg_equity_rank.unwrap_or(self.rank)
    }

    fn comparable_unit(&self) -> Option<&str> {
        match self.unit.as_str() {
            "cubeful" | "normalized_cubeful" => Some("cubeful"),
            "cubeless" => Some("cubeless"),
            "mwc" => Some("mwc"),
            _ => None,
        }
    }

    pub fn unit_label(&self, locale: Locale) -> &'static str {
        match self.unit.as_str() {
            "cubeful" => "Cubeful",
            "cubeless" => "Cubeless",
            "normalized_cubeful" => I18n::t(locale, "candidate", "normalizedCubeful"),
            "mwc" => "MWC",
            _ => I18n::t(locale, "candidate", "unknownUnit"),
        }
    }

    fn delta(&self, reference: &Self) -> Option<f64> {
        if self.comparable_unit()? != reference.comparable_unit()? {
            return None;
        }
        Some(self.equity? - reference.equity?).filter(|v| v.is_finite())
    }

    pub fn loss(&self, best: Option<&Self>) -> Option<f64> {
        let best = best?;
        if self.unit != best.unit {
            return None;
        }
        best.delta(self).filter(|loss| *loss >= 0.0)
    }

    pub fn equity_text(&self, locale: Locale, digits: usize) -> String {
        self.equity.map_or_else(
            || "—".into(),
            |v| match self.unit.as_str() {
                "mwc" => format_percent(locale, v, digits),
                _ => format_equity_digits(locale, v, digits),
            },
        )
    }

    pub fn difference_text(
        &self,
        locale: Locale,
        difference: Option<f64>,
        digits: usize,
        signed: bool,
    ) -> String {
        let Some(v) = difference else {
            return "—".into();
        };
        let mwc = self.unit == "mwc";
        let value = if mwc { v * 100.0 } else { v };
        let number = if signed {
            format_equity_digits(locale, value, digits)
        } else {
            format_decimal(locale, value, digits, digits)
        };
        if mwc {
            I18n::format(
                locale,
                "candidate",
                "percentagePoints",
                &[("value", number.into())],
            )
        } else {
            number
        }
    }

    /// Display the outcome groups as total, gammon only, and backgammon.
    pub fn outcomes(&self) -> [Option<f64>; 6] {
        let mut p = self.probabilities;
        for side in p.as_chunks_mut::<3>().0 {
            let (total, gammon, bg) = (side[0], side[1], side[2]);
            // XG can store a total and its sub-outcomes as float32.
            let exceeds =
                |a: Option<f64>, b: Option<f64>| a.zip(b).is_some_and(|(a, b)| a - b > 1e-7);
            if exceeds(gammon, total) || exceeds(bg, gammon) || exceeds(bg, total) {
                side[1] = None;
                side[2] = None;
            } else {
                side[1] = gammon.zip(bg).map(|(g, b)| (g - b).max(0.0));
            }
        }
        p
    }

    pub fn explanation_chances(&self) -> Option<[f64; 6]> {
        let [w, wg, wb, l, lg, lb] = self.probabilities;
        // The explanation accepts complete cumulative probability vectors.
        self.outcomes()
            .iter()
            .all(Option::is_some)
            .then_some([w?, wg?, wb?, l?, lg?, lb?])
    }
}

fn is_xg_engine(engine: &str) -> bool {
    let engine = engine.to_ascii_lowercase();
    engine
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|w| w == "xg")
        || engine.match_indices("extreme gammon").any(|(i, name)| {
            let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
            !engine[..i].ends_with(word) && !engine[i + name.len()..].starts_with(word)
        })
}

pub fn extract_candidates(
    result: &SelectedPositionAnalysis,
    player: i32,
    slot: Option<CompareEngineSlot>,
) -> Vec<CheckerCandidate> {
    if slot != Some(CompareEngineSlot::Xg) {
        let native = result
            .analysis
            .as_ref()
            .map(|a| (&a.candidates, "cubeful"))
            .or_else(|| {
                result
                    .decision
                    .as_ref()
                    .map(|d| (&d.checker_candidates, "normalized_cubeful"))
            });
        if let Some((candidates, unit)) = native
            && !candidates.is_empty()
        {
            return candidates
                .iter()
                .map(|c| CheckerCandidate::native(c, unit))
                .collect();
        }
        if slot.is_some() {
            return vec![];
        }
    }
    let Some(record) = result.records.iter().find(|r| {
        r.data.kind == "checker"
            && r.data.player == player
            && !r.data.candidates.is_empty()
            && (slot.is_none() || is_xg_engine(&r.data.engine))
    }) else {
        return vec![];
    };
    let mut candidates = record
        .data
        .candidates
        .iter()
        .map(CheckerCandidate::imported)
        .collect::<Vec<_>>();
    if (record.source_format == "xg" || slot == Some(CompareEngineSlot::Xg))
        && candidates[0].comparable_unit().is_some()
        && candidates
            .iter()
            .all(|c| c.equity.is_some() && c.unit == candidates[0].unit)
    {
        candidates.sort_by(|a, b| {
            b.equity
                .partial_cmp(&a.equity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.rank.cmp(&b.rank))
        });
        for (i, candidate) in candidates.iter_mut().enumerate() {
            candidate.xg_equity_rank = Some(i as i32 + 1);
        }
    }
    candidates
}

pub fn comparison_options<'a>(
    entries: &'a [AnalysisCatalogEntry],
    selected: Option<&'a AnalysisCatalogEntry>,
    pos: &Position,
    slot: CompareEngineSlot,
) -> Vec<&'a AnalysisCatalogEntry> {
    let mut options: Vec<_> = entries
        .iter()
        .chain(selected.filter(|s| !entries.iter().any(|e| e.id == s.id)))
        .filter(|entry| {
            if entry.status != "completed"
                || (entry.scope == "position" && entry.position_id != Some(pos.id))
                || (entry.scope == "game" && entry.game_no != Some(pos.game_no))
            {
                return false;
            }
            if slot == CompareEngineSlot::Xg {
                return entry.source == "imported" && is_xg_engine(&entry.engine);
            }
            if entry.source != "native" {
                return false;
            }
            if let Some(snapshot) = entry.job.as_ref().and_then(|j| j.engine_snapshot.as_ref()) {
                return snapshot.engine_id
                    == if slot == CompareEngineSlot::Gnubg {
                        "gnubg"
                    } else {
                        "bgsage"
                    };
            }
            let engine = entry.engine.to_ascii_lowercase();
            let names: &[&str] = if slot == CompareEngineSlot::Gnubg {
                &["gnu backgammon", "gnubg"]
            } else {
                &["neongammon"]
            };
            names.iter().any(|name| {
                engine.strip_prefix(*name).is_some_and(|rest| {
                    !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                })
            })
        })
        .collect();
    options.sort_by_key(|e| Reverse((e.job.as_ref().map(|job| job.plies), e.created_at)));
    options
}

pub fn board_key(pos: &Position, notation: &str) -> String {
    if let (Ok(board), Ok(player), Ok(dice)) = (
        pos.board.as_slice().try_into(),
        u8::try_from(pos.player),
        pos.dice
            .iter()
            .map(|&v| u8::try_from(v))
            .collect::<Result<Vec<_>, _>>(),
    ) && let neondojo_board::MovePreview::Ok { board: after, .. } =
        neondojo_board::project_move(board, player, &dice, notation)
    {
        return format!("board:{after:?}");
    }
    format!("notation:{}", notation.trim())
}

pub struct CandidateCompareRow<'a> {
    pub display_notation: &'a str,
    pub played: bool,
    pub slots: [Option<&'a CheckerCandidate>; 3],
}

impl CandidateCompareRow<'_> {
    pub fn delta(&self, column: usize) -> Option<f64> {
        self.slots[column]?.delta(self.slots[0]?)
    }
}

pub fn build_candidate_compare_rows<'a>(
    position: &Position,
    columns: [&'a [CheckerCandidate]; 3],
    sort: Option<CompareEngineSlot>,
) -> Vec<CandidateCompareRow<'a>> {
    let mut keys = HashMap::new();
    let mut key_for = |notation: &str| {
        keys.entry(notation.to_owned())
            .or_insert_with(|| board_key(position, notation))
            .clone()
    };
    let played = key_for(&position.notation);
    let mut rows: Vec<CandidateCompareRow> = Vec::new();
    let mut indices = HashMap::new();
    for (index, candidates) in columns.iter().enumerate() {
        for candidate in *candidates {
            let key = key_for(candidate.preview());
            let i = indices.entry(key.clone()).or_insert_with(|| {
                rows.push(CandidateCompareRow {
                    display_notation: &candidate.notation,
                    played: key == played,
                    slots: [None; 3],
                });
                rows.len() - 1
            });
            rows[*i].slots[index].get_or_insert(candidate);
        }
    }
    if let Some(slot) = sort {
        rows.sort_by(|a, b| {
            let magnitude =
                |row: &CandidateCompareRow| row.delta(slot as usize).map(f64::abs).unwrap_or(-1.0);
            magnitude(b).total_cmp(&magnitude(a))
        });
    }
    rows
}
