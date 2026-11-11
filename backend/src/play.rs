//! Durable native GNUbg matches. Each action runs in a bounded isolated process;
//! optimistic revisions fence concurrent requests without holding DB locks.
mod feedback;
mod library;
mod reports;
pub use feedback::{
    FeedbackOperation, PlayAlternative, PlayAttempt, PlayDecision, PlayFeedbackRequest, PlayMode,
    PlayReview, PlayReviewRequest,
};
use feedback::{analyze_decision, default_analysis_plies, present, stage_choice};
pub(crate) use feedback::{default_auto_analysis, default_play_mode, default_threshold};
pub use library::{BOT_DATABASE_ID, backfill as backfill_library_matches};

use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, resolve_binary},
    model::Position,
};
use anyhow::{Context, ensure};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    routing::{get, post},
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayEngine {
    pub id: String,
    pub version: String,
    pub plies: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayStep {
    pub from: i32,
    pub to: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayMove {
    pub notation: String,
    pub steps: Vec<PlayStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistory {
    pub player: i32,
    pub dice: Vec<i32>,
    pub notation: String,
}
/// Read-only snapshots aligned one-for-one with the saved session history.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistorySnapshots {
    pub session_id: Uuid,
    pub version: i64,
    pub items: Vec<PlayHistorySnapshot>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayHistorySnapshot {
    pub index: usize,
    pub game_no: i32,
    pub move_no: i32,
    #[schema(examples("checker", "cube", "resign"))]
    pub kind: String,
    pub before: Position,
    pub after: Position,
}
#[derive(Debug, Deserialize)]
struct HistoryQuery {
    version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlayPhase {
    OpeningRoll,
    OpeningResult,
    Roll,
    Move,
    DoubleResponse,
    GameOver,
    MatchOver,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OpeningRoll {
    pub white_die: i32,
    pub black_die: i32,
    pub winner: i32,
    pub rolls: Vec<[i32; 2]>,
}
/// Completed source games, attached after their final played history event.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlayGameResult {
    pub game_no: i32,
    pub history_index: usize,
    pub result: crate::model::GameResult,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PlaySession {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_results: Option<Vec<PlayGameResult>>,
    /// Current linked library entry; projected from the relational session link.
    #[serde(default)]
    pub library_match_id: Option<Uuid>,
    #[serde(default)]
    pub learning_used: bool,
    #[serde(default)]
    pub game_learning_used: bool,
    /// An explicitly closed money session cannot start another game.
    #[serde(default)]
    pub money_session_ended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_report_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_report_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_game_no: Option<i32>,
    #[serde(default)]
    pub review: Option<PlayReview>,
    #[serde(default)]
    pub play_mode: PlayMode,
    #[serde(default = "default_threshold")]
    pub tutor_threshold: f64,
    #[serde(default = "default_auto_analysis")]
    pub training_auto_analysis: bool,
    #[serde(default = "default_analysis_plies")]
    pub analysis_plies: i32,
    #[serde(default)]
    pub pending_decision: Option<PlayDecision>,
    #[serde(default)]
    pub decisions: Vec<PlayDecision>,
    pub id: Uuid,
    pub version: i64,
    pub human_player: i32,
    pub human_name: String,
    pub match_length: i32,
    pub bearoff_direction: String,
    pub engine: PlayEngine,
    pub position: Position,
    pub phase: PlayPhase,
    pub can_double: bool,
    pub legal_moves: Vec<PlayMove>,
    pub history: Vec<PlayHistory>,
    pub scores: [i32; 2],
    pub game_no: i32,
    pub winner: Option<i32>,
    #[serde(default)]
    pub opening: Option<OpeningRoll>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePlaySession {
    pub play_mode: Option<PlayMode>,
    pub tutor_threshold: Option<f64>,
    pub training_auto_analysis: Option<bool>,
    #[schema(minimum = 0, maximum = 1)]
    pub human_player: i32,
    /// Zero starts a continuous money-game session; 1..25 starts a point match.
    #[schema(minimum = 0, maximum = 25)]
    pub match_length: i32,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayModeRequest {
    pub version: i64,
    pub play_mode: PlayMode,
    pub tutor_threshold: Option<f64>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlayAction {
    OpeningRoll,
    StartOpeningMove,
    Roll,
    Move,
    Pass,
    Double,
    Take,
    Drop,
    Resign,
    NextGame,
    EndSession,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayActionRequest {
    pub decision_id: Option<Uuid>,
    pub version: i64,
    pub action: PlayAction,
    #[serde(rename = "move")]
    pub move_index: Option<usize>,
    /// Resign the current game for a single (1), gammon (2), or backgammon (3).
    /// Required for resign; invalid for every other action.
    #[schema(minimum = 1, maximum = 3)]
    pub resign_level: Option<i32>,
}
#[derive(FromRow)]
struct Saved {
    state: sqlx::types::Json<PlaySession>,
    gnubg_path: String,
    sgf: String,
    native_id: String,
}
#[derive(Serialize)]
struct NativeRequest<'a> {
    new: bool,
    human_player: i32,
    match_length: i32,
    plies: i32,
    native_id: &'a str,
    action: &'a str,
    notation: Option<&'a str>,
    resign_level: Option<i32>,
}
#[derive(Deserialize)]
struct NativeSnapshot {
    native_id: String,
    board: Vec<i32>,
    player: i32,
    dice: Vec<i32>,
    phase: PlayPhase,
    can_double: bool,
    legal_moves: Vec<PlayMove>,
    history: Vec<PlayHistory>,
    scores: [i32; 2],
    game_no: i32,
    winner: Option<i32>,
    #[serde(default)]
    opening: Option<OpeningRoll>,
    #[serde(default)]
    resignation_declined: bool,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/play/sessions",
            get(list_sessions).post(create_session),
        )
        .route("/api/play/sessions/{id}", get(get_session))
        .route("/api/play/sessions/{id}/actions", post(act))
        .route("/api/play/sessions/{id}/mode", post(change_mode))
        .route("/api/play/sessions/{id}/feedback", post(feedback))
        .route("/api/play/sessions/{id}/review", post(review))
        .route("/api/play/sessions/{id}/sgf", get(export_sgf))
        .route("/api/play/sessions/{id}/history", get(history_snapshots))
}
pub(crate) async fn require_review_available(pool: &sqlx::PgPool, match_id: Uuid) -> ApiResult<()> {
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM play_sessions WHERE library_match_id=$1 AND coalesce(state->>'play_mode','competition')='competition' AND state->>'phase'<>'match_over' AND NOT ((state->>'match_length')::int=0 AND state->>'phase'='game_over'))").bind(match_id).fetch_one(pool).await.map_err(anyhow::Error::from)?;
    if active {
        return Err(ApiError::invalid(
            "Complete the competition match before requesting analysis",
        ));
    }
    Ok(())
}
pub(crate) async fn require_position_review_available(
    pool: &sqlx::PgPool,
    position_id: Uuid,
) -> ApiResult<()> {
    let match_id: Option<Uuid> = sqlx::query_scalar("SELECT match_uuid FROM positions WHERE id=$1")
        .bind(position_id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(match_id) = match_id {
        require_review_available(pool, match_id).await?;
    }
    Ok(())
}
pub(crate) async fn require_job_review_available(
    pool: &sqlx::PgPool,
    job: &crate::model::Job,
) -> ApiResult<()> {
    if let Some(match_id) = job.match_uuid {
        require_review_available(pool, match_id).await?;
    }
    if let Some(position_id) = job.position_uuid {
        require_position_review_available(pool, position_id).await?;
    }
    Ok(())
}
async fn saved(state: &AppState, id: Uuid) -> ApiResult<Saved> {
    sqlx::query_as("SELECT jsonb_set(state,'{library_match_id}',coalesce(to_jsonb(library_match_id),'null'::jsonb)) AS state,gnubg_path,sgf,native_id FROM play_sessions WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}
async fn present_with_results(session: PlaySession, sgf: String) -> ApiResult<PlaySession> {
    Ok(
        tokio::task::spawn_blocking(move || project_game_results(session, &sgf))
            .await
            .map_err(anyhow::Error::from)?,
    )
}

fn project_game_results(mut session: PlaySession, sgf: &str) -> PlaySession {
    // Project after persistence, never write derived results into legacy state.
    session.game_results = None;
    if !sgf.trim().is_empty()
        && let Ok((imported, events)) = crate::import::sgf::parse_native_sgf_source_moves(sgf)
        && events.len() == session.history.len()
        && events.iter().zip(&session.history).all(|(event, row)| {
            let action = row.notation.trim().to_ascii_lowercase();
            let kind = match action.as_str() {
                "double" | "take" | "drop" => "cube",
                "resign" => "resign",
                _ => "checker",
            };
            let mut dice = row.dice.clone();
            dice.sort_unstable();
            let mut expected = if kind == "checker" {
                event.dice.to_vec()
            } else {
                vec![]
            };
            expected.sort_unstable();
            event.player == row.player
                && event.kind == kind
                && dice == expected
                && (kind == "checker" || event.notation.eq_ignore_ascii_case(&action))
        })
    {
        let results = imported
            .games
            .into_iter()
            .filter_map(|game| {
                let result = game.result?;
                let history_index = events
                    .iter()
                    .rposition(|event| event.game_no == game.game_no)?;
                Some(PlayGameResult {
                    game_no: game.game_no,
                    history_index,
                    result,
                })
            })
            .collect::<Vec<_>>();
        session.game_results = Some(results);
    }
    present(session)
}

#[utoipa::path(get,path="/api/play/sessions",responses((status=200,body=Vec<PlaySession>)),tag="Playing")]
async fn list_sessions(State(state): State<AppState>) -> ApiResult<Json<Vec<PlaySession>>> {
    let rows: Vec<(sqlx::types::Json<PlaySession>, String)> = sqlx::query_as(
        "SELECT jsonb_set(state,'{library_match_id}',coalesce(to_jsonb(library_match_id),'null'::jsonb)),sgf FROM play_sessions ORDER BY updated_at DESC,id DESC LIMIT 100",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let mut sessions = Vec::with_capacity(rows.len());
    for (session, sgf) in rows {
        // Keep the list bounded: only one source replay is queued at a time.
        sessions.push(present_with_results(session.0, sgf).await?);
    }
    Ok(Json(sessions))
}
#[utoipa::path(get,path="/api/play/sessions/{id}",params(("id"=Uuid,Path)),responses((status=200,body=PlaySession),(status=404,body=ErrorBody)),tag="Playing")]
async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<PlaySession>> {
    let row = saved(&state, id).await?;
    Ok(Json(present_with_results(row.state.0, row.sgf).await?))
}
#[utoipa::path(get,path="/api/play/sessions/{id}/history",params(("id"=Uuid,Path),("version"=i64,Query,description="Saved session revision")),responses((status=200,body=PlayHistorySnapshots),(status=404,body=ErrorBody),(status=409,body=ErrorBody)),tag="Playing")]
async fn history_snapshots(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<PlayHistorySnapshots>> {
    // Fetch state and its exact raw checkpoint together. No archive lookup or
    // native replay: detached sessions and unavailable engines remain readable.
    let checkpoint = saved(&state, id).await?;
    if checkpoint.state.version != query.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; refresh history".into(),
        ));
    }
    let response = tokio::task::spawn_blocking(move || {
        build_history_snapshots(&checkpoint.state, &checkpoint.sgf)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(response))
}

pub(crate) fn build_history_snapshots(
    session: &PlaySession,
    sgf: &str,
) -> anyhow::Result<PlayHistorySnapshots> {
    let events = if sgf.trim().is_empty() {
        Vec::new()
    } else {
        let source = library::normalize_native_setup_turn(sgf.to_owned())?;
        crate::import::sgf::parse_native_sgf_source_moves(&source)?.1
    };
    ensure!(
        events.len() == session.history.len(),
        "Saved history differs from checkpoint event count"
    );
    let mut ply = 0;
    let mut game_no = 0;
    let mut items = Vec::with_capacity(events.len());
    for (index, (event, record)) in events.into_iter().zip(&session.history).enumerate() {
        ensure!(
            event.player == record.player,
            "Saved history differs from checkpoint player"
        );
        let action = record.notation.trim().to_ascii_lowercase();
        let record_kind = match action.as_str() {
            "double" | "take" | "drop" => "cube",
            "resign" => "resign",
            _ => "checker",
        };
        ensure!(
            event.kind == record_kind,
            "Saved history differs from checkpoint action kind"
        );
        if event.kind != "checker" {
            ensure!(
                event.notation.eq_ignore_ascii_case(&action),
                "Saved history differs from checkpoint action"
            );
            ensure!(
                record.dice.is_empty(),
                "Cube/resignation history contains dice"
            );
        } else {
            let mut record_dice = record.dice.clone();
            record_dice.sort_unstable();
            let mut source_dice = event.dice.to_vec();
            source_dice.sort_unstable();
            ensure!(
                record_dice == source_dice,
                "Saved history differs from checkpoint dice"
            );
        }
        if event.game_no != game_no {
            game_no = event.game_no;
            ply = 0;
        }
        if let Some(checker_ply) = event.checker_ply {
            ply = checker_ply;
        }
        let before = Position {
            id: session.id,
            match_uuid: session.id,
            ordinal: index.try_into()?,
            game_no: event.game_no,
            ply,
            player: event.player,
            dice: record.dice.clone(),
            board: event.board,
            position_id: event.position_id,
            gnubg_match_id: event.match_id,
            notation: event.notation,
        };
        let after_board: [i32; 26] = event.after_board.as_slice().try_into()?;
        // IDs encode checkers relative to player-on-roll, while the review
        // perspective remains the action's actor (also true for cube replies).
        let match_bytes =
            base64::engine::general_purpose::STANDARD_NO_PAD.decode(&event.after_match_id)?;
        let after_on_roll = i32::from((match_bytes[0] >> 6) & 1);
        let after = Position {
            board: event.after_board,
            position_id: crate::import::ids::position_id(&after_board, after_on_roll),
            gnubg_match_id: event.after_match_id,
            dice: vec![],
            notation: String::new(),
            ..before.clone()
        };
        items.push(PlayHistorySnapshot {
            index,
            game_no: event.game_no,
            move_no: event.move_no,
            kind: event.kind.into(),
            before,
            after,
        });
    }
    Ok(PlayHistorySnapshots {
        session_id: session.id,
        version: session.version,
        items,
    })
}

#[utoipa::path(post,path="/api/play/sessions",request_body=CreatePlaySession,responses((status=201,body=PlaySession),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreatePlaySession>,
) -> ApiResult<(StatusCode, Json<PlaySession>)> {
    if !(0..=1).contains(&request.human_player) || !(0..=25).contains(&request.match_length) {
        return Err(ApiError::invalid(
            "human_player must be 0 or 1 and match_length must be 0 (money game) or 1..25",
        ));
    }
    let settings = crate::settings::load(&state).await?;
    if request
        .tutor_threshold
        .is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1.0)
    {
        return Err(ApiError::invalid(
            "tutor_threshold must be greater than 0 and at most 1",
        ));
    }
    if settings.play_engine != "gnubg" {
        return Err(ApiError::invalid("Only GNUbg is available for playing"));
    }
    let pinned_path = resolve_binary(&settings.gnubg_path).map_err(native_error)?;
    let engine = Engine::new(pinned_path.clone());
    let version = engine.version().await.map_err(native_error)?;
    let id = Uuid::new_v4();
    let mut session = PlaySession {
        money_session_ended: false,
        game_results: None,
        study_report_id: None,
        study_report_version: None,
        review_game_no: None,
        game_learning_used: request.play_mode.unwrap_or(settings.default_play_mode)
            != PlayMode::Competition,
        library_match_id: None,
        learning_used: request.play_mode.unwrap_or(settings.default_play_mode)
            != PlayMode::Competition,
        play_mode: request.play_mode.unwrap_or(settings.default_play_mode),
        tutor_threshold: request.tutor_threshold.unwrap_or(settings.tutor_threshold),
        training_auto_analysis: request
            .training_auto_analysis
            .unwrap_or(settings.training_auto_analysis),
        analysis_plies: settings.analysis_plies,
        pending_decision: None,
        review: None,
        decisions: vec![],
        id,
        version: 0,
        human_player: request.human_player,
        human_name: if settings.player_name.is_empty() {
            "Human".into()
        } else {
            settings.player_name
        },
        match_length: request.match_length,
        bearoff_direction: settings.bearoff_direction,
        engine: PlayEngine {
            id: settings.play_engine,
            version,
            plies: settings.play_plies,
        },
        position: Position {
            id,
            match_uuid: id,
            ordinal: 0,
            game_no: 1,
            ply: 0,
            player: request.human_player,
            dice: vec![],
            board: vec![],
            position_id: String::new(),
            gnubg_match_id: String::new(),
            notation: String::new(),
        },
        phase: PlayPhase::OpeningRoll,
        can_double: false,
        legal_moves: vec![],
        history: vec![],
        scores: [0, 0],
        game_no: 1,
        winner: None,
        opening: None,
    };
    probe_engine(&session, &pinned_path)
        .await
        .map_err(native_error)?;
    stage_opening(&mut session, false)?;
    let sgf = String::new();
    let native_id = String::new();
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    library::lock_bot_database(&mut tx).await?;
    sqlx::query(
        "INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(sqlx::types::Json(&session))
    .bind(pinned_path)
    .bind(&sgf)
    .bind(native_id)
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    session.library_match_id = Some(library::sync(&mut tx, &session, &sgf, None).await?);
    sqlx::query("UPDATE play_sessions SET state=$2 WHERE id=$1")
        .bind(id)
        .bind(sqlx::types::Json(&session))
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(session)))
}
// Staging does not consume RNG or begin a native game. For later games the
// previous native checkpoint already contains NextTurn's Crawford transition.
fn stage_opening(session: &mut PlaySession, next: bool) -> anyhow::Result<()> {
    let key = base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(&session.position.gnubg_match_id)
        .unwrap_or_default();
    let crawford = next && key.first().is_some_and(|byte| byte & 0x80 != 0);
    if next {
        session.game_no += 1;
    }
    let board = crate::import::sgf::initial_board();
    session.position = Position {
        id: session.id,
        match_uuid: session.id,
        ordinal: session.version.try_into()?,
        game_no: session.game_no,
        ply: session.history.len().try_into()?,
        player: session.human_player,
        dice: vec![],
        board: board.to_vec(),
        position_id: crate::import::ids::position_id(&board, session.human_player),
        gnubg_match_id: crate::import::ids::match_id(
            session.match_length,
            session.scores,
            1,
            -1,
            crawford,
            session.match_length == 0,
            session.human_player,
            [0, 0],
        ),
        notation: String::new(),
    };
    session.phase = PlayPhase::OpeningRoll;
    session.opening = None;
    session.winner = None;
    session.legal_moves.clear();
    session.can_double = false;
    Ok(())
}
async fn probe_engine(session: &PlaySession, path: &str) -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    tokio::fs::write(directory.path().join("probe.py"),"import gnubg\nassert all(callable(getattr(gnubg,key,None)) for key in ['command','nextturn','board','posinfo','gnubgid','match','hint','navigate'])\nopen('ready.txt','w').write('ready')\n").await?;
    Engine::new(path.to_owned())
        .with_expected_version(Some(session.engine.version.clone()))
        .run_with_timeout(
            directory.path(),
            "load python probe.py\n",
            std::time::Duration::from_secs(20),
        )
        .await?;
    ensure!(
        tokio::fs::read_to_string(directory.path().join("ready.txt")).await? == "ready",
        "GNUbg embedded Python is unavailable"
    );
    Ok(())
}
// Internal checkpoints keep native names so old GNUbg's 31-byte name buffer
// cannot truncate a user's UTF-8 name during restore. Export full saved names
// by replacing root player properties outside escaped SGF values.
fn named_export(sgf: &str, human: &str, player: i32) -> String {
    let names = if player == 0 {
        [crate::import::sgf::escape_value(human), "GNUbg".into()]
    } else {
        ["GNUbg".into(), crate::import::sgf::escape_value(human)]
    };
    let bytes = sgf.as_bytes();
    let mut replacements = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let key = &sgf[start..i];
            while i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b']' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if let Some(index) = match key {
                    "PW" => Some(0),
                    "PB" => Some(1),
                    _ => None,
                } {
                    replacements.push((start, i, names[index].as_str()));
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    let mut result = sgf.to_owned();
    for (start, end, name) in replacements.into_iter().rev() {
        result.replace_range(start..end, name);
    }
    result
}
fn native_error(error: anyhow::Error) -> ApiError {
    tracing::warn!(error=?error,"GNUbg play action failed; no session changes saved");
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        format!("GNUbg could not complete this action; the saved session is unchanged. {error}"),
    )
}
fn validate_action<'a>(
    session: &'a PlaySession,
    request: &PlayActionRequest,
) -> ApiResult<(&'static str, Option<&'a str>)> {
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before playing".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    if !matches!(request.action, PlayAction::Move) && request.move_index.is_some() {
        return Err(ApiError::invalid("move is only valid for checker moves"));
    }
    if matches!(request.action, PlayAction::Resign) {
        if !request
            .resign_level
            .is_some_and(|level| (1..=3).contains(&level))
        {
            return Err(ApiError::invalid("Choose resignation level 1, 2, or 3"));
        }
    } else if request.resign_level.is_some() {
        return Err(ApiError::invalid(
            "resign_level is only valid for resignation",
        ));
    }
    if session.money_session_ended {
        return Err(ApiError::invalid("This money session has ended"));
    }
    let action = match (&session.phase, &request.action) {
        (PlayPhase::OpeningRoll, PlayAction::OpeningRoll) => "opening_roll",
        (PlayPhase::OpeningResult, PlayAction::StartOpeningMove) => "start_opening_move",
        (PlayPhase::Roll | PlayPhase::Move, PlayAction::Resign) => "resign",
        (PlayPhase::Roll, PlayAction::Roll) => "roll",
        (PlayPhase::Roll, PlayAction::Double) if session.can_double => "double",
        (PlayPhase::Move, PlayAction::Move) => {
            let selected = request
                .move_index
                .and_then(|index| session.legal_moves.get(index))
                .ok_or_else(|| ApiError::invalid("Choose a legal move index"))?;
            return Ok(("move", Some(&selected.notation)));
        }
        (PlayPhase::Move, PlayAction::Pass) if session.legal_moves.is_empty() => "pass",
        (PlayPhase::DoubleResponse, PlayAction::Take) => "take",
        (PlayPhase::DoubleResponse, PlayAction::Drop) => "drop",
        (PlayPhase::GameOver | PlayPhase::MatchOver, PlayAction::EndSession)
            if reports::money_finished(session) =>
        {
            "end_session"
        }
        (PlayPhase::GameOver, PlayAction::NextGame) => "next_game",
        (PlayPhase::MatchOver, PlayAction::NextGame) if session.match_length == 0 => "next_game",
        _ => {
            return Err(ApiError::invalid(
                "Action is not available at this decision",
            ));
        }
    };
    Ok((action, None))
}
#[utoipa::path(post,path="/api/play/sessions/{id}/mode",params(("id"=Uuid,Path)),request_body=PlayModeRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Playing")]
async fn change_mode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayModeRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before changing playing mode".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    if matches!(session.phase, PlayPhase::MatchOver) || reports::money_finished(&session) {
        return Err(ApiError::invalid(
            "Completed matches keep their playing mode",
        ));
    }
    if let Some(threshold) = request.tutor_threshold {
        if !threshold.is_finite() || threshold <= 0.0 || threshold > 1.0 {
            return Err(ApiError::invalid(
                "tutor_threshold must be greater than 0 and at most 1",
            ));
        }
        session.tutor_threshold = threshold;
    }
    session.learning_used |=
        session.play_mode != PlayMode::Competition || request.play_mode != PlayMode::Competition;
    session.game_learning_used |=
        session.play_mode != PlayMode::Competition || request.play_mode != PlayMode::Competition;
    session.play_mode = request.play_mode;
    if let Some(pending) = session.pending_decision.as_mut() {
        if session.play_mode == PlayMode::Coach
            || (session.play_mode == PlayMode::Training && session.training_auto_analysis)
        {
            pending.revealed = true;
        }
        pending.warning = session.play_mode == PlayMode::Tutor
            && pending.action.is_some()
            && pending
                .loss
                .is_some_and(|loss| loss >= session.tutor_threshold);
    }
    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}
#[utoipa::path(post,path="/api/play/sessions/{id}/actions",params(("id"=Uuid,Path)),request_body=PlayActionRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
async fn act(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayActionRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let (action, notation) = validate_action(&previous.state, &request)?;
    let mut working = previous.state.0.clone();
    working.learning_used |= working.play_mode != PlayMode::Competition;
    working.game_learning_used |= working.play_mode != PlayMode::Competition;
    if matches!(request.action, PlayAction::EndSession) {
        if working.review.is_none() {
            reports::build(&mut working, &previous.sgf, &previous.gnubg_path)
                .await
                .map_err(native_error)?;
        }
        working.phase = PlayPhase::MatchOver;
        working.money_session_ended = true;
        working.version += 1;
        return persist_session(
            &state,
            &working,
            &previous.sgf,
            &previous.native_id,
            request.version,
            false,
        )
        .await;
    }
    if let Some(pending) = &working.pending_decision {
        if let Some(choice) = &pending.action {
            if request.decision_id != Some(pending.id)
                || choice != action
                || pending.move_index != request.move_index
            {
                return Err(ApiError::invalid(
                    "Continue or retry the pending decision before playing",
                ));
            }
        } else if request.decision_id.is_some() {
            return Err(ApiError::invalid("Choose a decision before continuing"));
        }
    } else if request.decision_id.is_some() {
        return Err(ApiError::invalid("No pending decision to continue"));
    }
    let is_decision = matches!(
        request.action,
        PlayAction::Move
            | PlayAction::Pass
            | PlayAction::Double
            | PlayAction::Take
            | PlayAction::Drop
    ) || (matches!(request.action, PlayAction::Roll) && working.can_double);
    if is_decision
        && request.decision_id.is_none()
        && (working.play_mode != PlayMode::Competition || working.pending_decision.is_some())
    {
        let alternatives = if let Some(pending) = &working.pending_decision {
            pending.alternatives.clone()
        } else {
            analyze_decision(&working, &previous.gnubg_path)
                .await
                .map_err(native_error)?
        };
        stage_choice(&mut working, action, request.move_index, alternatives)?;
        let should_pause = working.play_mode == PlayMode::Coach
            || (working.play_mode == PlayMode::Training && working.training_auto_analysis)
            || working.pending_decision.as_ref().is_some_and(|d| d.warning);
        if should_pause {
            working.version += 1;
            return persist_session(
                &state,
                &working,
                &previous.sgf,
                &previous.native_id,
                request.version,
                false,
            )
            .await;
        }
    }
    // Native action is the sole commit point. A failed process leaves the
    // original pending checkpoint and its version untouched.
    let mut completed_report = None;
    let (mut session, sgf, native_id) = if matches!(request.action, PlayAction::NextGame) {
        if reports::money_finished(&working) {
            if working.review.is_none() {
                reports::build(&mut working, &previous.sgf, &previous.gnubg_path)
                    .await
                    .map_err(native_error)?;
            }
            completed_report = Some(working.clone());
            working.review = None;
            working.study_report_id = None;
            working.study_report_version = None;
            working.review_game_no = None;
            working.game_learning_used = working.play_mode != PlayMode::Competition;
        }
        working.version += 1;
        stage_opening(&mut working, true)?;
        (working, previous.sgf, previous.native_id)
    } else {
        let checkpoint = if previous.sgf.is_empty() {
            None
        } else {
            Some((previous.sgf.as_str(), previous.native_id.as_str()))
        };
        let (snapshot, sgf) = native(
            &working,
            &previous.gnubg_path,
            checkpoint,
            action,
            notation,
            request.resign_level,
        )
        .await
        .map_err(native_error)?;
        if snapshot.resignation_declined {
            return Err(ApiError::invalid(
                "GNUbg declined this resignation. Choose a higher level or continue playing; the saved game is unchanged",
            ));
        }
        working.version += 1;
        let native_id = apply_snapshot(&mut working, snapshot)?;
        (working, sgf, native_id)
    };
    // Resignation abandons a hint/retry checkpoint; its checker/cube choice
    // was never played and must not become a committed learning decision.
    if let Some(mut decision) = session.pending_decision.take()
        && !matches!(request.action, PlayAction::Resign)
    {
        decision.committed = true;
        session.decisions.push(decision);
    }
    persist_session_with_report(
        &state,
        &session,
        &sgf,
        &native_id,
        request.version,
        true,
        completed_report.as_ref(),
    )
    .await
}
async fn persist_session(
    state: &AppState,
    session: &PlaySession,
    sgf: &str,
    native_id: &str,
    version: i64,
    sync: bool,
) -> ApiResult<Json<PlaySession>> {
    persist_session_with_report(state, session, sgf, native_id, version, sync, None).await
}
#[allow(clippy::too_many_arguments)]
async fn persist_session_with_report(
    state: &AppState,
    session: &PlaySession,
    sgf: &str,
    native_id: &str,
    version: i64,
    sync: bool,
    completed_report: Option<&PlaySession>,
) -> ApiResult<Json<PlaySession>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let linked = library::lock_match(&mut tx, session.id).await?;
    let mut current = session.clone();
    current.library_match_id = linked;
    if let Some(report) = completed_report {
        let mut captured = report.clone();
        captured.library_match_id = linked;
        reports::archive(&mut tx, &captured, sgf).await?;
    }
    if current.review.is_some()
        && (matches!(current.phase, PlayPhase::MatchOver) || reports::money_finished(&current))
    {
        let (report_id, report_version) = reports::archive(&mut tx, &current, sgf).await?;
        current.study_report_id = Some(report_id);
        current.study_report_version = Some(report_version);
        current.review_game_no = (current.match_length == 0).then_some(current.game_no);
    }
    let updated=sqlx::query("UPDATE play_sessions SET state=$1,sgf=$2,native_id=$3,version=version+1,updated_at=now() WHERE id=$4 AND version=$5")
        .bind(sqlx::types::Json(&current)).bind(sgf).bind(native_id).bind(session.id).bind(version)
        .execute(&mut *tx).await.map_err(anyhow::Error::from)?.rows_affected();
    if updated != 1 {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before playing".into(),
        ));
    }
    if let Some(match_id) = linked {
        if sync {
            library::sync(&mut tx, &current, sgf, Some(match_id)).await?;
        } else {
            library::update_mode_metadata(&mut tx, &current, match_id).await?;
        }
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(present_with_results(current, sgf.to_owned()).await?))
}
#[utoipa::path(post,path="/api/play/sessions/{id}/feedback",params(("id"=Uuid,Path)),request_body=PlayFeedbackRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
async fn feedback(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayFeedbackRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before requesting feedback".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    if session.play_mode == PlayMode::Competition {
        return Err(ApiError::invalid(
            "Feedback is available after a competition match",
        ));
    }
    if !(matches!(session.phase, PlayPhase::Move | PlayPhase::DoubleResponse)
        || matches!(session.phase, PlayPhase::Roll) && session.can_double)
    {
        return Err(ApiError::invalid(
            "No human decision available for feedback",
        ));
    }
    if let Some(id) = request.decision_id
        && session.pending_decision.as_ref().is_none_or(|d| d.id != id)
    {
        return Err(ApiError::invalid(
            "Feedback decision does not match pending decision",
        ));
    }
    session.game_learning_used = true;
    match request.operation {
        FeedbackOperation::Hint => {
            if session.pending_decision.is_none() {
                let alternatives = analyze_decision(&session, &previous.gnubg_path)
                    .await
                    .map_err(native_error)?;
                let mut decision = feedback::empty_decision(alternatives);
                decision.game_no = session.game_no;
                decision.history_index = session.history.len();
                session.pending_decision = Some(decision);
            }
            let pending = session.pending_decision.as_mut().unwrap();
            pending.revealed = true;
            pending.assisted = true;
        }
        FeedbackOperation::Reveal => {
            let pending = session
                .pending_decision
                .as_mut()
                .ok_or_else(|| ApiError::invalid("No decision to reveal"))?;
            pending.revealed = true;
            pending.assisted = true;
        }
        FeedbackOperation::Retry => {
            let pending = session
                .pending_decision
                .as_mut()
                .ok_or_else(|| ApiError::invalid("No decision to retry"))?;
            if pending.action.is_none() {
                return Err(ApiError::invalid("Choose a move before retrying"));
            }
            pending.action = None;
            pending.move_index = None;
            pending.retry_count += 1;
            pending.assisted = true;
            pending.warning = false;
        }
    }
    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}
#[utoipa::path(post,path="/api/play/sessions/{id}/review",params(("id"=Uuid,Path)),request_body=PlayReviewRequest,responses((status=200,body=PlaySession),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Playing")]
async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<PlayReviewRequest>,
) -> ApiResult<Json<PlaySession>> {
    let previous = saved(&state, id).await?;
    let mut session = previous.state.0;
    if request.version != session.version {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Session changed; reload before reviewing".into(),
        ));
    }
    if request.version < 0 || request.version == i64::MAX {
        return Err(ApiError::invalid("Invalid session version"));
    }
    if !matches!(session.phase, PlayPhase::MatchOver) && !reports::money_finished(&session) {
        return Err(ApiError::invalid(
            "Complete the match or money game before reviewing",
        ));
    }
    if session.review.is_some() {
        return Ok(Json(present_with_results(session, previous.sgf).await?));
    }
    reports::build(&mut session, &previous.sgf, &previous.gnubg_path)
        .await
        .map_err(native_error)?;

    session.version += 1;
    persist_session(
        &state,
        &session,
        &previous.sgf,
        &previous.native_id,
        request.version,
        false,
    )
    .await
}
#[utoipa::path(get,path="/api/play/sessions/{id}/sgf",params(("id"=Uuid,Path)),responses((status=200,body=String,content_type="application/x-go-sgf"),(status=404,body=ErrorBody)),tag="Playing")]
async fn export_sgf(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<([(header::HeaderName, String); 2], String)> {
    let row = saved(&state, id).await?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "application/x-go-sgf; charset=utf-8".into(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"bgtutor-{id}.sgf\""),
            ),
        ],
        named_export(
            &if row.sgf.is_empty() {
                format!(
                    "(;FF[4]GM[6]MI[length:{}][game:0][ws:0][bs:0]PW[White]PB[Black]RU[Crawford])\n",
                    row.state.match_length
                )
            } else {
                row.sgf
            },
            &row.state.human_name,
            row.state.human_player,
        ),
    ))
}
async fn native(
    session: &PlaySession,
    path: &str,
    previous: Option<(&str, &str)>,
    action: &str,
    notation: Option<&str>,
    resign_level: Option<i32>,
) -> anyhow::Result<(NativeSnapshot, String)> {
    let directory = tempfile::tempdir()?;
    if let Some((sgf, _)) = previous {
        tokio::fs::write(directory.path().join("input.sgf"), sgf).await?;
    }
    let request = NativeRequest {
        new: previous.is_none(),
        human_player: session.human_player,
        match_length: session.match_length,
        plies: session.engine.plies,
        native_id: previous.map_or("", |(_, id)| id),
        action,
        notation,
        resign_level,
    };
    tokio::fs::write(
        directory.path().join("request.json"),
        serde_json::to_vec(&request)?,
    )
    .await?;
    tokio::fs::write(
        directory.path().join("adapter.py"),
        include_str!("play/native.py"),
    )
    .await?;
    let engine =
        Engine::new(path.to_owned()).with_expected_version(Some(session.engine.version.clone()));
    let output = engine
        .run_with_timeout(
            directory.path(),
            "load python adapter.py\n",
            std::time::Duration::from_secs(180),
        )
        .await?;
    let bytes = tokio::fs::read(directory.path().join("snapshot.json"))
        .await
        .context("GNUbg produced no complete snapshot")?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        "GNUbg snapshot exceeds limit"
    );
    let mut snapshot: NativeSnapshot = serde_json::from_slice(&bytes)?;
    if action == "opening_roll" {
        let expression = regex::Regex::new(r"(?m)^.* rolls ([1-6]), .* rolls ([1-6])\.\r?$")?;
        let rolls: Vec<[i32; 2]> = expression
            .captures_iter(&output)
            .map(|hit| [hit[1].parse().unwrap(), hit[2].parse().unwrap()])
            .collect();
        let last = *rolls
            .last()
            .context("GNUbg did not report its opening dice")?;
        ensure!(
            last[0] != last[1]
                && rolls[..rolls.len() - 1]
                    .iter()
                    .all(|pair| pair[0] == pair[1]),
            "Invalid native opening roll sequence"
        );
        let winner = i32::from(last[1] > last[0]);
        ensure!(
            snapshot.player == winner
                && snapshot.dice == [last[0].max(last[1]), last[0].min(last[1])],
            "Native opening roll result disagrees with checkpoint"
        );
        snapshot.opening = Some(OpeningRoll {
            white_die: last[0],
            black_die: last[1],
            winner,
            rolls,
        });
    }
    let sgf = tokio::fs::read_to_string(directory.path().join("output.sgf")).await?;
    ensure!(
        !sgf.is_empty() && sgf.len() <= 32 * 1024 * 1024,
        "GNUbg SGF missing or exceeds limit"
    );
    Ok((snapshot, sgf))
}
fn apply_snapshot(session: &mut PlaySession, snapshot: NativeSnapshot) -> anyhow::Result<String> {
    ensure!(
        snapshot.board.len() == 26,
        "Native board must have 26 points"
    );
    let (position_id, match_id) = snapshot
        .native_id
        .split_once(':')
        .context("GNUbg ID missing")?;
    session.position = Position {
        id: session.id,
        match_uuid: session.id,
        ordinal: session.version.try_into()?,
        game_no: snapshot.game_no,
        ply: snapshot.history.len().try_into()?,
        player: snapshot.player,
        dice: snapshot.dice,
        board: snapshot.board,
        position_id: position_id.into(),
        gnubg_match_id: match_id.into(),
        notation: String::new(),
    };
    session.phase = snapshot.phase;
    session.can_double = snapshot.can_double;
    session.legal_moves = snapshot.legal_moves;
    session.history = snapshot.history;
    session.scores = snapshot.scores;
    session.game_no = snapshot.game_no;
    session.winner = snapshot.winner;
    if snapshot.opening.is_some() {
        session.opening = snapshot.opening;
    }
    Ok(snapshot.native_id)
}
#[derive(OpenApi)]
#[openapi(
    paths(
        list_sessions,
        get_session,
        history_snapshots,
        create_session,
        act,
        change_mode,
        feedback,
        review,
        export_sgf
    ),
    components(schemas(
        PlaySession,
        PlayGameResult,
        crate::model::GameResult,
        crate::model::GameResultReason,
        PlayEngine,
        PlayMove,
        PlayStep,
        PlayHistory,
        PlayHistorySnapshot,
        PlayHistorySnapshots,
        PlayPhase,
        OpeningRoll,
        CreatePlaySession,
        PlayActionRequest,
        PlayModeRequest,
        PlayAction,
        PlayMode,
        PlayAlternative,
        PlayAttempt,
        PlayDecision,
        PlayFeedbackRequest,
        FeedbackOperation,
        PlayReview,
        PlayReviewRequest
    ))
)]
pub(crate) struct PlayDoc;
