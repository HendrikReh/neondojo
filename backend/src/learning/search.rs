//! Read-only browsing of shared cards. Filtering happens before cursor pagination.
use super::*;
use crate::search::literal_pattern;
use axum::extract::rejection::QueryRejection;
use sqlx::{Postgres, QueryBuilder, Row};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    Checker,
    CubeOffer,
    CubeResponse,
}
impl CardKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Checker => "checker",
            Self::CubeOffer => "cube_offer",
            Self::CubeResponse => "cube_response",
        }
    }
}
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CardReviewState {
    New,
    Due,
    Reviewed,
}
#[derive(Debug, Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct CardQuery {
    pub after: Option<Uuid>,
    #[param(minimum = 1, maximum = 100)]
    pub limit: Option<i64>,
    pub deck_id: Option<Uuid>,
    /// Literal substring of players, question, IDs, deck names, tags or comments.
    pub q: Option<String>,
    /// Literal substring of the name of the player on roll.
    pub player: Option<String>,
    pub tag: Option<String>,
    pub comment: Option<String>,
    /// Exact 14-character GNUbg checker position ID.
    pub position_id: Option<String>,
    pub kind: Option<CardKind>,
    /// New and due may overlap. Reviewed means at least one completed review.
    pub status: Option<CardReviewState>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CardDeck {
    pub id: Uuid,
    pub name: String,
}
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct CardBrowseContext {
    pub decks: Vec<CardDeck>,
    pub tags: Vec<String>,
    pub comment_excerpt: String,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CardBrowseItem {
    #[serde(flatten)]
    pub card: CardQuestion,
    /// Library metadata only; engine answers remain behind explicit reveal.
    pub browse: CardBrowseContext,
}
impl CardQuery {
    fn validate(mut self) -> ApiResult<Self> {
        if !(1..=100).contains(&self.limit.unwrap_or(50)) {
            return Err(ApiError::invalid("limit must be 1..100"));
        }
        for (value, max) in [
            (&mut self.q, 200),
            (&mut self.player, 240),
            (&mut self.tag, 64),
            (&mut self.comment, 200),
            (&mut self.position_id, 14),
        ] {
            if let Some(text) = value {
                *text = text.trim().to_owned();
                if text.chars().count() > max || text.chars().any(char::is_control) {
                    return Err(ApiError::invalid(
                        "Card filter text is too long or contains control characters",
                    ));
                }
                if text.is_empty() {
                    *value = None;
                }
            }
        }
        if let Some(tag) = &mut self.tag {
            *tag = tag.to_lowercase();
        }
        if self.position_id.as_ref().is_some_and(|id| {
            id.len() != 14
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+/".contains(&b))
        }) {
            return Err(ApiError::invalid(
                "position_id must be a 14-character GNUbg checker ID",
            ));
        }
        Ok(self)
    }
}
// Include all note scopes. A retained position can now belong to its private source
// container, while the snapshot still points to the original source match.
const NOTE_SCOPE: &str = r#"(
    a.match_uuid=(c.payload#>>'{source,match_id}')::uuid
    OR a.position_uuid=(c.payload#>>'{source,position_id}')::uuid
    OR a.match_uuid=(SELECT p.match_uuid FROM positions p WHERE p.id=(c.payload#>>'{source,position_id}')::uuid)
    OR a.game_uuid IN (
        SELECT g.id FROM match_games g
        WHERE (g.match_uuid=(c.payload#>>'{source,match_id}')::uuid AND g.game_no=(c.payload#>>'{source,game_no}')::int)
        OR EXISTS(SELECT 1 FROM positions p WHERE p.id=(c.payload#>>'{source,position_id}')::uuid AND p.match_uuid=g.match_uuid AND p.game_no=g.game_no)
    )
    OR a.move_uuid IN (
        SELECT mv.id FROM match_moves mv
        WHERE mv.position_uuid=(c.payload#>>'{source,position_id}')::uuid
        OR (mv.match_uuid=(c.payload#>>'{source,match_id}')::uuid
            AND mv.game_no=(c.payload#>>'{source,game_no}')::int
            AND c.payload->>'played'<>'no double'
            AND mv.player=(c.payload#>>'{position,player}')::int
            AND mv.kind=CASE WHEN c.payload->>'kind'='checker' THEN 'checker' ELSE 'cube' END
            AND mv.move_no=(
                SELECT count(*) FROM move_analyses ma
                WHERE ma.job_id=(c.payload#>>'{source,job_id}')::uuid
                AND ma.game_no=(c.payload#>>'{source,game_no}')::int
                AND ma.decision_no<=(c.payload#>>'{source,decision_no}')::int
                AND ma.data->>'played'<>'no double'
            )
        )
    )
)"#;
fn filters(sql: &mut QueryBuilder<'_, Postgres>, q: &CardQuery, deck: Option<Uuid>) {
    sql.push(" WHERE true");
    if let Some(id) = deck {
        sql.push(" AND EXISTS(SELECT 1 FROM deck_cards dc WHERE dc.card_id=c.id AND dc.deck_id=")
            .push_bind(id)
            .push(")");
    }
    if let Some(kind) = q.kind {
        sql.push(" AND c.payload->>'kind'=")
            .push_bind(kind.as_str());
    }
    if let Some(status) = q.status {
        sql.push(match status {
            CardReviewState::New => " AND c.review_count=0",
            CardReviewState::Due => " AND c.due_at<=now()",
            CardReviewState::Reviewed => " AND c.review_count>0",
        });
    }
    if let Some(id) = &q.position_id {
        sql.push(" AND c.payload#>>'{position,position_id}'=")
            .push_bind(id.clone());
    }
    if let Some(player) = &q.player {
        sql.push(
            " AND lower(c.payload->'players'->>((c.payload#>>'{position,player}')::int)) LIKE ",
        )
        .push_bind(literal_pattern(player))
        .push(" ESCAPE '\\'");
    }
    for (value, column) in [(&q.comment, "lower(a.comment)"), (&q.tag, "")] {
        if let Some(value) = value {
            sql.push(" AND EXISTS(SELECT 1 FROM annotations a WHERE ")
                .push(NOTE_SCOPE)
                .push(" AND ");
            if column.is_empty() {
                sql.push_bind(value.clone()).push("=ANY(a.tags)");
            } else {
                sql.push(column)
                    .push(" LIKE ")
                    .push_bind(literal_pattern(value))
                    .push(" ESCAPE '\\'");
            }
            sql.push(")");
        }
    }
    if let Some(text) = &q.q {
        let value = literal_pattern(text);
        sql.push(" AND (lower(coalesce(c.payload->'players'->>0,'')||' '||coalesce(c.payload->'players'->>1,'')||' '||coalesce(c.payload->>'prompt','')||' '||coalesce(c.payload#>>'{position,position_id}','')||' '||coalesce(c.payload#>>'{position,gnubg_match_id}','')) LIKE ").push_bind(value.clone()).push(" ESCAPE '\\' OR EXISTS(SELECT 1 FROM annotations a WHERE ").push(NOTE_SCOPE).push(" AND a.search_text LIKE ").push_bind(value.clone()).push(" ESCAPE '\\') OR EXISTS(SELECT 1 FROM deck_cards dc JOIN decks d ON d.id=dc.deck_id WHERE dc.card_id=c.id AND lower(d.name) LIKE ").push_bind(value).push(" ESCAPE '\\'))");
    }
}
#[utoipa::path(get,path="/api/cards",params(CardQuery),responses((status=200,body=CardPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Learning")]
pub(super) async fn all_cards(
    State(state): State<AppState>,
    query: Result<Query<CardQuery>, QueryRejection>,
) -> ApiResult<Json<CardPage>> {
    load(&state, None, query).await
}
pub(super) async fn load(
    state: &AppState,
    path: Option<Uuid>,
    query: Result<Query<CardQuery>, QueryRejection>,
) -> ApiResult<Json<CardPage>> {
    let q = query
        .map_err(|e| ApiError::invalid(e.body_text()))?
        .0
        .validate()?;
    if path.zip(q.deck_id).is_some_and(|(a, b)| a != b) {
        return Err(ApiError::invalid(
            "deck_id does not match the requested deck",
        ));
    }
    let deck = path.or(q.deck_id);
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    // These bounded pages do not amortize JIT startup. Correlated note scopes can
    // exceed PostgreSQL's estimated-cost threshold even for a tiny collection.
    sqlx::query("SET LOCAL jit=off")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    if let Some(id) = deck {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM decks WHERE id=$1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::missing());
        }
    }
    let mut count = QueryBuilder::new("SELECT count(*) FROM learning_cards c");
    filters(&mut count, &q, deck);
    let total: i64 = count
        .build_query_scalar()
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let limit = q.limit.unwrap_or(50);
    let mut sql = QueryBuilder::new("SELECT c.* FROM learning_cards c");
    filters(&mut sql, &q, deck);
    if let Some(after) = q.after {
        sql.push(" AND c.id>").push_bind(after);
    }
    sql.push(" ORDER BY c.id LIMIT ").push_bind(limit + 1);
    let mut rows: Vec<CardRow> = sql
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let next_cursor = if rows.len() > limit as usize {
        rows.truncate(limit as usize);
        rows.last().map(|r| r.id)
    } else {
        None
    };
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut contexts: BTreeMap<Uuid, CardBrowseContext> = ids
        .iter()
        .map(|id| (*id, CardBrowseContext::default()))
        .collect();
    if !ids.is_empty() {
        let memberships=sqlx::query("SELECT dc.card_id,d.id,d.name FROM deck_cards dc JOIN decks d ON d.id=dc.deck_id WHERE dc.card_id=ANY($1) ORDER BY lower(d.name),d.id").bind(&ids).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
        for row in memberships {
            contexts
                .get_mut(&row.get::<Uuid, _>("card_id"))
                .unwrap()
                .decks
                .push(CardDeck {
                    id: row.get("id"),
                    name: row.get("name"),
                });
        }
        let notes=sqlx::query(&format!("SELECT c.id,a.tags,a.comment FROM learning_cards c JOIN annotations a ON {NOTE_SCOPE} WHERE c.id=ANY($1) ORDER BY a.target_kind,a.id")).bind(&ids).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
        let mut tags: BTreeMap<Uuid, BTreeSet<String>> = BTreeMap::new();
        for row in notes {
            let id: Uuid = row.get("id");
            tags.entry(id)
                .or_default()
                .extend(row.get::<Vec<String>, _>("tags"));
            let comment: String = row.get("comment");
            let context = contexts.get_mut(&id).unwrap();
            if !comment.trim().is_empty() && context.comment_excerpt.chars().count() < 280 {
                let separator = if context.comment_excerpt.is_empty() {
                    ""
                } else {
                    "\n\n"
                };
                let remaining = 280 - context.comment_excerpt.chars().count();
                context
                    .comment_excerpt
                    .extend(separator.chars().chain(comment.chars()).take(remaining));
            }
        }
        for (id, values) in tags {
            contexts.get_mut(&id).unwrap().tags = values.into_iter().collect();
        }
    }
    let items = rows
        .iter()
        .map(|row| {
            Ok(CardBrowseItem {
                card: row.question()?,
                browse: contexts.remove(&row.id).unwrap_or_default(),
            })
        })
        .collect::<anyhow::Result<_>>()?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(Json(CardPage {
        items,
        next_cursor,
        total,
    }))
}
