use super::*;
use uuid::Uuid;

fn position(sgf: &str) -> Position {
    let imported = crate::import::parse_sgf(sgf).unwrap();
    let p = &imported.positions[0];
    Position {
        cube_enabled: None,
        id: Uuid::nil(),
        match_uuid: Uuid::nil(),
        ordinal: 1,
        game_no: p.game_no,
        ply: p.ply,
        player: p.player,
        dice: p.dice.to_vec(),
        board: p.board.clone(),
        position_id: p.position_id.clone(),
        gnubg_match_id: p.match_id.clone(),
        notation: p.notation.clone(),
    }
}

#[test]
fn canonical_key_collapses_away_pair_and_orders_dice() {
    let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
    let (key, cube) = canonical_keys(&p).unwrap();
    assert_eq!(key, "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:3:10");
    assert_eq!(cube.unwrap(), key.replace(":31:", ":00:"));
}

#[test]
fn canonical_key_mirrors_mover_scores_and_cube_owner() {
    let p = position("(;GM[6]MI[length:9][ws:2][bs:4]CV[2]CO[w];B[52lqqs])");
    let (key, cube) = canonical_keys(&p).unwrap();
    assert_eq!(key, "-b----E-C---eE---c-e----B-:1:-1:1:52:2:0:0:7:10");
    assert_eq!(cube, None, "Opponent owns cube, so the mover cannot double");
}

#[test]
fn crawford_and_money_jacoby_have_distinct_keys() {
    let p = position("(;GM[6]MI[length:7][ws:6][bs:2]RU[Crawford:CrawfordGame];W[31hefe])");
    let (key, cube) = canonical_keys(&p).unwrap();
    assert_eq!(key, "-b----E-C---eE---c-e----B-:0:0:1:31:4:0:1:5:10");
    assert!(cube.is_none());
    let jacoby = position("(;GM[6]RU[Jacoby];W[31hefe])");
    let no_jacoby = position("(;GM[6];W[31hefe])");
    assert_eq!(
        canonical_keys(&jacoby).unwrap().0,
        "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:1:0:10"
    );
    assert_eq!(
        canonical_keys(&no_jacoby).unwrap().0,
        "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:0:10"
    );
}

#[test]
fn contradictory_or_unsupported_context_is_rejected() {
    let mut p = position("(;GM[6]MI[length:7];W[31hefe])");
    p.player = 1;
    assert!(canonical_keys(&p).is_err());
    p.player = 0;
    p.dice = vec![6, 6];
    assert!(canonical_keys(&p).is_err());
    p.dice = vec![3, 1];
    p.board[25] = -1;
    assert!(canonical_keys(&p).is_err());
}

#[test]
fn only_centered_money_cubes_are_collapsed() {
    let money = position("(;GM[6]CV[4]CO[c];W[31hefe])");
    assert_eq!(
        canonical_keys(&money).unwrap().0,
        "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:0:10"
    );
    let owned = position("(;GM[6]CV[4]CO[w];W[31hefe])");
    let (key, cube) = canonical_keys(&owned).unwrap();
    assert_eq!(key, "-b----E-C---eE---c-e----B-:2:1:1:31:0:0:0:0:10");
    assert!(cube.is_some());
    let match_play = position("(;GM[6]MI[length:7]CV[4]CO[c];W[31hefe])");
    assert_eq!(
        canonical_keys(&match_play).unwrap().0,
        "-b----E-C---eE---c-e----B-:2:0:1:31:0:0:0:7:10"
    );
}

#[test]
fn invalid_scores_pending_cube_and_reserved_rules_are_rejected() {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    let source = position("(;GM[6]MI[length:7];W[31hefe])");
    for (offset, width, value) in [
        (12, 1, 1),
        (13, 2, 2),
        (67, 1, 1),
        (36, 15, 7),
        (7, 1, 1),
        (4, 2, 2),
    ] {
        let mut p = source.clone();
        let mut bytes = STANDARD_NO_PAD.decode(&p.gnubg_match_id).unwrap();
        for index in 0..width {
            bytes[(offset + index) / 8] &= !(1 << ((offset + index) % 8));
            bytes[(offset + index) / 8] |= (((value >> index) & 1) as u8) << ((offset + index) % 8);
        }
        p.gnubg_match_id = STANDARD_NO_PAD.encode(bytes);
        assert!(
            canonical_keys(&p).is_err(),
            "offset {offset}, value {value}"
        );
    }
    let long = position("(;GM[6]MI[length:26];W[31hefe])");
    assert!(canonical_keys(&long).is_err());
}

fn book(compressed: bool) -> (tempfile::TempDir, BookConfig) {
    use flate2::{Compress, Compression, FlushCompress};
    use rusqlite::params;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.sqlite");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value); CREATE TABLE entries(xgid TEXT PRIMARY KEY,payload NOT NULL) WITHOUT ROWID; CREATE TABLE contexts(name TEXT PRIMARY KEY,entries INTEGER,depth INTEGER,complete INTEGER);").unwrap();
    for (key, value) in [
        ("formatVersion", "1"),
        ("engineVersion", "1.08.003"),
        ("compression", if compressed { "zlib" } else { "none" }),
        ("topPly", "3"),
        ("cubePly", "3"),
        ("license", "CC-BY-4.0"),
        ("source", "https://backgammondb.com"),
        (
            "credits",
            "BackgammonDB opening book, generated with GNU Backgammon 1.08.003",
        ),
    ] {
        connection
            .execute("INSERT INTO meta VALUES(?1,?2)", params![key, value])
            .unwrap();
    }
    let dict = br#"plays move probabilities win winGammon winBackgammon loseGammon loseBackgammon cubelessEquity cubefulEquity"#;
    if compressed {
        connection
            .execute("INSERT INTO meta VALUES('zdict',?1)", params![dict])
            .unwrap();
    }
    let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
    let checker_key = "-b----E-C---eE---c-e----B-:0:0:1:31:0:0:0:3:10";
    let checker = serde_json::json!({"plays":[{"move":[[8,5],[6,5]],"probabilities":{"win":0.55,"winGammon":0.17,"winBackgammon":0.01,"loseGammon":0.12,"loseBackgammon":0.005},"cubelessEquity":0.159,"cubefulEquity":0.219}]});
    let cube = serde_json::json!({"probabilities":{"win":0.55,"winGammon":0.17,"winBackgammon":0.01,"loseGammon":0.12,"loseBackgammon":0.005},"equities":{"noDouble":0.2,"doubleTake":0.1,"doublePass":1.0},"verdict":"no-double"});
    for (key, payload) in [
        (checker_key.to_string(), checker),
        (checker_key.replace(":31:", ":00:"), cube),
    ] {
        if compressed {
            let mut encoder = Compress::new(Compression::best(), true);
            encoder.set_dictionary(dict).unwrap();
            let mut bytes = Vec::with_capacity(8192);
            encoder
                .compress_vec(
                    payload.to_string().as_bytes(),
                    &mut bytes,
                    FlushCompress::Finish,
                )
                .unwrap();
            connection
                .execute("INSERT INTO entries VALUES(?1,?2)", params![key, bytes])
                .unwrap();
        } else {
            connection
                .execute(
                    "INSERT INTO entries VALUES(?1,?2)",
                    params![key, payload.to_string()],
                )
                .unwrap();
        }
    }
    connection
        .execute("INSERT INTO contexts VALUES('3a3a',2,3,1)", [])
        .unwrap();
    drop(p);
    (
        dir,
        BookConfig {
            version: 0,
            provider: "backgammondb".into(),
            path: path.to_string_lossy().into_owned(),
            xg_path: xg_book::default_path(),
        },
    )
}

#[tokio::test]
async fn sqlite_lookup_reads_compressed_and_plain_candidates_and_cube() {
    for compressed in [false, true] {
        let (_dir, config) = book(compressed);
        let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
        let result = lookup(&config, &p, None).await.unwrap();
        assert!(result.hit, "{:?}", result.reason);
        let checker = result.checker.unwrap();
        // The fixture records GNUbg's native topPly 3.
        assert_eq!(checker.plies, Some(4));
        assert_eq!(checker.candidates[0].notation, "8/5 6/5");
        assert_eq!(checker.candidates[0].equity, 0.219);
        assert_eq!(
            checker.candidates[0].probabilities,
            vec![0.55, 0.17, 0.01, 0.44999999999999996, 0.12, 0.005]
        );
        assert_eq!(result.cube.unwrap().verdict, "no-double");
        let provenance = result.provenance.unwrap();
        assert_eq!(provenance.evaluation_kind, "evaluation");
        assert_eq!(provenance.context_depth, Some(3));
        assert_eq!(provenance.engine, "GNU Backgammon 1.08.003");
        assert_eq!(
            serde_json::to_value(&provenance).unwrap()["equity_kind"],
            "normalized_cubeful"
        );
        let state = status(&config).await;
        assert!(state.available);
        assert_eq!(state.entries, 2);
    }
}

#[test]
fn books_whose_tables_are_views_or_virtual_tables_are_rejected() {
    let (_dir, config) = book(false);
    Book::open(&config.path).unwrap();
    for (table, replacement) in [
        (
            "entries",
            "CREATE VIEW entries AS SELECT xgid,payload FROM original",
        ),
        (
            "meta",
            "CREATE VIRTUAL TABLE meta USING fts5(key,value); INSERT INTO meta SELECT key,value FROM original",
        ),
        (
            "contexts",
            "CREATE VIEW contexts AS SELECT name,entries,depth,complete FROM original",
        ),
    ] {
        let (_dir, config) = book(false);
        rusqlite::Connection::open(&config.path)
            .unwrap()
            .execute_batch(&format!(
                "ALTER TABLE {table} RENAME TO original; {replacement};"
            ))
            .unwrap();
        let error = Book::open(&config.path).err().expect(table).to_string();
        assert_eq!(
            error,
            format!("Opening book table {table} is missing or not a regular table")
        );
    }
}

#[tokio::test]
async fn off_missing_wrong_score_and_wrong_engine_return_misses() {
    let (_dir, mut config) = book(true);
    let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
    let wrong_engine = lookup(&config, &p, Some("GNU Backgammon 1.05.002  Oct  8 2025"))
        .await
        .unwrap();
    assert!(!wrong_engine.hit);
    assert!(wrong_engine.reason.unwrap().contains("version"));
    assert!(
        lookup(&config, &p, Some("GNU Backgammon 1.08.003"))
            .await
            .unwrap()
            .hit
    );
    let wrong_score = position("(;GM[6]MI[length:7][ws:3][bs:4];W[31hefe])");
    assert!(!lookup(&config, &wrong_score, None).await.unwrap().hit);
    config.provider = "off".into();
    assert!(!lookup(&config, &p, None).await.unwrap().hit);
    config.provider = "automatic".into();
    config.path.push_str(".missing");
    assert!(!lookup(&config, &p, None).await.unwrap().hit);
    assert!(!status(&config).await.available);
}

#[tokio::test]
async fn unsupported_money_rules_never_match_bgdb_no_beaver_data() {
    let (_dir, config) = book(false);
    // A match-play book does not depend on the money-game beaver rule.
    let p = position("(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])");
    let result = lookup_with_rules(&config, &p, None, Some(true))
        .await
        .unwrap();
    assert_eq!(result.provider, "backgammondb");
    assert!(result.hit);
    let source = position("(;GM[6]RU[Jacoby];W[31hefe])");
    // Put an exact money checker key into the real SQLite fixture.
    let connection = rusqlite::Connection::open(&config.path).unwrap();
    connection
        .execute(
            "UPDATE entries SET xgid=replace(xgid,':0:0:0:3:10',':0:0:1:0:10')",
            [],
        )
        .unwrap();
    assert!(
        lookup_with_rules(&config, &source, None, Some(false))
            .await
            .unwrap()
            .hit
    );
    assert!(
        !lookup_with_rules(&config, &source, None, Some(true))
            .await
            .unwrap()
            .hit
    );
    assert!(
        !lookup_with_rules(&config, &source, None, None)
            .await
            .unwrap()
            .hit
    );
}

#[tokio::test]
async fn installs_valid_book_atomically_and_rejects_invalid_source() {
    let (_source, config) = book(true);
    let dest = tempfile::tempdir().unwrap();
    let destination = BookConfig {
        version: 0,
        xg_path: xg_book::default_path(),
        provider: "automatic".into(),
        path: dest
            .path()
            .join("book.sqlite")
            .to_string_lossy()
            .into_owned(),
    };
    let installed = install_from_path(&destination, &config.path).await.unwrap();
    assert!(installed.available);
    let invalid = dest.path().join("invalid");
    std::fs::write(&invalid, b"not sqlite").unwrap();
    assert!(
        install_from_path(&destination, invalid.to_str().unwrap())
            .await
            .is_err()
    );
    assert!(status(&destination).await.available);
}

#[tokio::test]
#[ignore = "requires installed official BackgammonDB SQLite"]
async fn released_full_book_serves_real_scores_both_movers_and_cube() {
    let config = BookConfig {
        version: 0,
        provider: "backgammondb".into(),
        path: std::env::var("NEONDOJO_OPENING_BOOK").unwrap_or_else(|_| default_path()),
        xg_path: xg_book::default_path(),
    };
    let state = status(&config).await;
    assert!(state.available, "{:?}", state.error);
    assert_eq!(state.entries, 4_230_443);
    assert_eq!(state.contexts.len(), 351);
    for length in [0, 1, 2, 3, 5, 7, 9, 15, 25] {
        for (mover, movement) in [("W", "hefe"), ("B", "qtst")] {
            let p = position(&format!(
                "(;GM[6]MI[length:{length}];{mover}[31{movement}])"
            ));
            let result = lookup(&config, &p, None).await.unwrap();
            assert!(
                result.hit,
                "length={length}, mover={mover}: {:?}",
                result.reason
            );
            assert_eq!(result.checker.unwrap().candidates[0].notation, "8/5 6/5");
            assert!(
                result.cube.is_none(),
                "The initial opening roll has no cube decision"
            );
        }
    }
    for metadata in [
        "MI[length:7][ws:4][bs:4]",
        "MI[length:9][ws:2][bs:4]",
        "MI[length:7][ws:6][bs:2]RU[Crawford:CrawfordGame]",
        "RU[Jacoby]",
    ] {
        let p = position(&format!("(;GM[6]{metadata};W[31hefe])"));
        assert!(lookup(&config, &p, None).await.unwrap().hit, "{metadata}");
    }
    let source = crate::import::parse_sgf("(;GM[6]MI[length:7];W[31hefe];B[42qusu])").unwrap();
    let p = &source.positions[1];
    let reply = Position {
        cube_enabled: None,
        id: Uuid::nil(),
        match_uuid: Uuid::nil(),
        ordinal: 2,
        game_no: p.game_no,
        ply: p.ply,
        player: p.player,
        dice: p.dice.to_vec(),
        board: p.board.clone(),
        position_id: p.position_id.clone(),
        gnubg_match_id: p.match_id.clone(),
        notation: p.notation.clone(),
    };
    let result = lookup(&config, &reply, None).await.unwrap();
    assert!(result.checker.is_some(), "{:?}", result.reason);
    assert!(result.cube.is_some());
    assert_eq!(result.provenance.unwrap().evaluation_kind, "evaluation");
    assert!(
        !lookup(&config, &reply, Some("GNU Backgammon 1.05.002"))
            .await
            .unwrap()
            .hit
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn settings_routes_preserve_cas_and_position_lookup(pool: sqlx::PgPool) {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    async fn request(app: Router, method: &str, url: &str, body: Value) -> (u16, Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(url)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status().as_u16();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    let app = routes().with_state(AppState::new(
        pool.clone(),
        crate::engine::Engine::new("/missing/gnubg".into()),
    ));
    let (_, settings) = request(app.clone(), "GET", "/api/opening-book", Value::Null).await;
    assert_eq!(settings["config"]["provider"], "automatic");
    let (_dir, fixture) = book(true);
    let config = json!({"version":0,"provider":"backgammondb","path":fixture.path});
    let (status, saved) = request(app.clone(), "PUT", "/api/opening-book", config.clone()).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["config"]["version"], 1);
    assert_eq!(saved["status"]["entries"], 2);
    assert_eq!(
        request(app.clone(), "PUT", "/api/opening-book", config)
            .await
            .0,
        409
    );
    assert_eq!(
        request(
            app.clone(),
            "PUT",
            "/api/opening-book",
            json!({"version":1,"provider":"unsupported","path":"/book"})
        )
        .await
        .0,
        422
    );
    sqlx::query("UPDATE opening_book_settings SET xg_path='/custom/XG.ob' WHERE singleton=true")
        .execute(&pool)
        .await
        .unwrap();
    let (code, preserved) = request(
        app.clone(),
        "PUT",
        "/api/opening-book",
        json!({"version":1,"provider":"backgammondb","path":fixture.path}),
    )
    .await;
    assert_eq!(code, 200, "{preserved}");
    assert_eq!(
        preserved["config"]["xg_path"], "/custom/XG.ob",
        "Old clients must preserve a custom XG path"
    );
    let sgf = "(;GM[6]MI[length:7][ws:4][bs:4];W[31hefe])";
    let imported = crate::store::import_match(
        &pool,
        "book.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &crate::import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let positions = crate::store::positions(&pool, Some(imported.r#match.id), None, None, 10)
        .await
        .unwrap();
    let (status, result) = request(
        app.clone(),
        "GET",
        &format!("/api/positions/{}/opening-book", positions.items[0].id),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(result["checker"]["candidates"][0]["notation"], "8/5 6/5");
    assert_eq!(result["provenance"]["engine"], "GNU Backgammon 1.08.003");
    assert_eq!(
        request(
            app,
            "GET",
            &format!("/api/positions/{}/opening-book", Uuid::new_v4()),
            Value::Null
        )
        .await
        .0,
        404
    );
}

#[tokio::test]
#[ignore = "requires official installed XG opening book"]
async fn xg_provider_and_automatic_keep_references_separate_from_candidates() {
    let source = crate::import::parse_sgf("(;GM[6]MI[length:7];W[31hefe];B[42qusu])").unwrap();
    let p = &source.positions[1];
    let position = Position {
        cube_enabled: None,
        id: Uuid::nil(),
        match_uuid: Uuid::nil(),
        ordinal: 1,
        game_no: p.game_no,
        ply: p.ply,
        player: p.player,
        dice: p.dice.to_vec(),
        board: p.board.clone(),
        position_id: p.position_id.clone(),
        gnubg_match_id: p.match_id.clone(),
        notation: p.notation.clone(),
    };
    let mut config = BookConfig {
        version: 0,
        provider: "xg".into(),
        path: default_path(),
        xg_path: xg_book::default_path(),
    };
    let result = lookup(&config, &position, None).await.unwrap();
    assert!(result.hit, "{:?}", result.reason);
    assert_eq!(result.provider, "xg");
    assert!(result.checker.is_none() && result.cube.is_none() && result.provenance.is_none());
    assert!(result.xg_position.is_some());
    assert!(
        !lookup(&config, &position, Some("GNU Backgammon 1.08.003"))
            .await
            .unwrap()
            .hit,
        "An XG reference cannot fulfill a GNUbg cache request"
    );
    config.provider = "automatic".into();
    let result = lookup(&config, &position, None).await.unwrap();
    assert!(result.checker.is_some() && result.xg_position.is_some());
    let money_source = crate::import::parse_sgf("(;GM[6]RU[Jacoby];W[31hefe];B[42qusu])").unwrap();
    let p = &money_source.positions[1];
    let money = Position {
        cube_enabled: None,
        id: Uuid::nil(),
        match_uuid: Uuid::nil(),
        ordinal: 1,
        game_no: p.game_no,
        ply: p.ply,
        player: p.player,
        dice: p.dice.to_vec(),
        board: p.board.clone(),
        position_id: p.position_id.clone(),
        gnubg_match_id: p.match_id.clone(),
        notation: p.notation.clone(),
    };
    let beavers = lookup_with_rules(&config, &money, None, Some(true))
        .await
        .unwrap();
    assert!(beavers.xg_position.is_some());
    assert!(
        beavers.checker.is_none() && beavers.cube.is_none(),
        "BGDB cannot answer the beaver-rule context"
    );
    assert!(
        !lookup_with_rules(&config, &money, None, None)
            .await
            .unwrap()
            .hit
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn source_money_rules_are_explicit_and_unknown_mat_rules_miss(pool: sqlx::PgPool) {
    let sgf = "(;GM[6]RU[Jacoby];W[31hefe])";
    let parsed = crate::import::parse_sgf(sgf).unwrap();
    let cache = LookupCache::default();
    for (format, expected) in [("sgf", Some(false)), ("bgf", Some(false)), ("mat", None)] {
        // Separate content hashes produce independent source rows.
        let content = format!("{format}:{sgf}");
        let imported =
            crate::store::import_match(&pool, "rules", format, content.as_bytes(), sgf, &parsed)
                .await
                .unwrap();
        let page = crate::store::positions(&pool, Some(imported.r#match.id), None, None, 10)
            .await
            .unwrap();
        assert_eq!(
            cache
                .source_beaver_rule(&pool, &page.items[0])
                .await
                .unwrap(),
            expected
        );
    }
}
