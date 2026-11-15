//! Rank library checker positions by the number of checker-pips separating their boards.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::Position,
    move_assessments::after_play,
    play::PlayStep,
    practice::{EVALUATION, checker_turn},
    store,
};
use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::{get, post},
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::BinaryHeap;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod history;
pub(crate) use history::SimilarPracticeHistoryDoc;

const RESULT_LIMIT: usize = 50;
const REVIEW_BLOCKED: &str = "EXISTS(SELECT 1 FROM play_sessions live
    WHERE live.library_match_id=m.id AND coalesce(live.state->>'play_mode','competition')='competition'
      AND live.state->>'phase'<>'match_over'
      AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))";

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
struct SimilarQuery {
    /// Include other occurrences from the source match and its copies. Defaults to false.
    #[serde(default)]
    include_current_match: bool,
    /// Require the same unordered dice pair as the target.
    #[serde(default)]
    same_dice: bool,
    /// Require the same match length, score from the mover's perspective, and Crawford state.
    /// For a money game target, require a money game.
    #[serde(default)]
    same_score: bool,
    /// Require the same cube value and ownership relative to the player on roll.
    #[serde(default)]
    same_cube: bool,
}

#[derive(Serialize, ToSchema, FromRow)]
struct SimilarHit {
    #[sqlx(flatten)]
    position: Position,
    match_name: String,
    white: String,
    black: String,
    /// Minimum checker-pips needed to transform one arrangement into the other.
    distance: i32,
}

#[derive(Serialize, ToSchema)]
struct SimilarPositions {
    /// Up to 50 checker occurrences, nearest first.
    items: Vec<SimilarHit>,
}

#[derive(Serialize, ToSchema)]
struct CheckerPracticeAnswer {
    best: String,
    engine: String,
    depth: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CheckerPracticeMove {
    /// Complete ordered checker steps in board coordinates. An empty list is a forced pass.
    #[schema(max_items = 4)]
    steps: Vec<PlayStep>,
}

#[derive(Serialize, ToSchema)]
struct CheckerPracticeScore {
    #[serde(flatten)]
    answer: CheckerPracticeAnswer,
    played: String,
    best_equity: f64,
    played_equity: f64,
    /// Best minus played normalized cubeful equity, from the mover's perspective.
    equity_loss: f64,
}

#[derive(FromRow)]
struct Target {
    board: Vec<i32>,
    player: i32,
    source_match: Uuid,
    source_hash: Option<String>,
    review_blocked: bool,
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/positions/{id}/similar", get(search))
        .route("/api/positions/{id}/practice-answer", post(reveal))
        .route("/api/positions/{id}/practice-score", post(score))
        .merge(history::routes())
}

#[utoipa::path(get, path="/api/positions/{id}/similar",
    params(("id"=Uuid, Path, description="Target checker occurrence"), SimilarQuery),
    responses((status=200,body=SimilarPositions),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),
    tag="Positions")]
async fn search(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<SimilarQuery>, QueryRejection>,
) -> ApiResult<Json<SimilarPositions>> {
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let target: Target = sqlx::query_as(&format!(
        "SELECT p.board,p.player,coalesce(m.source_match_id,m.id) AS source_match,
                CASE WHEN NOT m.collection_only THEN m.sha256 END AS source_hash,
                {REVIEW_BLOCKED} AS review_blocked
         FROM positions p JOIN matches m ON m.id=p.match_uuid WHERE p.id=$1"
    ))
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    if target.review_blocked {
        return Err(ApiError::invalid(
            "Complete the competition match before requesting analysis",
        ));
    }
    let origin = profile(&target.board, target.player)
        .ok_or_else(|| ApiError::invalid("Target position has an invalid checker arrangement"))?;

    // Read only board vectors while ranking. Keep the closest 50 in memory and
    // load their full decision context from the same snapshot afterward.
    let mut closest = BinaryHeap::with_capacity(RESULT_LIMIT + 1);
    {
        let statement = format!(
            "SELECT p.id,p.board,p.player
             FROM positions p JOIN matches m ON m.id=p.match_uuid
             JOIN positions origin ON origin.id=$1
             JOIN matches origin_match ON origin_match.id=origin.match_uuid
             WHERE p.id<>$1
               AND ($2 OR (coalesce(m.source_match_id,m.id)<>$3
                            AND ($4::text IS NULL OR m.sha256<>$4)))
               AND (NOT $5 OR (
                    least(p.dice[1],p.dice[2])=least(origin.dice[1],origin.dice[2])
                    AND greatest(p.dice[1],p.dice[2])=greatest(origin.dice[1],origin.dice[2])
               ))
               AND (NOT $6 OR (
                    m.match_length=origin_match.match_length AND (
                        m.match_length=0 OR (
                            p.source_crawford=origin.source_crawford
                            AND (p.source_white_score,p.source_black_score)=(
                                CASE WHEN p.player=origin.player THEN origin.source_white_score
                                     ELSE origin.source_black_score END,
                                CASE WHEN p.player=origin.player THEN origin.source_black_score
                                     ELSE origin.source_white_score END
                            )
                        )
                    )
               ))
               AND (NOT $7 OR (
                    p.source_cube_value=origin.source_cube_value
                    AND p.source_cube_owner=CASE
                        WHEN p.player=origin.player OR origin.source_cube_owner=-1
                            THEN origin.source_cube_owner
                        ELSE 1-origin.source_cube_owner
                    END
               ))
               AND NOT {REVIEW_BLOCKED}"
        );
        let mut candidates = sqlx::query_as::<_, (Uuid, Vec<i32>, i32)>(&statement)
            .bind(id)
            .bind(query.include_current_match)
            .bind(target.source_match)
            .bind(target.source_hash)
            .bind(query.same_dice)
            .bind(query.same_score)
            .bind(query.same_cube)
            .fetch(&mut *tx);
        while let Some((id, board, player)) =
            candidates.try_next().await.map_err(anyhow::Error::from)?
        {
            let Some(candidate) = profile(&board, player) else {
                continue;
            };
            let rank = (distance(&origin, &candidate), id);
            if closest.len() < RESULT_LIMIT {
                closest.push(rank);
            } else if closest.peek().is_some_and(|worst| rank < *worst) {
                *closest.peek_mut().expect("full heap") = rank;
            }
        }
    }
    let (distances, ids): (Vec<i32>, Vec<Uuid>) = closest.into_sorted_vec().into_iter().unzip();
    let items = sqlx::query_as(
        "SELECT p.*,ranked.distance,coalesce(nullif(m.display_name,''),m.filename) AS match_name,
                m.white,m.black
         FROM unnest($1::uuid[],$2::integer[]) WITH ORDINALITY AS ranked(id,distance,ordinal)
         JOIN positions p ON p.id=ranked.id JOIN matches m ON m.id=p.match_uuid
         ORDER BY ranked.ordinal",
    )
    .bind(ids)
    .bind(distances)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(SimilarPositions { items }))
}

#[utoipa::path(post, path="/api/positions/{id}/practice-answer",
    params(("id"=Uuid, Path, description="Checker occurrence to reveal")),
    responses((status=200,body=CheckerPracticeAnswer),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=500,body=ErrorBody)),
    tag="Positions")]
async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CheckerPracticeAnswer>> {
    let position = practice_position(&state, id).await?;
    let answer = checker_answer(&state, &position).await?;
    crate::play::require_review_available(&state.pool, position.match_uuid).await?;
    Ok(Json(answer))
}

async fn practice_position(state: &AppState, id: Uuid) -> ApiResult<Position> {
    let position = store::get_position(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_review_available(&state.pool, position.match_uuid).await?;
    if !matches!(position.dice.as_slice(), [1..=6, 1..=6]) {
        return Err(ApiError::invalid("Practice requires a checker roll"));
    }
    Ok(position)
}

#[utoipa::path(post, path="/api/positions/{id}/practice-score",
    params(("id"=Uuid, Path, description="Checker occurrence to score")),
    request_body=CheckerPracticeMove,
    responses((status=200,body=CheckerPracticeScore),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=500,body=ErrorBody)),
    tag="Positions")]
async fn score(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    input: Result<Json<CheckerPracticeMove>, JsonRejection>,
) -> ApiResult<Json<CheckerPracticeScore>> {
    let Json(input) = input.map_err(|error| ApiError::invalid(error.body_text()))?;
    let position = practice_position(&state, id).await?;
    let after = checker_turn(&position, &input.steps)
        .ok_or_else(|| ApiError::invalid("Play a complete legal checker move"))?;

    let _evaluation = EVALUATION.lock().await;
    crate::play::require_review_available(&state.pool, position.match_uuid).await?;
    let settings = crate::settings::load(&state).await?;
    let analysis = settings
        .gnubg()
        .analyze_checker_choices(
            &position.position_id,
            &position.gnubg_match_id,
            settings.analysis_plies,
            input.steps.is_empty(),
        )
        .await?;
    let best = analysis
        .candidates
        .first()
        .ok_or_else(|| anyhow::anyhow!("GNUbg returned no checker choice"))?;
    let played = analysis
        .candidates
        .iter()
        .find(|candidate| after_play(&position, &candidate.notation) == Some(after))
        .ok_or_else(|| anyhow::anyhow!("GNUbg did not evaluate the submitted checker choice"))?;
    crate::play::require_review_available(&state.pool, position.match_uuid).await?;
    Ok(Json(CheckerPracticeScore {
        answer: CheckerPracticeAnswer {
            best: best.notation.clone(),
            engine: analysis.engine,
            depth: Some(format!("{}-ply", analysis.plies)),
        },
        played: played.notation.clone(),
        best_equity: best.equity,
        played_equity: played.equity,
        equity_loss: crate::match_analysis::normalized_loss(played.equity, best.equity),
    }))
}

async fn checker_answer(state: &AppState, position: &Position) -> ApiResult<CheckerPracticeAnswer> {
    let Some((best, engine, depth)) =
        crate::learning::saved_checker_move(&state.pool, position).await?
    else {
        return Err(ApiError::invalid(
            "The selected analysis has no supported checker answer for this position",
        ));
    };
    Ok(CheckerPracticeAnswer {
        best,
        engine,
        depth,
    })
}

type CheckerProfile = [[i32; 25]; 2];

fn profile(board: &[i32], player: i32) -> Option<CheckerProfile> {
    if board.len() != 26
        || !matches!(player, 0 | 1)
        || board[0] > 0
        || board[25] < 0
        || board.iter().any(|n| !(-15..=15).contains(n))
    {
        return None;
    }
    let mut profiles = [[0; 25]; 2];
    let mut remaining = [0; 2];
    // Each side's bar is point 25, and its borne-off checkers are at 0.
    // Count checkers across each boundary from 25 down to 1. Missing
    // checkers at point 0 are implicit, so bearoffs contribute correctly.
    for point in (1..=25).rev() {
        remaining[0] += board[point].max(0);
        remaining[1] += (-board[25 - point]).max(0);
        profiles[0][point - 1] = remaining[0];
        profiles[1][point - 1] = remaining[1];
    }
    if remaining.iter().any(|count| *count > 15) {
        return None;
    }
    if player == 1 {
        profiles.swap(0, 1);
    }
    Some(profiles)
}

fn distance(first: &CheckerProfile, second: &CheckerProfile) -> i32 {
    first
        .iter()
        .flatten()
        .zip(second.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .sum()
}

#[derive(OpenApi)]
#[openapi(
    paths(search, reveal, score),
    components(schemas(
        SimilarHit,
        SimilarPositions,
        CheckerPracticeAnswer,
        CheckerPracticeMove,
        CheckerPracticeScore
    ))
)]
pub(crate) struct SimilarPositionsDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_bar_and_bearoff_are_measured_in_checker_pips() {
        let mut board = [0; 26];
        board[25] = 1;
        board[1] = 1;
        board[17] = -2;
        let origin = profile(&board, 0).unwrap();
        let flipped: [i32; 26] = std::array::from_fn(|point| -board[25 - point]);
        assert_eq!(distance(&origin, &profile(&flipped, 1).unwrap()), 0);
        board[25] = 0;
        board[23] = 1;
        board[1] = 0;
        board[17] = -1;
        board[18] = -1;
        assert_eq!(distance(&origin, &profile(&board, 0).unwrap()), 4);
    }
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn ranks_real_positions_and_filters_their_source_and_context(pool: sqlx::PgPool) {
        use crate::import::ids::{match_id, position_id};
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use serde_json::{Value, json};
        use tower::ServiceExt;

        let default_db = Uuid::from_u128(1);
        let copy_db = Uuid::from_u128(10);
        sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Copies')")
            .bind(copy_db)
            .execute(&pool)
            .await
            .unwrap();
        for (id, hash, database) in [
            (100, "source", default_db),
            (101, "source", copy_db),
            (102, "other", default_db),
            (103, "live", default_db),
        ] {
            sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,database_id) VALUES($1,'similar.sgf','sgf',$2,'White','Black',7,1,''::bytea,'',$3)")
                .bind(Uuid::from_u128(id)).bind(hash).bind(database)
                .execute(&pool).await.unwrap();
        }

        let mut board = [0; 26];
        board[12] = 2;
        board[18] = -2;
        let flipped = std::array::from_fn(|point| -board[25 - point]);
        let mut nearby = board;
        nearby[12] -= 1;
        nearby[11] += 1;
        // Each mismatch differs from the source in just one optional filter.
        for (id, source, ordinal, player, board, dice, score, owner) in [
            (1, 100, 0, 0, board, [3, 1], [2, 4], 0),
            (2, 100, 1, 0, board, [3, 1], [2, 4], 0),
            (3, 101, 0, 0, board, [3, 1], [2, 4], 0),
            (4, 102, 0, 1, flipped, [1, 3], [4, 2], 1),
            (5, 102, 1, 0, nearby, [3, 1], [2, 4], 0),
            (6, 102, 2, 0, board, [6, 5], [2, 4], 0),
            (7, 102, 3, 0, board, [3, 1], [2, 5], 0),
            (8, 102, 4, 0, board, [3, 1], [2, 4], 1),
            (9, 103, 0, 0, board, [3, 1], [2, 4], 0),
        ] {
            sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,$3,1,$3+1,$4,$5,$6,$7,$8,'')")
                .bind(Uuid::from_u128(id)).bind(Uuid::from_u128(source)).bind(ordinal)
                .bind(player).bind(dice.as_slice()).bind(board.as_slice())
                .bind(position_id(&board, player))
                .bind(match_id(7, score, 2, owner, false, false, player, dice))
                .execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id) VALUES($1,$2,'unused','','native',$3)")
            .bind(Uuid::new_v4())
            .bind(json!({"play_mode":"competition","phase":"move","match_length":7}))
            .bind(Uuid::from_u128(103))
            .execute(&pool).await.unwrap();

        let app = crate::api::router(AppState::new(
            pool,
            crate::engine::Engine::new("unused".into()),
        ));
        let read = |id, query: &str| {
            let app = app.clone();
            let uri = format!("/api/positions/{}/similar{query}", Uuid::from_u128(id));
            async move {
                let response = app
                    .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                let status = response.status().as_u16();
                let body: Value = serde_json::from_slice(
                    &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                )
                .unwrap();
                (status, body)
            }
        };
        let hits = |body: &Value| {
            body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hit| {
                    let id = hit["position"]["id"]
                        .as_str()
                        .unwrap()
                        .parse::<Uuid>()
                        .unwrap();
                    (id.as_u128(), hit["distance"].as_i64().unwrap())
                })
                .collect::<Vec<_>>()
        };
        let (status, body) = read(1, "").await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(hits(&body), [(4, 0), (6, 0), (7, 0), (8, 0), (5, 1)]);

        let filters = "?same_dice=true&same_score=true&same_cube=true";
        let (status, body) = read(1, filters).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(hits(&body), [(4, 0), (5, 1)]);

        let (status, body) = read(1, &format!("{filters}&include_current_match=true")).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(hits(&body), [(2, 0), (3, 0), (4, 0), (5, 1)]);
        assert_eq!(read(9, "").await.0, 422);
    }
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL and GNUbg"]
    async fn practice_scores_a_non_best_legal_move_at_the_configured_depth(pool: sqlx::PgPool) {
        use axum::{
            body::{Body, to_bytes},
            http::Request,
        };
        use serde_json::{Value, json};
        use tower::ServiceExt;

        let sgf = "(;GM[6]FF[4]PW[Practice]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "practice.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let id: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
            .bind(imported.r#match.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let binary = std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into());
        let state = AppState::new(pool.clone(), crate::engine::Engine::new(binary));
        let app = crate::api::router(state.clone());
        let post = |route: &str, body: Value| {
            let app = app.clone();
            let uri = format!("/api/positions/{id}/{route}");
            async move {
                let response = app
                    .oneshot(
                        Request::post(uri)
                            .header("content-type", "application/json")
                            .body(Body::from(body.to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = response.status().as_u16();
                let body: Value = serde_json::from_slice(
                    &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                )
                .unwrap();
                (status, body)
            }
        };
        let (status, error) = post("practice-score", json!({"steps":[{"from":24,"to":21}]})).await;
        assert_eq!(status, 422, "{error}");
        let (status, value) = post(
            "practice-score",
            json!({"steps":[{"from":24,"to":21},{"from":24,"to":23}]}),
        )
        .await;
        assert_eq!(status, 200, "{value}");
        assert_eq!(value["depth"], "2-ply");
        assert_ne!(value["best"], value["played"]);
        let loss = value["equity_loss"].as_f64().unwrap();
        assert!(loss > 0.0, "{value}");
        assert!(
            (value["best_equity"].as_f64().unwrap()
                - value["played_equity"].as_f64().unwrap()
                - loss)
                .abs()
                < 1e-6
        );
        let (status, unavailable) = post("practice-answer", Value::Null).await;
        assert_eq!(status, 422, "{unavailable}");
        assert_eq!(
            unavailable["error"],
            "The selected analysis has no supported checker answer for this position"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "scoring and revealing must not silently create a persisted analysis"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM analysis_selections")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0,
            "scoring and revealing must not silently choose a saved analysis"
        );
        let job = store::create_job(&pool, id, "analysis", 2).await.unwrap();
        assert!(crate::jobs::run_one(&state).await.unwrap());
        let completed = store::get_job(&pool, job.id).await.unwrap().unwrap();
        assert_eq!(completed.status, "completed", "{:?}", completed.error);
        let newer = store::create_job(&pool, id, "analysis", 0).await.unwrap();
        assert!(crate::jobs::run_one(&state).await.unwrap());
        let newer = store::get_job(&pool, newer.id).await.unwrap().unwrap();
        assert_eq!(newer.status, "completed", "{:?}", newer.error);
        let selected_id = format!("job:{}", job.id);
        let response = app
            .clone()
            .oneshot(
                Request::put(format!(
                    "/api/matches/{}/analysis-selection",
                    imported.r#match.id
                ))
                .header("content-type", "application/json")
                .body(Body::from(json!({"analysis_id":selected_id}).to_string()))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        let (status, answer) = post("practice-answer", Value::Null).await;
        assert_eq!(status, 200, "{answer}");
        assert_eq!(answer["best"], value["best"]);
        assert_eq!(answer["depth"], "2-ply", "{answer}");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT analysis_id FROM analysis_selections")
                .fetch_one(&pool)
                .await
                .unwrap(),
            selected_id
        );
        for saved in [completed, newer] {
            assert_eq!(
                store::get_job(&pool, saved.id)
                    .await
                    .unwrap()
                    .unwrap()
                    .result,
                saved.result,
                "revealing must preserve both saved analyses"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM jobs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2,
            "revealing the saved answer must reuse the existing analysis"
        );
    }
}
