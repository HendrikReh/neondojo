//! Native BGBlitz BGF 1.0 transport and played-match conversion.
use anyhow::{Context, Result, bail, ensure};
use flate2::bufread::GzDecoder;
use serde::Deserialize;
use std::io::Read;

const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_DECODED: usize = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 200_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    format: String,
    version: String,
    compress: bool,
    use_smile: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MatchData {
    matchlen: i32,
    count: usize,
    game_mode: i32,
    name_green: String,
    name_red: String,
    use_crawford: bool,
    use_jacoby: bool,
    use_cube: bool,
    use_beaver: bool,
    cube_limit: i32,
    games: Vec<Game>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Game {
    game_mode: i32,
    initial: Initial,
    name_green: String,
    name_red: String,
    matchlen: i32,
    score_green: i32,
    score_red: i32,
    is_crawford: bool,
    lost_on_time: bool,
    was_forfeit: bool,
    #[serde(default)]
    won_points: i32,
    moves: Vec<Move>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Initial {
    game_mode: i32,
    points: [i32; 28],
    cube: i32,
    cube_owner: i32,
    egyptian: bool,
}
#[derive(Deserialize)]
struct Move {
    #[serde(rename = "type")]
    kind: String,
    player: i32,
    green: i32,
    red: i32,
    from: [i32; 4],
    to: [i32; 4],
    #[serde(flatten)]
    foreign: std::collections::BTreeMap<String, serde_json::Value>,
}

fn payload(header: &Header, bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    if !header.compress {
        ensure!(
            bytes.len() <= limit,
            "BGF: decoded payload exceeds size limit"
        );
        return Ok(bytes.to_vec());
    }
    let mut decoder = GzDecoder::new(bytes);
    let mut out = Vec::new();
    (&mut decoder)
        .take(limit as u64 + 1)
        .read_to_end(&mut out)
        .context("BGF: damaged gzip payload")?;
    ensure!(
        out.len() <= limit,
        "BGF: decoded payload exceeds size limit"
    );
    ensure!(
        decoder.get_ref().is_empty(),
        "BGF: trailing bytes after gzip payload"
    );
    Ok(out)
}
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace(']', "\\]")
}
fn player(value: i32) -> Result<i32> {
    match value {
        -1 => Ok(0), // Green, mapped to SGF White.
        1 => Ok(1),  // Red, mapped to SGF Black.
        _ => bail!("BGF: invalid active player {value}"),
    }
}
fn coordinate(point: usize, side: i32) -> char {
    match point {
        0 => 'z',
        25 => 'y',
        _ if side == 0 => (b'a' + point as u8 - 1) as char,
        _ => (b'x' + 1 - point as u8) as char,
    }
}
fn checker(
    board: &mut [i32; 26],
    side: i32,
    dice: [i32; 2],
    runs: &[(usize, usize)],
) -> Result<String> {
    let steps = super::sgf::expand_xg_play(board, side, dice, runs)?;
    let color = if side == 0 { 'W' } else { 'B' };
    let mut result = format!(";{color}[{}{}", dice[0], dice[1]);
    for (from, to) in steps {
        result.push(coordinate(from, side));
        result.push(coordinate(to, side));
    }
    result.push(']');
    Ok(result)
}
fn runs(record: &Move) -> Result<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    let mut terminated = false;
    for (&from, &to) in record.from.iter().zip(&record.to) {
        if from == -1 && to == -1 {
            terminated = true;
            continue;
        }
        ensure!(
            !terminated && (1..=25).contains(&from) && (0..from).contains(&to),
            "BGF: malformed checker endpoints"
        );
        out.push((from as usize, to as usize));
    }
    Ok(out)
}
fn empty_coordinates(record: &Move) -> Result<()> {
    ensure!(
        record.from == [-1; 4] && record.to == [-1; 4],
        "BGF: unexpected coordinates on control action"
    );
    Ok(())
}

/// Decode native BGF 1.0. Keep producer analysis as a separate SGF extension;
/// the played line is still validated by independent legal replay.
pub fn bgf_to_sgf(input: &[u8]) -> Result<String> {
    ensure!(input.len() <= MAX_FILE, "BGF file exceeds 16 MiB limit");
    let header_end = input
        .iter()
        .take(4097)
        .position(|b| *b == b'\n')
        .context("BGF: missing JSON header line")?;
    ensure!(header_end <= 4096, "BGF: header exceeds 4096 bytes");
    let header: Header =
        serde_json::from_slice(&input[..header_end]).context("BGF: invalid JSON header")?;
    ensure!(
        header.format == "BGF" && header.version == "1.0",
        "Unsupported BGF format/version"
    );
    let bytes = payload(&header, &input[header_end + 1..], MAX_DECODED)?;
    let data: MatchData = if header.use_smile {
        serde_smile::from_slice(&bytes).context("BGF: invalid Smile payload")?
    } else {
        serde_json::from_slice(&bytes).context("BGF: invalid JSON payload")?
    };
    ensure!(
        data.game_mode == 0,
        "Only standard backgammon BGF matches are supported"
    );
    ensure!(
        (0..=32767).contains(&data.matchlen),
        "BGF: match length exceeds GNUbg range"
    );
    ensure!(
        data.use_cube || data.matchlen == 1,
        "BGF: cubeless sessions are not supported"
    );
    ensure!(
        !(data.use_beaver && data.matchlen == 0),
        "BGF: beaver money sessions are not supported"
    );
    ensure!(
        data.cube_limit == -1,
        "BGF: capped cube limit is not supported"
    );
    ensure!(
        !data.games.is_empty() && data.games.len() <= 10000 && data.count == data.games.len(),
        "BGF: invalid game count"
    );
    ensure!(
        data.games.iter().map(|g| g.moves.len()).sum::<usize>() <= MAX_RECORDS,
        "BGF: too many move records"
    );
    let names = [&data.name_green, &data.name_red];
    ensure!(
        names
            .iter()
            .all(|n| n.chars().count() <= 1024 && !n.contains('\0')),
        "BGF: invalid player name"
    );
    let mut out = String::new();
    for (game_index, game) in data.games.iter().enumerate() {
        ensure!(
            game.game_mode == 0 && game.initial.game_mode == 0 && !game.initial.egyptian,
            "BGF: unsupported game variant"
        );
        ensure!(
            game.matchlen == data.matchlen && [&game.name_green, &game.name_red] == names,
            "BGF: game metadata changes match identity"
        );
        ensure!(
            !game.lost_on_time && !game.was_forfeit,
            "BGF: games decided by time or forfeit are not supported"
        );
        let standard = [
            2, 0, 0, 0, 0, -5, 0, -3, 0, 0, 0, 5, -5, 0, 0, 0, 3, 0, 5, 0, 0, 0, 0, -2, 0, 0, 0, 0,
        ];
        ensure!(
            game.initial.points == standard
                && game.initial.cube == 1
                && game.initial.cube_owner == 0,
            "BGF: custom starting positions/cube setups are not supported"
        );
        let scores = if data.matchlen == 0 {
            [0, 0]
        } else {
            [game.score_green, game.score_red]
        };
        ensure!(
            scores.iter().all(|s| (0..=32767).contains(s)),
            "BGF: score exceeds GNUbg range"
        );
        let mut rules = Vec::new();
        if data.use_crawford {
            rules.push("Crawford");
        }
        if game.is_crawford {
            rules.push("CrawfordGame");
        }
        if data.use_jacoby && data.matchlen == 0 {
            rules.push("Jacoby");
        }
        out.push_str(&format!("(;FF[4]GM[6]CA[UTF-8]AP[BGTutor:BGF1]PW[{}]PB[{}]MI[length:{}][game:{game_index}][ws:{}][bs:{}]",escape(names[0]),escape(names[1]),data.matchlen,scores[0],scores[1]));
        if !rules.is_empty() {
            out.push_str(&format!("RU[{}]", rules.join(":")));
        }
        let root_end = out.len();
        let mut board = super::sgf::initial_board();
        let mut cube = 1i32;
        let mut dropped = false;
        let mut resigned = false;
        let mut pending = None;
        let mut winner = None;
        let mut won = false;
        for (move_index, record) in game.moves.iter().enumerate() {
            let mut apply = || -> Result<(String, Option<i32>, Option<i32>, bool)> {
                ensure!(!won, "BGF: records after game-win marker");
                ensure!(
                    ["move", "amove"].contains(&record.kind.as_str()),
                    "BGF: unsupported move record type {}",
                    record.kind
                );
                let side = player(record.player)?;
                let color = if side == 0 { 'W' } else { 'B' };
                if record.green != 7 {
                    ensure!(
                        pending.is_none() && winner.is_none(),
                        "BGF: checker play outside an active turn"
                    );
                    let pairs = runs(record)?;
                    let value = checker(&mut board, side, [record.green, record.red], &pairs)?;
                    let finished = if side == 0 {
                        !board.iter().any(|n| *n > 0)
                    } else {
                        !board.iter().any(|n| *n < 0)
                    };
                    return Ok((
                        value,
                        pending,
                        if finished { Some(side) } else { None },
                        false,
                    ));
                }
                if record.red == 5 {
                    ensure!(
                        pending.is_none() && winner.is_none(),
                        "BGF: dance outside an active turn"
                    );
                    ensure!(
                        record.from[1..] == [-1; 3] && record.to[1..] == [-1; 3],
                        "BGF: malformed dance dice"
                    );
                    return Ok((
                        checker(&mut board, side, [record.from[0], record.to[0]], &[])?,
                        pending,
                        winner,
                        false,
                    ));
                }
                empty_coordinates(record)?;
                match record.red {
                    1 => {
                        ensure!(
                            pending.is_none() && winner.is_none(),
                            "BGF: double outside active game"
                        );
                        Ok((format!(";{color}[double]"), Some(side), None, false))
                    }
                    2 | 3 => {
                        let doubler = pending.context("BGF: cube response without a double")?;
                        ensure!(
                            side != doubler && winner.is_none(),
                            "BGF: invalid cube responder"
                        );
                        Ok((
                            format!(
                                ";{color}[{}]",
                                if record.red == 2 { "take" } else { "drop" }
                            ),
                            None,
                            if record.red == 3 { Some(doubler) } else { None },
                            false,
                        ))
                    }
                    4 => {
                        ensure!(
                            pending.is_none() && winner.is_none_or(|w| w == side),
                            "BGF: inconsistent game winner"
                        );
                        Ok((String::new(), None, Some(side), true))
                    }
                    9 => {
                        let opponent = 1 - side;
                        ensure!(
                            pending.is_none() && winner.is_none_or(|w| w == opponent),
                            "BGF: inconsistent resignation"
                        );
                        Ok((String::new(), None, Some(opponent), false))
                    }
                    6 | 8 => {
                        ensure!(winner.is_none(), "BGF: control record after game end");
                        Ok((String::new(), pending, winner, false))
                    }
                    action => bail!("BGF: unsupported control action {action}"),
                }
            };
            let (value, next_pending, next_winner, next_won) = apply().with_context(|| {
                format!("BGF game {}, record {}", game_index + 1, move_index + 1)
            })?;
            if record.green == 7 {
                match record.red {
                    2 => {
                        cube = cube
                            .checked_mul(2)
                            .filter(|c| *c <= 32768)
                            .context("BGF: cube exceeds GNUbg range")?
                    }
                    3 => dropped = true,
                    9 => resigned = true,
                    _ => {}
                }
            }
            out.push_str(&value);
            if !value.is_empty() && !record.foreign.is_empty() {
                out.push_str("BGA[");
                out.push_str(&super::sgf::encode_retained_analysis(
                    &serde_json::to_value(&record.foreign)?,
                )?);
                out.push(']');
            }
            pending = next_pending;
            winner = next_winner;
            won = next_won;
        }
        ensure!(
            pending.is_none(),
            "BGF: unanswered cube offer in game {}",
            game_index + 1
        );
        ensure!(
            won || game_index + 1 == data.games.len(),
            "BGF: unfinished game before another game"
        );
        if data.matchlen > 0
            && won
            && let Some(side) = winner
        {
            // BGBlitz can leave wonPoints=0 after a completed bear-off. A
            // replayed terminal board or drop proves the award independently.
            let sign = if side == 0 { 1 } else { -1 };
            let points = if dropped {
                Some(cube)
            } else if !board.iter().any(|n| n * sign > 0) {
                let remaining: i32 = board.iter().map(|n| (-n * sign).max(0)).sum();
                let loser_in_home = if side == 0 {
                    board[0] < 0 || board[1..=6].iter().any(|n| *n < 0)
                } else {
                    board[25] > 0 || board[19..=24].iter().any(|n| *n > 0)
                };
                Some(
                    cube * if remaining < 15 {
                        1
                    } else if loser_in_home {
                        3
                    } else {
                        2
                    },
                )
            } else if resigned && (1..=32767).contains(&game.won_points) {
                Some(game.won_points)
            } else {
                None
            };
            // Legal cube values can produce awards beyond SGF's supported
            // result range. Keep the played line without inventing a result.
            if let Some(points) = points.filter(|p| (1..=32767).contains(p)) {
                let color = if side == 0 { 'W' } else { 'B' };
                let suffix = if resigned { "R" } else { "" };
                out.insert_str(root_end, &format!("RE[{color}+{points}{suffix}]"));
            }
        }
        out.push_str(")\n");
    }
    super::parse_sgf(&out).context("BGF played-line validation failed")?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use serde_json::{Value, json};
    use std::io::Write;

    fn record(player: i32, dice: [i32; 2], from: [i32; 4], to: [i32; 4]) -> Value {
        json!({"type":"move","player":player,"green":dice[0],"red":dice[1],"from":from,"to":to})
    }
    fn special(player: i32, action: i32) -> Value {
        record(player, [7, action], [-1; 4], [-1; 4])
    }
    fn fixture() -> Value {
        let initial = json!({"cube":1,"cubeOwner":0,"egyptian":false,"gameMode":0,"points":[2,0,0,0,0,-5,0,-3,0,0,0,5,-5,0,0,0,3,0,5,0,0,0,0,-2,0,0,0,0]});
        json!({"matchlen":7,"count":1,"gameMode":0,"nameGreen":"Green ] player","nameRed":"Red \\ player","useCrawford":true,"useJacoby":true,"useCube":true,"useBeaver":false,"cubeLimit":-1,
          "games":[{"gameMode":0,"initial":initial,"nameGreen":"Green ] player","nameRed":"Red \\ player","matchlen":7,"scoreGreen":0,"scoreRed":0,"isCrawford":false,"lostOnTime":false,"wasForfeit":false,"wasResignation":true,"wonPoints":2,
          "moves":[record(-1,[3,1],[8,6,-1,-1],[5,5,-1,-1]),record(1,[4,2],[8,6,-1,-1],[4,4,-1,-1]),special(-1,1),special(1,2),record(-1,[5,1],[13,-1,-1,-1],[7,-1,-1,-1]),special(1,9),special(-1,4)]}]})
    }
    fn package(value: &Value, smile: bool, compressed: bool) -> Vec<u8> {
        let mut payload = if smile {
            serde_smile::to_vec(value).unwrap()
        } else {
            serde_json::to_vec(value).unwrap()
        };
        if compressed {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(&payload).unwrap();
            payload = encoder.finish().unwrap();
        }
        let mut output = serde_json::to_vec(
            &json!({"format":"BGF","version":"1.0","useSmile":smile,"compress":compressed}),
        )
        .unwrap();
        output.push(b'\n');
        output.extend(payload);
        output
    }

    #[test]
    fn completed_resignation_preserves_explicit_winner_and_points() {
        let sgf = bgf_to_sgf(&package(&fixture(), true, true)).unwrap();
        let trees = super::super::sgf::parse_trees(&sgf).unwrap();
        assert_eq!(trees[0][0].get("RE"), Some(&vec!["W+2R".to_owned()]));
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(2), Some(0))
        );
    }

    #[test]
    fn completed_drop_scores_from_the_accepted_cube_when_won_points_is_zero() {
        let mut value = fixture();
        value["games"][0]["scoreGreen"] = json!(5);
        value["games"][0]["wonPoints"] = json!(0);
        value["games"][0]["wasResignation"] = json!(false);
        let mut moves = value["games"][0]["moves"].as_array().unwrap()[..5].to_vec();
        moves.extend([special(1, 1), special(-1, 3), special(1, 4)]);
        value["games"][0]["moves"] = json!(moves);
        let sgf = bgf_to_sgf(&package(&value, false, false)).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(5), Some(2))
        );
    }

    #[test]
    fn unfinished_resignation_without_points_does_not_invent_a_result() {
        let mut value = fixture();
        value["games"][0]["wonPoints"] = json!(0);
        let sgf = bgf_to_sgf(&package(&value, false, false)).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(0), Some(0))
        );
        assert_eq!(metadata.result_state, "ongoing");
    }

    #[test]
    fn analyzed_moves_preserve_foreign_fields_and_depth() {
        let mut value = fixture();
        value["games"][0]["moves"][0]["type"] = json!("amove");
        value["games"][0]["moves"][0]["ply"] = json!(3);
        value["games"][0]["moves"][0]["moveAnalysis"] = json!([{"move":{"from":[8,6,-1,-1],"to":[5,5,-1,-1]},"eq":{"equity":0.123456789012345,"myWins":0.55}}]);
        for (smile, gzip) in [(false, false), (true, true)] {
            let sgf = bgf_to_sgf(&package(&value, smile, gzip)).unwrap();
            let trees = super::super::sgf::parse_trees(&sgf).unwrap();
            let node = trees[0]
                .iter()
                .find(|n| n.contains_key("BGA"))
                .expect("BGF analysis must survive conversion");
            let data: Value = super::super::sgf::decode_retained_analysis(&node["BGA"][0]).unwrap();
            assert_eq!(data["ply"], 3);
            assert_eq!(data["moveAnalysis"][0]["eq"]["equity"], 0.123456789012345);
        }
    }
    #[test]
    fn json_and_smile_optional_gzip_preserve_plays_and_cube_context() {
        for smile in [false, true] {
            for compressed in [false, true] {
                let sgf = bgf_to_sgf(&package(&fixture(), smile, compressed)).unwrap();
                let parsed = crate::import::parse_sgf(&sgf).unwrap();
                assert_eq!(parsed.players, ["Green ] player", "Red \\ player"]);
                assert_eq!(parsed.match_length, 7);
                assert_eq!(parsed.positions.len(), 3);
                assert_eq!(parsed.positions[0].notation, "8/5 6/5");
                assert_eq!(parsed.positions[0].player, 0);
                assert_eq!(parsed.positions[1].player, 1);
                assert_eq!(parsed.positions[2].dice, [5, 1]);
                assert!(sgf.contains(";W[double];B[take]"));
            }
        }
    }
    #[test]
    fn colors_dances_and_cube_decisions_match_independent_native_oracle() {
        let mut value = fixture();
        value["matchlen"] = json!(11);
        value["nameGreen"] = json!("Douglas Mayfield");
        value["nameRed"] = json!("Akiko Yazawa");
        let game = &mut value["games"][0];
        game["matchlen"] = json!(11);
        game["nameGreen"] = json!("Douglas Mayfield");
        game["nameRed"] = json!("Akiko Yazawa");
        game["moves"] = json!([
            record(-1, [6, 2], [24, 13, -1, -1], [18, 11, -1, -1]),
            record(1, [5, 1], [13, 8, -1, -1], [8, 7, -1, -1]),
            record(-1, [3, 1], [25, 13, -1, -1], [24, 10, -1, -1]),
            record(1, [5, 1], [13, 8, -1, -1], [8, 7, -1, -1]),
            record(-1, [4, 2], [8, 6, -1, -1], [4, 4, -1, -1]),
            record(1, [5, 1], [24, 23, -1, -1], [23, 18, -1, -1]),
            record(-1, [6, 6], [13, 13, 7, 7], [7, 7, 1, 1]),
            record(1, [7, 5], [6, -1, -1, -1], [1, -1, -1, -1]),
            special(-1, 1),
            special(1, 3),
            special(-1, 4)
        ]);
        let sgf = bgf_to_sgf(&package(&value, true, true)).unwrap();
        let parsed = crate::import::parse_sgf(&sgf).unwrap();
        let oracle =
            crate::import::parse_sgf(include_str!("../../tests/fixtures/WC2014.gnubg.sgf"))
                .unwrap();
        assert_eq!(parsed.positions.len(), 8);
        for (actual, expected) in parsed.positions.iter().zip(&oracle.positions) {
            assert_eq!(actual.player, expected.player);
            assert_eq!(actual.position_id, expected.position_id);
            assert_eq!(actual.match_id, expected.match_id);
            assert_eq!(actual.board, expected.board);
            assert_eq!(actual.dice, expected.dice);
        }
        assert_eq!(parsed.positions[7].notation, "Cannot move");
    }
    #[test]
    fn full_public_match_roundtrips_all_460_native_position_and_match_ids() {
        let source = include_str!("../../tests/fixtures/WC2014.gnubg.sgf");
        let trees = crate::import::sgf::parse_trees(source).unwrap();
        let mut value = fixture();
        value["matchlen"] = json!(11);
        value["count"] = json!(trees.len());
        value["nameGreen"] = json!("Douglas Mayfield");
        value["nameRed"] = json!("Akiko Yazawa");
        let template = value["games"][0].clone();
        let mut games = Vec::new();
        for nodes in trees {
            let mut game = template.clone();
            game["matchlen"] = json!(11);
            game["nameGreen"] = json!("Douglas Mayfield");
            game["nameRed"] = json!("Akiko Yazawa");
            for field in &nodes[0]["MI"] {
                if let Some(score) = field.strip_prefix("ws:") {
                    game["scoreGreen"] = json!(score.parse::<i32>().unwrap());
                }
                if let Some(score) = field.strip_prefix("bs:") {
                    game["scoreRed"] = json!(score.parse::<i32>().unwrap());
                }
            }
            game["isCrawford"] = json!(
                nodes[0]
                    .get("RU")
                    .is_some_and(|r| r[0].contains("CrawfordGame"))
            );
            let mut moves = Vec::new();
            for node in &nodes {
                for (property, side) in [("W", -1), ("B", 1)] {
                    let Some(values) = node.get(property) else {
                        continue;
                    };
                    let play = &values[0];
                    let action = match play.as_str() {
                        "double" => Some(1),
                        "take" => Some(2),
                        "drop" => Some(3),
                        _ => None,
                    };
                    if let Some(action) = action {
                        moves.push(special(side, action));
                        continue;
                    }
                    let b = play.as_bytes();
                    let dice = [i32::from(b[0] - b'0'), i32::from(b[1] - b'0')];
                    if b.len() == 2 {
                        moves.push(record(
                            side,
                            [7, 5],
                            [dice[0], -1, -1, -1],
                            [dice[1], -1, -1, -1],
                        ));
                        continue;
                    }
                    let mut from = [-1; 4];
                    let mut to = [-1; 4];
                    let point = |c: u8| match c {
                        b'y' => 25,
                        b'z' => 0,
                        _ if side == -1 => i32::from(c - b'a' + 1),
                        _ => i32::from(b'x' - c + 1),
                    };
                    for (i, pair) in b[2..].chunks_exact(2).enumerate() {
                        from[i] = point(pair[0]);
                        to[i] = point(pair[1]);
                    }
                    moves.push(record(side, dice, from, to));
                }
            }
            if let Some(result) = nodes[0].get("RE") {
                let side = if result[0].starts_with('W') { -1 } else { 1 };
                if result[0].contains('R') {
                    moves.push(special(-side, 9));
                }
                moves.push(special(side, 4));
            }
            game["moves"] = json!(moves);
            games.push(game);
        }
        value["games"] = json!(games);
        // Some BGBlitz exports leave wonPoints at zero even after a full bear-off.
        for game in value["games"].as_array_mut().unwrap() {
            game["wonPoints"] = json!(0);
        }
        let sgf = bgf_to_sgf(&package(&value, true, true)).unwrap();
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(9), Some(11))
        );
        assert_eq!(metadata.result_state, "completed");
        let parsed = crate::import::parse_sgf(&sgf).unwrap();
        let expected = crate::import::parse_sgf(source).unwrap();
        assert_eq!(parsed.positions.len(), 460);
        for (a, b) in parsed.positions.iter().zip(&expected.positions) {
            assert_eq!((a.game_no, a.ply, a.player), (b.game_no, b.ply, b.player));
            assert_eq!(a.position_id, b.position_id);
            assert_eq!(a.match_id, b.match_id);
            assert_eq!(a.board, b.board);
            assert_eq!(a.dice, b.dice);
        }
    }

    #[test]
    fn high_cube_bearoff_keeps_legal_import_when_award_exceeds_sgf_result_range() {
        let source = include_str!("../../tests/fixtures/WC2014.gnubg.sgf");
        let trees = crate::import::sgf::parse_trees(source).unwrap();
        let mut value = fixture();
        value["games"][0]["wonPoints"] = json!(0);
        let mut moves = Vec::new();
        let mut checker_count = 0;
        for node in trees.last().unwrap() {
            for (property, side) in [("W", -1), ("B", 1)] {
                let Some(play) = node.get(property).map(|values| values[0].as_bytes()) else {
                    continue;
                };
                if !play[0].is_ascii_digit() {
                    continue;
                }
                if (2..17).contains(&checker_count) {
                    moves.extend([special(side, 1), special(-side, 2)]);
                }
                let point = |c| match c {
                    b'y' => 25,
                    b'z' => 0,
                    _ if side == -1 => i32::from(c - b'a' + 1),
                    _ => i32::from(b'x' - c + 1),
                };
                let mut from = [-1; 4];
                let mut to = [-1; 4];
                for (i, pair) in play[2..].chunks_exact(2).enumerate() {
                    from[i] = point(pair[0]);
                    to[i] = point(pair[1]);
                }
                moves.push(record(
                    side,
                    [i32::from(play[0] - b'0'), i32::from(play[1] - b'0')],
                    from,
                    to,
                ));
                checker_count += 1;
            }
        }
        moves.push(special(1, 4)); // Black bears off for a single at cube 32768.
        value["games"][0]["moves"] = json!(moves);
        let sgf = bgf_to_sgf(&package(&value, false, false))
            .expect("legal played line must remain importable");
        let imported = crate::import::parse_sgf(&sgf).unwrap();
        assert_eq!(imported.positions.len(), checker_count);
        assert!(
            !sgf.contains("RE["),
            "unrepresentable result must not be emitted or clamped"
        );
        let metadata = crate::databases::source_metadata(&sgf);
        assert_eq!(metadata.result_state, "ongoing");
        assert_eq!(
            (metadata.white_score, metadata.black_score),
            (Some(0), Some(0)),
            "only the recorded starting score is known"
        );
    }

    #[test]
    fn capped_cube_rules_are_not_silently_analyzed_as_unlimited() {
        let mut value = fixture();
        value["cubeLimit"] = json!(2);
        assert!(
            bgf_to_sgf(&package(&value, true, true))
                .unwrap_err()
                .to_string()
                .contains("cube limit")
        );
    }

    #[test]
    fn transport_expansion_limit_and_multiple_gzip_members_are_rejected() {
        for compressed in [false, true] {
            let bytes = package(&fixture(), false, compressed);
            let split = bytes.iter().position(|b| *b == b'\n').unwrap();
            let header: Header = serde_json::from_slice(&bytes[..split]).unwrap();
            assert!(
                payload(&header, &bytes[split + 1..], 32)
                    .unwrap_err()
                    .to_string()
                    .contains("size limit")
            );
        }
        let mut bytes = package(&fixture(), true, true);
        let another = package(&fixture(), true, true);
        let split = another.iter().position(|b| *b == b'\n').unwrap();
        bytes.extend_from_slice(&another[split + 1..]);
        assert!(bgf_to_sgf(&bytes).is_err());
    }

    #[test]
    fn malformed_headers_versions_gzip_and_trailing_payload_are_rejected() {
        for input in [
            b"".as_slice(),
            b"BGF",
            b"{}\n{}",
            b"{\"format\":\"BGF\",\"version\":\"2.0\",\"compress\":false,\"useSmile\":false}\n{}",
        ] {
            assert!(bgf_to_sgf(input).is_err());
        }
        let mut bytes = package(&fixture(), true, true);
        let last = bytes.len() - 5;
        bytes[last] ^= 1;
        assert!(bgf_to_sgf(&bytes).is_err());
        for (smile, compressed) in [(false, false), (true, false), (true, true)] {
            let mut bytes = package(&fixture(), smile, compressed);
            bytes.extend_from_slice(b"extra");
            assert!(bgf_to_sgf(&bytes).is_err());
        }
    }
    #[test]
    fn invalid_actors_actions_variants_and_incomplete_cube_offers_are_rejected() {
        let mut value = fixture();
        value["games"][0]["moves"][0]["player"] = json!(0);
        assert!(bgf_to_sgf(&package(&value, false, false)).is_err());
        let mut value = fixture();
        value["games"][0]["moves"][0] = special(-1, 99);
        assert!(bgf_to_sgf(&package(&value, false, false)).is_err());
        let mut value = fixture();
        value["gameMode"] = json!(1);
        assert!(bgf_to_sgf(&package(&value, false, false)).is_err());
        let mut value = fixture();
        value["games"][0]["moves"]
            .as_array_mut()
            .unwrap()
            .truncate(3);
        assert!(bgf_to_sgf(&package(&value, false, false)).is_err());
        let mut value = fixture();
        value["games"][0]["moves"][0]["from"] = json!([8, -1, 6, -1]);
        assert!(bgf_to_sgf(&package(&value, false, false)).is_err());
    }
}
