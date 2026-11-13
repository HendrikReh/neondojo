//! Reviewable flashcard selections from completed match analyses.
use super::*;
use crate::{
    game_plan::GamePlan,
    recurring_mistakes::{
        self, MistakeKind,
        source::{self, SourceDecision},
    },
};
use std::collections::{HashMap, HashSet};

const MAX_SUGGESTIONS: usize = 10;
const MAX_RECURRING_PICKS: usize = 3;

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub(super) struct SuggestionQuery {
    match_id: Uuid,
    player: Option<i32>,
    min_loss: f64,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize, ToSchema,
)]
#[serde(deny_unknown_fields)]
pub(super) struct SuggestionReference {
    job_id: Uuid,
    game_no: i32,
    decision_no: i32,
}

#[derive(Serialize, ToSchema)]
pub(super) struct CardSuggestion {
    reference: SuggestionReference,
    position: Position,
    player_name: String,
    mistake_kind: MistakeKind,
    game_plan: GamePlan,
    equity_loss: f64,
    /// Includes the current match when the group occurs in multiple matches.
    recurring_matches: usize,
    #[schema(required = true)]
    card_id: Option<Uuid>,
    in_deck: bool,
}

#[derive(Serialize, ToSchema)]
pub(super) struct CardSuggestions {
    items: Vec<CardSuggestion>,
    /// False when no completed native analysis is available for any game.
    has_analysis: bool,
    /// False when the history exceeds the recurring-mistake query bounds.
    history_available: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct SaveSuggestions {
    match_id: Uuid,
    #[schema(min_items = 1, max_items = 10)]
    decisions: Vec<SuggestionReference>,
}

#[derive(FromRow)]
struct MatchScope {
    database_id: Uuid,
    white: String,
    black: String,
}

async fn match_scope(pool: &PgPool, id: Uuid) -> ApiResult<MatchScope> {
    sqlx::query_as(
        "SELECT database_id,white,black FROM matches WHERE id=$1 AND NOT collection_only",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)
}

struct Eligible {
    reference: SuggestionReference,
    content: CardContent,
    preview: Position,
    plan: GamePlan,
    mistake_kind: MistakeKind,
    equity_loss: f64,
}

impl Eligible {
    fn from_decision(row: SourceDecision) -> Option<Self> {
        let question = row.question;
        let reference = SuggestionReference {
            job_id: question.reference.job_id?,
            game_no: question.reference.game_no,
            decision_no: question.reference.decision_no,
        };
        let mistake_kind = recurring_mistakes::kind(&row.decision);
        let equity_loss = row.decision.equity_loss;
        let mut position = question.position.clone();
        position.gnubg_match_id = row.original_match_state?;
        Some(Self {
            reference,
            content: content::match_card(
                &question.kind,
                position,
                question.players,
                reference.job_id,
                question.engine,
                question.plies?,
                row.decision,
            ),
            preview: question.position,
            plan: row.plan,
            mistake_kind,
            equity_loss,
        })
    }

    fn group(&self) -> (usize, Group) {
        (
            self.preview.player as usize,
            (
                crate::position_game_plans::label(self.plan),
                self.mistake_kind,
            ),
        )
    }
}

type Group = (&'static str, MistakeKind);

fn group(plan: GamePlan, decision: &Decision) -> Group {
    (
        crate::position_game_plans::label(plan),
        recurring_mistakes::kind(decision),
    )
}

#[utoipa::path(get,path="/api/decks/{id}/card-suggestions",params(("id"=Uuid,Path), SuggestionQuery),responses((status=200,body=CardSuggestions),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<SuggestionQuery>, QueryRejection>,
) -> ApiResult<Json<CardSuggestions>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    if !query.min_loss.is_finite()
        || query.min_loss < 0.0
        || query.player.is_some_and(|p| !(0..=1).contains(&p))
    {
        return Err(ApiError::invalid(
            "min_loss must be finite and nonnegative; player must be 0 or 1",
        ));
    }

    let pool = &state.pool;
    require_deck(pool, id).await?;
    let scope = match_scope(pool, query.match_id).await?;
    let (has_analysis, rows) = source::load_native_match(pool, query.match_id).await?;
    let mut candidates: Vec<_> = rows
        .into_iter()
        .filter(|row| {
            query
                .player
                .is_none_or(|player| player == row.decision.player)
                && row.meets_loss_threshold(query.min_loss)
        })
        .filter_map(Eligible::from_decision)
        .collect();
    candidates.sort_by(|a, b| {
        b.equity_loss
            .total_cmp(&a.equity_loss)
            .then_with(|| a.reference.game_no.cmp(&b.reference.game_no))
            .then_with(|| a.reference.decision_no.cmp(&b.reference.decision_no))
    });
    let mut seen = HashSet::new();
    candidates.retain(|item| seen.insert(card_signature(&item.content)));

    let mut historical = BTreeMap::new();
    let mut history_available = true;
    for (side, name) in [scope.white, scope.black].iter().enumerate() {
        let name = name.trim().to_lowercase();
        if name.is_empty()
            || !candidates
                .iter()
                .any(|candidate| candidate.preview.player as usize == side)
        {
            continue;
        }
        let rows = match source::load(pool, Some(scope.database_id), None, Some(&name)).await {
            Ok(rows) => rows,
            Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, message))
                if message == source::TOO_BROAD =>
            {
                history_available = false;
                continue;
            }
            Err(error) => return Err(error),
        };
        let mut by_group: BTreeMap<Group, HashSet<Uuid>> = BTreeMap::new();
        for row in rows {
            if row.question.position.match_uuid != query.match_id
                && row.in_scope(&name, None)
                && row.meets_loss_threshold(query.min_loss)
            {
                by_group
                    .entry(group(row.plan, &row.decision))
                    .or_default()
                    .insert(row.question.position.match_uuid);
            }
        }
        historical.extend(
            by_group
                .into_iter()
                .map(|(group, matches)| ((side, group), matches.len() + 1)),
        );
    }

    // Reserve space for distinct recurring groups, then fill by largest loss.
    let mut groups = std::collections::BTreeSet::new();
    let mut recurring: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let key = candidate.group();
            let count = *historical.get(&key)?;
            if !groups.insert(key) {
                return None;
            }
            Some((index, count))
        })
        .collect();
    recurring.sort_by_key(|&(index, count)| (std::cmp::Reverse(count), index));
    let mut chosen: Vec<_> = recurring
        .into_iter()
        .take(MAX_RECURRING_PICKS)
        .map(|(index, _)| index)
        .collect();
    for index in 0..candidates.len() {
        if chosen.len() == MAX_SUGGESTIONS {
            break;
        }
        if !chosen.contains(&index) {
            chosen.push(index);
        }
    }
    let signatures: Vec<_> = chosen
        .iter()
        .map(|&index| card_signature(&candidates[index].content))
        .collect();
    let existing: HashMap<_, _> = sqlx::query_as::<_, (String, Uuid, bool)>(
        "SELECT c.signature,c.id,dc.card_id IS NOT NULL FROM learning_cards c
         LEFT JOIN deck_cards dc ON dc.card_id=c.id AND dc.deck_id=$1
         WHERE c.signature=ANY($2)",
    )
    .bind(id)
    .bind(&signatures)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?
    .into_iter()
    .map(|(signature, card, in_deck)| (signature, (card, in_deck)))
    .collect();
    let items = chosen
        .into_iter()
        .zip(signatures)
        .map(|(index, signature)| {
            let item = &candidates[index];
            let existing = existing.get(&signature);
            CardSuggestion {
                reference: item.reference,
                position: item.preview.clone(),
                player_name: item.content.players[item.preview.player as usize].clone(),
                mistake_kind: item.mistake_kind,
                game_plan: item.plan,
                equity_loss: item.equity_loss,
                recurring_matches: historical.get(&item.group()).copied().unwrap_or(0),
                card_id: existing.map(|&(id, _)| id),
                in_deck: existing.is_some_and(|&(_, in_deck)| in_deck),
            }
        })
        .collect();
    Ok(Json(CardSuggestions {
        items,
        has_analysis,
        history_available,
    }))
}

#[utoipa::path(post,path="/api/decks/{id}/card-suggestions",params(("id"=Uuid,Path)),request_body=SaveSuggestions,responses((status=200,body=AddCardsResult),(status=404,body=ErrorBody),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn save(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(body): LearningJson<SaveSuggestions>,
) -> ApiResult<Json<AddCardsResult>> {
    let mut requested: HashSet<_> = body.decisions.iter().copied().collect();
    if requested.is_empty()
        || requested.len() > MAX_SUGGESTIONS
        || requested.len() != body.decisions.len()
    {
        return Err(ApiError::invalid("Select one to ten distinct decisions"));
    }

    let pool = &state.pool;
    require_deck(pool, id).await?;
    match_scope(pool, body.match_id).await?;
    let (_, rows) = source::load_native_match(pool, body.match_id).await?;
    let mut unique = BTreeMap::new();
    for row in rows
        .into_iter()
        .filter(|row| row.meets_loss_threshold(0.0))
        .filter_map(Eligible::from_decision)
    {
        if requested.remove(&row.reference) {
            unique.insert(card_signature(&row.content), row.content);
        }
    }
    if !requested.is_empty() {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Match analysis has changed. Reload suggestions.".into(),
        ));
    }
    Ok(Json(
        write_match_cards(pool, id, unique, &BTreeSet::new()).await?,
    ))
}
