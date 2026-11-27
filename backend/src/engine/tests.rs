use super::parse::{ROLL_MARKER, RollProgress, gnubg_diagnostics, split_rolls};
use super::*;

#[test]
fn reports_each_gnubg_diagnostic_line_once() {
    let stderr = "input.mat: not a valid .mat file\n".repeat(6);
    assert_eq!(
        gnubg_diagnostics(&stderr),
        "input.mat: not a valid .mat file"
    );
    assert_eq!(
        gnubg_diagnostics("  first\n\nsecond\r\nfirst\n second \n"),
        "first\nsecond"
    );
    assert_eq!(gnubg_diagnostics(" \n\t\n"), "");
}

#[test]
fn parses_ranked_engine_hints_and_rejects_missing_results() {
    let output = "    1. Cubeful 0-ply    8/5 6/5                      Eq.: +0.219\n       0.551 0.174 0.013 - 0.449 0.124 0.005\n        0-ply cubeful prune [expert]\n    2. Cubeful 0-ply    24/23 13/10                  Eq.: -0.012 (-0.231)\n       0.497 0.137 0.008 - 0.503 0.140 0.007\n";
    let hints = parse_hints(output, true).unwrap();
    assert_eq!(hints.len(), 2);
    assert_eq!(hints[0].notation, "8/5 6/5");
    assert_eq!(hints[0].equity, 0.219);
    assert_eq!(hints[0].probabilities[0], 0.551);
    assert!(parse_hints("No game in progress", true).is_err());
    assert!(parse_full_checker_hints(output, 2, true).is_err());
    let single = output.split("    2.").next().unwrap();
    // GNUbg's native 0-ply is NeonDojo's 1-ply raw evaluation.
    assert_eq!(parse_full_checker_hints(single, 1, true).unwrap().1, 1);
    assert_eq!(hints[0].plies, Some(1));
}

#[cfg(unix)]
#[tokio::test]
async fn installation_check_bounds_a_hanging_evaluation() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("hanging-gnubg");
    std::fs::write(
        &binary,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'GNU Backgammon fixture'; exit 0; fi\nexec sleep 60\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = Engine::new(binary.to_str().unwrap().to_owned())
        .with_installation_timeout(Duration::from_millis(300));
    let started = Instant::now();
    let error = engine.check_installation().await.unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error:#}");
    // The timed-out evaluation is dropped, which kills the hanging child.
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// A wrapper fixture whose run executes `during_run` and logs every launch.
#[cfg(unix)]
fn fenced_wrapper(during_run: &str) -> (tempfile::TempDir, Engine) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().to_str().unwrap();
    std::fs::write(directory.path().join("version"), "GNU Backgammon fixture\n").unwrap();
    let binary = directory.path().join("gnubg-wrapper");
    std::fs::write(
        &binary,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo version >>'{root}/launches'; cat '{root}/version'; exit 0; fi\necho run >>'{root}/launches'\ncat '{root}/version'\n{during_run}\n"
        ),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = Engine::new(binary.to_str().unwrap().to_owned())
        .with_expected_version(Some("GNU Backgammon fixture".into()));
    (directory, engine)
}

#[cfg(unix)]
#[tokio::test]
async fn saved_version_run_rechecks_a_wrapper_and_its_bytes() {
    let (directory, engine) = fenced_wrapper("");
    let output = engine
        .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
        .await
        .unwrap();
    assert!(output.starts_with("GNU Backgammon fixture"));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("launches")).unwrap(),
        "version\nrun\nversion\n"
    );
    // Without a saved version no invocation is fenced.
    let unfenced = Engine::new(engine.binary().to_owned());
    std::fs::remove_file(directory.path().join("launches")).unwrap();
    unfenced
        .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(directory.path().join("launches")).unwrap(),
        "run\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn saved_version_run_discards_output_after_same_version_replacement() {
    // The replacement reports the same version, so only its bytes differ.
    let (directory, engine) = fenced_wrapper(
        "cp \"$0\" \"$0.new\"; echo '# replaced' >>\"$0.new\"; mv \"$0.new\" \"$0\"",
    );
    let error = engine
        .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("GNUbg changed during execution") && error.contains("installation changed"),
        "{error}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn saved_version_run_rechecks_the_wrapper_target_version() {
    let (directory, engine) =
        fenced_wrapper("echo 'GNU Backgammon new' >\"$(dirname \"$0\")/version\"");
    let error = engine
        .run_with_timeout(directory.path(), "", std::time::Duration::from_secs(5))
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("GNUbg changed during execution")
            && error.contains("found 'GNU Backgammon new'"),
        "{error}"
    );
}

/// The real executable copied to a temporary path, so it is a direct
/// binary whose bytes the test can replace.
#[cfg(unix)]
fn copied_gnubg() -> (tempfile::TempDir, String, String) {
    let source = resolve_binary(&std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("gnubg");
    std::fs::copy(&source, &binary).unwrap();
    let version = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let version = version.lines().next().unwrap().trim().to_owned();
    (directory, binary.to_str().unwrap().to_owned(), version)
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires a local native GNUbg binary"]
async fn saved_version_run_of_a_direct_binary_checks_its_banner_and_bytes() {
    use std::time::{Duration, Instant};
    let (directory, binary, version) = copied_gnubg();
    let work = tempfile::tempdir().unwrap();
    let fenced = Engine::new(binary.clone()).with_expected_version(Some(version.clone()));
    let output = fenced
        .run_with_timeout(work.path(), "show threads\n", Duration::from_secs(10))
        .await
        .unwrap();
    assert!(output.starts_with(&version));

    // The banner of the running process is the saved-version check.
    let error = Engine::new(binary.clone())
        .with_expected_version(Some("GNU Backgammon 0.0".into()))
        .run_with_timeout(work.path(), "show threads\n", Duration::from_secs(10))
        .await
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("Cannot execute the saved GNUbg job configuration")
            && error.contains(&format!("found '{version}'")),
        "{error}"
    );

    // Fencing cost of one run: two version probes before, the bytes now.
    let unfenced = Engine::new(binary.clone());
    let started = Instant::now();
    for _ in 0..5 {
        unfenced.version().await.unwrap();
        unfenced.version().await.unwrap();
    }
    let probes = started.elapsed() / 5;
    let started = Instant::now();
    for _ in 0..5 {
        let installation = Installation::bind(&fenced).await.unwrap();
        installation.recheck(&fenced).await.unwrap();
    }
    eprintln!(
        "fence per run: two --version probes {probes:?}, digest before and after {:?}",
        started.elapsed() / 5
    );

    // A replacement after the start reports the same version, yet differs.
    let replace = format!(
        "cp '{binary}' '{binary}.new' && printf x >>'{binary}.new' && mv '{binary}.new' '{binary}'"
    );
    let installation = Installation::bind(&fenced).await.unwrap();
    assert!(installation.reports_version_at_startup());
    assert!(
        std::process::Command::new("/bin/sh")
            .args(["-c", &replace])
            .status()
            .unwrap()
            .success()
    );
    let error = installation.recheck(&fenced).await.unwrap_err();
    assert!(
        format!("{error:#}").contains("installation changed"),
        "{error:#}"
    );
    drop(directory);
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_finds_opening_31() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let result = engine
        .analyze("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, None)
        .await
        .unwrap();
    assert_eq!(result.candidates[0].notation, "8/5 6/5");
    assert!(result.candidates[0].equity > 0.1);
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_position_hint_retains_the_native_move_path() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let result = engine
        .analyze("4HPwATDgc/ABMA", "cAmvAgAAAAAE", 2, None)
        .await
        .unwrap();
    let candidate = result
        .candidates
        .iter()
        .find(|c| c.notation == "24/15")
        .unwrap();
    assert_eq!(candidate.path.as_deref(), Some("24/18 18/15"));
}

#[tokio::test]
#[ignore = "requires local GNUbg with weights"]
async fn real_gnubg_evaluates_a_forced_pass() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let result = engine
        .analyze("Y54pATDg28EBYA", "cAlnAQAAAAAE", 1, None)
        .await
        .unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(result.candidates[0].notation, "dance");
    assert!(result.candidates[0].equity < -0.9);
    assert!(result.candidates[0].probabilities[0] < 0.4);
}

fn all_rolls() -> Vec<[i32; 2]> {
    (1..=6).flat_map(|a| (a..=6).map(move |b| [a, b])).collect()
}

/// The batch must reproduce each individual evaluation exactly.
async fn assert_batch_matches_individual_rolls(
    engine: &Engine,
    position_id: &str,
    match_id: &str,
    plies: i32,
) -> Vec<Analysis> {
    let rolls = all_rolls();
    let mut reported = Vec::new();
    let batch = engine
        .analyze_rolls(position_id, match_id, plies, &rolls, |done| {
            reported.push(done);
            std::future::ready(Ok(()))
        })
        .await
        .unwrap();
    assert_eq!(reported.last(), Some(&21));
    assert!(
        reported.windows(2).all(|pair| pair[0] <= pair[1]),
        "{reported:?}"
    );
    for (dice, analysis) in rolls.iter().zip(&batch) {
        let single = engine
            .analyze(position_id, match_id, plies, Some(*dice))
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(analysis).unwrap(),
            serde_json::to_value(&single).unwrap(),
            "{dice:?} at {plies}-ply"
        );
    }
    batch
}

#[tokio::test]
#[ignore = "requires local GNUbg"]
async fn real_gnubg_roll_batch_matches_individual_evaluations() {
    let binary = std::env::var("GNUBG_BIN").unwrap_or("gnubg".into());
    let engine = Engine::new(binary.clone());
    let opening =
        assert_batch_matches_individual_rolls(&engine, "4HPwATDgc/ABMA", "cInlAAAAAAAE", 1).await;
    assert!(
        opening
            .iter()
            .all(|analysis| analysis.candidates[0].path.is_some())
    );
    // NeonDojo's 3-ply is GNUbg's 2-ply, pruned by the analysis move filter.
    let filtered = Engine::new(binary).with_move_filter(Some(GnubgMoveFilter::Top14));
    assert_batch_matches_individual_rolls(&filtered, "4HPwATDgc/ABMA", "cInlAAAAAAAE", 3).await;
}

#[tokio::test]
#[ignore = "requires local GNUbg"]
async fn real_gnubg_roll_batch_evaluates_forced_passes_in_place() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let analyses =
        assert_batch_matches_individual_rolls(&engine, "Y54pATDg28EBYA", "cAlnAQAAAAAE", 1).await;
    let passes: Vec<[i32; 2]> = all_rolls()
        .into_iter()
        .zip(&analyses)
        .filter(|(_, analysis)| analysis.candidates[0].notation == "dance")
        .map(|(dice, _)| dice)
        .collect();
    assert_eq!(passes, [[1, 1], [1, 4], [1, 6], [4, 4], [4, 6], [6, 6]]);
}

#[tokio::test]
#[ignore = "requires local GNUbg"]
async fn real_gnubg_roll_batch_stops_when_progress_cannot_be_saved() {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let error = engine
        .analyze_rolls("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, &all_rolls(), |done| {
            std::future::ready(if done > 0 {
                Err(anyhow::anyhow!("Job lease was lost"))
            } else {
                Ok(())
            })
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Job lease was lost");
}

#[cfg(unix)]
fn dice_fixture(script: &str) -> (tempfile::TempDir, Engine) {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("dice-fixture");
    std::fs::write(&binary, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let engine = Engine::new(binary.to_str().unwrap().to_owned());
    (directory, engine)
}

#[cfg(unix)]
#[tokio::test]
async fn dice_progress_never_finishes_on_extra_markers() {
    let (directory, mut engine) = dice_fixture(
        r#"printf '%s\n' "$$" > "${0%/*}/pid"
         printf '%s\n' 'GNU Backgammon fixture' \
          'The dice have been set to 1 and 1.' \
          'The dice have been set to 1 and 2.' \
          'The dice have been set to 1 and 3.' \
          'The dice have been set to 1 and 4.'
         exec sleep 60"#,
    );
    assert_eq!(engine.roll_timeout, std::time::Duration::from_secs(90));
    engine.roll_timeout = std::time::Duration::from_millis(500);
    let mut reports = Vec::new();
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        engine.analyze_rolls(
            "4HPwATDgc/ABMA",
            "cInlAAAAAAAE",
            1,
            &[[1, 1], [1, 2]],
            |done| {
                reports.push(done);
                std::future::ready(Ok(()))
            },
        ),
    )
    .await
    .expect("the batch must enforce its scaled deadline")
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("GNUbg timed out after 1 seconds"),
        "{error:#}"
    );
    assert_eq!(reports, [1]);
    let pid = std::fs::read_to_string(directory.path().join("pid")).unwrap();
    let process = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .output()
        .unwrap();
    assert!(!process.status.success(), "timed-out child was not reaped");
}

#[cfg(unix)]
#[tokio::test]
async fn dice_batch_rejects_output_past_the_limit() {
    let (_directory, engine) = dice_fixture("exec head -c 8388609 /dev/zero");
    let error = engine
        .analyze_rolls("4HPwATDgc/ABMA", "cInlAAAAAAAE", 1, &[[1, 1]], |_| {
            std::future::ready(Ok(()))
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "GNUbg output limit exceeded");
}

#[test]
fn roll_progress_counts_markers_once_across_read_boundaries() {
    let output = b"banner\nThe dice have been set to 1 and 1.\nanalysis\nThe dice have been set to 1 and 2.\nanalysis\nThe dice have been set to 1 and 3.\n";
    let marker = format!("\n{ROLL_MARKER}");
    for chunk_size in 1..=marker.len() + 1 {
        let mut progress = RollProgress::default();
        for end in (chunk_size..output.len())
            .step_by(chunk_size)
            .chain([output.len()])
        {
            let prefix = &output[..end];
            let expected = prefix
                .windows(marker.len())
                .filter(|window| *window == marker.as_bytes())
                .count()
                .saturating_sub(1);
            assert_eq!(
                progress.finished_rolls(prefix),
                expected,
                "chunk size {chunk_size}, length {end}"
            );
        }
    }
}

#[test]
#[ignore = "requires replaying a synthetic 8 MiB GNUbg batch in a release build"]
fn benchmark_dice_progress_replay() {
    use std::{hint::black_box, time::Instant};
    const CHUNK_BYTES: usize = 8 * 1024;
    for bytes in [80 * 1024, 8 * 1024 * 1024] {
        let mut output = vec![b'x'; bytes];
        for end in output.iter_mut().skip(79).step_by(80) {
            *end = b'\n';
        }
        let rolls = all_rolls();
        for (index, [a, b]) in rolls.iter().enumerate() {
            let marker = format!("\n{ROLL_MARKER}{a} and {b}.\n");
            let start = (index + 1) * (bytes / (rolls.len() + 1));
            output[start..start + marker.len()].copy_from_slice(marker.as_bytes());
        }
        let measure = |observe: &mut dyn FnMut(&[u8]) -> usize| {
            let started = Instant::now();
            let mut finished = 0;
            for end in (CHUNK_BYTES..bytes).step_by(CHUNK_BYTES).chain([bytes]) {
                finished = black_box(observe(black_box(&output[..end])));
            }
            let elapsed_ms = started.elapsed().as_secs_f64() * 1_000.0;
            assert_eq!(finished, rolls.len() - 1);
            elapsed_ms
        };
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..5 {
            // Mirror the full-prefix scan from main (0c8d0d0).
            before.push(measure(&mut |prefix| {
                let marker = format!("\n{ROLL_MARKER}");
                prefix
                    .windows(marker.len())
                    .filter(|window| *window == marker.as_bytes())
                    .count()
                    .saturating_sub(1)
            }));
            let mut scanner = RollProgress::default();
            after.push(measure(&mut |prefix| scanner.finished_rolls(prefix)));
        }
        before.sort_by(f64::total_cmp);
        after.sort_by(f64::total_cmp);
        println!(
            "{bytes} bytes, {CHUNK_BYTES} bytes/read, median ms {:.4} -> {:.4}",
            before[2], after[2]
        );
    }
}

#[test]
fn roll_output_is_split_at_the_dice_markers_in_order() {
    let output = "banner\nThe dice have been set to 1 and 2.\n    1. first\nSetting GNUbg ID x\nboard\nThe dice have been set to 6 and 6.\n    1. second\n";
    let parts = split_rolls(output, &[[1, 2], [6, 6]]).unwrap();
    assert_eq!(
        parts,
        [
            "The dice have been set to 1 and 2.\n    1. first\n",
            "The dice have been set to 6 and 6.\n    1. second\n"
        ]
    );
    assert!(split_rolls(output, &[[6, 6], [1, 2]]).is_err());
    assert!(split_rolls(output, &[[1, 2], [6, 6], [3, 3]]).is_err());
}

mod benchmark_gate_tests {
    use super::super::*;

    #[test]
    fn cube_analysis_keeps_outcome_chances_in_the_equity_perspective() {
        let block = "Cube analysis\n0-ply cubeless equity -0.007818 (Money: -0.007901)\n  0.492641 0.140604 0.005944 - 0.507359 0.133160 0.006572\nCubeful equities:\n1. No double           -0.013765\n2. Double, pass        +1.000000  (+1.013765)\n3. Double, take        -0.373791  (-0.360025)\n";
        let equities = parse_cube_equities(block).unwrap();
        assert_eq!(equities.no_double, -0.013765);
        let chances = equities.probabilities.unwrap();
        assert_eq!(
            chances,
            [0.492641, 0.140604, 0.005944, 0.507359, 0.133160, 0.006572]
        );
        assert_eq!(flip_probabilities(chances)[0], 0.507359);
        // Equities without the chance line stay usable.
        let bare = "1. No double +0.1\n2. Double, pass +1.0\n3. Double, take +0.2\n";
        assert!(parse_cube_equities(bare).unwrap().probabilities.is_none());
    }

    #[tokio::test]
    async fn exclusive_benchmark_blocks_normal_invocations_but_allows_its_own_checks() {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(benchmark_exclusive(async move {
            entered_tx.send(()).unwrap();
            assert!(
                Engine::new("/missing-engine".into())
                    .version()
                    .await
                    .is_err()
            );
            release_rx.await.unwrap();
        }));
        entered_rx.await.unwrap();
        use tower::ServiceExt;
        let app = crate::api::router(crate::api::AppState::new(
            sqlx::PgPool::connect_lazy("postgres://unused").unwrap(),
            Engine::new("/missing-engine".into()),
        ));
        let response = tokio::time::timeout(
            Duration::from_secs(1),
            app.oneshot(
                axum::http::Request::post("/api/play/sessions")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"human_player":0,"match_length":5}"#,
                    ))
                    .unwrap(),
            ),
        )
        .await
        .expect("Play startup waited on the benchmark writer")
        .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
        let body = axum::body::to_bytes(response.into_body(), 2048)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&body).contains("benchmark"));
        let normal = Engine::new("/missing-engine".into());
        let waiting = normal.version();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut waiting)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        owner.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiting)
                .await
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn shared_pipeline_waits_before_start_and_reuses_access_with_a_writer_queued() {
        let (exclusive_tx, exclusive_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let exclusive = tokio::spawn(benchmark_exclusive(async move {
            exclusive_tx.send(()).unwrap();
            release_rx.await.unwrap();
        }));
        exclusive_rx.await.unwrap();
        let (started_tx, mut started_rx) = tokio::sync::oneshot::channel();
        let pipeline = tokio::spawn(async move {
            let mut writer = None;
            with_shared_access(async {
                started_tx.send(()).unwrap();
                // The writer must queue behind our shared permit before nested access.
                writer = Some(tokio::spawn(benchmark_exclusive(async {})));
                tokio::task::yield_now().await;
                assert!(
                    Engine::new("/missing-engine".into())
                        .version()
                        .await
                        .is_err()
                );
            })
            .await;
            writer.unwrap().await.unwrap();
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(40), &mut started_rx)
                .await
                .is_err()
        );
        release_tx.send(()).unwrap();
        exclusive.await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), pipeline)
            .await
            .expect("nested engine access must not deadlock behind queued writer")
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn benchmark_invocation_can_disable_deadline_without_changing_bounded_calls() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("slow-native-test");
        std::fs::write(
            &binary,
            "#!/usr/bin/env python3\nimport time\ntime.sleep(0.1)\nprint('finished')\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        let engine = Engine::new(binary.to_string_lossy().into_owned());
        let bounded = engine
            .run_with_limit(directory.path(), "", Some(Duration::from_millis(5)))
            .await;
        assert!(bounded.unwrap_err().to_string().contains("timed out"));
        let unlimited = engine
            .run_with_limit(directory.path(), "", None)
            .await
            .unwrap();
        assert_eq!(unlimited.trim(), "finished");
    }
}
