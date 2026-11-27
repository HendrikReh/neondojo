//! Shared cards power native Anki exports and the integrated FSRS trainer.
mod anki;
pub(crate) use anki::OperationLimits;
pub(crate) use anki::auto_sync::AnkiConnection;
pub use anki::{auto_sync::worker as anki_sync_worker, flashcards as anki_flashcards};
mod attempts;
mod card_writes;
mod content;
mod daily;
mod decks;
mod exercises;
mod management;
mod rounds;
mod routes;
mod scheduler;
mod search;
mod suggestions;
mod topics;
mod transfer;
use crate::practice::evaluation::{self, PracticeAnswer, PracticeEvaluation, SavedEvaluation};
#[cfg(test)]
pub(crate) use content::position_cards_sql;
pub(crate) use content::{saved_checker_move, saved_position_cards};
pub use search::{
    CardBrowseContext, CardBrowseItem, CardDeck, CardKind, CardQuery, CardReviewState,
};

use crate::configuration::ConfigurationLocale;
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::Candidate,
    match_analysis::Decision,
    model::Position,
};
use axum::{
    Router,
    extract::State,
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

use attempts::*;
use card_writes::*;
use decks::*;
pub use routes::{LearningDoc, daily_doc, routes};

const MAX_CARDS: i64 = 1000;

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
    /// Every card whose due time has passed, including never-reviewed cards.
    due_count: i64,
    /// Due cards that have been reviewed before.
    review_due_count: i64,
    /// Due cards in a deck that have never been reviewed.
    new_count: i64,
    new_cards_per_day: i32,
    /// New cards still allowed today, at most `new_count`.
    new_available_today: i64,
    /// Due reviews plus the new cards still allowed today.
    today_count: i64,
    next_due_at: Option<DateTime<Utc>>,
    /// Deck of the next card to study today.
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
    pub analysis_id: Option<String>,
    pub match_id: Option<Uuid>,
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
impl AddPositions {
    pub(crate) fn validated_ids(self) -> ApiResult<Vec<Uuid>> {
        if !(1..=200).contains(&self.position_ids.len()) {
            return Err(ApiError::invalid("position_ids must contain 1..200 UUIDs"));
        }
        Ok(self.position_ids)
    }
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
#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AddDecisions {
    /// Analysed decisions in selection order, as decision search and recurring
    /// mistakes return them. Every occurrence receives an outcome.
    #[schema(min_items = 1, max_items = 200)]
    pub decisions: Vec<crate::recurring_mistakes::MistakeReference>,
}
impl AddDecisions {
    pub(crate) fn validated(self) -> ApiResult<Vec<crate::recurring_mistakes::MistakeReference>> {
        if !(1..=200).contains(&self.decisions.len()) {
            return Err(ApiError::invalid(
                "decisions must contain 1..200 references",
            ));
        }
        if self
            .decisions
            .iter()
            .any(|r| r.job_id.is_some() == r.imported_analysis_id.is_some())
        {
            return Err(ApiError::invalid(
                "Supply exactly one of job_id or imported_analysis_id per decision",
            ));
        }
        if self
            .decisions
            .iter()
            .any(|r| r.game_no < 1 || r.decision_no < 1)
        {
            return Err(ApiError::invalid(
                "game_no and decision_no must be positive",
            ));
        }
        Ok(self.decisions)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAddStatus {
    Added,
    Updated,
    /// The same card as an earlier decision of this request, or a card the
    /// deck already holds unchanged.
    Duplicate,
    /// The analysis has no supported card for this decision.
    Unavailable,
    NotFound,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DecisionAddItem {
    pub reference: crate::recurring_mistakes::MistakeReference,
    pub status: DecisionAddStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddDecisionsResult {
    pub added: i64,
    pub updated: i64,
    pub total: i64,
    /// Outcomes in the same order as the request, including repeated references.
    pub items: Vec<DecisionAddItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardSource {
    pub kind: String,
    pub job_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_analysis_id: Option<Uuid>,
    pub match_id: Uuid,
    pub position_id: Option<Uuid>,
    pub game_no: i32,
    pub decision_no: Option<i32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CardSolution {
    pub engine: String,
    pub plies: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported: Option<crate::imported_analysis::ImportedAnalysisData>,
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
    /// Due cards that have been reviewed before.
    pub due: i64,
    /// Due cards that have never been reviewed.
    pub new: i64,
    /// New cards of this deck still allowed today.
    #[serde(default)]
    pub new_available: i64,
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
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ExportQuery {
    pub locale: Option<ConfigurationLocale>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CardContent {
    kind: String,
    position: Position,
    players: [String; 2],
    prompt: String,
    source: CardSource,
    solution: CardSolution,
    played: String,
}
/// The stored payload of a card. Its `kind` is `anki` for a rendered Anki card
/// and the question type for a decision.
#[derive(Debug, Deserialize)]
#[serde(try_from = "serde_json::Value")]
enum CardPayload {
    Decision(Box<CardContent>),
    Anki(anki_flashcards::AnkiCard),
}
impl TryFrom<serde_json::Value> for CardPayload {
    type Error = serde_json::Error;

    fn try_from(payload: serde_json::Value) -> Result<Self, Self::Error> {
        if payload["kind"] == anki_flashcards::KIND {
            serde_json::from_value(payload).map(Self::Anki)
        } else {
            serde_json::from_value(payload).map(Self::Decision)
        }
    }
}
impl CardPayload {
    fn decision(&self) -> Option<&CardContent> {
        match self {
            Self::Decision(content) => Some(content),
            Self::Anki(_) => None,
        }
    }
}
#[derive(Debug, FromRow)]
struct CardRow {
    id: Uuid,
    version: i64,
    #[sqlx(json)]
    payload: CardPayload,
    due_at: DateTime<Utc>,
    review_count: i32,
    anki_review_count: i32,
    stability: Option<f64>,
    difficulty: Option<f64>,
    last_review: Option<DateTime<Utc>>,
    tags: Vec<String>,
    tags_version: i64,
}
impl CardRow {
    /// The decision this card asks about; none for a rendered Anki card.
    fn decision(&self) -> Option<&CardContent> {
        self.payload.decision()
    }

    fn question(&self) -> CardQuestion {
        let (kind, position, players, prompt, source) = match &self.payload {
            CardPayload::Anki(card) => (
                card.kind.clone(),
                None,
                [String::new(), String::new()],
                card.prompt.clone(),
                None,
            ),
            CardPayload::Decision(content) => {
                let c = content.as_ref();
                let mut position = c.position.clone();
                position.id = self.id;
                position.notation.clear();
                if matches!(c.kind.as_str(), "cube_offer" | "cube_response") {
                    if let Some(id) = crate::backgammon::ids::hide_dice(&position.gnubg_match_id) {
                        position.gnubg_match_id = id;
                    }
                    position.dice = vec![0, 0];
                }
                (
                    c.kind.clone(),
                    Some(position),
                    c.players.clone(),
                    c.prompt.clone(),
                    Some(c.source.clone()),
                )
            }
        };
        CardQuestion {
            id: self.id,
            version: self.version,
            kind,
            position,
            players,
            prompt,
            source,
            due_at: self.due_at,
            review_count: self.review_count + self.anki_review_count,
            tags_version: self.tags_version,
        }
    }

    async fn answer(&self, connection: &mut sqlx::PgConnection) -> anyhow::Result<CardAnswer> {
        let Some(content) = self.decision() else {
            return Ok(CardAnswer {
                solution: None,
                notes: String::new(),
                tags: self.tags.clone(),
                played: String::new(),
            });
        };
        content::answer_on_connection(connection, self.id, content).await
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

fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
