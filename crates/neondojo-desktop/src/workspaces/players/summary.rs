//! Pure domain helpers for the Players workspace:
//! - Profile depth formatting
//! - Unrated analyzable match filtering (`can_enter_rating`, `filter_unrated_analyzable_match_ids`)
//! - Decision-weighted overall / checker / cube PR split summary
//! - Opponent breakdown aggregation (`compute_opponent_breakdown`)
//! - Match row perspective helpers (opponent name, player side score/result, PR)
//! - Default selection resolver (`choose_initial_selection`, preferring `is_me` virtual profile)

use neondojo::databases::DatabaseMatch;
use neondojo::play::PlayMode;
use neondojo::player_directory::{PlayerRatingPoint, VirtualPlayerProfile};
use std::collections::HashMap;
use uuid::Uuid;

/// Active selection in the Players workspace master-detail view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerSelection {
    Real(Uuid),
    Virtual(Uuid),
}

/// Chooses the default initial selection when opening the Players workspace:
/// prefers the virtual profile marked `is_me` ("My player profile"), otherwise
/// falls back to the first virtual profile if any exist, or `None`.
pub fn choose_initial_selection(virtuals: &[VirtualPlayerProfile]) -> Option<PlayerSelection> {
    virtuals
        .iter()
        .find(|vp| vp.is_me)
        .or_else(|| virtuals.first())
        .map(|vp| PlayerSelection::Virtual(vp.id))
}

/// Formats a stored analysis profile identifier such as `"gnubg-2ply"` or `"3ply"`
/// into a compact depth label for display.
pub fn profile_depth(profile: &str) -> String {
    let trimmed = profile.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some((_, rest)) = trimmed.rsplit_once('-') {
        return rest.to_string();
    }
    trimmed.to_string()
}

/// Mirrors `canEnterRating` from `PlayerSummary.tsx`:
/// imported matches and unassisted Competition bot matches can enter PR.
pub fn can_enter_rating(play_mode: Option<PlayMode>, play_assisted: bool) -> bool {
    match play_mode {
        None => true,
        Some(PlayMode::Competition) => !play_assisted,
        Some(_) => false,
    }
}

/// Returns whether a match row has an incomplete rating for the selected player
/// and can be queued for bulk analysis.
pub fn is_match_unrated_and_analyzable(m: &DatabaseMatch) -> bool {
    if !m.source_available {
        return false;
    }
    if !can_enter_rating(m.play_mode, m.play_assisted) {
        return false;
    }
    // Active (non-finished) competition bot matches cannot be rated yet.
    if m.play_mode == Some(PlayMode::Competition) && m.play_phase.as_deref() != Some("match_over") {
        return false;
    }
    if matches!(
        m.analysis_status.as_deref(),
        Some("queued" | "running" | "completed")
    ) {
        return false;
    }
    let sides: &[i16] = match m.player_sides.as_deref() {
        Some(s) if !s.is_empty() => s,
        _ => &[0, 1],
    };
    let all_sides_rated = sides.iter().all(|&side| match side {
        0 => m.white_pr.is_some(),
        1 => m.black_pr.is_some(),
        _ => false,
    });
    !all_sides_rated
}

/// Extracts deduplicated match UUIDs eligible for `Analyze unrated matches…`.
pub fn filter_unrated_analyzable_match_ids(matches: &[DatabaseMatch]) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in matches {
        if is_match_unrated_and_analyzable(m) && seen.insert(m.id) {
            out.push(m.id);
        }
    }
    out
}

/// Decision-weighted overall, checker-play, and cube PR breakdown across a profile's
/// rating history (or across all history points when `profile_filter` is `None`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PrSplitSummary {
    pub overall_pr: Option<f64>,
    pub checker_pr: Option<f64>,
    pub cube_pr: Option<f64>,
    pub total_decisions: i64,
    pub match_count: usize,
}

/// Computes the decision-weighted overall PR alongside mean checker PR and mean cube PR.
pub fn compute_pr_split_summary(
    history: &[PlayerRatingPoint],
    profile_filter: Option<&str>,
) -> PrSplitSummary {
    let pts: Vec<&PlayerRatingPoint> = history
        .iter()
        .filter(|pt| profile_filter.is_none_or(|p| pt.profile == p))
        .collect();
    if pts.is_empty() {
        return PrSplitSummary::default();
    }

    let total_decisions: i64 = pts.iter().map(|pt| pt.decisions.max(0)).sum();
    let overall_pr = if total_decisions > 0 {
        let weighted: f64 = pts
            .iter()
            .map(|pt| pt.value * (pt.decisions.max(0) as f64))
            .sum();
        Some(weighted / (total_decisions as f64))
    } else {
        let sum: f64 = pts.iter().map(|pt| pt.value).sum();
        Some(sum / (pts.len() as f64))
    };

    let checker_vals: Vec<(f64, i64)> = pts
        .iter()
        .filter_map(|pt| pt.checker_value.map(|v| (v, pt.decisions.max(0))))
        .collect();
    let checker_pr = weighted_or_plain_mean(&checker_vals);

    let cube_vals: Vec<(f64, i64)> = pts
        .iter()
        .filter_map(|pt| pt.cube_value.map(|v| (v, pt.decisions.max(0))))
        .collect();
    let cube_pr = weighted_or_plain_mean(&cube_vals);

    PrSplitSummary {
        overall_pr,
        checker_pr,
        cube_pr,
        total_decisions,
        match_count: pts.len(),
    }
}

fn weighted_or_plain_mean(values: &[(f64, i64)]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let total_w: i64 = values.iter().map(|(_, w)| *w).sum();
    if total_w > 0 {
        let sum: f64 = values.iter().map(|(v, w)| *v * (*w as f64)).sum();
        Some(sum / (total_w as f64))
    } else {
        let sum: f64 = values.iter().map(|(v, _)| *v).sum();
        Some(sum / (values.len() as f64))
    }
}

/// Outcome of a match from the selected player's perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerMatchOutcome {
    Win,
    Loss,
    Draw,
    Unknown,
}

/// Perspective-resolved view of one match row for the selected player.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerMatchView {
    pub opponent_name: String,
    pub opponent_id: Option<Uuid>,
    pub both_sides: bool,
    pub player_score: Option<i32>,
    pub opponent_score: Option<i32>,
    pub outcome: PlayerMatchOutcome,
    pub player_pr: Option<f64>,
    pub opponent_pr: Option<f64>,
}

/// Resolves opponent name, score, outcome, and PR from a `DatabaseMatch`'s `player_sides`.
pub fn resolve_match_perspective(m: &DatabaseMatch) -> PlayerMatchView {
    let sides = m.player_sides.as_deref().unwrap_or(&[]);
    let has_white = sides.contains(&0);
    let has_black = sides.contains(&1);

    if has_white && has_black {
        let avg_pr = match (m.white_pr, m.black_pr) {
            (Some(w), Some(b)) => Some((w + b) / 2.0),
            (Some(w), None) => Some(w),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        return PlayerMatchView {
            opponent_name: format!("{} / {}", m.white, m.black),
            opponent_id: None,
            both_sides: true,
            player_score: m.white_score,
            opponent_score: m.black_score,
            outcome: PlayerMatchOutcome::Unknown,
            player_pr: avg_pr,
            opponent_pr: None,
        };
    }

    let is_black = has_black && !has_white;
    let (opponent_name, player_score, opponent_score, player_pr, opponent_pr) = if is_black {
        (
            m.white.clone(),
            m.black_score,
            m.white_score,
            m.black_pr,
            m.white_pr,
        )
    } else {
        (
            m.black.clone(),
            m.white_score,
            m.black_score,
            m.white_pr,
            m.black_pr,
        )
    };

    let outcome = match m.result_state.as_str() {
        "white_won" => {
            if is_black {
                PlayerMatchOutcome::Loss
            } else {
                PlayerMatchOutcome::Win
            }
        }
        "black_won" => {
            if is_black {
                PlayerMatchOutcome::Win
            } else {
                PlayerMatchOutcome::Loss
            }
        }
        "draw" => PlayerMatchOutcome::Draw,
        _ => match (player_score, opponent_score) {
            (Some(ps), Some(os)) if ps > os => PlayerMatchOutcome::Win,
            (Some(ps), Some(os)) if ps < os => PlayerMatchOutcome::Loss,
            (Some(ps), Some(os)) if ps == os && m.result_state == "completed" => {
                PlayerMatchOutcome::Draw
            }
            _ => PlayerMatchOutcome::Unknown,
        },
    };

    PlayerMatchView {
        opponent_name,
        opponent_id: m.opponent_player_id,
        both_sides: false,
        player_score,
        opponent_score,
        outcome,
        player_pr,
        opponent_pr,
    }
}

/// Aggregated head-to-head statistics against a single opponent.
#[derive(Debug, Clone, PartialEq)]
pub struct OpponentBreakdownRow {
    pub opponent_id: Option<Uuid>,
    pub name: String,
    pub matches: usize,
    pub wins: usize,
    pub losses: usize,
    pub draws: usize,
    pub avg_pr: Option<f64>,
    pub opponent_avg_pr: Option<f64>,
}

/// Aggregates opponent breakdown rows from a list of player matches, sorted by
/// match count descending and then opponent name ascending (case-insensitive).
pub fn compute_opponent_breakdown(matches: &[DatabaseMatch]) -> Vec<OpponentBreakdownRow> {
    #[derive(Hash, PartialEq, Eq)]
    enum Key {
        Id(Uuid),
        Name(String),
    }

    struct Acc {
        opponent_id: Option<Uuid>,
        name: String,
        matches: usize,
        wins: usize,
        losses: usize,
        draws: usize,
        pr_sum: f64,
        pr_count: usize,
        opp_pr_sum: f64,
        opp_pr_count: usize,
    }

    let mut groups: HashMap<Key, Acc> = HashMap::new();
    for m in matches {
        let view = resolve_match_perspective(m);
        if view.both_sides {
            continue;
        }
        let key = match view.opponent_id {
            Some(id) => Key::Id(id),
            None => Key::Name(view.opponent_name.trim().to_ascii_lowercase()),
        };
        let entry = groups.entry(key).or_insert_with(|| Acc {
            opponent_id: view.opponent_id,
            name: view.opponent_name.clone(),
            matches: 0,
            wins: 0,
            losses: 0,
            draws: 0,
            pr_sum: 0.0,
            pr_count: 0,
            opp_pr_sum: 0.0,
            opp_pr_count: 0,
        });
        entry.matches += 1;
        match view.outcome {
            PlayerMatchOutcome::Win => entry.wins += 1,
            PlayerMatchOutcome::Loss => entry.losses += 1,
            PlayerMatchOutcome::Draw => entry.draws += 1,
            PlayerMatchOutcome::Unknown => {}
        }
        if let Some(pr) = view.player_pr {
            entry.pr_sum += pr;
            entry.pr_count += 1;
        }
        if let Some(opr) = view.opponent_pr {
            entry.opp_pr_sum += opr;
            entry.opp_pr_count += 1;
        }
    }

    let mut rows: Vec<OpponentBreakdownRow> = groups
        .into_values()
        .map(|acc| OpponentBreakdownRow {
            opponent_id: acc.opponent_id,
            name: acc.name,
            matches: acc.matches,
            wins: acc.wins,
            losses: acc.losses,
            draws: acc.draws,
            avg_pr: (acc.pr_count > 0).then(|| acc.pr_sum / (acc.pr_count as f64)),
            opponent_avg_pr: (acc.opp_pr_count > 0)
                .then(|| acc.opp_pr_sum / (acc.opp_pr_count as f64)),
        })
        .collect();

    rows.sort_by(|a, b| {
        b.matches.cmp(&a.matches).then_with(|| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        })
    });
    rows
}

/// First alphanumeric word with at least 3 characters used to seed Merge search,
/// mirroring `nameSeed` in `PlayerMergeDialog.tsx`.
pub fn merge_search_seed(display_name: &str) -> String {
    display_name
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| w.chars().count() >= 3)
        .unwrap_or("")
        .to_string()
}

/// Groups rating history points by analysis profile, preserving chronological order
/// within each profile and returning only profiles with at least 2 points.
pub fn rating_histories_by_profile(
    history: &[PlayerRatingPoint],
) -> Vec<(String, Vec<PlayerRatingPoint>)> {
    let mut order: Vec<String> = Vec::new();
    let mut grouped: HashMap<String, Vec<PlayerRatingPoint>> = HashMap::new();
    for pt in history {
        if !grouped.contains_key(&pt.profile) {
            order.push(pt.profile.clone());
        }
        grouped
            .entry(pt.profile.clone())
            .or_default()
            .push(pt.clone());
    }
    order
        .into_iter()
        .filter_map(|profile| {
            let pts = grouped.remove(&profile)?;
            (pts.len() >= 2).then_some((profile, pts))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};

    #[allow(clippy::too_many_arguments)]
    fn sample_match(
        id: Uuid,
        white: &str,
        black: &str,
        player_sides: Vec<i16>,
        opponent_id: Option<Uuid>,
        white_pr: Option<f64>,
        black_pr: Option<f64>,
        result_state: &str,
    ) -> DatabaseMatch {
        DatabaseMatch {
            occurrence: None,
            player_sides: Some(player_sides),
            opponent_player_id: opponent_id,
            id,
            source_available: true,
            play_session_id: None,
            play_paused: false,
            study_report_id: None,
            play_phase: None,
            play_mode: None,
            play_assisted: false,
            match_no: 1,
            filename: "sample.xg".into(),
            format: "xg".into(),
            white: white.into(),
            black: black.into(),
            match_length: 7,
            position_count: 100,
            created_at: Utc::now(),
            database_id: Uuid::new_v4(),
            display_name: "sample".into(),
            played_on: NaiveDate::from_ymd_opt(2026, 3, 1),
            tournament: None,
            round: None,
            place: None,
            result_state: result_state.into(),
            white_score: Some(7),
            black_score: Some(5),
            white_pr,
            black_pr,
            pr_method: None,
            pr_profile: None,
            analysis_status: None,
            game_count: 5,
            import_warning_count: 0,
            tags: Vec::new(),
            comment: String::new(),
            version: 1,
        }
    }

    #[test]
    fn choose_initial_selection_prefers_is_me_virtual_profile() {
        let other = VirtualPlayerProfile {
            id: Uuid::new_v4(),
            name: "Club".into(),
            is_me: false,
            version: 1,
            member_count: 2,
            match_count: 10,
            rateable_match_count: 10,
            game_count: 50,
        };
        let me = VirtualPlayerProfile {
            id: Uuid::new_v4(),
            name: "Hendrik".into(),
            is_me: true,
            version: 1,
            member_count: 3,
            match_count: 20,
            rateable_match_count: 18,
            game_count: 90,
        };
        assert_eq!(
            choose_initial_selection(&[other.clone(), me.clone()]),
            Some(PlayerSelection::Virtual(me.id))
        );
        assert_eq!(
            choose_initial_selection(std::slice::from_ref(&other)),
            Some(PlayerSelection::Virtual(other.id))
        );
        assert_eq!(choose_initial_selection(&[]), None);
    }

    #[test]
    fn filter_unrated_excludes_assisted_queued_completed_and_rated_matches() {
        let m_unrated_id = Uuid::new_v4();
        let m_unrated = sample_match(
            m_unrated_id,
            "Me",
            "Opp",
            vec![0],
            None,
            None,
            None,
            "white_won",
        );
        let m_rated = sample_match(
            Uuid::new_v4(),
            "Me",
            "Opp",
            vec![0],
            None,
            Some(4.2),
            None,
            "white_won",
        );

        let mut m_assisted = sample_match(
            Uuid::new_v4(),
            "Me",
            "Opp",
            vec![0],
            None,
            None,
            None,
            "white_won",
        );
        m_assisted.play_mode = Some(PlayMode::Competition);
        m_assisted.play_assisted = true;
        m_assisted.play_phase = Some("match_over".into());

        let mut m_queued = sample_match(
            Uuid::new_v4(),
            "Me",
            "Opp",
            vec![0],
            None,
            None,
            None,
            "white_won",
        );
        m_queued.analysis_status = Some("queued".into());

        let mut m_active_comp = sample_match(
            Uuid::new_v4(),
            "Me",
            "Opp",
            vec![0],
            None,
            None,
            None,
            "white_won",
        );
        m_active_comp.play_mode = Some(PlayMode::Competition);
        m_active_comp.play_phase = Some("human_move".into());

        let ids = filter_unrated_analyzable_match_ids(&[
            m_unrated,
            m_rated,
            m_assisted,
            m_queued,
            m_active_comp,
        ]);
        assert_eq!(ids, vec![m_unrated_id]);
    }

    #[test]
    fn compute_opponent_breakdown_aggregates_record_and_pr() {
        let opp_a = Uuid::new_v4();
        let opp_b = Uuid::new_v4();
        let m1 = sample_match(
            Uuid::new_v4(),
            "Me",
            "Alice",
            vec![0],
            Some(opp_a),
            Some(4.0),
            Some(6.0),
            "white_won",
        );
        let m2 = sample_match(
            Uuid::new_v4(),
            "Alice",
            "Me",
            vec![1],
            Some(opp_a),
            Some(5.0),
            Some(6.0),
            "white_won",
        );
        let m3 = sample_match(
            Uuid::new_v4(),
            "Me",
            "Bob",
            vec![0],
            Some(opp_b),
            Some(3.0),
            Some(7.0),
            "white_won",
        );

        let rows = compute_opponent_breakdown(&[m1, m2, m3]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].opponent_id, Some(opp_a));
        assert_eq!(rows[0].name, "Alice");
        assert_eq!(rows[0].matches, 2);
        assert_eq!(rows[0].wins, 1);
        assert_eq!(rows[0].losses, 1);
        assert!((rows[0].avg_pr.unwrap() - 5.0).abs() < 1e-6);
        assert!((rows[0].opponent_avg_pr.unwrap() - 5.5).abs() < 1e-6);
        assert_eq!(rows[1].opponent_id, Some(opp_b));
        assert_eq!(rows[1].wins, 1);
    }

    #[test]
    fn compute_pr_split_weights_by_decision_count() {
        let pts = vec![
            PlayerRatingPoint {
                match_id: Uuid::new_v4(),
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                profile: "3ply".into(),
                value: 2.0,
                decisions: 100,
                checker_value: Some(1.5),
                cube_value: Some(3.0),
            },
            PlayerRatingPoint {
                match_id: Uuid::new_v4(),
                date: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                profile: "3ply".into(),
                value: 6.0,
                decisions: 300,
                checker_value: Some(5.5),
                cube_value: None,
            },
        ];
        let summary = compute_pr_split_summary(&pts, Some("3ply"));
        assert!((summary.overall_pr.unwrap() - 5.0).abs() < 1e-6);
        assert!((summary.checker_pr.unwrap() - 4.5).abs() < 1e-6);
        assert!((summary.cube_pr.unwrap() - 3.0).abs() < 1e-6);
        assert_eq!(summary.total_decisions, 400);
        assert_eq!(summary.match_count, 2);
    }

    #[test]
    fn merge_search_seed_picks_first_three_char_alphanumeric_word() {
        assert_eq!(merge_search_seed("H. Reh"), "Reh");
        assert_eq!(merge_search_seed("Alice-BG"), "Alice");
        assert_eq!(merge_search_seed("AB"), "");
    }
}
