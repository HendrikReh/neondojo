//! Portable checker evaluations captured while Open Sage plays an engine duel.
use super::{ImportedAnalysisData, ImportedCandidate, base};
use crate::{
    engines::{SageCheckerCandidate, SageSnapshot, validate_evaluation},
    import::sgf::{
        SourceMove, decode_retained_analysis, encode_retained_analysis, parse_sgf_source_moves,
        parse_trees, serialize_trees,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Serialize, Deserialize)]
pub(crate) struct RetainedChecker {
    version: u32,
    native_id: String,
    package_version: String,
    artifact_fingerprint: String,
    plies: i32,
    threads: u32,
    adapter_version: u32,
    candidates: Vec<SageCheckerCandidate>,
}

impl RetainedChecker {
    pub(crate) fn new(
        native_id: String,
        snapshot: &SageSnapshot,
        mut candidates: Vec<SageCheckerCandidate>,
    ) -> Self {
        // The played move is the best candidate; retain the same top-five scope
        // as position analysis without copying host-local installation paths.
        candidates.truncate(5);
        Self {
            version: 1,
            native_id,
            package_version: snapshot.package_version.clone(),
            artifact_fingerprint: snapshot.artifact_fingerprint.clone(),
            plies: snapshot.plies,
            threads: snapshot.threads,
            adapter_version: snapshot.adapter_version,
            candidates,
        }
    }

    fn record(self, source: &SourceMove) -> Result<ImportedAnalysisData> {
        ensure!(
            self.version == 1 && matches!(self.plies, 1 | 2),
            "Unsupported retained Open Sage evaluation"
        );
        ensure!(
            source.kind == "checker"
                && self.native_id == format!("{}:{}", source.position_id, source.match_id),
            "Retained Open Sage checker context differs from played move"
        );
        ensure!(
            !self.candidates.is_empty() && self.candidates.len() <= 5,
            "Invalid retained Open Sage candidates"
        );
        ensure!(
            self.candidates[0].board.as_slice() == source.after_board,
            "Retained Open Sage choice differs from played board"
        );
        for candidate in &self.candidates {
            validate_evaluation(candidate.equity, &candidate.probabilities)?;
        }
        let mut record = base(
            source,
            "checker",
            "Open Sage",
            Some(self.package_version.clone()),
        );
        record.method = "evaluation".into();
        record.depth = Some(format!("{}-ply cubeful", self.plies));
        record.depth_plies = Some(self.plies);
        record.raw = serde_json::to_value(&self)?;
        record.candidates = self.candidates.into_iter().enumerate().map(|(index, c)| {
            let p = c.probabilities;
            ImportedCandidate {
                rank: index as i32 + 1,
                notation: c.notation,
                equity: Some(c.equity),
                equity_kind: "cubeful".into(),
                probabilities: json!({"win":p[0], "win_gammon":p[1], "win_backgammon":p[2], "lose_gammon":p[4], "lose_backgammon":p[5]}),
                depth: record.depth.clone(),
                depth_plies: record.depth_plies,
                method: Some(record.method.clone()),
                raw: json!({"board":c.board}),
            }
        }).collect();
        Ok(record)
    }
}

pub(super) fn extract(source: &SourceMove, value: &str) -> Result<ImportedAnalysisData> {
    let saved: RetainedChecker = serde_json::from_value(decode_retained_analysis(value)?)
        .context("Invalid retained Open Sage checker evaluation")?;
    saved.record(source)
}

pub(crate) fn retain(sgf: &str, evaluations: Vec<(usize, RetainedChecker)>) -> Result<String> {
    if evaluations.is_empty() {
        return Ok(sgf.to_owned());
    }
    let (_, moves) = parse_sgf_source_moves(sgf)?;
    let mut trees = parse_trees(sgf)?;
    let mut nodes: Vec<_> = trees
        .iter_mut()
        .flatten()
        .filter(|node| node.contains_key("W") || node.contains_key("B"))
        .collect();
    ensure!(
        nodes.len() == moves.len(),
        "Engine duel action count differs from SGF"
    );
    for (ordinal, evaluation) in evaluations {
        let source = moves
            .get(ordinal)
            .context("Missing engine duel checker move")?;
        let value = encode_retained_analysis(&serde_json::to_value(&evaluation)?)?;
        evaluation.record(source)?;
        ensure!(
            nodes[ordinal].insert("BGTSC".into(), vec![value]).is_none(),
            "Duplicate retained Open Sage evaluation"
        );
    }
    Ok(serialize_trees(&trees))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "(;FF[4]GM[6];W[31hefe])";

    fn evaluation() -> RetainedChecker {
        let (_, moves) = parse_sgf_source_moves(SOURCE).unwrap();
        let m = &moves[0];
        RetainedChecker {
            version: 1,
            native_id: format!("{}:{}", m.position_id, m.match_id),
            package_version: "2.0.20260907".into(),
            artifact_fingerprint: "a".repeat(64),
            plies: 2,
            threads: 4,
            adapter_version: 1,
            candidates: vec![SageCheckerCandidate {
                board: m.after_board.clone().try_into().unwrap(),
                notation: "8/5 6/5".into(),
                equity: 0.125,
                probabilities: vec![0.6, 0.2, 0.03, 0.4, 0.1, 0.01],
            }],
        }
    }

    #[test]
    fn retained_checker_round_trip_preserves_values_and_portable_provenance() {
        let sgf = retain(SOURCE, vec![(0, evaluation())]).unwrap();
        let records = crate::imported_analysis::extract(&sgf).unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.engine, "Open Sage");
        assert_eq!(r.engine_version.as_deref(), Some("2.0.20260907"));
        assert_eq!(r.depth_plies, Some(2));
        assert_eq!(r.candidates[0].equity, Some(0.125));
        assert_eq!(r.candidates[0].notation, "8/5 6/5");
        assert_eq!(
            r.candidates[0].probabilities,
            json!({"win":0.6,"win_gammon":0.2,"win_backgammon":0.03,"lose_gammon":0.1,"lose_backgammon":0.01})
        );
        assert_eq!(r.raw["artifact_fingerprint"], "a".repeat(64));
        assert!(r.raw.get("python").is_none());
        assert_eq!(retain(SOURCE, vec![]).unwrap(), SOURCE);
    }

    #[test]
    fn retained_checker_rejects_wrong_occurrence_choice_and_invalid_values() {
        let mut wrong_context = evaluation();
        wrong_context.native_id.push('A');
        assert!(
            retain(SOURCE, vec![(0, wrong_context)])
                .unwrap_err()
                .to_string()
                .contains("context differs")
        );
        let mut wrong_board = evaluation();
        wrong_board.candidates[0].board.swap(1, 6);
        assert!(
            retain(SOURCE, vec![(0, wrong_board)])
                .unwrap_err()
                .to_string()
                .contains("played board")
        );
        let mut wrong_probabilities = evaluation();
        wrong_probabilities.candidates[0].probabilities = vec![0.5];
        assert!(retain(SOURCE, vec![(0, wrong_probabilities)]).is_err());
        assert!(retain(SOURCE, vec![(1, evaluation())]).is_err());
        assert!(retain(SOURCE, vec![(0, evaluation()), (0, evaluation())]).is_err());
    }
}
