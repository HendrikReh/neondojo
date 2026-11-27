//! Reader for the official public-domain XG opening position database.
//! Format and attribution terms: https://www.extremegammon.com/GetOb.aspx.

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::SystemTime,
};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XgBookStatus {
    pub available: bool,
    pub path: String,
    pub book_version: Option<String>,
    pub entries: i64,
    pub notes: String,
    pub source_url: String,
    pub license: String,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XgRollout {
    pub games: u32,
    pub standard_deviation: f64,
    pub checker_level: i32,
    pub cube_level: i32,
    pub dice_rotation: i32,
    pub seed: i32,
    pub truncation: i32,
    pub duration_seconds: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XgEvaluation {
    pub record_index: i32,
    pub source_author: String,
    pub engine: String,
    pub engine_version: String,
    pub method: String,
    pub depth: String,
    pub level: i32,
    pub plies: Option<i32>,
    pub equity: f64,
    pub equity_kind: String,
    pub probabilities: Vec<f64>,
    pub native_equity: f64,
    pub native_probabilities: Vec<f64>,
    pub rollout: Option<XgRollout>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XgPositionResult {
    pub position_key: String,
    pub perspective: String,
    pub native_perspective: String,
    pub book_version: String,
    pub source_url: String,
    pub license: String,
    pub evaluations: Vec<XgEvaluation>,
}

pub fn default_path() -> String {
    std::path::Path::new(&crate::opening_book::default_path())
        .with_file_name("OpeningbookV2.ob")
        .to_string_lossy()
        .into_owned()
}
const SOURCE_URL: &str = "https://www.extremegammon.com/GetOb.aspx";
const LICENSE: &str = "Public domain; preserve and display each entry's rollout author";
const MAX_FILE: usize = 16 * 1024 * 1024;

pub async fn status(path: &str) -> XgBookStatus {
    Cache::default().status(path).await
}

/// A reference for the board before this roll, never a ranked answer to its dice.
/// Money lookups require the source's actual beaver rule; unknown is a miss.
pub async fn lookup(
    path: &str,
    position: &crate::model::Position,
    beaver: Option<bool>,
) -> Result<Option<XgPositionResult>> {
    Cache::default().lookup(path, position, beaver).await
}

pub async fn install_from_path(destination: &str, source: &str) -> Result<XgBookStatus> {
    Cache::default()
        .install_from_path(destination, source)
        .await
}

#[derive(Clone, Default)]
pub(crate) struct Cache(Arc<Mutex<HashMap<String, Cached>>>);

impl Cache {
    pub(crate) async fn status(&self, path: &str) -> XgBookStatus {
        let path = if path.is_empty() {
            default_path()
        } else {
            path.to_owned()
        };
        let task_path = path.clone();
        let cache = self.clone();
        match tokio::task::spawn_blocking(move || {
            cache.load(&task_path).map(|book| book.status.clone())
        })
        .await
        {
            Ok(Ok(status)) => status,
            result => XgBookStatus {
                available: false,
                path,
                book_version: None,
                entries: 0,
                notes: String::new(),
                source_url: SOURCE_URL.into(),
                license: LICENSE.into(),
                error: Some(match result {
                    Ok(Err(error)) => format!("{error:#}"),
                    Err(error) => error.to_string(),
                    _ => unreachable!(),
                }),
            },
        }
    }
    pub(crate) async fn lookup(
        &self,
        path: &str,
        position: &crate::model::Position,
        beaver: Option<bool>,
    ) -> Result<Option<XgPositionResult>> {
        let position = position.clone();
        let path = if path.is_empty() {
            default_path()
        } else {
            path.into()
        };
        let cache = self.clone();
        tokio::task::spawn_blocking(move || {
            let (key, position_key) = Key::from_position(&position, beaver)?;
            let book = cache.load(&path)?;
            Ok(book.result(&key, position_key))
        })
        .await
        .context("XG opening book reader task failed")?
    }

    pub(crate) async fn install_from_path(
        &self,
        destination: &str,
        source: &str,
    ) -> Result<XgBookStatus> {
        let destination = destination.to_owned();
        let source = source.to_owned();
        let cache = self.clone();
        tokio::task::spawn_blocking(move || {
            use std::{fs, path::Path};
            let source = Path::new(&source);
            let destination = Path::new(&destination);
            cache.load(source.to_str().context("Invalid XG source path")?)?;
            if fs::canonicalize(source).ok() == fs::canonicalize(destination).ok()
                && destination.exists()
            {
                return Ok(cache
                    .load(destination.to_str().context("Invalid XG destination")?)?
                    .status
                    .clone());
            }
            let parent = destination
                .parent()
                .context("Invalid XG destination directory")?;
            crate::opening_book::install_validated_copy(source, destination, parent, |path| {
                cache.load(path.to_str().context("Invalid temporary XG path")?)?;
                Ok(())
            })?;
            Ok(cache
                .load(destination.to_str().context("Invalid XG destination")?)?
                .status
                .clone())
        })
        .await
        .context("XG book installer task failed")?
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    board: [i8; 26],
    cube: i32,
    owner: i32,
    away: [i32; 2],
    jacoby: bool,
    beaver: bool,
    crawford: bool,
}
impl Key {
    fn from_position(
        position: &crate::model::Position,
        beaver: Option<bool>,
    ) -> Result<(Self, String)> {
        use crate::backgammon::ids::{Field, MatchId};
        let (canonical, _) = crate::opening_book::canonical_keys(position)?;
        let mut parts: Vec<String> = canonical.split(':').map(str::to_owned).collect();
        let length = parts[8].parse::<i32>()?;
        ensure!(
            length > 0 || beaver.is_some(),
            "Money-game beaver rule is not recorded; no XG book context assumed"
        );
        let beaver = length == 0 && beaver == Some(true);
        let jacoby = length == 0 && parts[7] == "1";
        let away = if length == 0 {
            [-1, -1]
        } else {
            [
                length - parts[5].parse::<i32>()?,
                length - parts[6].parse::<i32>()?,
            ]
        };
        let cube_exp = MatchId::decode(&position.gnubg_match_id)
            .context("Invalid GNUbg Match ID")?
            .get(Field::CUBE_LOG2);
        let cube = 1i32 << cube_exp;
        let board: Vec<i8> = parts[0]
            .bytes()
            .map(|byte| match byte {
                b'A'..=b'O' => (byte - b'A' + 1) as i8,
                b'a'..=b'o' => -((byte - b'a' + 1) as i8),
                _ => 0,
            })
            .collect();
        let key = Self {
            board: board
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid XG board"))?,
            cube,
            owner: parts[2].parse()?,
            away,
            jacoby,
            beaver,
            crawford: length > 0 && parts[7] == "1",
        };
        // Keep the actual cube and beaver flag, which the BGDB key normalizes.
        parts[1] = cube_exp.to_string();
        parts[4] = "00".into();
        if length == 0 {
            parts[7] = (i32::from(jacoby) + 2 * i32::from(beaver)).to_string();
        }
        Ok((key, parts.join(":")))
    }
}

struct Book {
    status: XgBookStatus,
    index: HashMap<Key, Vec<XgEvaluation>>,
}
impl Book {
    fn parse(bytes: &[u8], path: &str) -> Result<Self> {
        ensure!(
            bytes.len() >= 256 && bytes.len() <= MAX_FILE && bytes.len().is_multiple_of(256),
            "Invalid XG book size or record boundary"
        );
        ensure!(
            integer(bytes, 0)? == 0 && bytes.get(4..8) == Some(b"OBDB"),
            "Not an XG opening book"
        );
        ensure!(
            integer(bytes, 8)? == 1 && integer(bytes, 12)? == 0 && integer(bytes, 16)? == 256,
            "Unsupported XG book layout"
        );
        let version_len = usize::from(bytes[32]);
        ensure!(version_len <= 8, "Invalid XG book version");
        let version = std::str::from_utf8(&bytes[33..33 + version_len])?.to_owned();
        let mut index: HashMap<Key, Vec<XgEvaluation>> = HashMap::new();
        let mut notes = String::new();
        let mut entries = 0;
        for (record_index, record) in bytes.chunks_exact(256).enumerate().skip(1) {
            match integer(record, 0)? {
                1 => {
                    notes.push_str(&unicode(record, 4, 96)?);
                }
                2 => {
                    ensure!(record[216] <= 1, "Invalid XG deletion flag");
                    if record[216] != 0 {
                        continue;
                    }
                    let source_author = unicode(record, 4, 32)?;
                    ensure!(
                        !source_author.trim().is_empty(),
                        "XG entry lacks its mandatory author attribution"
                    );
                    let board: [i8; 26] = std::array::from_fn(|i| record[68 + i] as i8);
                    ensure!(
                        board[0] <= 0
                            && board[25] >= 0
                            && board.iter().all(|n| (-15..=15).contains(n)),
                        "Invalid XG board"
                    );
                    for side in [1i32, -1] {
                        let count: i32 = board.iter().map(|n| (i32::from(*n) * side).max(0)).sum();
                        ensure!((1..=15).contains(&count), "Invalid XG checker count");
                    }
                    // v3.70 stores the actual cube value: 1 centered, 2 turned.
                    // The website's exponent comment does not match its file.
                    let cube = integer(record, 96)?;
                    let owner = integer(record, 100)?;
                    let away = [integer(record, 104)?, integer(record, 108)?];
                    ensure!(
                        cube > 0
                            && cube <= 32768
                            && (cube as u32).is_power_of_two()
                            && (-1..=1).contains(&owner),
                        "Invalid XG cube state"
                    );
                    ensure!(
                        away == [-1, -1] || away.iter().all(|score| *score > 0 && *score <= 32767),
                        "Invalid XG away score"
                    );
                    // Some official point-match records retain the producer's
                    // inert money-game flag. Validate it, then ignore it only
                    // for point matches; money contexts must remain distinct.
                    let beaver = flag(record, 116)? && away == [-1, -1];
                    let key = Key {
                        board,
                        cube,
                        owner,
                        away,
                        jacoby: flag(record, 112)?,
                        beaver,
                        crawford: flag(record, 120)?,
                    };
                    let native: Vec<f64> = (0..6)
                        .map(|i| single(record, 124 + 4 * i))
                        .collect::<Result<_>>()?;
                    // Historical rollout records retain finite decimal rounding
                    // error in win+loss (the official file includes 0.9999899).
                    // Preserve those source values rather than renormalizing.
                    ensure!(
                        native.iter().all(|n| (0.0..=1.0).contains(n))
                            && native[0] <= native[1]
                            && native[1] <= native[2]
                            && native[5] <= native[4]
                            && native[4] <= native[3]
                            && (native[2] + native[3] - 1.0).abs() < 0.0001,
                        "Invalid XG probabilities"
                    );
                    let native_equity = single(record, 148)?;
                    let level = integer(record, 152)?;
                    let program = integer(record, 156)?;
                    let engine = match program {
                        0 => "eXtreme Gammon".into(),
                        1 => "Snowie".into(),
                        2 => "GNU Backgammon".into(),
                        3 => "BGBlitz".into(),
                        other => format!("Unknown (XG book program {other})"),
                    };
                    let engine_version =
                        format!("{}.{:02}", integer(record, 160)?, integer(record, 164)?);
                    let (method, depth, plies) = match level {
                        100 => ("rollout", "Rollout".into(), None),
                        1000 => ("xgroller", "XGRoller".into(), None),
                        1001 => ("xgroller", "XGRoller+".into(), None),
                        1002 => ("xgroller", "XGRoller++".into(), None),
                        // Numeric levels are retained without assuming they are
                        // GNUbg plies or XG file-format level-table indices.
                        numeric @ 0..=64 => {
                            ("evaluation", format!("Native book level {numeric}"), None)
                        }
                        other => (
                            "unknown",
                            format!("Unknown native book level {other}"),
                            None,
                        ),
                    };
                    let rollout = if level == 100 {
                        Some(XgRollout {
                            games: u32::from_le_bytes(record[168..172].try_into()?),
                            standard_deviation: single(record, 172)?,
                            checker_level: integer(record, 176)?,
                            cube_level: integer(record, 180)?,
                            dice_rotation: integer(record, 184)?,
                            seed: integer(record, 188)?,
                            truncation: integer(record, 192)?,
                            duration_seconds: single(record, 196)?,
                        })
                    } else {
                        None
                    };
                    // Entries are successor boards with the next player on
                    // roll positive, but their values describe the prior mover.
                    // Independently verified against the publisher's Opening31
                    // money and 1-away/2-away tables. Keep the native vector and
                    // project to the current player by swapping sides/negating.
                    let evaluation = XgEvaluation {
                        record_index: record_index as i32,
                        source_author,
                        engine,
                        engine_version,
                        method: method.into(),
                        depth,
                        level,
                        plies,
                        equity: -native_equity,
                        equity_kind: "normalized_cubeful".into(),
                        probabilities: vec![
                            native[2], native[1], native[0], native[3], native[4], native[5],
                        ],
                        native_equity,
                        native_probabilities: native,
                        rollout,
                    };
                    index.entry(key).or_default().push(evaluation);
                    entries += 1;
                }
                other => anyhow::bail!("Unsupported XG book record type {other}"),
            }
        }
        ensure!(entries > 0, "XG book has no active position entries");
        Ok(Self {
            status: XgBookStatus {
                available: true,
                path: path.into(),
                book_version: Some(version),
                entries,
                notes,
                source_url: SOURCE_URL.into(),
                license: LICENSE.into(),
                error: None,
            },
            index,
        })
    }
    fn result(&self, key: &Key, position_key: String) -> Option<XgPositionResult> {
        self.index.get(key).map(|evaluations| XgPositionResult {
            position_key,
            perspective: "player_on_roll".into(),
            native_perspective: "opponent_of_player_on_roll".into(),
            book_version: self.status.book_version.clone().unwrap_or_default(),
            source_url: SOURCE_URL.into(),
            license: LICENSE.into(),
            evaluations: evaluations.clone(),
        })
    }
}
fn integer(bytes: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .context("Truncated XG integer")?
            .try_into()?,
    ))
}
fn flag(bytes: &[u8], offset: usize) -> Result<bool> {
    let value = integer(bytes, offset)?;
    ensure!(matches!(value, 0 | 1), "Invalid XG rule flag");
    Ok(value == 1)
}
fn single(bytes: &[u8], offset: usize) -> Result<f64> {
    let value = f32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .context("Truncated XG float")?
            .try_into()?,
    ) as f64;
    ensure!(value.is_finite(), "Non-finite XG evaluation");
    Ok(value)
}
fn unicode(bytes: &[u8], offset: usize, width: usize) -> Result<String> {
    let words: Vec<u16> = bytes
        .get(offset..offset + 2 * width)
        .context("Truncated XG author string")?
        .chunks_exact(2)
        .map(|word| u16::from_le_bytes([word[0], word[1]]))
        .take_while(|word| *word != 0)
        .collect();
    Ok(String::from_utf16(&words)?)
}

struct Cached {
    length: u64,
    modified: SystemTime,
    book: Arc<Book>,
}

impl Cache {
    fn load(&self, path: &str) -> Result<Arc<Book>> {
        let metadata =
            std::fs::metadata(path).with_context(|| format!("Cannot open XG book at {path}"))?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_FILE as u64,
            "XG book exceeds 16 MiB limit or is not a file"
        );
        let modified = metadata.modified()?;
        {
            let cache = self
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("XG book cache is unavailable"))?;
            if let Some(cached) = cache.get(path)
                && cached.length == metadata.len()
                && cached.modified == modified
            {
                return Ok(cached.book.clone());
            }
        }
        let bytes = std::fs::read(path)?;
        let book = Arc::new(Book::parse(&bytes, path)?);
        let mut cache = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("XG book cache is unavailable"))?;
        if cache.len() >= 4 {
            cache.clear();
        }
        cache.insert(
            path.into(),
            Cached {
                length: metadata.len(),
                modified,
                book: book.clone(),
            },
        );
        Ok(book)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 512];
        bytes[4..8].copy_from_slice(b"OBDB");
        bytes[8..12].copy_from_slice(&1i32.to_le_bytes());
        bytes[16..20].copy_from_slice(&256i32.to_le_bytes());
        bytes[32] = 4;
        bytes[33..37].copy_from_slice(b"3.70");
        bytes[256..260].copy_from_slice(&2i32.to_le_bytes());
        for (i, code) in "Source Author".encode_utf16().enumerate() {
            bytes[260 + i * 2..262 + i * 2].copy_from_slice(&code.to_le_bytes());
        }
        let mut board = [0i8; 26];
        for (point, count) in [
            (1, -2),
            (6, 5),
            (8, 3),
            (12, -5),
            (13, 5),
            (17, -3),
            (19, -5),
            (24, 2),
        ] {
            board[point] = count;
        }
        for (i, n) in board.iter().enumerate() {
            bytes[324 + i] = *n as u8;
        }
        for (offset, value) in [
            (96, 1),
            (104, 3),
            (108, 2),
            (152, 100),
            (160, 2),
            (164, 10),
            (168, 10368),
            (176, 1),
            (180, 12),
        ] {
            bytes[256 + offset..260 + offset].copy_from_slice(&i32::to_le_bytes(value));
        }
        for (offset, value) in [
            (124, 0.01f32),
            (128, 0.12),
            (132, 0.45),
            (136, 0.55),
            (140, 0.17),
            (144, 0.02),
            (148, 0.25),
            (172, 0.38),
        ] {
            bytes[256 + offset..260 + offset].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn parses_official_header_and_native_entry_layout() {
        assert!(Book::parse(&fixture(), "fixture.ob").is_ok());
    }
    #[test]
    fn rejects_unknown_layout_and_truncated_records() {
        let mut bytes = fixture();
        bytes[16] = 255;
        assert!(Book::parse(&bytes, "fixture.ob").is_err());
        assert!(Book::parse(&fixture()[..511], "fixture.ob").is_err());
    }
    #[test]
    fn keeps_author_and_native_values_while_projecting_current_player() {
        let book = Book::parse(&fixture(), "fixture.ob").unwrap();
        let entry = book.index.values().next().unwrap().first().unwrap();
        assert_eq!(entry.source_author, "Source Author");
        assert_eq!(entry.native_equity, 0.25);
        assert_eq!(entry.equity, -0.25);
        assert!((entry.probabilities[0] - 0.45).abs() < 0.000001);
        assert!((entry.probabilities[1] - 0.12).abs() < 0.000001);
        assert_eq!(entry.method, "rollout");
        assert_eq!(entry.rollout.as_ref().unwrap().games, 10368);
    }
    #[test]
    fn point_match_ignores_inert_beaver_flag_and_retains_each_author_record() {
        let mut bytes = fixture();
        let mut duplicate = bytes[256..512].to_vec();
        duplicate[116] = 1;
        bytes.extend(duplicate);
        let book = Book::parse(&bytes, "fixture.ob").unwrap();
        assert_eq!(book.index.len(), 1, "Beaver is inert in point matches");
        assert_eq!(book.index.values().next().unwrap().len(), 2);
        assert!(!book.index.keys().next().unwrap().beaver);
    }
    #[tokio::test]
    #[ignore = "requires official installed XG opening book"]
    async fn official_point_match_inert_beaver_record_remains_reachable() {
        let p = position("(;GM[6]MI[length:1];W[41mihg];B[42qusu])", 1);
        let result = lookup(&default_path(), &p, None).await.unwrap().unwrap();
        assert!(result.evaluations.iter().any(|e| e.record_index == 17912));
    }
    #[test]
    fn unknown_native_level_does_not_acquire_an_evaluation_method() {
        let mut bytes = fixture();
        bytes[408..412].copy_from_slice(&12345i32.to_le_bytes());
        let book = Book::parse(&bytes, "fixture.ob").unwrap();
        let entry = &book.index.values().next().unwrap()[0];
        assert_eq!(entry.method, "unknown");
        assert_eq!(entry.level, 12345);
        assert!(entry.plies.is_none());
    }
    fn position(sgf: &str, index: usize) -> crate::model::Position {
        let imported = crate::import::parse_sgf(sgf).unwrap();
        let p = &imported.positions[index];
        crate::model::Position {
            cube_enabled: None,
            id: uuid::Uuid::nil(),
            match_uuid: uuid::Uuid::nil(),
            ordinal: index as i32,
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
    fn setup_position(board: &str, cube: i32, owner: &str, player: &str) -> crate::model::Position {
        let mut white = String::from("AW");
        let mut black = String::from("AB");
        for (point, byte) in board.bytes().enumerate() {
            let count = match byte {
                b'A'..=b'O' => byte - b'A' + 1,
                b'a'..=b'o' => byte - b'a' + 1,
                _ => 0,
            };
            if count == 0 {
                continue;
            }
            let coordinate = if point == 0 || point == 25 {
                'y'
            } else {
                (b'a' + point as u8 - 1) as char
            };
            let output = if byte.is_ascii_uppercase() {
                &mut white
            } else {
                &mut black
            };
            for _ in 0..count {
                output.push_str(&format!("[{coordinate}]"));
            }
        }
        position(
            &format!(
                "(;GM[6]RU[Jacoby]AE[a:y]{white}{black}CV[{cube}]CO[{owner}];{player}[31{}])",
                if player == "W" { "hefe" } else { "qtst" }
            ),
            0,
        )
    }
    #[test]
    fn native_key_keeps_away_cube_beaver_and_mover_context() {
        let black = position("(;GM[6]MI[length:9][ws:7][bs:6]CV[2]CO[w];B[52lqqs])", 0);
        let (key, _) = Key::from_position(&black, None).unwrap();
        assert_eq!(key.away, [3, 2]);
        assert_eq!(key.owner, -1);
        assert_eq!(key.cube, 2);
        let money = position("(;GM[6]RU[Jacoby];W[31hefe])", 0);
        assert!(Key::from_position(&money, None).is_err());
        let (no_beaver, no_key) = Key::from_position(&money, Some(false)).unwrap();
        let (beaver, yes_key) = Key::from_position(&money, Some(true)).unwrap();
        assert_ne!(no_beaver, beaver);
        assert!(no_key.contains(":0:0:1:0:10"));
        assert!(yes_key.contains(":0:0:3:0:10"));
    }

    #[tokio::test]
    async fn exact_position_reference_ignores_roll_and_rejects_other_contexts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.ob");
        std::fs::write(&path, fixture()).unwrap();
        let path = path.to_str().unwrap();
        let first = position("(;GM[6]MI[length:9][ws:6][bs:7];W[31hefe])", 0);
        let reference = lookup(path, &first, None).await.unwrap().unwrap();
        assert_eq!(reference.perspective, "player_on_roll");
        assert_eq!(reference.native_perspective, "opponent_of_player_on_roll");
        assert!(reference.position_key.contains(":00:"));
        let other_roll = position("(;GM[6]MI[length:9][ws:6][bs:7];W[42hdfd])", 0);
        assert_eq!(
            lookup(path, &other_roll, None)
                .await
                .unwrap()
                .unwrap()
                .position_key,
            reference.position_key
        );
        let other_score = position("(;GM[6]MI[length:9][ws:7][bs:6];W[31hefe])", 0);
        assert!(lookup(path, &other_score, None).await.unwrap().is_none());
        let owned_cube = position("(;GM[6]MI[length:9][ws:6][bs:7]CV[2]CO[w];W[31hefe])", 0);
        assert!(lookup(path, &owned_cube, None).await.unwrap().is_none());
        let mirrored = position("(;GM[6]MI[length:9][ws:7][bs:6];B[31qtst])", 0);
        assert_eq!(
            lookup(path, &mirrored, None)
                .await
                .unwrap()
                .unwrap()
                .position_key,
            reference.position_key
        );
    }

    #[tokio::test]
    async fn local_install_validates_before_replacing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.ob");
        let dest = dir.path().join("installed.ob");
        std::fs::write(&source, fixture()).unwrap();
        assert!(
            install_from_path(dest.to_str().unwrap(), source.to_str().unwrap())
                .await
                .unwrap()
                .available
        );
        std::fs::write(&source, b"not an XG book").unwrap();
        assert!(
            install_from_path(dest.to_str().unwrap(), source.to_str().unwrap())
                .await
                .is_err()
        );
        assert!(status(dest.to_str().unwrap()).await.available);
    }
    #[tokio::test]
    #[ignore = "requires official installed XG opening book"]
    async fn official_book_matches_published_money_and_asymmetric_rollout_vectors() {
        let path = default_path();
        let state = status(&path).await;
        assert!(state.available, "{:?}", state.error);
        assert_eq!(state.entries, 53210);
        assert_eq!(state.book_version.as_deref(), Some("3.70"));
        let start = position("(;GM[6]RU[Jacoby];W[31hefe])", 0);
        assert!(lookup(&path, &start, Some(true)).await.unwrap().is_none());
        let money = position("(;GM[6]RU[Jacoby];W[31hefe];B[42qusu])", 1);
        let result = lookup(&path, &money, Some(true)).await.unwrap().unwrap();
        let entry = result
            .evaluations
            .iter()
            .find(|entry| entry.record_index == 18414)
            .unwrap();
        assert_eq!(entry.source_author, "GameSite 2000, Ltd");
        assert_eq!(entry.method, "rollout");
        assert!((entry.native_equity - 0.25366637110710144).abs() < 1e-12);
        assert!((entry.equity + 0.25366637110710144).abs() < 1e-12);
        assert!((entry.probabilities[0] - 0.44411638379096985).abs() < 1e-12);
        assert_eq!(entry.rollout.as_ref().unwrap().games, 10368);
        let mirrored = position("(;GM[6]RU[Jacoby];B[31qtst];W[42hdfd])", 1);
        let mirror = lookup(&path, &mirrored, Some(true)).await.unwrap().unwrap();
        assert_eq!(mirror.position_key, result.position_key);
        assert_eq!(mirror.evaluations[0].equity, result.evaluations[0].equity);
        assert!(lookup(&path, &money, None).await.is_err());
        let plain = lookup(&path, &money, Some(false)).await.unwrap().unwrap();
        assert!(
            plain
                .evaluations
                .iter()
                .all(|entry| entry.record_index != 18414)
        );
        let asymmetric = position("(;GM[6]MI[length:11][ws:10][bs:9];W[31hefe];B[42qusu])", 1);
        let result = lookup(&path, &asymmetric, None).await.unwrap().unwrap();
        assert!(
            result
                .evaluations
                .iter()
                .any(|entry| (entry.native_equity + 0.795830488204956).abs() < 1e-12)
        );
        let crawford = position(
            "(;GM[6]MI[length:11][ws:10][bs:9]RU[CrawfordGame];W[31hefe];B[42qusu])",
            1,
        );
        let result = lookup(&path, &crawford, None).await.unwrap().unwrap();
        assert!(
            result
                .evaluations
                .iter()
                .any(|entry| (entry.native_equity + 0.1163642406463623).abs() < 1e-12)
        );
    }

    #[tokio::test]
    #[ignore = "requires official installed XG opening book"]
    async fn official_owned_cube_records_keep_owner_value_beaver_and_mover() {
        let path = default_path();
        let board = "-b----E-C---dEa--c-da---B-";
        for (owner, index, native) in [
            ("w", 43217, -0.18780642747879028),
            ("b", 43219, 0.2108868956565857),
        ] {
            let p = setup_position(board, 2, owner, "W");
            let result = lookup(&path, &p, Some(true)).await.unwrap().unwrap();
            let entry = result
                .evaluations
                .iter()
                .find(|entry| entry.record_index == index)
                .unwrap();
            assert_eq!(entry.native_equity, native);
            assert_eq!(entry.equity, -native);
            assert_eq!(entry.source_author, "Dmitriy Obukhov");
            assert!(
                lookup(&path, &p, Some(false)).await.unwrap().is_none(),
                "Beaver rules must not be masked"
            );
            let mirrored: String = board
                .chars()
                .rev()
                .map(|c| {
                    if c.is_ascii_uppercase() {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect();
            let mirror = setup_position(&mirrored, 2, if owner == "w" { "b" } else { "w" }, "B");
            let mirrored = lookup(&path, &mirror, Some(true)).await.unwrap().unwrap();
            assert_eq!(mirrored.position_key, result.position_key);
            assert!(
                mirrored
                    .evaluations
                    .iter()
                    .any(|entry| entry.record_index == index && entry.equity == -native)
            );
        }
        let raised = setup_position(board, 4, "w", "W");
        assert!(
            lookup(&path, &raised, Some(true)).await.unwrap().is_none(),
            "Cube value must not be collapsed"
        );
    }
}
