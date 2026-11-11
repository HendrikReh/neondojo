//! Versioned, engine-free classification of saved immutable position contexts.
use crate::{
    api::{ApiError, ApiResult},
    game_plan::{self, GamePlan},
};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;
const BATCH: i64 = 512;
const MAX_BATCHES: usize = 8;

pub(crate) fn label(plan: GamePlan) -> &'static str {
    match plan {
        GamePlan::PureRace => "pure_race",
        GamePlan::Racing => "racing",
        GamePlan::Attacking => "attacking",
        GamePlan::Priming => "priming",
        GamePlan::Anchoring => "anchoring",
    }
}
/// Bound each request's work. Large preexisting collections make durable progress
/// and return an explicit retry instead of searching a partially prepared cache.
pub(crate) async fn ensure_saved(pool: &PgPool, database: Option<Uuid>) -> ApiResult<()> {
    for batch in 0..=MAX_BATCHES {
        let rows:Vec<(Uuid,Vec<i32>,i32)>=sqlx::query_as("SELECT p.id,p.board,p.player FROM positions p JOIN saved_positions s ON s.position_uuid=p.id JOIN matches m ON m.id=p.match_uuid WHERE ($1::uuid IS NULL OR coalesce(m.source_database_id,m.database_id)=$1) AND NOT EXISTS(SELECT 1 FROM position_game_plans c WHERE c.position_uuid=p.id AND c.classifier=$2) ORDER BY p.id LIMIT $3").bind(database).bind(game_plan::CLASSIFIER).bind(if batch==MAX_BATCHES{1}else{BATCH}).fetch_all(pool).await.map_err(anyhow::Error::from)?;
        if rows.is_empty() {
            return Ok(());
        }
        if batch == MAX_BATCHES {
            return Err(ApiError::invalid(
                "Game-plan search is preparing this existing collection in bounded batches; retry this search to continue, or choose a smaller database",
            ));
        }
        let values: Vec<_> = rows
            .into_iter()
            .map(|(id, board, player)| {
                let plan = <&[i32; 26]>::try_from(board.as_slice())
                    .ok()
                    .and_then(|b| game_plan::classify(b).ok())
                    .map(|p| label(if player == 0 { p.white } else { p.black }));
                (id, plan)
            })
            .collect();
        let mut sql = QueryBuilder::<Postgres>::new(
            "INSERT INTO position_game_plans(position_uuid,classifier,game_plan) ",
        );
        sql.push_values(values, |mut row, (id, plan)| {
            row.push_bind(id)
                .push_bind(game_plan::CLASSIFIER)
                .push_bind(plan);
        });
        sql.push(" ON CONFLICT(position_uuid,classifier) DO NOTHING");
        sql.build()
            .execute(pool)
            .await
            .map_err(anyhow::Error::from)?;
    }
    unreachable!("bounded cache loop always returns")
}
