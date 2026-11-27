use super::*;
use std::io::{Cursor, Read};
fn sample_build(
    deck: Uuid,
    name: &str,
    cards: &[(Uuid, CardContent, CardAnswer)],
    locale: ConfigurationLocale,
) -> anyhow::Result<Vec<u8>> {
    build(deck, name, &prepare(cards.to_vec(), locale)?)
}
#[test]
fn black_exports_number_points_from_the_movers_perspective() {
    let source = "(;GM[6]MI[length:7]AE[a:y]AW[a][a][f]AB[s][u][x]PL[B]CV[2]CO[b];B[double];W[take];B[31uxst])";
    let (mut imported, contexts) = crate::import::sgf::parse_sgf_with_context(source).unwrap();
    let p = imported.positions.remove(0);
    assert_eq!(p.player, 1);
    assert_eq!(p.notation, "4/1 6/5");
    assert_eq!((p.board[19], p.board[21], p.board[24]), (-1, -1, -1));
    let id = Uuid::new_v4();
    let mut content = CardContent {
        kind: "checker".into(),
        position: Position {
            cube_enabled: None,
            id,
            match_uuid: id,
            ordinal: 0,
            game_no: 1,
            ply: 1,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board,
            position_id: p.position_id,
            gnubg_match_id: p.match_id,
            notation: String::new(),
        },
        players: ["White".into(), "Black".into()],
        prompt: String::new(),
        source: CardSource {
            kind: "match".into(),
            job_id: Some(id),
            analysis_id: None,
            imported_analysis_id: None,
            match_id: id,
            position_id: Some(id),
            game_no: 1,
            decision_no: Some(3),
        },
        solution: CardSolution {
            engine: "GNUbg".into(),
            plies: Some(0),
            imported: None,
            candidates: vec![],
            decision: None,
        },
        played: p.notation,
    };
    let answer = CardAnswer {
        solution: Some(content.solution.clone()),
        notes: String::new(),
        tags: vec![],
        played: content.played.clone(),
    };
    let mut cards = vec![(id, content.clone(), answer.clone())];
    // Reuse the actual pre-offer and response IDs, rather than changing only the player field.
    for (index, player, kind) in [(0, 1, "cube_offer"), (1, 0, "cube_response")] {
        content.kind = kind.into();
        content.position.player = player;
        content.position.dice = vec![0, 0];
        let (position_id, match_id) = &contexts[0].decision_ids[index];
        content.position.position_id = position_id.clone();
        content.position.gnubg_match_id = match_id.clone();
        cards.push((Uuid::new_v4(), content.clone(), answer.clone()));
    }
    let bytes = sample_build(id, "Black decisions", &cards, ConfigurationLocale::En).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    for index in 0..3 {
        let mut svg = String::new();
        zip.by_name(&index.to_string())
            .unwrap()
            .read_to_string(&mut svg)
            .unwrap();
        for (x, black_label, white_label) in [(378, 6, 19), (478, 4, 21), (628, 1, 24)] {
            let label = if index < 2 { black_label } else { white_label };
            assert!(svg.contains(&format!("<text x=\"{x}\" y=\"21\" text-anchor=\"middle\" font-size=\"13\" fill=\"#fff\">{label}</text>")), "card {index}, label {label}");
            // Physical checker positions and colors remain absolute for both players.
            assert!(svg.contains(&format!(
                "<circle cx=\"{x}\" cy=\"60\" r=\"13\" fill=\"#252b29\""
            )));
        }
        let lower_label = if index < 2 { 19 } else { 6 };
        assert!(svg.contains(&format!("<text x=\"378\" y=\"430\" text-anchor=\"middle\" font-size=\"13\" fill=\"#fff\">{lower_label}</text>")));
    }
}
#[test]
fn native_package_has_stable_notes_safe_html_and_self_contained_boards() {
    let p = crate::import::parse_sgf("(;GM[6];W[31hefe])")
        .unwrap()
        .positions
        .remove(0);
    let id = Uuid::new_v4();
    let deck = Uuid::new_v4();
    let content = CardContent {
        kind: "checker".into(),
        position: Position {
            cube_enabled: None,
            id,
            match_uuid: deck,
            ordinal: 0,
            game_no: 1,
            ply: 1,
            player: p.player,
            dice: p.dice.to_vec(),
            board: p.board,
            position_id: p.position_id,
            gnubg_match_id: p.match_id,
            notation: String::new(),
        },
        players: ["<script>x</script>".into(), "Bob & Co".into()],
        prompt: "What is the best checker play?".into(),
        source: CardSource {
            kind: "analysis".into(),
            job_id: Some(id),
            analysis_id: None,
            imported_analysis_id: None,
            match_id: deck,
            position_id: Some(id),
            game_no: 1,
            decision_no: None,
        },
        solution: CardSolution {
            engine: "GNUbg".into(),
            plies: Some(0),
            imported: None,
            candidates: vec![Candidate {
                rank: 1,
                notation: "8/5 6/5".into(),
                path: None,
                equity: 0.2,
                probabilities: vec![],
                plies: Some(0),
            }],
            decision: None,
        },
        played: "8/5 6/5".into(),
    };
    let answer = CardAnswer {
        solution: Some(content.solution.clone()),
        notes: "<img src=x onerror=alert(1)>\n& lesson".into(),
        tags: vec!["test".into()],
        played: content.played.clone(),
    };
    let cards = vec![(id, content.clone(), answer.clone())];
    let bytes = sample_build(deck, "Practice", &cards, ConfigurationLocale::De).unwrap();
    let imported = super::super::import::read_package(&bytes).unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(card_signature(&imported[0].0), card_signature(&content));
    assert_eq!(imported[0].0.source.match_id, Uuid::nil());
    assert_eq!(imported[0].1, BTreeSet::from(["test".into()]));
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("collection.anki2");
    let mut db = Vec::new();
    zip.by_name("collection.anki2")
        .unwrap()
        .read_to_end(&mut db)
        .unwrap();
    std::fs::write(&path, db).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    let (guid, fields): (String, String) = conn
        .query_row("SELECT guid,flds FROM notes", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(fields.contains("&lt;script&gt;"));
    assert!(!fields.contains("<script>"));
    assert!(fields.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(fields.contains("Bester Zug"));
    assert!(fields.contains("Jacoby ✗"));
    let mut jacoby_card = content.clone();
    jacoby_card.position.gnubg_match_id = crate::import::parse_sgf("(;GM[6]RU[Jacoby];W[31hefe])")
        .unwrap()
        .positions
        .remove(0)
        .match_id;
    assert!(
        question(
            &jacoby_card,
            "board.svg",
            &Labels::new(ConfigurationLocale::De)
        )
        .unwrap()
        .contains("Jacoby ✓")
    );
    assert!(!fields.split('\x1f').nth(1).unwrap().contains("8/5 6/5"));
    for (kind, best, played, expected_best, expected_played) in [
        (
            "cube_offer",
            "double",
            "no double",
            "Doppeln",
            "Nicht doppeln",
        ),
        (
            "cube_offer",
            "no double",
            "double",
            "Nicht doppeln",
            "Doppeln",
        ),
        ("cube_response", "take", "drop", "Annehmen", "Passen"),
        ("cube_response", "drop", "take", "Passen", "Annehmen"),
    ] {
        let mut cube = content.clone();
        cube.kind = kind.into();
        cube.played = played.into();
        cube.solution.candidates.clear();
        cube.solution.decision = Some(crate::match_analysis::decision(
            cube.position.player,
            "cube",
            played,
            best,
            Some((0.0, 0.1)),
        ));
        let answer = CardAnswer {
            solution: Some(cube.solution.clone()),
            played: cube.played.clone(),
            notes: String::new(),
            tags: vec![],
        };
        let cards = vec![(id, cube, answer)];
        let german = prepare(cards.clone(), ConfigurationLocale::De)
            .unwrap()
            .remove(0);
        let english = prepare(cards, ConfigurationLocale::En).unwrap().remove(0);
        assert!(
            german.fields.contains(&format!(
                "<p class=\"best\">Bester Zug: {expected_best}</p><p>Gespielt: {expected_played}</p>"
            )),
            "{}",
            german.fields
        );
        assert!(english.fields.contains(&format!(
            "<p class=\"best\">Best play: {best}</p><p>Played: {played}</p>"
        )));
        assert_eq!(german.guid, english.guid);
        assert_eq!(german.structured, english.structured);
    }
    let mut media = String::new();
    zip.by_name("media")
        .unwrap()
        .read_to_string(&mut media)
        .unwrap();
    let media: serde_json::Value = serde_json::from_str(&media).unwrap();
    assert!(media["0"].as_str().unwrap().ends_with(".svg"));
    let mut svg = String::new();
    zip.by_name("0").unwrap().read_to_string(&mut svg).unwrap();
    assert!(svg.contains("<svg"));
    assert!(!svg.contains("href="));
    assert_eq!(
        conn.query_row("SELECT count(*) FROM revlog", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT reps FROM cards", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    let bytes = sample_build(deck, "Practice renamed", &cards, ConfigurationLocale::En).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut db = Vec::new();
    zip.by_name("collection.anki2")
        .unwrap()
        .read_to_end(&mut db)
        .unwrap();
    let path = temp.path().join("second.anki2");
    std::fs::write(&path, db).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        guid,
        conn.query_row("SELECT guid FROM notes", [], |r| r.get::<_, String>(0))
            .unwrap()
    );
    if let Ok(output) = std::env::var("NEONDOJO_TEST_APKG") {
        std::fs::write(
            output,
            sample_build(deck, "Practice", &cards, ConfigurationLocale::De).unwrap(),
        )
        .unwrap();
    }
}
