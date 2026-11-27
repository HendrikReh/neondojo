//! Portable checker evaluations captured while NeonGammon plays an engine duel.
use super::{ImportedAnalysisData, ImportedCandidate, base};
use crate::{
    engines::{SageCheckerCandidate, SageSnapshot, valid_sage_depth, validate_evaluation},
    import::sgf::{
        SourceMove, decode_retained_analysis, encode_retained_analysis, parse_sgf_events,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cubeless: Option<bool>,
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
            cubeless: Some(snapshot.cubeless),
            candidates,
        }
    }

    fn record(self, source: &SourceMove, cube_enabled: bool) -> Result<ImportedAnalysisData> {
        ensure!(
            self.version == 1 && valid_sage_depth(self.plies),
            "Unsupported retained NeonGammon evaluation"
        );
        ensure!(
            source.kind == "checker"
                && self.native_id == format!("{}:{}", source.position_id, source.match_id),
            "Retained NeonGammon checker context differs from played move"
        );
        ensure!(
            !self.candidates.is_empty() && self.candidates.len() <= 5,
            "Invalid retained NeonGammon candidates"
        );
        ensure!(
            self.candidates[0].board.as_slice() == source.after_board,
            "Retained NeonGammon choice differs from played board"
        );
        for candidate in &self.candidates {
            validate_evaluation(candidate.equity, &candidate.probabilities)?;
        }
        let mut record = base(
            source,
            "checker",
            "NeonGammon",
            Some(self.package_version.clone()),
        );
        record.method = "evaluation".into();
        // Legacy duel exports omit the profile flag. Those duels used a
        // cubeless evaluator exactly when the game's SGF rules disabled the cube.
        let mode = if self.cubeless.unwrap_or(!cube_enabled) {
            "cubeless"
        } else {
            "cubeful"
        };
        record.depth = Some(format!("{}-ply {mode}", self.plies));
        record.depth_plies = Some(self.plies);
        record.raw = serde_json::to_value(&self)?;
        record.candidates = self.candidates.into_iter().enumerate().map(|(index, c)| {
            let p = c.probabilities;
            ImportedCandidate {
                rank: index as i32 + 1,
                notation: c.notation,
                equity: Some(c.equity),
                equity_kind: mode.into(),
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

pub(super) fn extract(
    source: &SourceMove,
    cube_enabled: bool,
    value: &str,
) -> Result<ImportedAnalysisData> {
    let saved: RetainedChecker = serde_json::from_value(decode_retained_analysis(value)?)
        .context("Invalid retained NeonGammon checker evaluation")?;
    saved.record(source, cube_enabled)
}

pub(crate) fn retain(sgf: &str, evaluations: Vec<(usize, RetainedChecker)>) -> Result<String> {
    if evaluations.is_empty() {
        return Ok(sgf.to_owned());
    }
    let (_, games, moves) = parse_sgf_events(sgf)?;
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
        evaluation.record(source, games[(source.game_no - 1) as usize].cube_enabled)?;
        ensure!(
            nodes[ordinal].insert("BGTSC".into(), vec![value]).is_none(),
            "Duplicate retained NeonGammon evaluation"
        );
    }
    Ok(serialize_trees(&trees))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::sgf::parse_sgf_source_moves;

    const SOURCE: &str = "(;FF[4]GM[6];W[31hefe])";
    const CUBELESS_SOURCE: &str = "(;FF[4]GM[6]RU[NoCube];W[31hefe])";

    fn evaluation() -> RetainedChecker {
        let (_, moves) = parse_sgf_source_moves(SOURCE).unwrap();
        let m = &moves[0];
        RetainedChecker {
            version: 1,
            native_id: format!("{}:{}", m.position_id, m.match_id),
            package_version: "3.0.0".into(),
            artifact_fingerprint: "a".repeat(64),
            plies: 2,
            threads: 4,
            adapter_version: 1,
            cubeless: None,
            candidates: vec![SageCheckerCandidate {
                board: m.after_board.clone().try_into().unwrap(),
                notation: "8/5 6/5".into(),
                equity: 0.125,
                probabilities: vec![0.6, 0.2, 0.03, 0.4, 0.1, 0.01],
            }],
        }
    }

    fn legacy_source(source: &str) -> (String, serde_json::Value) {
        let (_, moves) = parse_sgf_source_moves(source).unwrap();
        let mut legacy = serde_json::to_value(evaluation()).unwrap();
        legacy.as_object_mut().unwrap().remove("cubeless");
        legacy["native_id"] = json!(format!("{}:{}", moves[0].position_id, moves[0].match_id));
        let value = encode_retained_analysis(&legacy).unwrap();
        let sgf = source.replace("W[31hefe]", &format!("W[31hefe]BGTSC[{value}]"));
        (sgf, legacy)
    }

    #[test]
    fn retained_checker_round_trip_preserves_values_and_portable_provenance() {
        let sgf = retain(SOURCE, vec![(0, evaluation())]).unwrap();
        let records = crate::imported_analysis::extract(&sgf).unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.engine, "NeonGammon");
        assert_eq!(r.engine_version.as_deref(), Some("3.0.0"));
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
    fn retained_checker_round_trip_uses_the_evaluation_profile_mode() {
        for (cubeless, source, mode) in [
            (false, SOURCE, "cubeful"),
            (true, CUBELESS_SOURCE, "cubeless"),
        ] {
            let (_, moves) = parse_sgf_source_moves(source).unwrap();
            let m = &moves[0];
            let original = evaluation();
            let snapshot = SageSnapshot {
                python: "/host-local/python".into(),
                package_version: original.package_version,
                artifact_fingerprint: original.artifact_fingerprint,
                plies: original.plies,
                threads: original.threads,
                adapter_version: original.adapter_version,
                one_ply_verified: false,
                cubeless,
            };
            let saved = RetainedChecker::new(
                format!("{}:{}", m.position_id, m.match_id),
                &snapshot,
                original.candidates,
            );
            let sgf = retain(source, vec![(0, saved)]).unwrap();
            let records = crate::imported_analysis::extract(&sgf).unwrap();
            let record = &records[0];
            let depth = format!("2-ply {mode}");
            assert_eq!(record.raw["cubeless"], cubeless);
            assert_eq!(record.depth.as_deref(), Some(depth.as_str()));
            assert_eq!(record.candidates[0].depth, record.depth);
            assert_eq!(record.candidates[0].equity_kind, mode);
            assert_eq!(record.candidates[0].equity, Some(0.125));
        }
    }

    #[test]
    fn legacy_retained_checker_uses_each_games_cube_rule() {
        let (cube, cube_raw) = legacy_source(SOURCE);
        let (cubeless, cubeless_raw) = legacy_source(CUBELESS_SOURCE);
        let records = crate::imported_analysis::extract(&format!("{cube}{cubeless}")).unwrap();
        assert_eq!(records.len(), 2);
        for (record, (mode, legacy)) in records
            .iter()
            .zip([("cubeful", cube_raw), ("cubeless", cubeless_raw)])
        {
            assert_eq!(record.candidates[0].equity_kind, mode);
            assert_eq!(record.depth, Some(format!("2-ply {mode}")));
            assert_eq!(record.raw, legacy);
        }
    }

    #[sqlx::test(migrations = false)]
    async fn upgrading_repairs_only_legacy_cubeless_retained_labels(pool: sqlx::PgPool) {
        use sqlx::migrate::Migrate;
        let mut db = pool.acquire().await.unwrap();
        db.ensure_migrations_table().await.unwrap();
        for migration in crate::schema_migrations::MIGRATOR
            .iter()
            .filter(|m| m.version <= 96)
        {
            db.apply(migration).await.unwrap();
        }
        drop(db);
        let (cube, _) = legacy_source(SOURCE);
        let (cubeless, _) = legacy_source(CUBELESS_SOURCE);
        let sgf = format!("{cube}{cubeless}{cubeless}");
        let imported = crate::import::parse_sgf(&sgf).unwrap();
        let id = crate::store::import_match(
            &pool,
            "older-duel.sgf",
            "sgf",
            sgf.as_bytes(),
            &sgf,
            &imported,
        )
        .await
        .unwrap()
        .r#match
        .id;
        // Model a cube rule that migration 0095 marked for replay.
        sqlx::query("UPDATE match_games SET cube_enabled=NULL WHERE match_uuid=$1 AND game_no=3")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        // Reproduce the old catalog labels even after the decoder is fixed.
        sqlx::query(
            "UPDATE imported_analyses SET data=jsonb_set(jsonb_set(jsonb_set(
            data, '{depth}', '\"2-ply cubeful\"'), '{candidates,0,depth}', '\"2-ply cubeful\"'),
            '{candidates,0,equity_kind}', '\"cubeful\"') WHERE match_uuid=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO imported_analysis_projections (imported_analysis_id,revision,projection)
            SELECT id,2,'{}' FROM imported_analyses WHERE match_uuid=$1",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        let before: Vec<(uuid::Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT id,data FROM imported_analyses WHERE match_uuid=$1 ORDER BY game_no",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();

        crate::schema_migrations::run(&pool).await.unwrap();
        let after: Vec<(uuid::Uuid, serde_json::Value)> = sqlx::query_as(
            "SELECT id,data FROM imported_analyses WHERE match_uuid=$1 ORDER BY game_no",
        )
        .bind(id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(after.len(), 3);
        assert_eq!(after[0], before[0]);
        for (original, repaired) in before[1..].iter().zip(&after[1..]) {
            let (original_id, mut expected) = original.clone();
            expected["depth"] = json!("2-ply cubeless");
            expected["candidates"][0]["depth"] = json!("2-ply cubeless");
            expected["candidates"][0]["equity_kind"] = json!("cubeless");
            assert_eq!(repaired, &(original_id, expected));
        }
        let cached: Vec<uuid::Uuid> =
            sqlx::query_scalar("SELECT imported_analysis_id FROM imported_analysis_projections")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(cached, vec![before[0].0]);
        let original: Vec<u8> = sqlx::query_scalar("SELECT original FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(original, sgf.as_bytes());
    }

    #[test]
    fn retained_checker_preserves_duel_depths_above_two() {
        for plies in [3, 4] {
            let mut saved = evaluation();
            saved.plies = plies;
            let sgf = retain(SOURCE, vec![(0, saved)]).unwrap();
            let records = crate::imported_analysis::extract(&sgf).unwrap();
            assert_eq!(records[0].depth_plies, Some(plies));
        }
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
