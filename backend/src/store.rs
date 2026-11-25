use crate::{import::ImportedMatch, model::*, search::literal_pattern};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

// Keep large source artifacts out of every summary/list response at the SQL boundary.
const MATCH_SUMMARY: &str = "SELECT id,NOT collection_only AS source_available,filename,format,white,black,match_length,position_count,created_at FROM matches";

pub async fn import_match(
    pool: &PgPool,
    filename: &str,
    format: &str,
    bytes: &[u8],
    sgf: &str,
    imported: &ImportedMatch,
) -> Result<ImportResponse> {
    import_match_in_database(
        pool,
        filename,
        format,
        bytes,
        sgf,
        imported,
        crate::databases::default_database_id(pool).await?,
    )
    .await
}

pub async fn import_match_in_database(
    pool: &PgPool,
    filename: &str,
    format: &str,
    bytes: &[u8],
    sgf: &str,
    imported: &ImportedMatch,
    database_id: Uuid,
) -> Result<ImportResponse> {
    import_source_in_database(
        pool,
        filename,
        format,
        bytes,
        sgf,
        imported,
        Vec::new(),
        database_id,
    )
    .await
}

/// Import a source file together with the irregularities its conversion
/// resolved. They are saved with the match and returned in the response.
#[allow(clippy::too_many_arguments)]
pub async fn import_source_in_database(
    pool: &PgPool,
    filename: &str,
    format: &str,
    bytes: &[u8],
    sgf: &str,
    imported: &ImportedMatch,
    warnings: Vec<ImportWarning>,
    database_id: Uuid,
) -> Result<ImportResponse> {
    let prepared = MatchImport::prepare(filename, format, bytes, sgf, imported)
        .await?
        .with_warnings(warnings);
    let mut tx = pool.begin().await?;
    let result = insert_match_import(&mut tx, database_id, prepared).await?;
    tx.commit().await?;
    Ok(result)
}

/// Library entry archived from exactly these source bytes, preferring `preferred` when it was copied.
pub(crate) async fn library_match_by_source(
    pool: &PgPool,
    bytes: &[u8],
    preferred: Uuid,
) -> Result<Option<MatchSummary>> {
    let hash = format!("{:x}", Sha256::digest(bytes));
    // One indexed (database_id, sha256) probe per library database also finds moved entries.
    Ok(sqlx::query_as(
        "SELECT m.id,true AS source_available,m.filename,m.format,m.white,m.black,m.match_length,m.position_count,m.created_at
         FROM databases d
         CROSS JOIN LATERAL (SELECT id,filename,format,white,black,match_length,position_count,created_at,collection_only
               FROM matches WHERE database_id=d.id AND sha256=$1) m
         WHERE NOT d.collection_only AND NOT m.collection_only AND d.id<>$3
         ORDER BY d.id=$2 DESC,m.created_at,m.id LIMIT 1",
    )
    .bind(hash)
    .bind(preferred)
    .bind(crate::play::PAUSED_MATCHES_DATABASE_ID)
    .fetch_optional(pool)
    .await?)
}

pub(crate) struct MatchImport<'a> {
    filename: &'a str,
    format: &'a str,
    bytes: &'a [u8],
    sgf: &'a str,
    imported: &'a ImportedMatch,
    hash: String,
    metadata: crate::databases::SourceMetadata,
    source_analysis: Vec<crate::imported_analysis::ImportedAnalysisData>,
    warnings: Vec<ImportWarning>,
}

impl<'a> MatchImport<'a> {
    pub(crate) async fn prepare(
        filename: &'a str,
        format: &'a str,
        bytes: &'a [u8],
        sgf: &'a str,
        imported: &'a ImportedMatch,
    ) -> Result<Self> {
        ensure!(
            !imported.positions.is_empty(),
            "The file contains no checker moves"
        );
        ensure!(
            imported.positions.len() <= 100_000,
            "Too many positions in one file"
        );
        let hash = format!("{:x}", Sha256::digest(bytes));
        let metadata = crate::databases::source_metadata(sgf);
        let analysis_source = sgf.to_owned();
        let source_analysis = tokio::task::spawn_blocking(move || {
            crate::imported_analysis::extract(&analysis_source)
        })
        .await??;
        Ok(Self {
            filename,
            format,
            bytes,
            sgf,
            imported,
            hash,
            metadata,
            source_analysis,
            warnings: Vec::new(),
        })
    }

    pub(crate) fn with_warnings(mut self, warnings: Vec<ImportWarning>) -> Self {
        self.warnings = warnings;
        self
    }
}

pub(crate) async fn insert_match_import(
    tx: &mut Transaction<'_, Postgres>,
    database_id: Uuid,
    prepared: MatchImport<'_>,
) -> Result<ImportResponse> {
    let MatchImport {
        filename,
        format,
        bytes,
        sgf,
        imported,
        hash,
        metadata,
        source_analysis,
        warnings,
    } = prepared;
    let id = Uuid::new_v4();
    // Match moves and database deletion lock the owner before touching matches.
    let available: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM databases WHERE id=$1 AND NOT collection_only AND id<>$2 FOR KEY SHARE",
    )
    .bind(database_id)
    .bind(crate::play::PAUSED_MATCHES_DATABASE_ID)
    .fetch_optional(&mut **tx)
    .await?;
    ensure!(available.is_some(), "Database not found");
    let inserted: Option<Uuid> = sqlx::query_scalar("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,database_id,played_on,tournament,round,place,result_state,white_score,black_score,source_metadata_loaded,import_warnings) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,true,$19) ON CONFLICT(database_id,sha256) DO NOTHING RETURNING id")
        .bind(id).bind(filename).bind(format).bind(&hash).bind(&imported.players[0]).bind(&imported.players[1])
        .bind(imported.match_length).bind(imported.positions.len() as i32).bind(bytes).bind(sgf).bind(database_id).bind(metadata.played_on).bind(metadata.tournament).bind(metadata.round).bind(metadata.place).bind(metadata.result_state).bind(metadata.white_score).bind(metadata.black_score).bind(sqlx::types::Json(&warnings))
        .fetch_optional(&mut **tx).await?;
    if inserted.is_none() {
        let existing = sqlx::query_as::<_, MatchSummary>(&format!(
            "{MATCH_SUMMARY} WHERE sha256=$1 AND database_id=$2"
        ))
        .bind(hash)
        .bind(database_id)
        .fetch_one(&mut **tx)
        .await?;
        return Ok(ImportResponse {
            r#match: existing,
            duplicate: true,
            warnings: Vec::new(),
        });
    }
    for (batch, positions) in imported.positions.chunks(500).enumerate() {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) ",
        );
        query.push_values(positions.iter().enumerate(), |mut q, (index, p)| {
            q.push_bind(Uuid::new_v4())
                .push_bind(id)
                .push_bind((batch * 500 + index) as i32)
                .push_bind(p.game_no)
                .push_bind(p.ply)
                .push_bind(p.player)
                .push_bind(p.dice.to_vec())
                .push_bind(&p.board)
                .push_bind(&p.position_id)
                .push_bind(&p.match_id)
                .push_bind(&p.notation);
        });
        query.build().execute(&mut **tx).await?;
    }
    insert_games(tx, id, &imported.games).await?;
    crate::imported_analysis::insert_records(tx, id, &source_analysis).await?;
    let summary = sqlx::query_as::<_, MatchSummary>(&format!("{MATCH_SUMMARY} WHERE id=$1"))
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(ImportResponse {
        r#match: summary,
        duplicate: false,
        warnings,
    })
}

async fn insert_games(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    match_id: Uuid,
    games: &[MatchGame],
) -> Result<()> {
    for batch in games.chunks(500) {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford,cube_enabled) ",
        );
        query.push_values(batch, |mut q, game| {
            q.push_bind(match_id)
                .push_bind(game.game_no)
                .push_bind(game.position_count)
                .push_bind(game.white_score)
                .push_bind(game.black_score)
                .push_bind(game.crawford)
                .push_bind(game.cube_enabled);
        });
        query.build().execute(&mut **tx).await?;
    }
    Ok(())
}

/// Imported sources are immutable. Read the catalog directly, or populate it once
/// under the match row lock for libraries created before the catalog migration.
/// The game catalog of a match, built once from the stored SGF for older
/// imports. Results are not derived; scope checks need only the games.
const GAME_CATALOG: &str = "SELECT game_no,position_count,white_score,black_score,crawford,cube_enabled FROM match_games WHERE match_uuid=$1 ORDER BY game_no";
pub async fn game_catalog(pool: &PgPool, match_id: Uuid) -> Result<Option<Vec<MatchGame>>> {
    let games = sqlx::query_as::<_, MatchGame>(GAME_CATALOG)
        .bind(match_id)
        .fetch_all(pool)
        .await?;
    if !games.is_empty() {
        return Ok(Some(games));
    }
    let mut tx = pool.begin().await?;
    let source: Option<String> =
        sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 FOR UPDATE")
            .bind(match_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(source) = source else {
        return Ok(None);
    };
    // A concurrent request may have populated the catalog while this one waited.
    let games = sqlx::query_as::<_, MatchGame>(GAME_CATALOG)
        .bind(match_id)
        .fetch_all(&mut *tx)
        .await?;
    if !games.is_empty() {
        tx.commit().await?;
        return Ok(Some(games));
    }
    let games = catalog_source_games(&mut tx, match_id, source).await?;
    tx.commit().await?;
    Ok(Some(games))
}

pub(crate) async fn catalog_source_games(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    match_id: Uuid,
    source: String,
) -> Result<Vec<MatchGame>> {
    let games =
        tokio::task::spawn_blocking(move || crate::import::parse_sgf(&source).map(|m| m.games))
            .await??;
    insert_games(tx, match_id, &games).await?;
    Ok(games)
}

pub async fn import_warnings(pool: &PgPool, match_id: Uuid) -> Result<Option<Vec<ImportWarning>>> {
    let warnings: Option<sqlx::types::Json<Vec<ImportWarning>>> =
        sqlx::query_scalar("SELECT import_warnings FROM matches WHERE id=$1")
            .bind(match_id)
            .fetch_optional(pool)
            .await?;
    Ok(warnings.map(|sqlx::types::Json(warnings)| warnings))
}

pub async fn match_games(pool: &PgPool, match_id: Uuid) -> Result<Option<Vec<MatchGame>>> {
    match game_catalog(pool, match_id).await? {
        Some(games) => Ok(Some(enrich_game_results(pool, match_id, games).await?)),
        None => Ok(None),
    }
}

/// The game catalog with results inside the caller's snapshot. Unlike
/// [`match_games`] it never builds a missing catalog; call [`game_catalog`]
/// before opening a read-only snapshot.
pub(crate) async fn match_games_on(
    connection: &mut sqlx::PgConnection,
    match_id: Uuid,
) -> Result<Vec<MatchGame>> {
    let games = sqlx::query_as::<_, MatchGame>(GAME_CATALOG)
        .bind(match_id)
        .fetch_all(&mut *connection)
        .await?;
    enrich_game_results(&mut *connection, match_id, games).await
}

// Computed metadata deliberately stays out of match_games and retained source-less
// collection context. A damaged/unavailable source must not hide the catalog.
async fn enrich_game_results(
    executor: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    mut games: Vec<MatchGame>,
) -> Result<Vec<MatchGame>> {
    let source: Option<String> =
        sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(executor)
            .await?;
    if let Some(source) = source.filter(|source| !source.trim().is_empty()) {
        let parsed = tokio::task::spawn_blocking(move || {
            crate::import::sgf::parse_sgf_with_context(&source)
        })
        .await?;
        if let Ok((imported, _)) = parsed {
            for game in &mut games {
                if let Some(source_game) = imported.games.iter().find(|g| g.game_no == game.game_no)
                    && source_game.white_score == game.white_score
                    && source_game.black_score == game.black_score
                    && source_game.position_count == game.position_count
                {
                    game.result = source_game.result.clone();
                }
            }
        }
    }
    Ok(games)
}

pub async fn get_match(pool: &PgPool, id: Uuid) -> Result<Option<MatchSummary>> {
    Ok(sqlx::query_as(&format!("{MATCH_SUMMARY} WHERE id=$1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn matches(pool: &PgPool, q: &str, after: Option<Uuid>, limit: i64) -> Result<MatchPage> {
    let mut query =
        QueryBuilder::<Postgres>::new(format!("{MATCH_SUMMARY} WHERE NOT collection_only"));
    if !q.is_empty() {
        let pattern = literal_pattern(q);
        // UNION collects indexed hits before fetching summaries; avoid an OR with
        // correlated child-position scans across the entire match library.
        query
            .push(" AND id IN (SELECT id FROM matches WHERE search_text LIKE ")
            .push_bind(pattern.clone())
            .push(" UNION SELECT coalesce(a.match_uuid,p.match_uuid,g.match_uuid,mv.match_uuid) FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid LEFT JOIN match_games g ON g.id=a.game_uuid LEFT JOIN match_moves mv ON mv.id=a.move_uuid WHERE a.search_text LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(after) = after {
        query.push(" AND id > ").push_bind(after);
    }
    query.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let mut items = query
        .build_query_as::<MatchSummary>()
        .fetch_all(pool)
        .await?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|m| m.id)
    } else {
        None
    };
    Ok(MatchPage { items, next_cursor })
}

/// Positions embedded in independent cards can have no local match. For a
/// local match, use the game's source rule (including pre-catalog imports).
pub(crate) async fn game_cube_enabled(pool: &PgPool, position: &Position) -> Result<bool> {
    let rule: Option<Option<bool>> = sqlx::query_scalar(
        "SELECT cube_enabled FROM match_games WHERE match_uuid=$1 AND game_no=$2",
    )
    .bind(position.match_uuid)
    .bind(position.game_no)
    .fetch_optional(pool)
    .await?;
    if let Some(rule) = rule {
        return rule.context("The source game's cube rule is unavailable");
    }
    let Some(games) = game_catalog(pool, position.match_uuid).await? else {
        return Ok(position.cube_enabled.unwrap_or(true));
    };
    games
        .iter()
        .find(|game| game.game_no == position.game_no)
        .and_then(|game| game.cube_enabled)
        .context("The source game's cube rule is unavailable")
}

pub async fn get_position(pool: &PgPool, id: Uuid) -> Result<Option<Position>> {
    Ok(sqlx::query_as("SELECT * FROM positions WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn positions(
    pool: &PgPool,
    match_id: Option<Uuid>,
    position_id: Option<&str>,
    after: Option<Uuid>,
    limit: i64,
) -> Result<PositionPage> {
    positions_scoped(pool, match_id, position_id, after, limit, None).await
}

pub async fn positions_scoped(
    pool: &PgPool,
    match_id: Option<Uuid>,
    position_id: Option<&str>,
    after: Option<Uuid>,
    limit: i64,
    game_no: Option<i32>,
) -> Result<PositionPage> {
    positions_scoped_in_database(pool, match_id, position_id, after, limit, game_no, None).await
}

pub async fn positions_scoped_in_database(
    pool: &PgPool,
    match_id: Option<Uuid>,
    position_id: Option<&str>,
    after: Option<Uuid>,
    limit: i64,
    game_no: Option<i32>,
    database_id: Option<Uuid>,
) -> Result<PositionPage> {
    ensure!(
        game_no.is_none() || match_id.is_some(),
        "game_no requires match_id"
    );
    let mut query = QueryBuilder::<Postgres>::new(concat!(
        "SELECT * FROM positions WHERE NOT ",
        crate::play::review_blocked_sql!("positions.match_uuid")
    ));
    if let Some(database_id) = database_id {
        query
            .push(" AND match_uuid IN (SELECT id FROM matches WHERE database_id=")
            .push_bind(database_id)
            .push(")");
    }

    if let Some(game_no) = game_no {
        query.push(" AND game_no = ").push_bind(game_no);
    }
    if let Some(id) = match_id {
        query.push(" AND match_uuid = ").push_bind(id);
    }
    if let Some(id) = position_id {
        query.push(" AND position_id = ").push_bind(id);
    }
    if let Some(cursor) = after {
        if let Some(match_id) = match_id {
            query
                .push(" AND ordinal > (SELECT ordinal FROM positions WHERE id = ")
                .push_bind(cursor)
                .push(" AND match_uuid = ")
                .push_bind(match_id)
                .push(")");
        } else {
            query.push(" AND id > ").push_bind(cursor);
        }
    }
    query
        .push(if match_id.is_some() {
            " ORDER BY ordinal LIMIT "
        } else {
            " ORDER BY id LIMIT "
        })
        .push_bind(limit + 1);
    let mut items = query.build_query_as::<Position>().fetch_all(pool).await?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|p| p.id)
    } else {
        None
    };
    Ok(PositionPage { items, next_cursor })
}

pub async fn dice_stats(pool: &PgPool, id: Uuid) -> Result<DiceStats> {
    dice_stats_scoped(pool, id, None).await
}

pub async fn dice_stats_scoped(pool: &PgPool, id: Uuid, game_no: Option<i32>) -> Result<DiceStats> {
    let counts: Vec<(i32,i32,i64)> = sqlx::query_as("SELECT LEAST(dice[1],dice[2]) AS a, GREATEST(dice[1],dice[2]) AS b, count(*) FROM positions WHERE match_uuid=$1 AND ($2::integer IS NULL OR game_no=$2) GROUP BY a,b")
        .bind(id).bind(game_no).fetch_all(pool).await?;
    let total = counts.iter().map(|(_, _, n)| n).sum();
    let mut rolls = Vec::new();
    for a in 1..=6 {
        for b in a..=6 {
            rolls.push(DiceCount {
                dice: [a, b],
                count: counts
                    .iter()
                    .find(|(x, y, _)| *x == a && *y == b)
                    .map(|(_, _, n)| *n)
                    .unwrap_or(0),
                expected_probability: if a == b { 1.0 / 36.0 } else { 2.0 / 36.0 },
            });
        }
    }
    Ok(DiceStats { total, rolls })
}

pub async fn get_job(pool: &PgPool, id: Uuid) -> Result<Option<Job>> {
    Ok(sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

#[derive(sqlx::FromRow)]
pub(crate) struct JobReviewStatus {
    #[sqlx(flatten)]
    pub job: Job,
    pub restricted: bool,
}

pub(crate) async fn jobs_with_review_status(
    pool: &PgPool,
    ids: &[Uuid],
) -> Result<Vec<JobReviewStatus>> {
    Ok(sqlx::query_as(concat!(
        "SELECT j.*, ",
        crate::play::review_blocked_sql!("coalesce(j.match_uuid,p.match_uuid)"),
        " AS restricted
         FROM jobs j LEFT JOIN positions p ON p.id=j.position_uuid
         WHERE j.id=ANY($1) ORDER BY j.id"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await?)
}

pub async fn create_job(pool: &PgPool, position_id: Uuid, kind: &str, plies: i32) -> Result<Job> {
    create_job_with_snapshot(pool, position_id, kind, plies, None).await
}

pub async fn create_job_with_snapshot(
    pool: &PgPool,
    position_id: Uuid,
    kind: &str,
    plies: i32,
    snapshot: Option<&crate::settings::EngineSnapshot>,
) -> Result<Job> {
    let engine_config = snapshot.map_or_else(|| "legacy".into(), |s| s.fingerprint());
    let snapshot = snapshot.map(serde_json::to_value).transpose()?;
    let mut tx = pool.begin().await?;
    // Position jobs reference positions, so their FK does not itself protect the
    // owning match. Match management takes FOR UPDATE before checking active jobs.
    // Lock the same parent until submission commits to prevent accepted jobs from
    // disappearing in a concurrent match/database cascade.
    let owner = crate::position_retention::lock_owner(&mut tx, position_id).await?;
    ensure!(owner.is_some(), "Position not found");

    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,position_uuid,kind,plies,status,engine_config,engine_snapshot) VALUES($1,$2,$3,$4,'queued',$5,$6) ON CONFLICT (position_uuid,kind,plies,engine_config) WHERE status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(position_id).bind(kind).bind(plies).bind(&engine_config).bind(&snapshot).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id)
                .bind(serde_json::json!({"status":"queued","progress":0}))
                .execute(&mut *tx)
                .await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE position_uuid=$1 AND kind=$2 AND plies=$3 AND engine_config=$4 AND status IN ('queued','running')")
            .bind(position_id).bind(kind).bind(plies).bind(&engine_config).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
        // A worker completed the conflicting job after the INSERT snapshot.
        // READ COMMITTED gives the next INSERT a fresh snapshot; try again.
    };
    tx.commit().await?;
    Ok(job)
}

pub async fn events(pool: &PgPool, job_id: Uuid, after: i64) -> Result<Vec<JobEvent>> {
    Ok(
        sqlx::query_as("SELECT * FROM job_events WHERE job_id=$1 AND id>$2 ORDER BY id LIMIT 100")
            .bind(job_id)
            .bind(after)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn latest_match_job(pool: &PgPool, match_id: Uuid) -> Result<Option<Job>> {
    latest_match_job_scoped(pool, match_id, None, None).await
}

pub async fn latest_match_job_scoped(
    pool: &PgPool,
    match_id: Uuid,
    game_no: Option<i32>,
    engine_id: Option<&str>,
) -> Result<Option<Job>> {
    let mut query = QueryBuilder::new("SELECT * FROM jobs");
    query
        .push(" WHERE source_revision=(SELECT source_revision FROM matches WHERE id=")
        .push_bind(match_id)
        .push(") AND match_uuid=")
        .push_bind(match_id)
        .push(" AND game_no IS NOT DISTINCT FROM ")
        .push_bind(game_no)
        .push(" AND kind='match'");
    if let Some(engine_id) = engine_id {
        query
            .push(" AND coalesce(engine_snapshot->>'engine_id','gnubg')=")
            .push_bind(engine_id);
    }
    query.push(" ORDER BY created_at DESC,id DESC LIMIT 1");
    Ok(query.build_query_as().fetch_optional(pool).await?)
}

pub async fn match_analysis_jobs_for_scope(
    pool: &PgPool,
    match_id: Uuid,
    game_no: Option<i32>,
) -> Result<Vec<AnalysisComparisonRun>> {
    // The latest run in each state keeps completed results accessible even
    // during a rerun and bounds the response by provider, depth and status.
    Ok(sqlx::query_as(
        "SELECT * FROM (
            SELECT DISTINCT ON (coalesce(j.engine_snapshot->>'engine_id','gnubg'), j.plies, j.status, (j.game_no IS NULL))
                j.*, CASE WHEN j.game_no IS NULL THEN c.summary END AS game_summary
            FROM jobs j JOIN matches m ON m.id=j.match_uuid AND m.source_revision=j.source_revision
            LEFT JOIN match_analysis_games c ON c.job_id=j.id AND c.game_no=$2 AND j.game_no IS NULL
            WHERE j.match_uuid=$1 AND j.kind='match'
              AND NOT m.collection_only
              AND (coalesce(j.game_no,0)=coalesce($2::integer,0)
                   OR (c.job_id IS NOT NULL AND j.status='completed' AND j.completed_games=j.total_games))
            ORDER BY coalesce(j.engine_snapshot->>'engine_id','gnubg'), j.plies, j.status,
                     (j.game_no IS NULL), (c.summary IS NOT NULL) DESC, j.created_at DESC, j.id DESC
        ) history ORDER BY created_at DESC, id DESC",
    )
    .bind(match_id)
    .bind(game_no)
    .fetch_all(pool)
    .await?)
}

pub async fn create_match_job(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
) -> Result<Job> {
    create_match_job_scoped(pool, match_id, plies, total_games, None).await
}

pub async fn create_match_job_scoped(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
    game_no: Option<i32>,
) -> Result<Job> {
    create_match_job_scoped_with_snapshot(pool, match_id, plies, total_games, game_no, None).await
}

pub async fn create_match_job_scoped_with_snapshot(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
    game_no: Option<i32>,
    snapshot: Option<&crate::settings::EngineSnapshot>,
) -> Result<Job> {
    ensure!(
        game_no.is_none_or(|n| n > 0 && total_games == 1),
        "Invalid game scope"
    );
    let engine_config = snapshot.map_or_else(|| "legacy".into(), |s| s.fingerprint());
    let snapshot = snapshot.map(serde_json::to_value).transpose()?;
    let mut tx = pool.begin().await?;
    let source: Option<bool> =
        sqlx::query_scalar("SELECT NOT collection_only FROM matches WHERE id=$1 FOR SHARE")
            .bind(match_id)
            .fetch_optional(&mut *tx)
            .await?;
    ensure!(
        source == Some(true),
        "Source match is unavailable; analyze the saved position instead"
    );
    // API preflight may precede a live match appending another game. Derive the
    // scope under the same parent lock that protects the job's source snapshot.
    let mut games: Vec<i32> =
        sqlx::query_scalar("SELECT game_no FROM match_games WHERE match_uuid=$1 ORDER BY game_no")
            .bind(match_id)
            .fetch_all(&mut *tx)
            .await?;
    if games.is_empty() {
        let source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
            .bind(match_id)
            .fetch_one(&mut *tx)
            .await?;
        // Parse the source read under the lock, but off the async worker.
        games = tokio::task::spawn_blocking(move || {
            crate::import::sgf::parse_sgf_with_context(&source).map(|(imported, _)| {
                imported
                    .games
                    .into_iter()
                    .map(|game| game.game_no)
                    .collect()
            })
        })
        .await??;
    }
    let total_games = if let Some(game_no) = game_no {
        ensure!(games.contains(&game_no), "Game not found");
        1
    } else {
        i32::try_from(games.len())?
    };
    ensure!(total_games > 0, "Match contains no games");
    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,game_no,engine_config,engine_snapshot) VALUES($1,$2,'match',$3,'queued',$4,$5,$6,$7) ON CONFLICT(match_uuid,(coalesce(game_no,0)),plies,engine_config) WHERE kind='match' AND status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(match_id).bind(plies).bind(total_games).bind(game_no).bind(&engine_config).bind(&snapshot).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id).bind(serde_json::json!({"status":"queued","progress":0,"total_games":total_games,"completed_games":0}))
                .execute(&mut *tx).await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE match_uuid=$1 AND plies=$2 AND game_no IS NOT DISTINCT FROM $3 AND engine_config=$4 AND kind='match' AND status IN ('queued','running')")
            .bind(match_id).bind(plies).bind(game_no).bind(&engine_config).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
    };
    tx.commit().await?;
    Ok(job)
}

pub async fn move_analyses(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
) -> Result<MoveAnalysisPage> {
    move_analyses_scoped(pool, job_id, after, limit, errors_only, player, kind, None).await
}

#[allow(clippy::too_many_arguments)]
pub async fn move_analyses_scoped(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
    game_no: Option<i32>,
) -> Result<MoveAnalysisPage> {
    move_analyses_filtered(
        pool,
        job_id,
        after,
        limit,
        errors_only,
        player,
        kind,
        game_no,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn move_analyses_filtered(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
    game_no: Option<i32>,
    severity: Option<&str>,
) -> Result<MoveAnalysisPage> {
    let thresholds = crate::configuration::store::error_thresholds(pool).await?;
    move_analyses_with_thresholds(
        pool,
        job_id,
        after,
        limit,
        errors_only,
        player,
        kind,
        game_no,
        severity,
        thresholds,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn move_analyses_with_thresholds(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
    game_no: Option<i32>,
    severity: Option<&str>,
    thresholds: crate::configuration::ErrorThresholds,
) -> Result<MoveAnalysisPage> {
    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT id,position_uuid,game_no,data,grade FROM (SELECT id,position_uuid,game_no,data,",
    );
    thresholds.push_classification(&mut q, "equity_loss", "data->>'evaluation_status'");
    q.push(" AS grade FROM move_analyses WHERE job_id=");

    q.push_bind(job_id).push(" AND id>").push_bind(after);
    if let Some(game_no) = game_no {
        q.push(" AND game_no=").push_bind(game_no);
    }
    if errors_only {
        q.push(" AND equity_loss >= ")
            .push_bind(thresholds.questionable);
    }
    if let Some(player) = player {
        q.push(" AND player=").push_bind(player);
    }
    if let Some(kind) = kind {
        q.push(" AND kind=").push_bind(kind);
    }
    q.push(") AS moves WHERE TRUE");
    if errors_only {
        q.push(" AND grade <> 'none'");
    }
    if let Some(severity) = severity {
        q.push(" AND grade=").push_bind(severity);
    }
    q.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows = q
        .build_query_as::<(i64, Option<Uuid>, i32, serde_json::Value, String)>()
        .fetch_all(pool)
        .await?;
    let mut items: Vec<MoveAnalysis> = rows
        .into_iter()
        .map(|(id, position_uuid, game_no, data, severity)| {
            let mut decision: crate::match_analysis::Decision = serde_json::from_value(data)?;
            decision.severity = severity;
            Ok(MoveAnalysis {
                id,
                position_uuid,
                game_no,
                decision,
            })
        })
        .collect::<Result<_>>()?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|row| row.id)
    } else {
        None
    };
    Ok(MoveAnalysisPage { items, next_cursor })
}

/// A legacy row has no snapshot and intentionally retains the injected engine.
/// New and recovered jobs use exactly the installation saved at submission time.
/// Read after the worker registers for cancellation so a revoked claim cannot
/// start an engine even if its cancellation signal preceded registration.
pub(crate) async fn claimed_job_engine(
    pool: &PgPool,
    job_id: Uuid,
    lease: Uuid,
    fallback: &crate::engine::Engine,
) -> Result<crate::engines::AnalysisEngine> {
    let snapshot: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT engine_snapshot FROM jobs WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
            .bind(job_id)
            .bind(lease)
            .fetch_optional(pool)
            .await?
            .context("Job lease was lost")?;
    match snapshot {
        Some(snapshot) => {
            serde_json::from_value::<crate::settings::EngineSnapshot>(snapshot)?.engine()
        }
        None => Ok(crate::engines::AnalysisEngine::Gnubg(fallback.clone())),
    }
}
