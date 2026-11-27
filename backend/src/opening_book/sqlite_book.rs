use super::*;

pub(super) struct Book {
    pub(super) connection: rusqlite::Connection,
    pub(super) status: BookStatus,
    pub(super) dictionary: Option<Vec<u8>>,
    pub(super) top_ply: Option<i32>,
    pub(super) cube_ply: Option<i32>,
}

impl Book {
    pub(super) fn open(path: &str) -> Result<Self> {
        use rusqlite::types::ValueRef;
        let connection = crate::untrusted_sqlite::open(path)
            .with_context(|| format!("Cannot open opening book at {path}"))?;
        connection.busy_timeout(std::time::Duration::from_millis(250))?;
        for table in ["meta", "contexts", "entries"] {
            ensure!(
                crate::untrusted_sqlite::is_regular_table(&connection, table)?,
                "Opening book table {table} is missing or not a regular table"
            );
        }
        let mut metadata = std::collections::HashMap::new();
        let mut dictionary = None;
        {
            let mut statement = connection.prepare("SELECT key,value FROM meta")?;
            let mut rows = statement.query([])?;
            while let Some(row) = rows.next()? {
                let key: String = row.get(0)?;
                let value = row.get_ref(1)?;
                if key == "zdict" {
                    if let ValueRef::Blob(bytes) = value {
                        dictionary = Some(bytes.to_vec());
                    }
                } else {
                    let value = match value {
                        ValueRef::Text(bytes) => String::from_utf8(bytes.to_vec())?,
                        ValueRef::Integer(number) => number.to_string(),
                        _ => continue,
                    };
                    metadata.insert(key, value);
                }
            }
        }
        ensure!(
            metadata.get("formatVersion").map(String::as_str) == Some("1"),
            "Unsupported opening book formatVersion"
        );
        let version = metadata
            .get("engineVersion")
            .filter(|value| !value.is_empty())
            .context("Opening book lacks engineVersion")?
            .clone();
        let compression = metadata
            .get("compression")
            .context("Opening book lacks compression")?
            .clone();
        ensure!(
            matches!(compression.as_str(), "none" | "zlib"),
            "Unsupported opening book compression"
        );
        if compression == "zlib" {
            ensure!(
                dictionary
                    .as_ref()
                    .is_some_and(|bytes| !bytes.is_empty() && bytes.len() <= 32768),
                "Opening book lacks a valid zlib dictionary"
            );
        }
        let contexts: Vec<BookContext> = connection
            .prepare("SELECT name,entries,depth,complete FROM contexts ORDER BY name")?
            .query_map([], |row| {
                Ok(BookContext {
                    name: row.get(0)?,
                    entries: row.get(1)?,
                    depth: row.get(2)?,
                    complete: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        ensure!(
            contexts.iter().all(|context| context.entries >= 0),
            "Invalid book context row counts"
        );
        // Prepare an indexed lookup to validate the table without scanning millions of entries.
        connection.prepare("SELECT payload FROM entries WHERE xgid=?1")?;
        let plies = |key: &str| -> Result<Option<i32>> {
            metadata
                .get(key)
                .map(|value| {
                    let value = value.parse::<i32>()?;
                    ensure!((0..=7).contains(&value), "Invalid evaluation ply metadata");
                    Ok(value)
                })
                .transpose()
        };
        // BackgammonDB records GNUbg's native levels; NeonDojo shows its own
        // convention, so GNUbg's regular 3-ply book reads as 4-ply.
        let top_ply = plies("topPly")?.map(crate::engine::plies_from_gnubg);
        let cube_ply = plies("cubePly")?.map(crate::engine::plies_from_gnubg);
        let status = BookStatus {
            available: true,
            path: path.to_owned(),
            engine_version: Some(version.clone()),
            entries: contexts.iter().map(|context| context.entries).sum(),
            contexts,
            compression: Some(compression),
            license: metadata.get("license").cloned(),
            source_url: metadata.get("source").cloned(),
            attribution: Some(format!(
                "BackgammonDB opening book, generated with GNU Backgammon {version}"
            )),
            error: None,
        };
        Ok(Self {
            connection,
            status,
            dictionary,
            top_ply,
            cube_ply,
        })
    }
    pub(super) fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        use rusqlite::{OptionalExtension, types::ValueRef};
        let payload = self
            .connection
            .query_row(
                "SELECT payload FROM entries WHERE xgid=?1",
                [key],
                |row| match row.get_ref(0)? {
                    ValueRef::Text(value) | ValueRef::Blob(value) => Ok(value.to_vec()),
                    _ => Err(rusqlite::Error::InvalidColumnType(
                        0,
                        "payload".into(),
                        row.get_ref(0)?.data_type(),
                    )),
                },
            )
            .optional()?;
        payload
            .map(|payload| {
                ensure!(
                    payload.len() <= MAX_PAYLOAD,
                    "Opening book payload is too large"
                );
                let bytes = if self.status.compression.as_deref() == Some("zlib") {
                    use flate2::{Decompress, FlushDecompress, Status};
                    let mut decoder = Decompress::new(true);
                    let mut output = vec![0; MAX_PAYLOAD];
                    let first = decoder.decompress(&payload, &mut output, FlushDecompress::Finish);
                    let state = match first {
                        Err(error) if error.needs_dictionary().is_some() => {
                            decoder.set_dictionary(
                                self.dictionary
                                    .as_deref()
                                    .context("Missing zlib dictionary")?,
                            )?;
                            decoder.decompress(
                                &payload[decoder.total_in() as usize..],
                                &mut output,
                                FlushDecompress::Finish,
                            )?
                        }
                        other => other?,
                    };
                    ensure!(
                        state == Status::StreamEnd && decoder.total_in() as usize == payload.len(),
                        "Invalid or oversized opening book payload"
                    );
                    output.truncate(decoder.total_out() as usize);
                    output
                } else {
                    payload
                };
                Ok(serde_json::from_slice(&bytes)?)
            })
            .transpose()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Probabilities {
    pub(super) win: f64,
    pub(super) win_gammon: f64,
    pub(super) win_backgammon: f64,
    pub(super) lose_gammon: f64,
    pub(super) lose_backgammon: f64,
}

impl Probabilities {
    pub(super) fn vector(&self) -> Result<Vec<f64>> {
        let values = vec![
            self.win,
            self.win_gammon,
            self.win_backgammon,
            1.0 - self.win,
            self.lose_gammon,
            self.lose_backgammon,
        ];
        ensure!(
            values
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                && self.win_backgammon <= self.win_gammon
                && self.win_gammon <= self.win
                && self.lose_backgammon <= self.lose_gammon
                && self.lose_gammon <= 1.0 - self.win + 0.000001,
            "Invalid book probabilities"
        );
        Ok(values)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Play {
    r#move: Vec<[usize; 2]>,
    pub(super) probabilities: Probabilities,
    pub(super) cubeful_equity: f64,
}

pub(super) fn parse_checker(
    payload: serde_json::Value,
    key: &CanonicalKey,
    plies: Option<i32>,
) -> Result<BookChecker> {
    #[derive(Deserialize)]
    struct Entry {
        plays: Vec<Play>,
    }
    let entry: Entry = serde_json::from_value(payload)?;
    ensure!(
        !entry.plays.is_empty() && entry.plays.len() <= 8,
        "Invalid book candidate count"
    );
    let mut candidates = Vec::new();
    for (index, play) in entry.plays.into_iter().enumerate() {
        ensure!(play.cubeful_equity.is_finite(), "Invalid book equity");
        let mut board = key.board;
        let mut notation = Vec::new();
        ensure!(
            !play.r#move.is_empty() && play.r#move.len() <= 4,
            "Invalid book move"
        );
        for [from, to] in play.r#move {
            ensure!(
                (1..=25).contains(&from) && to < from && board[from] > 0,
                "Invalid book move step"
            );
            let hit = to > 0 && board[to] == -1;
            board[from] -= 1;
            if to > 0 {
                ensure!(board[to] >= -1, "Book move is blocked");
                board[to] = board[to].max(0) + 1;
            }
            notation.push(crate::import::sgf::coord::format_step(from, to, hit));
        }
        candidates.push(Candidate {
            rank: index as i32 + 1,
            notation: notation.join(" "),
            path: None,
            equity: play.cubeful_equity,
            probabilities: play.probabilities.vector()?,
            plies,
        });
    }
    Ok(BookChecker { candidates, plies })
}

pub(super) fn parse_cube(payload: serde_json::Value, plies: Option<i32>) -> Result<BookCube> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Equities {
        no_double: f64,
        double_take: f64,
        double_pass: f64,
    }
    #[derive(Deserialize)]
    struct Entry {
        probabilities: Probabilities,
        equities: Equities,
        verdict: String,
    }
    let entry: Entry = serde_json::from_value(payload)?;
    ensure!(
        matches!(
            entry.verdict.as_str(),
            "no-double" | "double-take" | "double-pass" | "too-good"
        ),
        "Invalid book cube verdict"
    );
    ensure!(
        [
            entry.equities.no_double,
            entry.equities.double_take,
            entry.equities.double_pass
        ]
        .iter()
        .all(|value| value.is_finite()),
        "Invalid book cube equity"
    );
    Ok(BookCube {
        probabilities: entry.probabilities.vector()?,
        equities: BookCubeEquities {
            no_double: entry.equities.no_double,
            double_take: entry.equities.double_take,
            double_pass: entry.equities.double_pass,
        },
        verdict: entry.verdict,
        plies,
    })
}

pub(super) fn context_name(key: &CanonicalKey) -> String {
    if key.is_money() {
        return if key.is_jacoby() { "money" } else { "money-nj" }.into();
    }
    let [bottom, top] = key.away();
    format!(
        "{}a{}a{}",
        bottom.max(top),
        bottom.min(top),
        if key.is_crawford() { "c" } else { "" }
    )
}
