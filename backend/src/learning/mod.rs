//! Shared cards power native Anki exports and the integrated FSRS trainer.
pub mod anki_flashcards;
mod anki_import;
mod apkg;
mod content;
mod management;
mod rounds;
mod scheduler;
mod search;
mod suggestions;
mod topics;
mod transfer;
use crate::practice::evaluation::{self, PracticeAnswer, PracticeEvaluation, SavedEvaluation};
pub(crate) use content::saved_checker_move;
pub use search::{
    CardBrowseContext, CardBrowseItem, CardDeck, CardKind, CardQuery, CardReviewState,
};

use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::Candidate,
    match_analysis::Decision,
    model::Position,
};
use axum::{
    Json, Router,
    extract::rejection::{PathRejection, QueryRejection},
    extract::{FromRequest, Path, Query, Request, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const MAX_CARDS: i64 = 1000;

// Axum's default JSON rejection is plain text; all learning API errors use ErrorBody.
struct LearningJson<T>(T);
impl<S, T> FromRequest<S> for LearningJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| ApiError::invalid(error.body_text()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Deck {
    pub id: Uuid,
    pub name: String,
    /// Changes when the deck is renamed or gains memberships.
    #[schema(minimum = 0)]
    pub version: i64,
    pub card_count: i64,
    pub due_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
struct StudyDeck {
    id: Uuid,
    name: String,
}

#[derive(Serialize, ToSchema)]
struct LearningSummary {
    card_count: i64,
    due_count: i64,
    next_due_at: Option<DateTime<Utc>>,
    next_deck: Option<StudyDeck>,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateDeck {
    pub name: String,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameDeck {
    pub name: String,
    #[schema(minimum = 0)]
    pub expected_version: i64,
}
#[derive(Debug, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct DeleteDeckQuery {
    #[param(minimum = 0)]
    pub expected_version: i64,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddCards {
    /// Restrict a match analysis source to this source game.
    pub game_no: Option<i32>,
    pub position_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub min_loss: Option<f64>,
    pub player: Option<i32>,
    #[serde(default)]
    pub tags: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddCardsResult {
    pub added: i64,
    pub updated: i64,
    pub total: i64,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddPositions {
    /// Saved position UUIDs in selection order. Every occurrence receives an outcome.
    #[schema(min_items = 1, max_items = 200)]
    pub position_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionAddStatus {
    Added,
    Updated,
    Duplicate,
    AnalysisRequired,
    NotFound,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PositionAddItem {
    pub position_id: Uuid,
    pub status: PositionAddStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddPositionsResult {
    pub added: i64,
    pub updated: i64,
    pub total: i64,
    /// Outcomes in the same order as the request, including repeated UUIDs.
    pub items: Vec<PositionAddItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardSource {
    pub kind: String,
    pub job_id: Uuid,
    pub match_id: Uuid,
    pub position_id: Option<Uuid>,
    pub game_no: i32,
    pub decision_no: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardSolution {
    pub engine: String,
    pub plies: i32,
    pub candidates: Vec<Candidate>,
    pub decision: Option<Decision>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardAnswer {
    pub solution: Option<CardSolution>,
    pub notes: String,
    pub tags: Vec<String>,
    pub played: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardQuestion {
    pub id: Uuid,
    pub version: i64,
    /// checker, cube_offer, cube_response, or anki.
    pub kind: String,
    pub position: Option<Position>,
    /// Absolute colors: White, Black. Empty for Anki cards.
    pub players: [String; 2],
    pub prompt: String,
    pub source: Option<CardSource>,
    pub due_at: DateTime<Utc>,
    pub review_count: i32,
    pub tags_version: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CardPage {
    pub items: Vec<CardBrowseItem>,
    pub next_cursor: Option<Uuid>,
    /// Matching cards before applying the cursor; a shared card appears once.
    pub total: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReviewCounts {
    pub total: i64,
    pub due: i64,
    pub new: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NextCard {
    pub card: Option<CardQuestion>,
    pub counts: ReviewCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_attempt: Option<AttemptedCard>,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct RevealCard {
    pub expected_version: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RatingIntervals {
    /// Seconds until due for each rating; Again and initial Hard use learning steps.
    pub again: i64,
    pub hard: i64,
    pub good: i64,
    pub easy: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RevealedCard {
    pub card_id: Uuid,
    pub version: i64,
    pub answer: CardAnswer,
    pub intervals: RatingIntervals,
    pub session_id: Uuid,
}
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CardAttemptRequest {
    expected_version: i64,
    request_id: Uuid,
    answer: PracticeAnswer,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AttemptedCard {
    revealed: RevealedCard,
    submitted: PracticeAnswer,
    evaluation: PracticeEvaluation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}
impl Rating {
    fn as_str(self) -> &'static str {
        match self {
            Self::Again => "again",
            Self::Hard => "hard",
            Self::Good => "good",
            Self::Easy => "easy",
        }
    }
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReviewCard {
    pub session_id: Uuid,
    pub request_id: Uuid,
    pub rating: Rating,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReviewResult {
    pub card_id: Uuid,
    pub version: i64,
    pub due_at: DateTime<Utc>,
    pub interval_seconds: i64,
    pub stability: f64,
    pub difficulty: f64,
    pub review_count: i32,
}
#[derive(Debug, Deserialize, IntoParams)]
pub struct ExportQuery {
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CardContent {
    kind: String,
    position: Position,
    players: [String; 2],
    prompt: String,
    source: CardSource,
    solution: CardSolution,
    played: String,
}
#[derive(Debug, FromRow)]
struct CardRow {
    id: Uuid,
    version: i64,
    payload: serde_json::Value,
    due_at: DateTime<Utc>,
    review_count: i32,
    stability: Option<f64>,
    difficulty: Option<f64>,
    last_review: Option<DateTime<Utc>>,
    tags: Vec<String>,
    tags_version: i64,
}
impl CardRow {
    fn content(&self) -> anyhow::Result<CardContent> {
        Ok(serde_json::from_value(self.payload.clone())?)
    }
    fn is_anki(&self) -> bool {
        self.payload["kind"] == "anki"
    }

    fn question(&self) -> anyhow::Result<CardQuestion> {
        let (kind, position, players, prompt, source) = if self.is_anki() {
            let card: anki_flashcards::AnkiCard = serde_json::from_value(self.payload.clone())?;
            (
                "anki".into(),
                None,
                [String::new(), String::new()],
                card.prompt,
                None,
            )
        } else {
            let c = self.content()?;
            let mut position = c.position;
            position.id = self.id;
            position.notation.clear();
            if matches!(c.kind.as_str(), "cube_offer" | "cube_response") {
                if let Some(id) = crate::import::ids::hide_dice(&position.gnubg_match_id) {
                    position.gnubg_match_id = id;
                }
                position.dice = vec![0, 0];
            }
            (c.kind, Some(position), c.players, c.prompt, Some(c.source))
        };
        Ok(CardQuestion {
            id: self.id,
            version: self.version,
            kind,
            position,
            players,
            prompt,
            source,
            due_at: self.due_at,
            review_count: self.review_count,
            tags_version: self.tags_version,
        })
    }

    async fn answer(&self, connection: &mut sqlx::PgConnection) -> anyhow::Result<CardAnswer> {
        if self.is_anki() {
            return Ok(CardAnswer {
                solution: None,
                notes: String::new(),
                tags: self.tags.clone(),
                played: String::new(),
            });
        }
        content::answer_on_connection(connection, self.id, &self.content()?).await
    }
}

#[derive(FromRow)]
struct AttemptRow {
    session_id: Uuid,
    request_id: Uuid,
    #[sqlx(json)]
    submitted: PracticeAnswer,
    #[sqlx(json)]
    evaluation: PracticeEvaluation,
}

#[derive(FromRow)]
struct DueCardRow {
    #[sqlx(flatten)]
    card: CardRow,
    #[sqlx(json(nullable))]
    active_attempt: Option<AttemptData>,
}

#[derive(Deserialize)]
struct AttemptData {
    session_id: Uuid,
    submitted: PracticeAnswer,
    evaluation: PracticeEvaluation,
}

impl AttemptRow {
    fn into_data(self) -> AttemptData {
        AttemptData {
            session_id: self.session_id,
            submitted: self.submitted,
            evaluation: self.evaluation,
        }
    }

    fn confirm_request(&self, request: &CardAttemptRequest) -> ApiResult<()> {
        if self.request_id != request.request_id
            || serde_json::to_value(&self.submitted).map_err(anyhow::Error::from)?
                != serde_json::to_value(&request.answer).map_err(anyhow::Error::from)?
        {
            return Err(conflict(
                "A first attempt already exists for this card version",
            ));
        }
        Ok(())
    }
}

async fn active_attempt(
    executor: impl sqlx::PgExecutor<'_>,
    card: &CardRow,
) -> ApiResult<Option<AttemptRow>> {
    Ok(sqlx::query_as(
        "SELECT id AS session_id, attempt_request_id AS request_id, attempt_input AS submitted,
                attempt_result AS evaluation FROM review_sessions
         WHERE card_id=$1 AND card_version=$2 AND attempt_request_id IS NOT NULL",
    )
    .bind(card.id)
    .bind(card.version)
    .fetch_optional(executor)
    .await
    .map_err(anyhow::Error::from)?)
}

async fn revealed_card(
    connection: &mut sqlx::PgConnection,
    card: &CardRow,
    session_id: Uuid,
    now: DateTime<Utc>,
) -> ApiResult<RevealedCard> {
    Ok(RevealedCard {
        card_id: card.id,
        version: card.version,
        answer: card.answer(connection).await?,
        intervals: scheduler::previews(card, now)?,
        session_id,
    })
}

async fn attempted_card(
    connection: &mut sqlx::PgConnection,
    card: &CardRow,
    attempt: AttemptData,
) -> ApiResult<AttemptedCard> {
    Ok(AttemptedCard {
        revealed: revealed_card(connection, card, attempt.session_id, Utc::now()).await?,
        submitted: attempt.submitted,
        evaluation: attempt.evaluation,
    })
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/decks", get(decks).post(create_deck))
        .route("/api/cards", get(search::all_cards))
        .route("/api/cards/delete", post(management::delete_cards))
        .route("/api/cards/duplicates", get(management::duplicates))
        .route(
            "/api/cards/duplicates/resolve",
            post(management::resolve_duplicate),
        )
        .route("/api/decks/{id}/move-cards", post(management::move_cards))
        .route(
            "/api/decks/{id}/add-cards",
            post(management::add_deck_cards),
        )
        .route(
            "/api/decks/{id}/remove-cards",
            post(management::remove_cards),
        )
        .route("/api/decks/{id}", put(rename_deck).delete(delete_deck))
        .route("/api/decks/{id}/cards", get(cards).post(add_cards))
        .route("/api/decks/{id}/positions", post(add_positions))
        .route(
            "/api/decks/{id}/card-suggestions",
            get(suggestions::preview).post(suggestions::save),
        )
        .route("/api/decks/{id}/export.apkg", get(export_deck))
        .route("/api/decks/{id}/import.apkg", post(anki_import::import))
        .route("/api/decks/{id}/next", get(next_card))
        .route("/api/study/learning-summary", get(learning_summary))
        .merge(rounds::routes())
        .merge(topics::routes())
        .merge(transfer::routes())
        .merge(anki_flashcards::routes())
        .route("/api/cards/{id}/reveal", post(reveal))
        .route("/api/cards/{id}/attempt", post(submit_attempt))
        .route("/api/cards/{id}/review", post(review))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        decks,
        create_deck,
        rename_deck,
        delete_deck,
        cards,
        search::all_cards,
        management::duplicates,
        management::resolve_duplicate,
        management::move_cards,
        management::remove_cards,
        management::add_deck_cards,
        management::delete_cards,
        add_cards,
        add_positions,
        suggestions::preview,
        suggestions::save,
        export_deck,
        anki_import::import,
        anki_flashcards::face,
        anki_flashcards::media,
        anki_flashcards::upload,
        next_card,
        learning_summary,
        rounds::start,
        rounds::read,
        rounds::answer,
        rounds::mark,
        rounds::advance,
        rounds::retry,
        topics::suggest_tags,
        topics::edit_tags,
        topics::list_topics,
        topics::save_topic,
        topics::delete_topic,
        transfer::start,
        transfer::history,
        reveal,
        submit_attempt,
        review
    ),
    components(schemas(
        Deck,
        CreateDeck,
        RenameDeck,
        AddCards,
        AddCardsResult,
        anki_import::AnkiUpload,
        anki_flashcards::ImportedAnkiDeck,
        management::DuplicateCard,
        management::DuplicateDeck,
        management::DuplicatePage,
        management::DuplicatePeer,
        management::MoveCards,
        management::MoveCardsResult,
        management::RemoveCards,
        management::AddDeckCards,
        management::AddDeckCardsResult,
        management::DeleteCards,
        management::DeleteCard,
        management::ResolveDuplicate,
        AddPositions,
        AddPositionsResult,
        PositionAddItem,
        PositionAddStatus,
        suggestions::SuggestionReference,
        suggestions::CardSuggestion,
        suggestions::CardSuggestions,
        suggestions::SaveSuggestions,
        CardSource,
        CardSolution,
        CardAnswer,
        CardQuestion,
        CardPage,
        CardBrowseItem,
        CardBrowseContext,
        CardDeck,
        CardKind,
        CardReviewState,
        CardQuery,
        topics::TagCard,
        topics::EditCardTags,
        topics::CardTags,
        topics::SaveCardTopic,
        topics::CardTopic,
        transfer::TransferRequest,
        transfer::TransferSummary,
        ReviewCounts,
        NextCard,
        LearningSummary,
        StudyDeck,
        RevealCard,
        RatingIntervals,
        RevealedCard,
        CardAttemptRequest,
        AttemptedCard,
        Rating,
        ReviewCard,
        ReviewResult,
        rounds::PracticeRound,
        rounds::RoundResult,
        rounds::RoundSummary,
        rounds::RoundAnswer,
        rounds::RoundAdvance,
        rounds::RoundVersion
    ))
)]
pub struct LearningDoc;

fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
async fn require_deck(executor: impl sqlx::PgExecutor<'_>, id: Uuid) -> ApiResult<()> {
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM decks WHERE id=$1)")
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(anyhow::Error::from)?
    {
        return Err(ApiError::missing());
    }
    Ok(())
}

#[utoipa::path(get,path="/api/decks",responses((status=200,body=Vec<Deck>)),tag="Learning")]
async fn decks(State(state): State<AppState>) -> ApiResult<Json<Vec<Deck>>> {
    Ok(Json(sqlx::query_as("SELECT d.id,d.name,d.version,d.created_at,count(c.id) AS card_count,count(c.id) FILTER (WHERE c.due_at<=now()) AS due_count FROM decks d LEFT JOIN deck_cards dc ON dc.deck_id=d.id LEFT JOIN learning_cards c ON c.id=dc.card_id GROUP BY d.id ORDER BY d.created_at,d.id").fetch_all(&state.pool).await.map_err(anyhow::Error::from)?))
}
fn deck_name(name: &str) -> ApiResult<&str> {
    let name = name.trim();
    if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "Deck name must be 1–120 bytes without control characters",
        ));
    }
    Ok(name)
}
#[utoipa::path(post,path="/api/decks",request_body=CreateDeck,responses((status=201,body=Deck),(status=422,body=ErrorBody)),tag="Learning")]
async fn create_deck(
    State(state): State<AppState>,
    LearningJson(body): LearningJson<CreateDeck>,
) -> ApiResult<(StatusCode, Json<Deck>)> {
    let name = deck_name(&body.name)?;
    let deck=sqlx::query_as("INSERT INTO decks(id,name) VALUES($1,$2) RETURNING id,name,version,created_at,0::bigint AS card_count,0::bigint AS due_count").bind(Uuid::new_v4()).bind(name).fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(deck)))
}

async fn lock_deck_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    expected_version: i64,
) -> ApiResult<()> {
    if expected_version < 0 {
        return Err(ApiError::invalid("expected_version must be nonnegative"));
    }
    // Match membership writes' deck-first lock order. Cards and FSRS rows remain untouched.
    let version: i64 = sqlx::query_scalar("SELECT version FROM decks WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if version != expected_version {
        return Err(conflict("Deck changed; reload it before trying again"));
    }
    Ok(())
}

#[utoipa::path(put,path="/api/decks/{id}",params(("id"=Uuid,Path)),request_body=RenameDeck,responses((status=200,body=Deck),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn rename_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    LearningJson(body): LearningJson<RenameDeck>,
) -> ApiResult<Json<Deck>> {
    let Path(id) = path.map_err(|error| ApiError::invalid(error.body_text()))?;
    let name = deck_name(&body.name)?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, body.expected_version).await?;
    let deck = sqlx::query_as("UPDATE decks d SET name=$2,version=version+1 WHERE id=$1 RETURNING d.id,d.name,d.version,d.created_at,(SELECT count(*) FROM deck_cards WHERE deck_id=d.id) AS card_count,(SELECT count(*) FROM deck_cards dc JOIN learning_cards c ON c.id=dc.card_id WHERE dc.deck_id=d.id AND c.due_at<=now()) AS due_count")
        .bind(id).bind(name).fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(deck))
}

#[utoipa::path(delete,path="/api/decks/{id}",params(("id"=Uuid,Path),DeleteDeckQuery),responses((status=204,description="Deck and its memberships deleted; shared cards and history retained"),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn delete_deck(
    State(state): State<AppState>,
    path: Result<Path<Uuid>, PathRejection>,
    query: Result<Query<DeleteDeckQuery>, QueryRejection>,
) -> ApiResult<StatusCode> {
    let Path(id) = path.map_err(|error| ApiError::invalid(error.body_text()))?;
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    lock_deck_version(&mut tx, id, query.expected_version).await?;
    // deck_cards cascades from decks; learning_cards, reviews and saved positions do not.
    sqlx::query("DELETE FROM decks WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/decks/{id}/cards",params(("id"=Uuid,Path),CardQuery),responses((status=200,body=CardPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<CardQuery>, QueryRejection>,
) -> ApiResult<Json<CardPage>> {
    search::load(&state, Some(id), query).await
}
#[utoipa::path(post,path="/api/decks/{id}/cards",params(("id"=Uuid,Path)),request_body=AddCards,responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn add_cards(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<AddCards>,
) -> ApiResult<Json<AddCardsResult>> {
    require_deck(&state.pool, id).await?;
    if body.position_id.is_some() == body.job_id.is_some() {
        return Err(ApiError::invalid(
            "Supply exactly one position_id or completed match-analysis job_id",
        ));
    }
    if body.min_loss.is_some_and(|n| !n.is_finite() || n < 0.0)
        || body.player.is_some_and(|p| !(0..=1).contains(&p))
    {
        return Err(ApiError::invalid(
            "min_loss must be finite and nonnegative; player must be 0 or 1",
        ));
    }
    if body.game_no.is_some_and(|n| n < 1) {
        return Err(ApiError::invalid("game_no must be positive"));
    }
    let contents = content::from_source(&state.pool, &body).await?;
    if contents.len() > MAX_CARDS as usize {
        return Err(ApiError::invalid(
            "Batch exceeds 1000 cards; narrow the player or minimum-loss filter",
        ));
    }
    // Keep the legacy last-payload-wins behavior for repeated source decisions.
    let mut unique = BTreeMap::new();
    for c in contents {
        unique.insert(card_signature(&c), c);
    }
    let tags = topics::tags(body.tags)?;
    Ok(Json(
        write_match_cards(&state.pool, id, unique, &tags).await?,
    ))
}

#[utoipa::path(post,path="/api/decks/{id}/positions",params(("id"=Uuid,Path)),request_body=AddPositions,responses((status=200,body=AddPositionsResult),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn add_positions(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<AddPositions>,
) -> ApiResult<Json<AddPositionsResult>> {
    require_deck(&state.pool, id).await?;
    if !(1..=200).contains(&body.position_ids.len()) {
        return Err(ApiError::invalid("position_ids must contain 1..200 UUIDs"));
    }
    let mut unique = BTreeMap::new();
    let mut items = Vec::with_capacity(body.position_ids.len());
    let mut resolved = Vec::with_capacity(body.position_ids.len());
    for position_id in body.position_ids {
        let request = AddCards {
            game_no: None,
            position_id: Some(position_id),
            job_id: None,
            min_loss: None,
            player: None,
            tags: Vec::new(),
        };
        // Use the same native-source selection and solutions as single-card imports.
        // Only expected missing/unanalysed sources become item outcomes; database or
        // malformed native-result errors must fail the whole request before any writes.
        let (status, message, source) = match content::from_source(&state.pool, &request).await {
            Ok(mut contents) => {
                let content = contents.pop().ok_or_else(|| {
                    ApiError::from(anyhow::anyhow!("Position source returned no learning card"))
                })?;
                if !has_usable_checker_solution(&content.solution) {
                    (
                        PositionAddStatus::AnalysisRequired,
                        Some("Analyze this position again; its selected native analysis has no usable checker solution".into()),
                        None,
                    )
                } else {
                    let signature = card_signature(&content);
                    let first = !unique.contains_key(&signature);
                    unique.entry(signature.clone()).or_insert(content);
                    (PositionAddStatus::Duplicate, None, Some((signature, first)))
                }
            }
            Err(ApiError(StatusCode::NOT_FOUND, message)) => {
                (PositionAddStatus::NotFound, Some(message), None)
            }
            Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, message)) => {
                (PositionAddStatus::AnalysisRequired, Some(message), None)
            }
            Err(error) => return Err(error),
        };
        items.push(PositionAddItem {
            position_id,
            status,
            card_id: None,
            message,
        });
        resolved.push(source);
    }
    let written = write_cards(&state.pool, id, unique, &BTreeSet::new()).await?;
    let mut added = 0;
    let mut updated = 0;
    for (item, source) in items.iter_mut().zip(resolved) {
        if let Some((signature, first)) = source {
            let card = &written.cards[&signature];
            item.card_id = Some(card.id);
            if first && card.added {
                item.status = PositionAddStatus::Added;
                added += 1;
            } else if first && card.changed {
                item.status = PositionAddStatus::Updated;
                updated += 1;
            }
        }
    }
    Ok(Json(AddPositionsResult {
        added,
        updated,
        total: written.total,
        items,
    }))
}

fn card_signature(content: &CardContent) -> String {
    format!(
        "{}:{}:{}",
        content.position.position_id, content.position.gnubg_match_id, content.kind
    )
}

// Bulk selection reports unavailable historical native results explicitly, while
// retaining the existing single-source endpoint and its source priority unchanged.
fn has_usable_checker_solution(solution: &CardSolution) -> bool {
    if let Some(decision) = &solution.decision {
        !decision.best.trim().is_empty()
            && match decision.evaluation_status.as_str() {
                "forced" => decision.equity_loss == 0.0,
                "evaluated" => {
                    decision.played_equity.is_some_and(f64::is_finite)
                        && decision.best_equity.is_some_and(f64::is_finite)
                }
                _ => false,
            }
    } else {
        solution
            .candidates
            .first()
            .is_some_and(|candidate| !candidate.notation.trim().is_empty())
    }
}

async fn write_match_cards(
    pool: &PgPool,
    deck_id: Uuid,
    contents: BTreeMap<String, CardContent>,
    tags: &BTreeSet<String>,
) -> ApiResult<AddCardsResult> {
    let written = write_cards(pool, deck_id, contents, tags).await?;
    let added = written.cards.values().filter(|card| card.added).count() as i64;
    Ok(AddCardsResult {
        added,
        updated: written.cards.len() as i64 - added,
        total: written.total,
    })
}

struct CardWrite {
    id: Uuid,
    added: bool,
    changed: bool,
}
struct WrittenCards {
    cards: BTreeMap<String, CardWrite>,
    total: i64,
}

async fn write_cards(
    pool: &PgPool,
    deck_id: Uuid,
    unique: BTreeMap<String, CardContent>,
    tags: &BTreeSet<String>,
) -> ApiResult<WrittenCards> {
    let mut tx = pool.begin().await.map_err(anyhow::Error::from)?;
    // Serializing deck mutations keeps the size bound valid under concurrent requests.
    sqlx::query("SELECT id FROM decks WHERE id=$1 FOR UPDATE")
        .bind(deck_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let mut cards = BTreeMap::new();
    // Byte-sorted signatures match export's C collation and prevent opposite source
    // orders in concurrent imports from acquiring shared-card locks in reverse order.
    for (signature, c) in unique {
        let payload = serde_json::to_value(&c).map_err(anyhow::Error::from)?;
        let new_id = Uuid::new_v4();
        // ON CONFLICT locks even unchanged rows, so a later lookup observes the exact
        // shared card protected by this transaction. No FSRS fields are overwritten.
        let changed_id: Option<Uuid> = sqlx::query_scalar("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3) ON CONFLICT(signature) DO UPDATE SET payload=EXCLUDED.payload,version=learning_cards.version+1,updated_at=now() WHERE learning_cards.payload IS DISTINCT FROM EXCLUDED.payload RETURNING id")
            .bind(new_id).bind(&signature).bind(payload).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
        let card_id = if let Some(card_id) = changed_id {
            card_id
        } else {
            sqlx::query_scalar("SELECT id FROM learning_cards WHERE signature=$1")
                .bind(&signature)
                .fetch_one(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?
        };
        if !tags.is_empty() {
            let mut card: topics::CardTags =
                sqlx::query_as("SELECT id,tags,tags_version FROM learning_cards WHERE id=$1")
                    .bind(card_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(anyhow::Error::from)?;
            topics::update_card(&mut tx, &mut card, tags, &BTreeSet::new()).await?;
        }
        let result = sqlx::query(
            "INSERT INTO deck_cards(deck_id,card_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(deck_id)
        .bind(card_id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        cards.insert(
            signature,
            CardWrite {
                id: card_id,
                added: result.rows_affected() == 1,
                changed: changed_id.is_some_and(|id| id != new_id),
            },
        );
    }
    let total = check_deck_capacity(&mut tx, deck_id).await?;
    if cards.values().any(|card| card.added) {
        sqlx::query("UPDATE decks SET version=version+1 WHERE id=$1")
            .bind(deck_id)
            .execute(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(WrittenCards { cards, total })
}
async fn check_deck_capacity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    deck_id: Uuid,
) -> ApiResult<i64> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM deck_cards WHERE deck_id=$1")
        .bind(deck_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(anyhow::Error::from)?;
    if total > MAX_CARDS {
        return Err(ApiError::invalid(
            "Deck exceeds 1000 cards; use another deck or narrow the filter",
        ));
    }
    Ok(total)
}

#[utoipa::path(get, path = "/api/study/learning-summary", responses((status = 200, body = LearningSummary)), tag = "Learning")]
async fn learning_summary(State(state): State<AppState>) -> ApiResult<Json<LearningSummary>> {
    #[derive(FromRow)]
    struct SummaryRow {
        card_count: i64,
        due_count: i64,
        next_due_at: Option<DateTime<Utc>>,
        deck_id: Option<Uuid>,
        deck_name: Option<String>,
    }

    let row: SummaryRow = sqlx::query_as(
        "WITH counts AS (
             SELECT count(*) AS card_count,
                    count(*) FILTER (WHERE c.due_at <= now()) AS due_count,
                    min(c.due_at) FILTER (WHERE c.due_at > now()) AS next_due_at
             FROM learning_cards c
             WHERE EXISTS (SELECT 1 FROM deck_cards dc WHERE dc.card_id = c.id)
         ), oldest AS (
             SELECT c.id FROM learning_cards c
             WHERE c.due_at <= now()
               AND EXISTS (SELECT 1 FROM deck_cards dc WHERE dc.card_id = c.id)
             ORDER BY c.due_at, c.id LIMIT 1
         ), next_deck AS (
             SELECT d.id, d.name FROM oldest
             JOIN deck_cards dc ON dc.card_id = oldest.id
             JOIN decks d ON d.id = dc.deck_id
             ORDER BY d.created_at, d.id LIMIT 1
         )
         SELECT counts.*, next_deck.id AS deck_id, next_deck.name AS deck_name
         FROM counts LEFT JOIN next_deck ON true",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;

    Ok(Json(LearningSummary {
        card_count: row.card_count,
        due_count: row.due_count,
        next_due_at: row.next_due_at,
        next_deck: row
            .deck_id
            .zip(row.deck_name)
            .map(|(id, name)| StudyDeck { id, name }),
    }))
}

#[utoipa::path(get,path="/api/decks/{id}/next",params(("id"=Uuid,Path)),responses((status=200,body=NextCard),(status=404,body=ErrorBody)),tag="Learning")]
async fn next_card(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<NextCard>> {
    require_deck(&state.pool, id).await?;
    let (total,due,new):(i64,i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE due_at<=now()),count(*) FILTER(WHERE review_count=0) FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id WHERE dc.deck_id=$1").bind(id).fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    let row: Option<DueCardRow> = sqlx::query_as(
        "SELECT due.*, (SELECT jsonb_build_object('session_id',s.id,'submitted',s.attempt_input,
                    'evaluation',s.attempt_result) FROM review_sessions s
                    WHERE s.card_id=due.id AND s.card_version=due.version
                      AND s.attempt_request_id IS NOT NULL) AS active_attempt
         FROM (SELECT c.* FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
               WHERE dc.deck_id=$1 AND c.due_at<=now() ORDER BY c.due_at,c.id LIMIT 1) due",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let (card, active_attempt) = if let Some(row) = row {
        let attempt = if let Some(attempt) = row.active_attempt {
            let mut connection = state.pool.acquire().await.map_err(anyhow::Error::from)?;
            Some(attempted_card(&mut connection, &row.card, attempt).await?)
        } else {
            None
        };
        (Some(row.card.question()?), attempt)
    } else {
        (None, None)
    };
    Ok(Json(NextCard {
        card,
        counts: ReviewCounts { total, due, new },
        active_attempt,
    }))
}
#[utoipa::path(post,path="/api/cards/{id}/reveal",params(("id"=Uuid,Path)),request_body=RevealCard,responses((status=200,body=RevealedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<RevealCard>,
) -> ApiResult<Json<RevealedCard>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if row.version != body.expected_version {
        return Err(conflict("Card changed; reload it before revealing"));
    }
    let now = Utc::now();
    if let Some(attempt) = active_attempt(&mut *tx, &row).await? {
        return Ok(Json(
            revealed_card(&mut tx, &row, attempt.session_id, now).await?,
        ));
    }
    let session_id = Uuid::new_v4();
    let revealed = revealed_card(&mut tx, &row, session_id, now).await?;
    sqlx::query(
        "INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES($1,$2,$3,$4)",
    )
    .bind(session_id)
    .bind(id)
    .bind(row.version)
    .bind(now + chrono::Duration::days(1))
    .execute(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(revealed))
}

#[utoipa::path(post,path="/api/cards/{id}/attempt",params(("id"=Uuid,Path)),request_body=CardAttemptRequest,responses((status=200,body=AttemptedCard),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
async fn submit_attempt(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<CardAttemptRequest>,
) -> ApiResult<Json<AttemptedCard>> {
    let row = load_attempt_card(&state.pool, id, body.expected_version, false).await?;
    if let Some(previous) = active_attempt(&state.pool, &row).await? {
        // Recheck under the same lock used to finish a review. The card may
        // have advanced since the preliminary lookup.
        previous.confirm_request(&body)?;
        let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
        let row = load_attempt_card(&mut *tx, id, body.expected_version, true).await?;
        let result = attempted_card(&mut tx, &row, previous.into_data()).await?;
        return Ok(Json(result));
    }
    if row.is_anki() {
        return Err(ApiError::invalid("Reveal the Anki card before rating"));
    }
    let card = row.content()?;
    crate::play::require_review_available(&state.pool, card.source.match_id).await?;
    let evaluation = evaluate_card(&state, &card, &body.answer).await?;

    // Evaluation can be slow. Serialize only the short write with reviews
    // and card updates and verify the snapshot version again at that point.
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row = load_attempt_card(&mut *tx, id, body.expected_version, true).await?;
    let attempt = if let Some(previous) = active_attempt(&mut *tx, &row).await? {
        previous.confirm_request(&body)?;
        previous.into_data()
    } else {
        crate::play::require_review_available_on(&mut *tx, card.source.match_id).await?;
        let session_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO review_sessions(id,card_id,card_version,expires_at,attempt_request_id,attempt_input,attempt_result)
             VALUES($1,$2,$3,now() + interval '1 day',$4,$5,$6)",
        )
        .bind(session_id)
        .bind(id)
        .bind(row.version)
        .bind(body.request_id)
        .bind(sqlx::types::Json(&body.answer))
        .bind(sqlx::types::Json(&evaluation))
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        AttemptData {
            session_id,
            submitted: body.answer,
            evaluation,
        }
    };
    let result = attempted_card(&mut tx, &row, attempt).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(result))
}

async fn evaluate_card(
    state: &AppState,
    card: &CardContent,
    answer: &PracticeAnswer,
) -> ApiResult<PracticeEvaluation> {
    evaluation::evaluate(
        state,
        &card.kind,
        &card.position,
        SavedEvaluation {
            engine: &card.solution.engine,
            plies: Some(card.solution.plies),
            decision: card.solution.decision.as_ref(),
            candidates: &card.solution.candidates,
            native: true,
        },
        answer,
    )
    .await
}

async fn load_attempt_card(
    executor: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    expected_version: i64,
    lock: bool,
) -> ApiResult<CardRow> {
    let sql = if lock {
        "SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM learning_cards WHERE id=$1"
    };
    let row: CardRow = sqlx::query_as(sql)
        .bind(id)
        .fetch_optional(executor)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if row.version != expected_version {
        return Err(conflict("Card changed; reload it before answering"));
    }
    Ok(row)
}
#[utoipa::path(post,path="/api/cards/{id}/review",params(("id"=Uuid,Path)),request_body=ReviewCard,responses((status=200,body=ReviewResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<ReviewCard>,
) -> ApiResult<Json<ReviewResult>> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    let previous: Option<(Uuid, String, serde_json::Value)> = sqlx::query_as(
        "SELECT session_id,rating,result FROM card_reviews WHERE card_id=$1 AND request_id=$2",
    )
    .bind(id)
    .bind(body.request_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if let Some((session, rating, result)) = previous {
        if session != body.session_id || rating != body.rating.as_str() {
            return Err(conflict(
                "request_id was already used for a different review",
            ));
        }
        return Ok(Json(
            serde_json::from_value(result).map_err(anyhow::Error::from)?,
        ));
    }
    let session: Option<(i64, DateTime<Utc>, bool)> = sqlx::query_as(
        "SELECT card_version,expires_at,attempt_request_id IS NOT NULL FROM review_sessions WHERE id=$1 AND card_id=$2",
    )
    .bind(body.session_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let (version, expires, is_attempt) =
        session.ok_or_else(|| conflict("Reveal this card before rating"))?;
    let now = Utc::now();
    if version != row.version || (!is_attempt && expires < now) {
        return Err(conflict(
            "Review session is stale; reload and reveal the card",
        ));
    }
    if !is_attempt && active_attempt(&mut *tx, &row).await?.is_some() {
        return Err(conflict("Rate the saved first attempt for this card"));
    }
    let result = scheduler::rate(&row, body.rating, now)?;
    sqlx::query("UPDATE learning_cards SET stability=$2,difficulty=$3,last_review=$4,due_at=$5,version=$6,review_count=$7,updated_at=$4 WHERE id=$1").bind(id).bind(result.stability).bind(result.difficulty).bind(now).bind(result.due_at).bind(result.version).bind(result.review_count).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    sqlx::query("INSERT INTO card_reviews(card_id,request_id,session_id,rating,result,reviewed_at) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(body.request_id).bind(body.session_id).bind(body.rating.as_str()).bind(serde_json::to_value(&result).map_err(anyhow::Error::from)?).bind(now).execute(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(result))
}
#[utoipa::path(get,path="/api/decks/{id}/export.apkg",params(("id"=Uuid,Path),ExportQuery),responses((status=200,description="Native Anki package; review history remains independent",content_type="application/zip",body=Vec<u8>),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody)),tag="Learning")]
async fn export_deck(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> ApiResult<impl IntoResponse> {
    static EXPORTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let locale = query.locale.unwrap_or_else(|| "de".into());
    if !["de", "en", "es", "fr", "pl", "it"].contains(&locale.as_str()) {
        return Err(ApiError::invalid("Unsupported export locale"));
    }
    let permit = EXPORTS.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Two exports are already in progress".into(),
        )
    })?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Hold a shared deck lock so concurrent moves or duplicate cleanup cannot
    // change its membership between reading IDs and locking the export cards.
    let name: String = sqlx::query_scalar("SELECT name FROM decks WHERE id=$1 FOR SHARE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
                        WHERE dc.deck_id=$1 AND c.payload->>'kind'='anki')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?
    {
        return Err(ApiError::invalid(
            "Use the original Anki package to export imported Anki cards",
        ));
    }
    let card_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id
         WHERE dc.deck_id=$1 ORDER BY c.signature COLLATE \"C\" LIMIT 1001",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if card_ids.len() > MAX_CARDS as usize {
        return Err(ApiError::invalid("Export exceeds 1000 cards"));
    }
    // Read content only after taking all card locks, in the same order as add_cards.
    for card_id in &card_ids {
        sqlx::query("SELECT id FROM learning_cards WHERE id=$1 FOR UPDATE")
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    }
    let mut cards = Vec::with_capacity(card_ids.len());
    for card_id in card_ids {
        let row: CardRow = sqlx::query_as("SELECT * FROM learning_cards WHERE id=$1")
            .bind(card_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let c = row.content()?;
        let a = content::answer_on_connection(&mut tx, row.id, &c).await?;
        cards.push((row.id, c, a));
    }
    let (mut prepared, permit) = tokio::task::spawn_blocking(move || {
        apkg::prepare(cards, &locale).map(|prepared| (prepared, permit))
    })
    .await
    .map_err(anyhow::Error::from)??;
    for card in &mut prepared {
        let fingerprint = card.fingerprint();
        card.modified = sqlx::query_scalar("INSERT INTO learning_card_exports(card_id,fingerprint,modified) VALUES($1,$2,$3) ON CONFLICT(card_id) DO UPDATE SET fingerprint=EXCLUDED.fingerprint,modified=CASE WHEN learning_card_exports.fingerprint=EXCLUDED.fingerprint THEN learning_card_exports.modified ELSE greatest(learning_card_exports.modified+1,EXCLUDED.modified) END RETURNING modified")
            .bind(card.id).bind(fingerprint).bind(Utc::now().timestamp()).fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
    }
    tx.commit().await.map_err(anyhow::Error::from)?;
    let bytes = tokio::task::spawn_blocking(move || {
        // Cancellation of the HTTP request must not release the CPU slot before ZIP work ends.
        let _permit = permit;
        apkg::build(id, &name, &prepared)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"bgtutor.apkg\"",
            ),
        ],
        bytes,
    ))
}

// Share the canonical absolute-color GNU ID decoder with read-only practice.
pub(crate) use content::decode_board as decode_decision_board;
