use crate::{import::ImportedMatch, model::*};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

// Keep large source artifacts out of every summary/list response at the SQL boundary.
const MATCH_SUMMARY: &str =
    "SELECT id,filename,format,white,black,match_length,position_count,created_at FROM matches";

pub async fn import_match(
    pool: &PgPool,
    filename: &str,
    format: &str,
    bytes: &[u8],
    sgf: &str,
    imported: &ImportedMatch,
) -> Result<ImportResponse> {
    ensure!(
        !imported.positions.is_empty(),
        "The file contains no checker moves"
    );
    ensure!(
        imported.positions.len() <= 100_000,
        "Too many positions in one file"
    );
    let hash = format!("{:x}", Sha256::digest(bytes));
    let id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    let inserted: Option<Uuid> = sqlx::query_scalar("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(sha256) DO NOTHING RETURNING id")
        .bind(id).bind(filename).bind(format).bind(&hash).bind(&imported.players[0]).bind(&imported.players[1])
        .bind(imported.match_length).bind(imported.positions.len() as i32).bind(bytes).bind(sgf)
        .fetch_optional(&mut *tx).await?;
    if inserted.is_none() {
        let existing =
            sqlx::query_as::<_, MatchSummary>(&format!("{MATCH_SUMMARY} WHERE sha256=$1"))
                .bind(hash)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        return Ok(ImportResponse {
            r#match: existing,
            duplicate: true,
        });
    }
    for (batch, positions) in imported.positions.chunks(500).enumerate() {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) ",
        );
        query.push_values(positions.iter().enumerate(), |mut q, (index, p)| {
            q.push_bind(Uuid::new_v4())
                .push_bind(id)
                .push_bind((batch * 500 + index) as i32)
                .push_bind(p.game_no)
                .push_bind(p.ply)
                .push_bind(p.player)
                .push_bind(p.dice.to_vec())
                .push_bind(&p.board)
                .push_bind(&p.position_id)
                .push_bind(&p.match_id)
                .push_bind(&p.notation);
        });
        query.build().execute(&mut *tx).await?;
    }
    let summary = sqlx::query_as::<_, MatchSummary>(&format!("{MATCH_SUMMARY} WHERE id=$1"))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ImportResponse {
        r#match: summary,
        duplicate: false,
    })
}

pub async fn get_match(pool: &PgPool, id: Uuid) -> Result<Option<MatchSummary>> {
    Ok(sqlx::query_as(&format!("{MATCH_SUMMARY} WHERE id=$1"))
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn matches(pool: &PgPool, q: &str, after: Option<Uuid>, limit: i64) -> Result<MatchPage> {
    let mut query = QueryBuilder::<Postgres>::new(format!("{MATCH_SUMMARY} WHERE true"));
    if !q.is_empty() {
        let escaped = q
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        query
            .push(" AND search_text LIKE ")
            .push_bind(format!("%{escaped}%"));
    }
    if let Some(after) = after {
        query.push(" AND id > ").push_bind(after);
    }
    query.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let mut items = query
        .build_query_as::<MatchSummary>()
        .fetch_all(pool)
        .await?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|m| m.id)
    } else {
        None
    };
    Ok(MatchPage { items, next_cursor })
}

pub async fn get_position(pool: &PgPool, id: Uuid) -> Result<Option<Position>> {
    Ok(sqlx::query_as("SELECT * FROM positions WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn positions(
    pool: &PgPool,
    match_id: Option<Uuid>,
    position_id: Option<&str>,
    after: Option<Uuid>,
    limit: i64,
) -> Result<PositionPage> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM positions WHERE true");
    if let Some(id) = match_id {
        query.push(" AND match_uuid = ").push_bind(id);
    }
    if let Some(id) = position_id {
        query.push(" AND position_id = ").push_bind(id);
    }
    if let Some(cursor) = after {
        if let Some(match_id) = match_id {
            query
                .push(" AND ordinal > (SELECT ordinal FROM positions WHERE id = ")
                .push_bind(cursor)
                .push(" AND match_uuid = ")
                .push_bind(match_id)
                .push(")");
        } else {
            query.push(" AND id > ").push_bind(cursor);
        }
    }
    query
        .push(if match_id.is_some() {
            " ORDER BY ordinal LIMIT "
        } else {
            " ORDER BY id LIMIT "
        })
        .push_bind(limit + 1);
    let mut items = query.build_query_as::<Position>().fetch_all(pool).await?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|p| p.id)
    } else {
        None
    };
    Ok(PositionPage { items, next_cursor })
}

pub async fn dice_stats(pool: &PgPool, id: Uuid) -> Result<DiceStats> {
    let counts: Vec<(i32,i32,i64)> = sqlx::query_as("SELECT LEAST(dice[1],dice[2]) AS a, GREATEST(dice[1],dice[2]) AS b, count(*) FROM positions WHERE match_uuid=$1 GROUP BY a,b")
        .bind(id).fetch_all(pool).await?;
    let total = counts.iter().map(|(_, _, n)| n).sum();
    let mut rolls = Vec::new();
    for a in 1..=6 {
        for b in a..=6 {
            rolls.push(DiceCount {
                dice: [a, b],
                count: counts
                    .iter()
                    .find(|(x, y, _)| *x == a && *y == b)
                    .map(|(_, _, n)| *n)
                    .unwrap_or(0),
                expected_probability: if a == b { 1.0 / 36.0 } else { 2.0 / 36.0 },
            });
        }
    }
    Ok(DiceStats { total, rolls })
}

pub async fn get_job(pool: &PgPool, id: Uuid) -> Result<Option<Job>> {
    Ok(sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

pub async fn create_job(pool: &PgPool, position_id: Uuid, kind: &str, plies: i32) -> Result<Job> {
    let mut tx = pool.begin().await?;
    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,position_uuid,kind,plies,status) VALUES($1,$2,$3,$4,'queued') ON CONFLICT (position_uuid,kind,plies) WHERE status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(position_id).bind(kind).bind(plies).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id)
                .bind(serde_json::json!({"status":"queued","progress":0}))
                .execute(&mut *tx)
                .await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE position_uuid=$1 AND kind=$2 AND plies=$3 AND status IN ('queued','running')")
            .bind(position_id).bind(kind).bind(plies).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
        // A worker completed the conflicting job after the INSERT snapshot.
        // READ COMMITTED gives the next INSERT a fresh snapshot; try again.
    };
    tx.commit().await?;
    Ok(job)
}

pub async fn events(pool: &PgPool, job_id: Uuid, after: i64) -> Result<Vec<JobEvent>> {
    Ok(
        sqlx::query_as("SELECT * FROM job_events WHERE job_id=$1 AND id>$2 ORDER BY id LIMIT 100")
            .bind(job_id)
            .bind(after)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn latest_match_job(pool: &PgPool, match_id: Uuid) -> Result<Option<Job>> {
    Ok(sqlx::query_as("SELECT * FROM jobs WHERE match_uuid=$1 AND kind='match' ORDER BY created_at DESC,id DESC LIMIT 1")
        .bind(match_id).fetch_optional(pool).await?)
}

pub async fn create_match_job(
    pool: &PgPool,
    match_id: Uuid,
    plies: i32,
    total_games: i32,
) -> Result<Job> {
    let mut tx = pool.begin().await?;
    let job = loop {
        let inserted = sqlx::query_as::<_, Job>("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games) VALUES($1,$2,'match',$3,'queued',$4) ON CONFLICT(match_uuid,plies) WHERE kind='match' AND status IN ('queued','running') DO NOTHING RETURNING *")
            .bind(Uuid::new_v4()).bind(match_id).bind(plies).bind(total_games).fetch_optional(&mut *tx).await?;
        if let Some(job) = inserted {
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'queued',$2)")
                .bind(job.id).bind(serde_json::json!({"status":"queued","progress":0,"total_games":total_games,"completed_games":0}))
                .execute(&mut *tx).await?;
            break job;
        }
        let existing = sqlx::query_as("SELECT * FROM jobs WHERE match_uuid=$1 AND plies=$2 AND kind='match' AND status IN ('queued','running')")
            .bind(match_id).bind(plies).fetch_optional(&mut *tx).await?;
        if let Some(job) = existing {
            break job;
        }
    };
    tx.commit().await?;
    Ok(job)
}

pub async fn move_analyses(
    pool: &PgPool,
    job_id: Uuid,
    after: i64,
    limit: i64,
    errors_only: bool,
    player: Option<i32>,
    kind: Option<&str>,
) -> Result<MoveAnalysisPage> {
    let mut q = QueryBuilder::<Postgres>::new(
        "SELECT id,position_uuid,game_no,data FROM move_analyses WHERE job_id=",
    );
    q.push_bind(job_id).push(" AND id>").push_bind(after);
    if errors_only {
        q.push(" AND equity_loss>=0.04");
    }
    if let Some(player) = player {
        q.push(" AND player=").push_bind(player);
    }
    if let Some(kind) = kind {
        q.push(" AND kind=").push_bind(kind);
    }
    q.push(" ORDER BY id LIMIT ").push_bind(limit + 1);
    let rows = q
        .build_query_as::<(i64, Option<Uuid>, i32, serde_json::Value)>()
        .fetch_all(pool)
        .await?;
    let mut items: Vec<MoveAnalysis> = rows
        .into_iter()
        .map(|(id, position_uuid, game_no, data)| {
            Ok(MoveAnalysis {
                id,
                position_uuid,
                game_no,
                decision: serde_json::from_value(data)?,
            })
        })
        .collect::<Result<_>>()?;
    let next_cursor = if items.len() > limit as usize {
        items.pop();
        items.last().map(|row| row.id)
    } else {
        None
    };
    Ok(MoveAnalysisPage { items, next_cursor })
}
