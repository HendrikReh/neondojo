//! GNUbg-backed Jellyfish MAT import and resignation dialect normalization.

use super::{event_date, normalize_native_sgf, recover};
use crate::{async_temp_dir::AsyncTempDir, engine::Engine, import::ConvertedMatch};
use anyhow::{Context, Result, bail, ensure};
use regex::Regex;
use std::sync::OnceLock;

pub async fn normalize_mat(engine: &Engine, bytes: &[u8]) -> Result<String> {
    let temp = AsyncTempDir::new().await?;
    tokio::fs::write(
        temp.path().join("input.mat"),
        normalize_mat_resignations(bytes)?,
    )
    .await?;
    let output = engine
        .run(
            temp.path(),
            "set confirm default yes\nset confirm new no\nset display off\nimport mat input.mat\nsave match canonical.sgf\n",
        )
        .await?;
    // GNUbg intentionally saves a valid prefix even after some MAT parse
    // failures. Exit status and artifact existence alone are insufficient.
    for line in output.lines() {
        let lower = line.trim().to_ascii_lowercase();
        if lower.contains("illegal")
            || lower.starts_with("unrecognised move")
            || lower.starts_with("warning")
            || lower.contains("match had errors")
            || lower.contains("not a valid .mat file")
            || lower.starts_with("invalid match length")
            || lower.starts_with("error reading")
        {
            bail!("GNUbg rejected the MAT file: {line}");
        }
    }
    let native = tokio::fs::read_to_string(temp.path().join("canonical.sgf"))
        .await
        .context("GNUbg could not import this MAT file")?;
    normalize_native_sgf(&native, event_date(bytes))
}

/// Import a MAT transcript, ending each game before a move that fails
/// [`normalize_mat`]. The warnings name every such game and each
/// header score that contradicts the previous result.
pub(crate) async fn import_mat(engine: &Engine, bytes: &[u8]) -> Result<ConvertedMatch> {
    let recovered = recover(
        bytes,
        |text| async move { normalize_mat(engine, &text).await },
    )
    .await?;
    Ok(ConvertedMatch {
        sgf: recovered.sgf,
        warnings: recovered.warnings,
    })
}

fn normalize_mat_resignations(bytes: &[u8]) -> Result<Vec<u8>> {
    const MARKER: &[u8] = b"wants to resign";
    static RESULT: OnceLock<Regex> = OnceLock::new();
    let result = RESULT.get_or_init(|| {
        Regex::new(r"(?i)^Wins +[1-9][0-9]* +points?(?: +and +the +match)?$").unwrap()
    });
    let is_result = |value: &[u8]| {
        std::str::from_utf8(value.trim_ascii()).is_ok_and(|value| result.is_match(value))
    };
    let lines = bytes.split_inclusive(|b| *b == b'\n').collect::<Vec<_>>();
    let mut output = Vec::with_capacity(bytes.len());
    for (line_index, line) in lines.iter().enumerate() {
        let mut normalized = line.to_vec();
        // Restrict this dialect alias to numbered move rows, preserving all
        // byte offsets used by GNUbg's two-column MAT reader.
        if let Some(end_number) = line.iter().position(|b| *b == b')') {
            let number = line[..end_number].trim_ascii();
            if !number.is_empty() && number.iter().all(u8::is_ascii_digit) {
                for start in end_number + 1..line.len().saturating_sub(MARKER.len()) + 1 {
                    if !line[start..].starts_with(MARKER) {
                        continue;
                    }
                    let left = line[end_number + 1..start]
                        .iter()
                        .all(u8::is_ascii_whitespace);
                    let right =
                        start >= 16 && line[start - 2..start].iter().all(u8::is_ascii_whitespace);
                    let suffix = &line[start + MARKER.len()..];
                    let result_column = suffix
                        .iter()
                        .take_while(|b| b.is_ascii_whitespace())
                        .count();
                    let resolved = if left && result_column >= 2 && is_result(suffix) {
                        true
                    } else if suffix.trim_ascii().is_empty() {
                        lines[line_index + 1..]
                            .iter()
                            .find(|next| !next.trim_ascii().is_empty())
                            .is_some_and(|next| {
                                let column =
                                    next.iter().position(|b| !b.is_ascii_whitespace()).unwrap();
                                is_result(next)
                                    && ((left && column >= 16) || (right && column < 16))
                            })
                    } else {
                        false
                    };
                    ensure!(
                        (left || right) && resolved,
                        "MAT: resignation marker requires an opposite-player result"
                    );
                    // GNUbg ignores Resigns and derives the exact award from
                    // the following Wins record, including gammons/backgammons.
                    normalized[start..start + MARKER.len()].copy_from_slice(b"Resigns        ");
                }
            }
        }
        output.extend_from_slice(&normalized);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn native_sgf_engine(
        sgf: &str,
        reject_resignation_marker: bool,
    ) -> (tempfile::TempDir, Engine) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("native-sgf-engine");
        let check = if reject_resignation_marker {
            "case \"$(cat input.mat)\" in *'wants to resign'*) echo 'Unrecognised move resignation in .mat file.';; esac\n"
        } else {
            ""
        };
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\n{check}cat >canonical.sgf <<'CANONICAL_SGF'\n{sgf}\nCANONICAL_SGF\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = Engine::new(binary.to_str().unwrap().to_owned());
        (directory, engine)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_native_joined_overshoot_bearoffs_become_strict_sgf_steps() {
        for (native, expected) in [
            ("(;GM[6]AE[a:y]AW[f][d][c]AB[x];W[52fz])", "6/4 4/off"),
            ("(;GM[6]AE[a:y]AW[d][c][c]AB[a][b];W[51dz])", "4/3 3/off"),
            ("(;GM[6]AE[a:y]AB[s][u][v]AW[a];B[52sz])", "6/4 4/off"),
        ] {
            assert!(crate::import::parse_sgf(native).is_err());
            let (_directory, engine) = native_sgf_engine(native, false);
            let normalized = normalize_mat(&engine, b"native import boundary fixture")
                .await
                .unwrap();
            let imported = crate::import::parse_sgf(&normalized).unwrap();
            assert_eq!(imported.positions.len(), 1);
            assert_eq!(imported.positions[0].notation, expected);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_numbered_resignation_cells_are_normalized_before_gnubg() {
        let (_directory, engine) = native_sgf_engine("(;GM[6]RE[W+1R];W[31hefe])", true);
        let input = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) wants to resign                Wins 1 point\n";
        assert!(normalize_mat(&engine, input).await.is_ok());
    }

    #[test]
    fn mat_resignation_alias_preserves_columns_and_requires_a_result() {
        let input = b" Player wants to resign : 0\r\n 1) 31: 8/5 6/5                 wants to resign\r\n      Wins 2 points\r\n 2) wants to resign              Wins 4 points and the match\r\n";
        let normalized = normalize_mat_resignations(input).unwrap();
        assert_eq!(normalized.len(), input.len());
        assert!(normalized.starts_with(b" Player wants to resign : 0\r\n"));
        assert_eq!(
            normalized
                .windows(b"Resigns        ".len())
                .filter(|w| *w == b"Resigns        ")
                .count(),
            2
        );
        for input in [
            b" 1) wants to resign\n".as_slice(),
            b" 1) wants to resign              unexpected text\n",
            b" 1) wants to resignWins 1 point\n",
            b" 1) 31: 8/5 6/5                 wants to resign\n                                  Wins 2 points\n",
            b" 1) wants to resign\n      Wins 2 points\n",
        ] {
            assert!(normalize_mat_resignations(input).is_err());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mat_native_normalization_rejects_incomplete_plays() {
        for native in [
            "(;GM[6];W[31he])",
            "(;GM[6]AE[a:y]AW[d][b]AB[c][c];W[51dz])",
            "(;GM[6];W[31hefe];W[42hdfd])",
        ] {
            let (_directory, engine) = native_sgf_engine(native, false);
            assert!(
                normalize_mat(&engine, b"native import boundary fixture")
                    .await
                    .is_err()
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires local GNUbg"]
    async fn mat_with_valid_prefix_and_corrupt_tail_is_rejected() {
        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
        let input = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\nThis is invalid trailing text\n";
        assert!(
            normalize_mat(&engine, input).await.is_err(),
            "must not import a silently truncated match"
        );
        let invalid_move = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/22 22/13\n";
        assert!(
            normalize_mat(&engine, invalid_move).await.is_err(),
            "must reject a semantically invalid final move"
        );
    }
}
