//! Versioned, engine-free classification of saved immutable position contexts.
use crate::game_plan::{self, GamePlan};
use sqlx::{PgConnection, Postgres, QueryBuilder};
use uuid::Uuid;
const BATCH: i64 = 512;
const MISSING: &str = "FROM saved_positions s
    JOIN positions p ON p.id=s.position_uuid JOIN matches m ON m.id=p.match_uuid
    WHERE NOT EXISTS(SELECT 1 FROM position_game_plans c WHERE c.position_uuid=p.id AND c.classifier=$1)";

pub(crate) fn label(plan: GamePlan) -> &'static str {
    match plan {
        GamePlan::Opening => "opening",
        GamePlan::PureRace => "pure_race",
        GamePlan::Racing => "racing",
        GamePlan::Attacking => "attacking",
        GamePlan::Priming => "priming",
        GamePlan::Anchoring => "anchoring",
    }
}
pub(crate) async fn pending(conn: &mut PgConnection, database: Option<Uuid>) -> sqlx::Result<bool> {
    sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 {MISSING}
        AND ($2::uuid IS NULL OR coalesce(m.source_database_id,m.database_id)=$2))"
    ))
    .bind(game_plan::CLASSIFIER)
    .bind(database)
    .fetch_one(conn)
    .await
}

/// A bookmark refreshes its own position; the worker drains older bookmarks in pages.
pub(crate) async fn refresh(conn: &mut PgConnection, position: Option<Uuid>) -> sqlx::Result<bool> {
    let rows: Vec<(Uuid, Vec<i32>, i32)> = sqlx::query_as(&format!(
        "SELECT p.id,p.board,p.player {MISSING} AND ($2::uuid IS NULL OR p.id=$2)
         ORDER BY p.id LIMIT $3"
    ))
    .bind(game_plan::CLASSIFIER)
    .bind(position)
    .bind(BATCH)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(false);
    }
    let full = rows.len() == BATCH as usize;
    let values: Vec<_> = rows
        .into_iter()
        .map(|(id, board, player)| {
            let plan = game_plan::classify_for_player(&board, player).map(label);
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
    sql.build().execute(conn).await?;
    Ok(full)
}
