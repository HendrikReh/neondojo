use bgtutor::engines::{SageEngine, SageSnapshot};

fn snapshot() -> SageSnapshot {
    SageSnapshot {
        python: "/missing/python".into(),
        package_version: "2.0.20260907".into(),
        artifact_fingerprint: "a".repeat(64),
        threads: 2,
        plies: 2,
        adapter_version: 1,
    }
}

#[test]
fn unsupported_profiles_are_rejected_before_starting_processes() {
    for (plies, threads) in [(1, 2), (3, 2), (2, 0), (2, 3), (2, 40)] {
        let mut value = snapshot();
        value.plies = plies;
        value.threads = threads;
        assert!(SageEngine::new(value).is_err());
    }
    let mut value = snapshot();
    value.artifact_fingerprint.clear();
    assert!(SageEngine::new(value).is_err());
}

#[tokio::test]
async fn missing_interpreter_fails_explicitly() {
    let error = SageEngine::probe("/missing/python".into(), 2)
        .await
        .unwrap_err();
    assert!(!error.to_string().is_empty());
}

#[tokio::test]
#[ignore = "requires the locked native macOS ARM64 Open Sage installation; no database"]
async fn native_checker_cube_and_match_analysis() {
    let python = std::env::var("BGTUTOR_SAGE_PYTHON")
        .expect("Set BGTUTOR_SAGE_PYTHON to the locked venv interpreter");
    let profile = SageEngine::probe(python, 2).await.unwrap();
    assert_eq!(profile.package_version, "2.0.20260907");
    let engine = SageEngine::new(profile).unwrap();
    let version = engine.version().await.unwrap();
    assert!(version.contains("2ply") && version.contains(&engine.snapshot().artifact_fingerprint));
    let analysis = engine
        .analyze("4HPwATDgc/ABMA", "cAkHAAAAAAAE", 2, Some([3, 1]))
        .await
        .unwrap();
    assert_eq!(analysis.candidates[0].notation, "8/5 6/5");
    assert_eq!(analysis.candidates[0].probabilities.len(), 6);
    let game = engine
        .analyze_game("(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])", 1, 2)
        .await
        .unwrap();
    assert_eq!(game.decisions.len(), 1);
    assert!(game.statistics.is_none());
    assert!(game.decisions[0].played_equity.is_some());
    let cube = engine.cube("4HPwATDgc/ABMA", "cAkHAAAAAAAE").await.unwrap();
    assert!(cube.no_double.is_finite() && cube.take.is_finite() && cube.pass.is_finite());
}

#[tokio::test]
#[ignore = "requires locked native Open Sage; no database"]
async fn native_reference_game_five_full_coverage() {
    let python = std::env::var("BGTUTOR_SAGE_PYTHON").unwrap();
    let engine = SageEngine::new(SageEngine::probe(python, 4).await.unwrap()).unwrap();
    let source = bgtutor::benchmarks::REFERENCE_SGF;
    let result = engine.analyze_game(source, 5, 2).await.unwrap();
    assert!(!result.decisions.is_empty());
}
