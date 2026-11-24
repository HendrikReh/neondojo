use super::*;
use crate::{import::ids, practice};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct PipAnswer {
    card_id: Uuid,
    counts: [i32; 2],
}

#[derive(Serialize, ToSchema)]
pub(super) struct PipResult {
    counts: [i32; 2],
    bar_pips: [i32; 2],
    correct: [bool; 2],
}

#[derive(Serialize, ToSchema)]
pub(super) struct ScoreQuestion {
    index: usize,
    card: CardQuestion,
}

#[derive(Serialize, ToSchema)]
pub(super) struct ScorePractice {
    id: Uuid,
    questions: Vec<ScoreQuestion>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ScoreAnswer {
    index: usize,
    practice_id: Uuid,
    answer: PracticeAnswer,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/study/card-pips", post(pips))
        .route(
            "/api/cards/{id}/score-variants",
            post(score_questions).put(score_answer),
        )
        .route("/api/cards/{id}/preview", get(preview))
}

async fn card(state: &AppState, id: Uuid) -> ApiResult<CardRow> {
    sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

#[utoipa::path(post,path="/api/study/card-pips",request_body=PipAnswer,responses((status=200,body=PipResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn pips(
    State(state): State<AppState>,
    Json(body): Json<PipAnswer>,
) -> ApiResult<Json<PipResult>> {
    if body.counts.iter().any(|count| !(0..=375).contains(count)) {
        return Err(ApiError::invalid("Pip counts must be between 0 and 375"));
    }
    let row = card(&state, body.card_id).await?;
    let Some(content) = row.decision() else {
        return Err(ApiError::invalid("This Anki card has no practice board"));
    };
    crate::play::require_review_available(&state.pool, content.source.match_id).await?;
    let board: [i32; 26] = content
        .position
        .board
        .as_slice()
        .try_into()
        .map_err(|_| ApiError::invalid("Invalid board"))?;
    let (counts, bar_pips) = crate::game_plan::absolute_pip_counts(&board);
    Ok(Json(PipResult {
        counts,
        bar_pips,
        correct: std::array::from_fn(|i| counts[i] == body.counts[i]),
    }))
}

fn cube_content(row: &CardRow) -> ApiResult<&CardContent> {
    row.decision()
        .ok_or_else(|| ApiError::invalid("This Anki card has no cube practice board"))
}

fn score_positions(content: &CardContent) -> ApiResult<Vec<Position>> {
    let response = content.kind == "cube_response";
    if !response && content.kind != "cube_offer" {
        return Err(ApiError::invalid(
            "Select a cube card to practice score variants",
        ));
    }
    let base = &content.position;
    let context = practice::cube_context(base, response)
        .map_err(|_| ApiError::invalid("This card has no valid cube context"))?;
    let match_id = ids::MatchId::decode(&context.match_id)
        .ok_or_else(|| ApiError::invalid("Invalid cube context"))?;
    let cube = 1 << match_id.get(ids::Field::CUBE_LOG2);
    let owner = match_id.get(ids::Field::CUBE_OWNER);
    let away = (cube + 1).max(3);
    let length = away + 3;
    let mut positions = Vec::new();
    for (length, scores) in [(0, [0, 0]), (length, [3, 1]), (length, [1, 3])] {
        let mut position = base.clone();
        // The responder-oriented representation keeps the pre-offer cube and
        // the original decision-maker's checker orientation.
        position.gnubg_match_id = ids::match_id(
            length,
            scores,
            cube,
            owner,
            false,
            match_id.get(ids::Field::NO_JACOBY) == 0,
            base.player,
            [0, 0],
        );
        position.dice = vec![0, 0];
        position.notation.clear();
        if practice::cube_context(&position, response).is_ok()
            && (response
                || crate::match_analysis::cube_analysis_available(
                    &position.board,
                    &position.gnubg_match_id,
                    position.player,
                )
                .unwrap_or(false))
        {
            positions.push(position);
        }
    }
    if positions.is_empty() {
        return Err(ApiError::invalid("No legal score variants are available"));
    }
    Ok(positions)
}

#[utoipa::path(post,path="/api/cards/{id}/score-variants",params(("id"=Uuid,Path)),responses((status=200,body=ScorePractice),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn score_questions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ScorePractice>> {
    let row = card(&state, id).await?;
    let content = cube_content(&row)?;
    crate::play::require_review_available(&state.pool, content.source.match_id).await?;
    let question = row.question();
    let questions = score_positions(content)?
        .into_iter()
        .enumerate()
        .map(|(index, mut position)| {
            position.id = row.id;
            ScoreQuestion {
                index,
                card: CardQuestion {
                    position: Some(position),
                    ..question.clone()
                },
            }
        })
        .collect();
    let (plies, engine) = crate::settings::snapshot_for_practice(&state).await?;
    let practice_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("DELETE FROM score_practice_profiles WHERE expires_at<=now()")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    load_attempt_card(&mut *tx, id, row.version, true).await?;
    sqlx::query("INSERT INTO score_practice_profiles(id,card_id,card_version,plies,engine) VALUES($1,$2,$3,$4,$5)")
        .bind(practice_id).bind(id).bind(row.version).bind(plies).bind(sqlx::types::Json(engine))
        .execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(ScorePractice {
        id: practice_id,
        questions,
    }))
}

#[utoipa::path(put,path="/api/cards/{id}/score-variants",params(("id"=Uuid,Path)),request_body=ScoreAnswer,responses((status=200,body=PracticeEvaluation),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn score_answer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ScoreAnswer>,
) -> ApiResult<Json<PracticeEvaluation>> {
    let row = card(&state, id).await?;
    let (version, plies, sqlx::types::Json(engine)): (i64, i32, sqlx::types::Json<crate::settings::EngineSnapshot>) =
        sqlx::query_as("SELECT card_version,plies,engine FROM score_practice_profiles WHERE id=$1 AND card_id=$2 AND expires_at>now()")
            .bind(body.practice_id).bind(id).fetch_optional(&state.pool).await.map_err(anyhow::Error::from)?.ok_or_else(ApiError::missing)?;
    if row.version != version {
        return Err(conflict("Card changed; reload its score variants"));
    }
    let content = cube_content(&row)?;
    crate::play::require_review_available(&state.pool, content.source.match_id).await?;
    let position = score_positions(content)?
        .into_iter()
        .nth(body.index)
        .ok_or_else(|| ApiError::invalid("Invalid score variant"))?;
    let result = evaluation::evaluate_with_snapshot(
        &state,
        &content.kind,
        &position,
        &body.answer,
        (plies, engine),
    )
    .await?;
    crate::play::require_review_available(&state.pool, content.source.match_id).await?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/cards/{id}/preview",params(("id"=Uuid,Path)),responses((status=200,body=CardAnswer),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CardAnswer>> {
    let mut connection = state.pool.acquire().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if let Some(content) = row.decision() {
        crate::play::require_review_available_on(&mut *connection, content.source.match_id).await?;
    }
    Ok(Json(row.answer(&mut connection).await?))
}
