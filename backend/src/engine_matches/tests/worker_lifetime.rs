use super::{DuelUnit, GameDice, dice, engine_match};
use crate::{
    engine::Engine,
    engines::{AnalysisEngine, SageEngine},
};
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Instant};
use uuid::Uuid;

/// Count native work separately from installation probes and delegate each
/// invocation to the configured executable.
fn counted_engine(directory: &Path, name: &str, program: &str) -> String {
    fs::File::create(directory.join(format!("{name}.starts"))).unwrap();
    let path = directory.join(name);
    let script = format!(
        r#"#!/bin/sh
if [ "$1" != --version ]; then
    echo start >> "$0.starts"
fi
exec {program} "$@"
"#
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    path.to_str().unwrap().to_owned()
}

#[tokio::test]
#[ignore = "requires GNUbg Python and locked NEONDOJO_SAGE_PYTHON"]
async fn two_neongammon_depths_reuse_workers_and_one_rules_process_per_game() {
    let dir = tempfile::tempdir().unwrap();
    let rules = Engine::new(counted_engine(
        dir.path(),
        "rules",
        r#""${GNUBG_BIN:-gnubg}""#,
    ));
    let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
    let bot = async |plies: i32, name: &str| {
        let mut snapshot = SageEngine::probe_profile(python.clone(), plies, 4)
            .await
            .unwrap();
        snapshot.python = counted_engine(dir.path(), name, r#""$NEONDOJO_SAGE_PYTHON""#);
        AnalysisEngine::OpenSage(SageEngine::new(snapshot).unwrap())
    };
    let first = bot(2, "two").await;
    let second = bot(3, "three").await;
    let starts = |name: &str| {
        fs::read_to_string(dir.path().join(format!("{name}.starts")))
            .unwrap()
            .lines()
            .count()
    };
    let rules_at_game_open = std::cell::RefCell::new(Vec::new());
    let seed = Uuid::from_u128(13);
    let started = Instant::now();
    let finished = engine_match::play(
        &rules,
        [&first, &second],
        [2, 3],
        DuelUnit::Match { length: 3 },
        |game| {
            rules_at_game_open.borrow_mut().push(starts("rules"));
            Ok(GameDice {
                opening: dice::opening(seed, game)?,
                turns: dice::turns(seed, game),
            })
        },
    )
    .await
    .unwrap();
    let elapsed = started.elapsed();
    let (_, moves) = crate::import::sgf::parse_sgf_source_moves(&finished.sgf).unwrap();
    let records = crate::imported_analysis::extract(&finished.sgf).unwrap();
    // GNUbg enumerates legal moves for both bots. Its internal hints must
    // never appear in the exported SGF as additional checker evaluations.
    assert!(
        records
            .iter()
            .filter(|r| r.kind == "checker")
            .all(|r| r.engine == "NeonGammon")
    );
    for (side, depth) in [(0, 2), (1, 3)] {
        let turns: Vec<_> = moves
            .iter()
            .filter(|m| m.player == side && m.kind == "checker")
            .collect();
        let cost = finished.costs[side as usize].unwrap();
        assert_eq!(cost.moves as usize, turns.len());
        assert!(cost.cost.cpu_us > 0 && cost.cost.wall_us > 0);
        for turn in turns.into_iter().filter(|t| t.board != t.after_board) {
            let record = records
                .iter()
                .find(|r| {
                    r.game_no == turn.game_no
                        && r.source_move_no == turn.move_no
                        && r.kind == "checker"
                })
                .unwrap();
            assert_eq!(record.engine, "NeonGammon");
            assert_eq!(record.depth_plies, Some(depth));
            assert_eq!(
                record.position_id.as_deref(),
                Some(turn.position_id.as_str())
            );
            assert_eq!(
                record.gnubg_match_id.as_deref(),
                Some(turn.match_id.as_str())
            );
            assert!(!record.candidates.is_empty());
        }
    }
    let transcript: Vec<_> = moves
        .iter()
        .map(|turn| {
            (
                turn.game_no,
                turn.player,
                turn.dice,
                &turn.notation,
                &turn.position_id,
                &turn.match_id,
            )
        })
        .collect();
    let digest = |data: &[u8]| format!("{:x}", Sha256::digest(data));
    let worker_starts = [starts("two"), starts("three")];
    let rules_starts = starts("rules");
    let mut boundaries = rules_at_game_open.into_inner();
    boundaries.push(rules_starts);
    let per_game: Vec<_> = boundaries
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    eprintln!(
        "games={} scores={:?} turns={} retained={} workers={worker_starts:?} rules_per_game={per_game:?} elapsed={elapsed:.2?} transcript={} evaluations={}",
        finished.games,
        finished.scores,
        moves.len(),
        records.len(),
        digest(&serde_json::to_vec(&transcript).unwrap()),
        digest(&serde_json::to_vec(&records).unwrap())
    );
    assert!(finished.games > 1, "exercise reuse across games");
    assert_eq!(worker_starts, [1, 1]);
    assert_eq!(per_game, vec![1; finished.games as usize]);
}
