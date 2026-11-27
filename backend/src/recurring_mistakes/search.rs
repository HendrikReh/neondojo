//! Filtered, paginated list of one player's analyzed decisions.
//!
//! The list reads the same selected analyses as the mistake groups, so a
//! decision found here can be practiced through the same references.
use super::{
    MistakeKind, MistakePracticeQuestion, kind as mistake_kind, source::SourceDecision,
    validate_loss, validate_player,
};
use crate::json_errors::{Json, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    cube_context::cube_context,
    game_plan::GamePlan,
};
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool, Postgres, postgres::PgArguments, query::QueryScalar};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
/// Decisions a page may skip: far more than one player has in a large
/// library, so every page the result list offers stays reachable.
const MAX_OFFSET: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Checker,
    CubeOffer,
    CubeResponse,
}

impl DecisionKind {
    fn name(self) -> &'static str {
        match self {
            Self::Checker => "checker",
            Self::CubeOffer => "cube_offer",
            Self::CubeResponse => "cube_response",
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct DecisionSearchQuery {
    /// Exact player name, ignoring case and surrounding spaces.
    pub player: String,
    pub database_id: Option<Uuid>,
    pub kind: Option<DecisionKind>,
    /// Only errors of this type; implies a positive loss.
    pub mistake_kind: Option<MistakeKind>,
    pub game_plan: Option<GamePlan>,
    /// Minimum normalized equity loss; 0 includes correct decisions.
    pub min_loss: Option<f64>,
    pub max_loss: Option<f64>,
    /// Points the deciding player still needs; match play only.
    pub away: Option<i32>,
    /// Points the opponent still needs; match play only.
    pub opponent_away: Option<i32>,
    /// Cube value before the decision.
    pub cube_value: Option<i32>,
    /// self, opponent or centered, relative to the deciding player.
    pub cube_owner: Option<CubeOwnerFilter>,
    /// Order by equity loss; the largest loss comes first by default.
    pub direction: Option<LossDirection>,
    /// One-based page.
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LossDirection {
    Asc,
    #[default]
    Desc,
}

impl LossDirection {
    /// Both orders follow decision_index_player_loss: descending reads it
    /// forward, ascending reverses every key so it is read backward. The row
    /// number within the match only orders otherwise equal decisions, such as
    /// a checker play and a cube action at the same imported move.
    fn order_by(self) -> &'static str {
        match self {
            Self::Desc => "d.equity_loss DESC,d.match_id,d.game_no,d.decision_no,d.seq",
            Self::Asc => {
                "d.equity_loss,d.match_id DESC,d.game_no DESC,d.decision_no DESC,d.seq DESC"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CubeOwnerFilter {
    #[serde(rename = "self")]
    Own,
    Opponent,
    Centered,
}

#[derive(Serialize, ToSchema)]
pub struct DecisionHit {
    #[serde(flatten)]
    pub question: MistakePracticeQuestion,
    pub game_plan: GamePlan,
    /// Error type when the played action was not best.
    pub mistake_kind: Option<MistakeKind>,
    pub played: String,
    pub best: String,
    pub equity_loss: f64,
    /// 0 is money play.
    pub match_length: i32,
    /// Points still needed by the deciding player and the opponent.
    pub away: Option<[i32; 2]>,
    pub crawford: bool,
    pub cube_value: i32,
    /// Absolute color; absent while the cube is centered.
    pub cube_owner: Option<i32>,
    /// True when the practice endpoints accept this reference.
    pub practicable: bool,
}

#[derive(Serialize, ToSchema)]
pub struct DecisionSearchPage {
    #[schema(value_type = Vec<DecisionHit>)]
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
}

pub(super) struct Context {
    pub(super) match_length: i32,
    pub(super) away: Option<[i32; 2]>,
    pub(super) crawford: bool,
    pub(super) cube_value: i32,
    pub(super) cube_owner: Option<i32>,
}

pub(super) fn context(row: &SourceDecision) -> Option<Context> {
    let player = row.decision.player;
    let decoded = cube_context(&row.question.position.gnubg_match_id, player).ok()?;
    let own = usize::try_from(player).ok()?;
    Some(Context {
        match_length: decoded.match_length,
        away: decoded.away.map(|away| [away[own], away[1 - own]]),
        crawford: decoded.crawford,
        cube_value: decoded.cube_value,
        cube_owner: decoded.cube_owner,
    })
}

/// Cube ownership relative to the deciding player.
pub(super) fn owner_filter(context: &Context, player: i32) -> CubeOwnerFilter {
    match context.cube_owner {
        None => CubeOwnerFilter::Centered,
        Some(owner) if owner == player => CubeOwnerFilter::Own,
        Some(_) => CubeOwnerFilter::Opponent,
    }
}

impl CubeOwnerFilter {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Own => "self",
            Self::Opponent => "opponent",
            Self::Centered => "centered",
        }
    }
}

impl DecisionHit {
    pub(super) fn new(row: SourceDecision, context: &Context, practicable: bool) -> Self {
        let error = (row.decision.equity_loss > 0.0).then(|| mistake_kind(&row.decision));
        Self {
            game_plan: row.plan,
            mistake_kind: error,
            match_length: context.match_length,
            away: context.away,
            crawford: context.crawford,
            cube_value: context.cube_value,
            cube_owner: context.cube_owner,
            practicable,
            played: row.decision.played,
            best: row.decision.best,
            equity_loss: row.decision.equity_loss,
            question: row.question,
        }
    }
}

/// Filters on the indexed decision; $3 kind, $4 mistake kind, $5 game plan,
/// $6/$7 loss range, $8/$9 away scores, $10/$11 cube. Every column is a key
/// or an included column of decision_index_player_loss, so the filters are
/// evaluated on index entries.
const FILTER: &str = "($3::text IS NULL OR d.question_kind=$3)
   AND ($4::text IS NULL OR (d.equity_loss>0 AND d.mistake_kind=$4))
   AND ($5::text IS NULL OR d.game_plan=$5)
   AND d.equity_loss>=$6
   AND ($7::float8 IS NULL OR d.equity_loss<=$7)
   AND ($8::int IS NULL OR d.away_own=$8)
   AND ($9::int IS NULL OR d.away_opponent=$9)
   AND ($10::int IS NULL OR d.cube_value=$10)
   AND ($11::text IS NULL OR d.cube_owner=$11)";

type Scalar<'q, O> = QueryScalar<'q, Postgres, O, PgArguments>;

/// One validated decision search: $1 the player's lowercase names, $2 the
/// database, then the filters.
pub(super) struct Search {
    player: Vec<String>,
    database: Option<Uuid>,
    kind: Option<&'static str>,
    mistake: Option<&'static str>,
    plan: Option<&'static str>,
    min_loss: f64,
    max_loss: Option<f64>,
    away: Option<i32>,
    opponent_away: Option<i32>,
    cube_value: Option<i32>,
    owner: Option<&'static str>,
    direction: LossDirection,
    page: usize,
    limit: usize,
}

impl Search {
    /// Validate a request and resolve its player to the profile's names.
    pub(super) async fn new(pool: &PgPool, q: DecisionSearchQuery) -> ApiResult<Self> {
        let player = validate_player(&q.player)?;
        let min_loss = validate_loss(q.min_loss.or(Some(0.0)))?;
        let player = crate::player_scope::names(pool, &player).await?;
        if q.max_loss
            .is_some_and(|max| !max.is_finite() || max < min_loss)
        {
            return Err(ApiError::invalid(
                "max_loss must be finite and at least min_loss",
            ));
        }
        if [q.away, q.opponent_away]
            .into_iter()
            .flatten()
            .any(|away| !(1..=64).contains(&away))
            || q.cube_value
                .is_some_and(|value| !crate::backgammon::board::is_valid_cube_value(value))
        {
            return Err(ApiError::invalid(format!(
                "Away scores must be 1 to 64 and the cube value a power of two up to {}",
                crate::backgammon::board::MAX_CUBE_VALUE
            )));
        }
        let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
        let page = q.page.unwrap_or(1);
        let skipped = page
            .checked_sub(1)
            .and_then(|index| index.checked_mul(limit));
        if !(1..=MAX_LIMIT).contains(&limit) || skipped.is_none_or(|skipped| skipped > MAX_OFFSET) {
            return Err(ApiError::invalid(format!(
                "limit must be 1 to {MAX_LIMIT} and page must start within the first {MAX_OFFSET} decisions"
            )));
        }
        if let Some(id) = q.database_id {
            super::source::require_database(pool, id).await?;
        }
        Ok(Self {
            player,
            database: q.database_id,
            kind: q.kind.map(DecisionKind::name),
            mistake: q.mistake_kind.map(super::history::kind_name),
            plan: q.game_plan.map(crate::position_game_plans::label),
            min_loss,
            max_loss: q.max_loss,
            away: q.away,
            opponent_away: q.opponent_away,
            cube_value: q.cube_value,
            owner: q.cube_owner.map(CubeOwnerFilter::name),
            direction: q.direction.unwrap_or_default(),
            page,
            limit,
        })
    }

    pub(super) fn player(&self) -> &[String] {
        &self.player
    }

    pub(super) fn database(&self) -> Option<Uuid> {
        self.database
    }

    fn offset(&self) -> usize {
        (self.page - 1) * self.limit
    }

    /// A database filter joins the match. Without one, no join is needed:
    /// every indexed decision belongs to an existing match.
    fn scope(&self) -> (&'static str, &'static str) {
        if self.database.is_some() {
            ("JOIN matches m ON m.id=d.match_id", "m.database_id=$2")
        } else {
            ("", "$2::uuid IS NULL")
        }
    }

    pub(super) fn count_sql(&self) -> String {
        let (join, scope) = self.scope();
        format!(
            "SELECT count(*) FROM decision_index d {join}
             WHERE d.player_name=ANY($1) AND {scope} AND {FILTER}"
        )
    }

    /// $12 limit, $13 offset. Each of the player's names is read in index
    /// order up to the end of the page, so only those entries are sorted, and
    /// only the hits of the chosen page are read from the table.
    pub(super) fn page_sql(&self) -> String {
        let (join, scope) = self.scope();
        let order = self.direction.order_by();
        format!(
            "WITH page AS MATERIALIZED (
                 SELECT d.* FROM (SELECT DISTINCT unnest($1::text[]) AS name) names
                 CROSS JOIN LATERAL (
                     SELECT d.match_id,d.seq,d.equity_loss,d.game_no,d.decision_no
                     FROM decision_index d {join}
                     WHERE d.player_name=names.name AND {scope} AND {FILTER}
                     ORDER BY {order} LIMIT $12+$13
                 ) d
                 ORDER BY {order} LIMIT $12 OFFSET $13
             )
             SELECT x.hit FROM page d JOIN decision_index x ON x.match_id=d.match_id AND x.seq=d.seq
             ORDER BY {order}"
        )
    }

    /// Bind $1 to $11. Which filters are set changes the best plan, so each
    /// request is planned for its own values.
    pub(super) fn bind<'q, O>(&'q self, query: Scalar<'q, O>) -> Scalar<'q, O> {
        query
            .bind(&self.player)
            .bind(self.database)
            .bind(self.kind)
            .bind(self.mistake)
            .bind(self.plan)
            .bind(self.min_loss)
            .bind(self.max_loss)
            .bind(self.away)
            .bind(self.opponent_away)
            .bind(self.cube_value)
            .bind(self.owner)
            .persistent(false)
    }

    /// Bind $1 to $13 of the page query.
    pub(super) fn bind_page<'q, O>(&'q self, query: Scalar<'q, O>) -> ApiResult<Scalar<'q, O>> {
        Ok(self
            .bind(query)
            .bind(i64::try_from(self.limit).map_err(anyhow::Error::from)?)
            .bind(i64::try_from(self.offset()).map_err(anyhow::Error::from)?))
    }

    /// The page and the total. Read both in one snapshot so they agree.
    pub(super) async fn read(
        &self,
        connection: &mut PgConnection,
    ) -> ApiResult<(Vec<serde_json::Value>, usize)> {
        let total: i64 = self
            .bind(sqlx::query_scalar(&self.count_sql()))
            .fetch_one(&mut *connection)
            .await
            .map_err(anyhow::Error::from)?;
        let total = usize::try_from(total).unwrap_or(0);
        // A page after the last result is empty; skip reading it.
        if self.offset() >= total {
            return Ok((Vec::new(), total));
        }
        let items = self
            .bind_page(sqlx::query_scalar(&self.page_sql()))?
            .fetch_all(&mut *connection)
            .await
            .map_err(anyhow::Error::from)?;
        Ok((items, total))
    }
}

pub(crate) async fn search_decisions_service(
    s: &AppState,
    q: DecisionSearchQuery,
) -> Result<DecisionSearchPage, crate::domain_error::DomainError> {
    let search = Search::new(&s.pool, q).await?;
    let mut tx = crate::database_pool::begin_read_snapshot(&s.pool).await?;
    super::index::require_current(s, &mut tx, search.database(), search.player(), false).await?;
    let (items, total) = search.read(&mut tx).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    Ok(DecisionSearchPage {
        items,
        total,
        page: search.page,
        limit: search.limit,
    })
}

#[utoipa::path(get, path = "/api/study/decisions", params(DecisionSearchQuery),
    responses((status = 200, body = DecisionSearchPage), (status = 422, body = ErrorBody),
              (status = 503, description = "Decision index is being prepared", body = ErrorBody)),
    tag = "Study")]
pub(super) async fn search(
    State(s): State<AppState>,
    Query(q): Query<DecisionSearchQuery>,
) -> ApiResult<Json<DecisionSearchPage>> {
    Ok(Json(search_decisions_service(&s, q).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str =
        "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])";

    fn ok<T>(result: ApiResult<T>) -> T {
        result.unwrap_or_else(|ApiError(status, body)| panic!("{status}: {}", body.error))
    }

    fn search(database: Option<Uuid>, limit: usize) -> Search {
        Search {
            player: vec!["alice".into()],
            database,
            kind: None,
            mistake: None,
            plan: None,
            min_loss: 0.0,
            max_loss: None,
            away: None,
            opponent_away: None,
            cube_value: None,
            owner: None,
            direction: LossDirection::Desc,
            page: 1,
            limit,
        }
    }

    async fn add_decision(pool: &PgPool, id: Uuid, seq: i32, loss: f64) {
        sqlx::query(
            "INSERT INTO decision_index(match_id,seq,game_no,decision_no,player_name,question_kind,
                 game_plan,mistake_kind,equity_loss,match_length,cube_value,cube_owner,played_on,hit)
             VALUES($1,$2,1,$2+1,'alice','checker','racing','checker',$3,7,1,'centered',
                    current_date,jsonb_build_object('seq',$2))",
        )
        .bind(id)
        .bind(seq)
        .bind(loss)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn page_and_total_come_from_one_snapshot(pool: PgPool) {
        let id = crate::store::import_match(
            &pool,
            "snapshot.sgf",
            "sgf",
            SOURCE.as_bytes(),
            SOURCE,
            &crate::import::parse_sgf(SOURCE).unwrap(),
        )
        .await
        .unwrap()
        .r#match
        .id;
        let database: Uuid = sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO decision_index_matches(match_id,fingerprint) VALUES($1,'test')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        add_decision(&pool, id, 0, 0.1).await;
        for scope in [None, Some(database)] {
            let search = search(scope, 1);
            let mut tx = crate::database_pool::begin_read_snapshot(&pool)
                .await
                .unwrap();
            let before = ok(search.read(&mut tx).await);
            assert_eq!(before.1, 1);
            // A rebuild publishes a larger loss between the total and the page.
            add_decision(&pool, id, 1, 0.2).await;
            let during = ok(search.read(&mut tx).await);
            assert_eq!(during, before, "the snapshot must hide the new decision");
            tx.commit().await.unwrap();
            let mut tx = crate::database_pool::begin_read_snapshot(&pool)
                .await
                .unwrap();
            let after = ok(search.read(&mut tx).await);
            assert_eq!(after.1, 2);
            assert_eq!(after.0, vec![serde_json::json!({"seq": 1})]);
            sqlx::query("DELETE FROM decision_index WHERE match_id=$1 AND seq=1")
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
    }
}
