//! Persisted head-to-head series, with a versioned dice schedule per match or pair.
mod dice;
mod runner;
#[cfg(test)]
mod tests;

pub(crate) use runner::ENGINE_DUELS_DATABASE_ID;
pub use runner::worker;

use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, resolve_binary},
    model::MatchSummary,
    settings::{self, EngineSnapshot},
    store,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DiceMode {
    #[default]
    Paired,
    Random,
}

impl DiceMode {
    fn matches_per_unit(self) -> i32 {
        match self {
            Self::Paired => 2,
            Self::Random => 1,
        }
    }
}

/// What one duel unit plays: a point match with the cube, or a block of
/// independent cubeless money games without the Jacoby rule.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DuelFormat {
    #[default]
    Match,
    MoneyCubeless,
}

const MAX_BLOCK_GAMES: i32 = 1000;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct Competitor {
    engine_id: String,
    plies: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSeries {
    a: Competitor,
    b: Competitor,
    #[serde(default)]
    format: DuelFormat,
    /// Point match length; 1–25 for matches, 0 or omitted for money blocks.
    #[serde(default)]
    #[schema(minimum = 0, maximum = 25)]
    match_length: i32,
    /// Games per stored block; required for cubeless money blocks.
    #[schema(minimum = 1, maximum = 1000)]
    block_games: Option<i32>,
    /// Matches or money blocks; pairs in paired mode.
    #[schema(minimum = 1, maximum = 1000)]
    count: i32,
    #[serde(default)]
    mode: DiceMode,
    seed: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct SeriesConfig {
    a: Competitor,
    b: Competitor,
    /// Absent in series created before money blocks existed.
    #[serde(default)]
    format: DuelFormat,
    /// 0 for money blocks.
    match_length: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    block_games: Option<i32>,
    count: i32,
    mode: DiceMode,
    seed: Uuid,
}

impl SeriesConfig {
    /// Validated play format of one unit, as the runner executes it.
    fn unit(&self) -> anyhow::Result<crate::play::engine_match::DuelUnit> {
        use crate::play::engine_match::DuelUnit;
        match (self.format, self.block_games) {
            (DuelFormat::Match, None) if (1..=25).contains(&self.match_length) => {
                Ok(DuelUnit::Match {
                    length: self.match_length,
                })
            }
            (DuelFormat::MoneyCubeless, Some(games))
                if self.match_length == 0 && (1..=MAX_BLOCK_GAMES).contains(&games) =>
            {
                Ok(DuelUnit::MoneyCubeless { games })
            }
            _ => anyhow::bail!("Invalid saved duel format"),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
struct SeriesEngines {
    rules_version: String,
    a: EngineSnapshot,
    b: EngineSnapshot,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct Series {
    id: Uuid,
    #[sqlx(json)]
    configuration: SeriesConfig,
    #[sqlx(json)]
    engines: SeriesEngines,
    dice_version: String,
    status: String,
    total_matches: i32,
    completed_matches: i32,
    error: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct MatchResult {
    match_index: i32,
    seed: Uuid,
    score_a: i32,
    score_b: i32,
    games: i32,
    #[serde(skip)]
    #[sqlx(flatten)]
    cost: MatchCost,
    /// Money blocks: games won by A as single, gammon, backgammon, then B.
    #[serde(skip)]
    outcomes: Option<Vec<i32>>,
}

/// Stored engine work per side. Each side is all NULL for matches played
/// before the measurement existed.
#[derive(Debug, Default, FromRow)]
struct MatchCost {
    cpu_us_a: Option<i64>,
    wall_us_a: Option<i64>,
    moves_a: Option<i32>,
    cpu_us_b: Option<i64>,
    wall_us_b: Option<i64>,
    moves_b: Option<i32>,
}

impl MatchCost {
    fn side(&self, side: usize) -> Option<(i64, i64, i32)> {
        match side {
            0 => Some((self.cpu_us_a?, self.wall_us_a?, self.moves_a?)),
            _ => Some((self.cpu_us_b?, self.wall_us_b?, self.moves_b?)),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
struct SeriesStatistics {
    wins_a: i32,
    wins_b: i32,
    win_rate_a: Option<f64>,
    complete_pairs: i32,
    /// 95% interval for A's match win rate; matches in paired mode only.
    confidence_interval_a: Option<[f64; 2]>,
    /// Cubeless money blocks only. `wins_a`/`wins_b` then count games won.
    money: Option<MoneyStatistics>,
    /// Engine work of A over the counted matches that recorded it.
    cost_a: Option<SideCostSummary>,
    /// Engine work of B over the counted matches that recorded it.
    cost_b: Option<SideCostSummary>,
}

/// Results of cubeless money blocks over the counted blocks.
#[derive(Debug, PartialEq, Serialize, ToSchema)]
struct MoneyStatistics {
    games: i64,
    /// Net points per game from A's perspective, between −3 and 3.
    points_per_game_a: Option<f64>,
    /// 95% interval for `points_per_game_a`, from 20 pairs or blocks.
    confidence_interval: Option<[f64; 2]>,
    outcomes_a: GameOutcomes,
    outcomes_b: GameOutcomes,
}

/// Games won by one side, by value.
#[derive(Debug, PartialEq, Serialize, ToSchema)]
struct GameOutcomes {
    single: i64,
    gammon: i64,
    backgammon: i64,
}

impl From<[i64; 3]> for GameOutcomes {
    fn from([single, gammon, backgammon]: [i64; 3]) -> Self {
        Self {
            single,
            gammon,
            backgammon,
        }
    }
}

/// Engine work per checker turn. CPU time covers all engine threads; both
/// include cube decisions but exclude process startup, replay and checks.
#[derive(Debug, PartialEq, Serialize, ToSchema)]
struct SideCostSummary {
    /// Counted matches with a measurement for this side.
    measured_matches: i32,
    /// Checker turns of this side in those matches, including forced moves.
    moves: i64,
    cpu_ms_per_move: f64,
    wall_ms_per_move: f64,
}

#[derive(Serialize, ToSchema)]
struct SeriesDetails {
    series: Series,
    statistics: SeriesStatistics,
    matches: Vec<MatchResult>,
    next_offset: Option<i32>,
    previous_offset: Option<i32>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
struct SeriesPage {
    runs: Vec<Series>,
    next_cursor: Option<Uuid>,
}

#[derive(Deserialize)]
struct ResultPage {
    #[serde(default)]
    offset: i32,
}

const PAGE_SIZE: usize = 50;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/engine-matches", get(list).post(create))
        .route("/api/engine-matches/{id}", get(details))
        .route("/api/engine-matches/{id}/cancel", post(cancel))
        .route("/api/engine-matches/{id}/matches/{index}/sgf", get(sgf))
        .route(
            "/api/engine-matches/{id}/matches/{index}/library",
            post(library_match),
        )
}

#[utoipa::path(get, path="/api/engine-matches", params(("before"=Option<Uuid>, Query)), responses((status=200, body=SeriesPage)), tag="Engine matches")]
async fn list(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<SeriesPage>> {
    let mut runs: Vec<Series> = sqlx::query_as(
        "SELECT * FROM engine_match_runs
         WHERE $1::uuid IS NULL OR (created_at,id) <
             (SELECT created_at,id FROM engine_match_runs WHERE id=$1)
         ORDER BY created_at DESC,id DESC LIMIT $2",
    )
    .bind(query.before)
    .bind(PAGE_SIZE as i64 + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let next_cursor = if runs.len() > PAGE_SIZE {
        runs.truncate(PAGE_SIZE);
        runs.last().map(|run| run.id)
    } else {
        None
    };
    Ok(Json(SeriesPage { runs, next_cursor }))
}

#[utoipa::path(post, path="/api/engine-matches", request_body=CreateSeries, responses((status=201, body=Series),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn create(
    State(state): State<AppState>,
    payload: Result<Json<CreateSeries>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    if !(1..=1000).contains(&request.count) {
        return Err(ApiError::invalid("Count must be 1–1000"));
    }
    let valid_format = match request.format {
        DuelFormat::Match => {
            (1..=25).contains(&request.match_length) && request.block_games.is_none()
        }
        DuelFormat::MoneyCubeless => {
            request.match_length == 0
                && request
                    .block_games
                    .is_some_and(|games| (1..=MAX_BLOCK_GAMES).contains(&games))
        }
    };
    if !valid_format {
        return Err(ApiError::invalid(
            "Matches need a length of 1–25 and no block size; cubeless money blocks need 1–1000 games and no match length",
        ));
    }
    let cubeless = request.format == DuelFormat::MoneyCubeless;
    for side in [&request.a, &request.b] {
        if !crate::configuration::engine_depths(&side.engine_id, "duel").contains(&side.plies) {
            return Err(ApiError::invalid("Unsupported playing engine or strength"));
        }
    }
    let settings = settings::load(&state).await?;
    let path =
        resolve_binary(&settings.gnubg_path).map_err(|e| ApiError::invalid(e.to_string()))?;
    let gnubg_threads = settings.gnubg_threads.filter(|&threads| threads != 1);
    let version = Engine::new(path.clone())
        .with_threads(gnubg_threads)
        .check_installation()
        .await
        .map_err(|error| {
            ApiError::invalid(format!("GNUbg installation check failed: {error:#}"))
        })?;
    crate::play::probe_native(
        &Engine::new(path.clone())
            .with_threads(gnubg_threads)
            .with_expected_version(Some(version.clone())),
    )
    .await
    .map_err(|error| {
        ApiError::invalid(format!("GNUbg play installation check failed: {error:#}"))
    })?;
    let sage_threads = settings.sage_analysis_threads();
    let snapshot = |side: &Competitor| {
        let path = path.clone();
        let version = version.clone();
        let id = side.engine_id.clone();
        let plies = side.plies;
        let state = &state;
        async move {
            if id == "gnubg" {
                Ok::<_, ApiError>(EngineSnapshot {
                    engine_id: id,
                    gnubg_path: path,
                    engine_version: Some(version),
                    gnubg_threads,
                    gnubg_move_filter: None,
                    open_sage: None,
                })
            } else {
                let mut snapshot =
                    settings::snapshot_for(state, &id, &path, None, sage_threads, plies)
                        .await
                        .map_err(|error| {
                            ApiError::invalid(format!(
                                "Open Sage installation check failed: {error:#}"
                            ))
                        })?;
                if cubeless {
                    snapshot = settings::cubeless_sage(snapshot).await.map_err(|error| {
                        ApiError::invalid(format!("Open Sage cubeless check failed: {error:#}"))
                    })?;
                }
                Ok(snapshot)
            }
        }
    };
    let a = snapshot(&request.a).await?;
    let b = if request.a.engine_id == request.b.engine_id && request.a.plies == request.b.plies {
        a.clone()
    } else {
        snapshot(&request.b).await?
    };
    let engines = SeriesEngines {
        rules_version: version,
        a,
        b,
    };
    let configuration = SeriesConfig {
        a: request.a,
        b: request.b,
        format: request.format,
        match_length: request.match_length,
        block_games: request.block_games,
        count: request.count,
        mode: request.mode,
        seed: request.seed.unwrap_or_else(Uuid::new_v4),
    };
    let series = sqlx::query_as("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,total_matches) VALUES($1,$2,$3,$4,$5) RETURNING *")
        .bind(Uuid::new_v4())
        .bind(sqlx::types::Json(&configuration))
        .bind(sqlx::types::Json(&engines))
        .bind(dice::VERSION)
        .bind(configuration.count * configuration.mode.matches_per_unit())
        .fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(series)))
}

#[utoipa::path(get, path="/api/engine-matches/{id}", params(("id"=Uuid, Path), ("offset"=Option<i32>, Query)), responses((status=200, body=SeriesDetails),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn details(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(page): Query<ResultPage>,
) -> ApiResult<Json<SeriesDetails>> {
    if !(0..=2000).contains(&page.offset) {
        return Err(ApiError::invalid("Invalid result offset"));
    }
    let offset = page.offset as usize;
    let series = load(&state.pool, id).await?;
    // Read only committed results up to this version of the progress counter.
    let mut results: Vec<MatchResult> = sqlx::query_as("SELECT match_index,seed,score_a,score_b,games,cpu_us_a,wall_us_a,moves_a,cpu_us_b,wall_us_b,moves_b,outcomes FROM engine_match_results WHERE run_id=$1 AND match_index<$2 ORDER BY match_index")
        .bind(id).bind(series.completed_matches).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    let statistics = summarize(
        series.configuration.format,
        series.configuration.mode,
        &results,
    );
    let next_offset =
        (offset.saturating_add(PAGE_SIZE) < results.len()).then(|| page.offset + PAGE_SIZE as i32);
    let previous_offset = (offset > 0).then(|| (page.offset - PAGE_SIZE as i32).max(0));
    let matches = results
        .drain(offset.min(results.len())..)
        .take(PAGE_SIZE)
        .collect();
    Ok(Json(SeriesDetails {
        series,
        statistics,
        matches,
        next_offset,
        previous_offset,
    }))
}

async fn load(pool: &PgPool, id: Uuid) -> ApiResult<Series> {
    sqlx::query_as("SELECT * FROM engine_match_runs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

#[utoipa::path(post, path="/api/engine-matches/{id}/cancel", params(("id"=Uuid, Path)), responses((status=200, body=Series),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn cancel(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Series>> {
    sqlx::query("UPDATE engine_match_runs SET status='cancelled',lease_token=NULL,lease_until=NULL WHERE id=$1 AND status IN ('queued','running')")
        .bind(id).execute(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(load(&state.pool, id).await?))
}

async fn stored_sgf(pool: &PgPool, id: Uuid, index: i32) -> ApiResult<String> {
    sqlx::query_scalar("SELECT sgf FROM engine_match_results WHERE run_id=$1 AND match_index=$2")
        .bind(id)
        .bind(index)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

#[utoipa::path(get, path="/api/engine-matches/{id}/matches/{index}/sgf", params(("id"=Uuid, Path),("index"=i32, Path)), responses((status=200, body=String),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn sgf(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, i32)>,
) -> ApiResult<impl IntoResponse> {
    let source = stored_sgf(&state.pool, id, index).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-go-sgf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"engine-match-{id}-{index}.sgf\""),
            ),
        ],
        source,
    ))
}

/// Library entry of one played match or money block, for opening it in Analysis.
///
/// Finds the archived entry by its exact source, also after it was moved to another
/// database. An entry deleted from the library is restored under Engine Duels.
#[utoipa::path(post, path="/api/engine-matches/{id}/matches/{index}/library", params(("id"=Uuid, Path),("index"=i32, Path)), responses((status=200, body=MatchSummary),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn library_match(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, i32)>,
) -> ApiResult<Json<MatchSummary>> {
    let source = stored_sgf(&state.pool, id, index).await?;
    if let Some(entry) =
        store::library_match_by_source(&state.pool, source.as_bytes(), ENGINE_DUELS_DATABASE_ID)
            .await?
    {
        return Ok(Json(entry));
    }
    let parse_source = source.clone();
    let imported = tokio::task::spawn_blocking(move || crate::import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)??;
    let restored = store::import_match_in_database(
        &state.pool,
        &runner::archive_filename(id, index),
        "sgf",
        source.as_bytes(),
        &source,
        &imported,
        ENGINE_DUELS_DATABASE_ID,
    )
    .await?;
    Ok(Json(restored.r#match))
}

fn summarize(format: DuelFormat, mode: DiceMode, results: &[MatchResult]) -> SeriesStatistics {
    let complete_pairs = if mode == DiceMode::Paired {
        results.len() / 2
    } else {
        0
    };
    let counted = if mode == DiceMode::Paired {
        &results[..complete_pairs * 2]
    } else {
        results
    };
    // Independent observations: pairs of legs, or single units without pairing.
    let units = counted.chunks(mode.matches_per_unit() as usize);
    let enough = units.len() >= MIN_PAIRS_FOR_INTERVAL;
    let (wins_a, wins_b, confidence_interval_a, money) = match format {
        DuelFormat::Match => {
            let wins_a = counted
                .iter()
                .filter(|result| result.score_a > result.score_b)
                .count() as i32;
            let interval = (mode == DiceMode::Paired && enough)
                .then(|| {
                    let means = units.map(|pair| {
                        let wins = pair.iter().filter(|r| r.score_a > r.score_b).count();
                        wins as f64 / pair.len() as f64
                    });
                    t_interval(&means.collect::<Vec<_>>(), 0.0, 1.0)
                })
                .flatten();
            (wins_a, counted.len() as i32 - wins_a, interval, None)
        }
        DuelFormat::MoneyCubeless => {
            let money = money_statistics(counted, units.clone(), enough);
            (
                money.games_won_a,
                money.games_won_b,
                None,
                Some(money.summary),
            )
        }
    };
    let decided = wins_a + wins_b;
    let win_rate_a = (decided > 0).then(|| f64::from(wins_a) / f64::from(decided));
    SeriesStatistics {
        wins_a,
        wins_b,
        win_rate_a,
        complete_pairs: complete_pairs as i32,
        confidence_interval_a,
        money,
        cost_a: side_cost(counted, 0),
        cost_b: side_cost(counted, 1),
    }
}

struct MoneyTotals {
    games_won_a: i32,
    games_won_b: i32,
    summary: MoneyStatistics,
}

/// Points per game for A over cubeless money blocks. The interval treats each
/// pair of blocks (or each block without pairing) as one observation.
fn money_statistics<'a>(
    counted: &[MatchResult],
    units: impl Iterator<Item = &'a [MatchResult]>,
    enough: bool,
) -> MoneyTotals {
    let mut outcomes = [[0_i64; 3]; 2];
    for counts in counted
        .iter()
        .filter_map(|result| result.outcomes.as_deref())
    {
        for (index, &count) in counts.iter().take(6).enumerate() {
            outcomes[index / 3][index % 3] += i64::from(count);
        }
    }
    let games: i64 = outcomes.iter().flatten().sum();
    let net = |results: &[MatchResult]| {
        let points: i64 = results
            .iter()
            .map(|r| i64::from(r.score_a) - i64::from(r.score_b))
            .sum();
        let games: i64 = results.iter().map(|r| i64::from(r.games)).sum();
        (points, games)
    };
    let (points, _) = net(counted);
    let interval = enough
        .then(|| {
            let means = units
                .map(|unit| {
                    let (points, games) = net(unit);
                    points as f64 / games.max(1) as f64
                })
                .collect::<Vec<_>>();
            t_interval(&means, -3.0, 3.0)
        })
        .flatten();
    let won = |side: [i64; 3]| side.iter().sum::<i64>();
    MoneyTotals {
        games_won_a: i32::try_from(won(outcomes[0])).unwrap_or(i32::MAX),
        games_won_b: i32::try_from(won(outcomes[1])).unwrap_or(i32::MAX),
        summary: MoneyStatistics {
            games,
            points_per_game_a: (games > 0).then(|| points as f64 / games as f64),
            confidence_interval: interval,
            outcomes_a: GameOutcomes::from(outcomes[0]),
            outcomes_b: GameOutcomes::from(outcomes[1]),
        },
    }
}

fn side_cost(counted: &[MatchResult], side: usize) -> Option<SideCostSummary> {
    let (mut matches, mut cpu_us, mut wall_us, mut moves) = (0_i32, 0_i64, 0_i64, 0_i64);
    for (cpu, wall, turns) in counted.iter().filter_map(|result| result.cost.side(side)) {
        matches += 1;
        cpu_us = cpu_us.saturating_add(cpu);
        wall_us = wall_us.saturating_add(wall);
        moves += i64::from(turns);
    }
    (moves > 0).then(|| SideCostSummary {
        measured_matches: matches,
        moves,
        cpu_ms_per_move: cpu_us as f64 / 1000.0 / moves as f64,
        wall_ms_per_move: wall_us as f64 / 1000.0 / moves as f64,
    })
}

const MIN_PAIRS_FOR_INTERVAL: usize = 20;

/// Two-sided 95% Student-t interval for the mean of independent observations,
/// using their sample variance and clamped to `[min, max]`. With paired dice,
/// each observation is a pair of legs, so the variance includes the reduction
/// from swapping sides. Returns `None` for fewer than two observations.
fn t_interval(values: &[f64], min: f64, max: f64) -> Option<[f64; 2]> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (n - 1) as f64;
    let radius = student_t_975(n - 1) * (variance / n as f64).sqrt();
    Some([(mean - radius).max(min), (mean + radius).min(max)])
}

/// 0.975 quantile of Student's t distribution with `df` degrees of freedom.
///
/// Cornish–Fisher expansion around the normal quantile (Abramowitz & Stegun
/// 26.7.5). Its error is below 1e-4 for the 19 or more degrees of freedom a
/// displayed interval requires.
fn student_t_975(df: usize) -> f64 {
    const Z: f64 = 1.959_963_984_540_054;
    let n = df as f64;
    let z2 = Z * Z;
    let g1 = (z2 + 1.0) * Z / 4.0;
    let g2 = ((5.0 * z2 + 16.0) * z2 + 3.0) * Z / 96.0;
    let g3 = (((3.0 * z2 + 19.0) * z2 + 17.0) * z2 - 15.0) * Z / 384.0;
    let g4 = ((((79.0 * z2 + 776.0) * z2 + 1482.0) * z2 - 1920.0) * z2 - 945.0) * Z / 92_160.0;
    Z + g1 / n + g2 / (n * n) + g3 / (n * n * n) + g4 / (n * n * n * n)
}

#[derive(OpenApi)]
#[openapi(
    paths(list, create, details, cancel, sgf, library_match),
    components(schemas(
        CreateSeries,
        Competitor,
        DiceMode,
        DuelFormat,
        SeriesConfig,
        SeriesEngines,
        Series,
        MatchResult,
        MoneyStatistics,
        GameOutcomes,
        SideCostSummary,
        SeriesStatistics,
        SeriesDetails,
        SeriesPage
    ))
)]
pub(crate) struct EngineMatchDoc;
