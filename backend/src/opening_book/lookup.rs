use super::*;
use crate::import::{BeaverRule, MatchFormat};

/// None means explicitly browse this book's own engine answers. A cache in front
/// of native analysis MUST pass the installed engine version; mismatches miss.
/// All SQLite, file IO and decompression run off Tokio's executor threads.
pub(super) async fn lookup_bgdb(
    config: &BookConfig,
    canonical: CanonicalKey,
    expected_engine_version: Option<&str>,
) -> Result<BookLookup> {
    if config.provider == "off" {
        return Ok(BookLookup::miss("Opening book is off"));
    }
    if !matches!(config.provider.as_str(), "automatic" | "backgammondb") {
        return Ok(BookLookup::miss("Unsupported opening book provider"));
    }
    let checker_key = canonical.checker_xgid();
    let cube_key = canonical.cube_xgid();
    let config = config.clone();
    let expected = expected_engine_version.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        let mut result = BookLookup::miss("No entry for this position, roll and match context");
        result.checker_xgid = Some(checker_key.clone());
        result.cube_xgid = cube_key.clone();
        let work = || -> Result<(Option<BookChecker>, Option<BookCube>, BookProvenance)> {
            let book = Book::open(
                if config.path.is_empty() {
                    default_path()
                } else {
                    config.path
                }
                .as_str(),
            )?;
            if let Some(version) = expected {
                let version = version
                    .strip_prefix("GNU Backgammon ")
                    .unwrap_or(&version)
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                ensure!(
                    book.status.engine_version.as_deref() == Some(version),
                    "Opening book engine version mismatch (book {}, requested {version})",
                    book.status.engine_version.as_deref().unwrap_or("unknown")
                );
            }
            let checker = book
                .get(&checker_key)?
                .map(|payload| parse_checker(payload, &canonical, book.top_ply))
                .transpose()?;
            let cube = cube_key
                .as_deref()
                .map(|key| book.get(key))
                .transpose()?
                .flatten()
                .map(|payload| parse_cube(payload, book.cube_ply))
                .transpose()?;
            let context = context_name(&canonical);
            let depth = book
                .status
                .contexts
                .iter()
                .find(|item| item.name == context)
                .and_then(|item| item.depth);
            let provenance = BookProvenance {
                provider: "backgammondb".into(),
                engine: format!(
                    "GNU Backgammon {}",
                    book.status.engine_version.as_deref().unwrap_or("unknown")
                ),
                evaluation_kind: "evaluation".into(),
                equity_kind: "normalized_cubeful".into(),
                context,
                context_depth: depth,
                source_url: book.status.source_url,
                license: book.status.license,
                attribution: book.status.attribution,
            };
            Ok((checker, cube, provenance))
        };
        match work() {
            Ok((checker, cube, provenance)) => {
                result.hit = checker.is_some() || cube.is_some();
                result.checker = checker;
                result.cube = cube;
                if result.hit {
                    result.reason = None;
                    result.provenance = Some(provenance);
                }
            }
            Err(error) => result.reason = Some(format!("Opening book unavailable: {error:#}")),
        }
        result
    })
    .await
    .context("Opening book reader task failed")
}

/// Canonical NeonDojo positions use the standard no-beaver contract. REST
/// callers resolve the immutable source's rule and use lookup_with_rules.
pub async fn lookup(
    config: &BookConfig,
    position: &Position,
    expected_engine_version: Option<&str>,
) -> Result<BookLookup> {
    lookup_with_rules(config, position, expected_engine_version, Some(false)).await
}

pub async fn lookup_with_rules(
    config: &BookConfig,
    position: &Position,
    expected_engine_version: Option<&str>,
    beaver: Option<bool>,
) -> Result<BookLookup> {
    LookupCache::default()
        .lookup(config, position, expected_engine_version, beaver)
        .await
}

#[derive(Default)]
pub(crate) struct LookupCache {
    pub(crate) xg: xg_book::Cache,
    rules: std::sync::Mutex<std::collections::HashMap<(Uuid, String), Option<bool>>>,
}

impl LookupCache {
    pub(super) async fn lookup(
        &self,
        config: &BookConfig,
        position: &Position,
        expected_engine_version: Option<&str>,
        beaver: Option<bool>,
    ) -> Result<BookLookup> {
        let finish = |mut result: BookLookup| {
            result.provider = config.provider.clone();
            result
        };
        if config.provider == "off" {
            return Ok(finish(BookLookup::miss("Opening book is off")));
        }
        if !matches!(
            config.provider.as_str(),
            "automatic" | "backgammondb" | "xg"
        ) {
            return Ok(finish(BookLookup::miss(
                "Unsupported opening book provider",
            )));
        }
        let canonical = match CanonicalKey::from_position(position) {
            Ok(key) => key,
            Err(error) => {
                return Ok(finish(BookLookup::miss(format!(
                    "Unsupported position context: {error:#}"
                ))));
            }
        };
        let money = canonical.is_money();
        if money && beaver.is_none() {
            return Ok(finish(BookLookup::miss(
                "Money-game beaver rule is not recorded; no book context assumed",
            )));
        }
        let mut result = if matches!(config.provider.as_str(), "automatic" | "backgammondb")
            && (!money || beaver == Some(false))
        {
            lookup_bgdb(config, canonical, expected_engine_version).await?
        } else {
            BookLookup::miss(if config.provider == "xg" {
                "No XG position reference for this board and rule context"
            } else {
                "BackgammonDB has no beaver-rule entries"
            })
        };
        // Native jobs must never be fulfilled with a different engine's reference.
        if matches!(config.provider.as_str(), "automatic" | "xg")
            && expected_engine_version.is_none()
        {
            match self
                .xg
                .lookup_canonical(&config.xg_path, canonical, beaver)
                .await
            {
                Ok(Some(reference)) => {
                    result.hit = true;
                    result.reason = None;
                    result.xg_position = Some(reference);
                }
                Ok(None) => {}
                Err(error) if config.provider == "xg" => {
                    result.reason = Some(format!("XG opening book unavailable: {error:#}"))
                }
                Err(error) => {
                    tracing::debug!(error=?error,"Optional XG position reference unavailable")
                }
            }
        }
        Ok(finish(result))
    }

    /// Beaver rules are not encoded by a GNUbg Match ID. Resolve them once from
    /// immutable source metadata rather than treating different money rules alike.
    pub(super) async fn source_beaver_rule(
        &self,
        pool: &sqlx::PgPool,
        position: &Position,
    ) -> Result<Option<bool>> {
        use crate::backgammon::ids::{Field, MatchId};
        let length = MatchId::decode(&position.gnubg_match_id)
            .context("Invalid GNUbg Match ID")?
            .get(Field::LENGTH);
        if length > 0 {
            return Ok(Some(false));
        }
        let row: Option<(MatchFormat, String, i32)> =
            sqlx::query_as("SELECT format,sha256,octet_length(original) FROM matches WHERE id=$1")
                .bind(position.match_uuid)
                .fetch_optional(pool)
                .await?;
        let Some((format, sha, length)) = row else {
            return Ok(None);
        };
        match format.beaver_rule() {
            BeaverRule::Off => Ok(Some(false)),
            BeaverRule::InOriginal(read) => {
                let key = (position.match_uuid, sha);
                if let Some(rule) = self
                    .rules
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Book rule cache unavailable"))?
                    .get(&key)
                    .copied()
                {
                    return Ok(rule);
                }
                // Bound the legacy source read separately from the 64 MiB XG
                // decompression limit. Larger sources miss without an assumption.
                if length > 16 * 1024 * 1024 {
                    return Ok(None);
                }
                let original: Vec<u8> =
                    sqlx::query_scalar("SELECT original FROM matches WHERE id=$1")
                        .bind(position.match_uuid)
                        .fetch_one(pool)
                        .await?;
                let rule = tokio::task::spawn_blocking(move || read(&original))
                    .await?
                    .ok();
                let mut cache = self
                    .rules
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Book rule cache unavailable"))?;
                if cache.len() >= 128 {
                    cache.clear();
                }
                cache.insert(key, rule);
                Ok(rule)
            }
            BeaverRule::Unknown => Ok(None),
        }
    }
}
