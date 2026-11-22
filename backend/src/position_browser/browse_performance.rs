//! Ignored `EXPLAIN (ANALYZE, BUFFERS)` checks of the saved-position browser
//! on a synthetic library of 10,000 matches and 450,000 positions.
use super::*;
use serde_json::Value;
use sqlx::PgConnection;

/// 10 user databases; database 1 holds 1% of the matches. 2% of the matches
/// are collection-only copies whose source database is `source_database_id`.
/// 60% of the matches carry a complete native match analysis (every tenth a
/// second, older one), 40% imported XG analyses with cached projections (10%
/// both), 10% none; 5% of all positions are saved.
const DATASET: &str = r#"
INSERT INTO databases(id,name) SELECT md5('perf-db-'||k)::uuid,'Perf database '||k FROM generate_series(1,10) k;
INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,
    database_id,display_name,tournament,round,moves_cataloged,imported_analysis_loaded,source_metadata_loaded,
    collection_only,source_match_id,source_match_no,source_database_id,source_database_name,created_at)
SELECT md5('match-'||n)::uuid,'match-'||n||'.xg','xg',md5(n::text),'Player '||(n%400),'Opponent '||(n%37),
    CASE WHEN n%3=0 THEN 0 ELSE 5+2*(n%4) END,45,''::bytea,'',
    CASE WHEN n>9800 THEN '00000000-0000-0000-0000-000000000002'::uuid
         WHEN n%100=0 THEN md5('perf-db-1')::uuid ELSE md5('perf-db-'||(2+n%9))::uuid END,
    CASE WHEN n%2=0 THEN '' ELSE 'Match '||n END,
    CASE WHEN n%5=0 THEN 'Tournament '||(n%20) END,CASE WHEN n%5=0 THEN 'Round '||(n%7) END,
    true,true,true,n>9800,
    CASE WHEN n>9800 THEN md5('source-'||n)::uuid END,CASE WHEN n>9800 THEN n END,
    CASE WHEN n>9800 THEN CASE WHEN n%4=0 THEN md5('perf-db-1')::uuid ELSE md5('perf-db-'||(2+n%9))::uuid END END,
    CASE WHEN n>9800 THEN 'Perf database' END,timestamptz '2026-01-01 00:00:00+00'+make_interval(secs=>n)
FROM generate_series(1,10000) n;
INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford)
SELECT md5('match-'||n)::uuid,g,15,g-1,0,false FROM generate_series(1,10000) n CROSS JOIN generate_series(1,3) g;
INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation)
SELECT md5('position-'||n||'-'||o)::uuid,md5('match-'||n)::uuid,o,(o-1)/15+1,(o-1)%15+1,o%2,
    ARRAY[1+o%6,1+(o*7+n)%6],array_fill(0,ARRAY[26]),substr(md5(n::text||'-'||o),1,14),'MIEEAAAAAAAE',
    (24-o%6)||'/'||(20-o%6)
FROM generate_series(1,10000) n CROSS JOIN generate_series(1,45) o;
INSERT INTO match_moves(match_uuid,game_no,move_no,kind,player,notation,position_uuid,board,dice,position_id,gnubg_match_id)
SELECT p.match_uuid,p.game_no,p.ply,'checker',p.player,p.notation,p.id,p.board,p.dice,p.position_id,p.gnubg_match_id FROM positions p;
INSERT INTO saved_positions(position_uuid)
SELECT md5('position-'||n||'-'||o)::uuid FROM generate_series(1,10000) n CROSS JOIN generate_series(1,45) o WHERE (n*45+o)%20=0;
INSERT INTO jobs(id,kind,plies,status,match_uuid,total_games,completed_games,result,engine_snapshot,created_at)
SELECT md5('job-'||n||'-'||v)::uuid,'match',CASE WHEN v=1 THEN 3 ELSE 2 END,'completed',md5('match-'||n)::uuid,3,3,
    '{"engine":"GNU Backgammon"}'::jsonb,'{"engine_id":"gnubg"}'::jsonb,timestamptz '2026-06-01 00:00:00+00'-make_interval(days=>v)
FROM generate_series(1,10000) n CROSS JOIN generate_series(1,2) v WHERE n%10<=5 AND (v=1 OR n%10=0);
INSERT INTO match_analysis_games(job_id,game_no,summary)
SELECT j.id,g,'{}'::jsonb FROM jobs j CROSS JOIN generate_series(1,3) g WHERE j.kind='match';
INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data)
SELECT j.id,p.id,p.game_no,p.ply,p.player,'checker',((abs(hashtext(p.id::text))%200)/1000.0),
    jsonb_build_object('best','8/5 6/5','played',p.notation,'evaluation_status','evaluated','played_equity',-0.2,'best_equity',-0.1,
        'equity_loss',((abs(hashtext(p.id::text))%200)/1000.0),'decision_no',p.ply)
FROM jobs j JOIN positions p ON p.match_uuid=j.match_uuid WHERE j.kind='match';
INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
SELECT md5('imported-'||p.id)::uuid,p.match_uuid,p.id,p.ordinal,p.game_no,p.ply,'checker',
    '{"engine":"eXtreme Gammon","engine_version":"2.19","depth":"3-ply","candidates":[{"move":"8/5 6/5","equity":-0.1}]}'::jsonb
FROM positions p JOIN matches m ON m.id=p.match_uuid WHERE split_part(split_part(m.filename,'-',2),'.',1)::int%10 BETWEEN 5 AND 8;
INSERT INTO imported_analysis_projections(imported_analysis_id,revision,source_revision,projection)
SELECT ia.id,1,0,jsonb_build_object('plies',3,'depth','3-ply','decision',jsonb_build_object('equity_loss',((abs(hashtext(ia.id::text))%200)/1000.0)))
FROM imported_analyses ia;
INSERT INTO analysis_selections(match_id,analysis_id)
SELECT md5('match-'||n)::uuid,'imported:'||md5('match-'||n)::uuid FROM generate_series(1,10000) n WHERE n%50=5;
INSERT INTO annotations(id,position_uuid,tags,comment,version)
SELECT md5('note-'||s.position_uuid)::uuid,s.position_uuid,ARRAY['tag'||(abs(hashtext(s.position_uuid::text))%12)],'Note '||s.position_uuid,1
FROM (SELECT position_uuid FROM saved_positions ORDER BY position_uuid LIMIT 2000) s;
INSERT INTO annotations(id,match_uuid,tags,comment,version)
SELECT md5('match-note-'||n)::uuid,md5('match-'||n)::uuid,ARRAY['opening'],'Match note '||n,1 FROM generate_series(1,10000,20) n;
INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id,library_match_id)
SELECT md5('session-'||n)::uuid,'{"play_mode":"competition","phase":"playing","match_length":"7"}'::jsonb,'gnubg','','native-'||n,md5('match-'||n)::uuid
FROM generate_series(7,10000,500) n;
"#;

/// Rows read from `relation` over all loops of the plan.
fn relation_rows(node: &Value, relation: &str) -> f64 {
    let own = if node["Relation Name"] == relation {
        node["Actual Rows"].as_f64().unwrap_or(0.0) * node["Actual Loops"].as_f64().unwrap_or(1.0)
    } else {
        0.0
    };
    own + node["Plans"]
        .as_array()
        .map(|plans| plans.iter().map(|p| relation_rows(p, relation)).sum())
        .unwrap_or(0.0)
}

fn summary(plan: &Value) -> String {
    let top = &plan[0];
    format!(
        "planning {:.3} ms, execution {:.3} ms, shared hit {} read {}",
        top["Planning Time"].as_f64().unwrap_or_default(),
        top["Execution Time"].as_f64().unwrap_or_default(),
        top["Plan"]["Shared Hit Blocks"],
        top["Plan"]["Shared Read Blocks"],
    )
}

/// The warm plan (third run) of a 25-row page.
async fn explain(
    connection: &mut PgConnection,
    filters: &PositionFilterDefinition,
    offset: Option<i64>,
    anchor: Option<Uuid>,
) -> Value {
    let mut plan = Value::Null;
    for _ in 0..3 {
        let mut sql = QueryBuilder::<Postgres>::new("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) ");
        push_browse_query(
            &mut sql,
            filters,
            browse_direction(filters),
            25,
            offset,
            anchor,
        );
        plan = sql
            .build_query_scalar()
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    }
    plan
}

fn report(name: &str, plan: &Value) -> String {
    format!(
        "{name}: {}; move_analyses {} rows, match_analysis_games {} rows",
        summary(plan),
        relation_rows(&plan[0]["Plan"], "move_analyses"),
        relation_rows(&plan[0]["Plan"], "match_analysis_games"),
    )
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a large synthetic dataset"]
async fn saved_position_browse_plans_on_a_large_library(pool: PgPool) {
    sqlx::raw_sql(DATASET).execute(&pool).await.unwrap();
    sqlx::query("VACUUM ANALYZE").execute(&pool).await.unwrap();
    let mut connection = pool.acquire().await.unwrap();
    // Like every connection of the application pool (database_pool.rs).
    sqlx::query("SET jit = off")
        .execute(&mut *connection)
        .await
        .unwrap();
    let small: Uuid = sqlx::query_scalar("SELECT md5('perf-db-1')::uuid")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let (saved, in_small): (i64, i64) = sqlx::query_as(
        "SELECT count(*),count(*) FILTER (WHERE coalesce(m.source_database_id,m.database_id)=$1) FROM saved_positions s JOIN positions p ON p.id=s.position_uuid JOIN matches m ON m.id=p.match_uuid",
    )
    .bind(small)
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    println!(
        "dataset: 10000 matches, 450000 positions, {saved} saved, {in_small} saved in database 1"
    );
    let default = PositionFilterDefinition::default();
    let scoped = PositionFilterDefinition {
        database_id: Some(small),
        ..PositionFilterDefinition::default()
    };
    let loss = PositionFilterDefinition {
        sort: PositionBrowserSort::Loss,
        ..PositionFilterDefinition::default()
    };
    let scoped_loss = PositionFilterDefinition {
        database_id: Some(small),
        sort: PositionBrowserSort::Loss,
        ..PositionFilterDefinition::default()
    };
    let ready = PositionFilterDefinition {
        analysis_state: Some(PositionAnalysisState::Ready),
        ..PositionFilterDefinition::default()
    };
    let anchor: Uuid = sqlx::query_scalar(
        "SELECT p.id FROM saved_positions s JOIN positions p ON p.id=s.position_uuid JOIN matches m ON m.id=p.match_uuid ORDER BY lower(coalesce(nullif(m.display_name,''),m.filename)),m.id,p.ordinal,p.id OFFSET 24 LIMIT 1",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    // Sorts and filters without analysis read it for the page rows only.
    for (name, filters, offset, anchor) in [
        ("default sort, page 1", &default, Some(0), None),
        ("default sort, page 400", &default, Some(9975), None),
        ("database 1, page 1", &scoped, Some(0), None),
        ("default sort, after anchor", &default, None, Some(anchor)),
    ] {
        let plan = explain(&mut connection, filters, offset, anchor).await;
        println!("{}", report(name, &plan));
        for relation in ["move_analyses", "match_analysis_games", "imported_analyses"] {
            let rows = relation_rows(&plan[0]["Plan"], relation);
            assert!(
                rows <= 260.0,
                "{name} reads {rows} {relation} rows: {plan:#}"
            );
        }
        if filters.database_id.is_some() {
            assert!(
                plan.to_string().contains("matches_effective_database_idx"),
                "{plan:#}"
            );
        }
    }
    // Loss and readiness need the analysis of every saved row in scope; the
    // selection is evaluated per match of those rows, not for the library.
    for (name, filters) in [
        ("loss sort, page 1", &loss),
        ("database 1 by loss, page 1", &scoped_loss),
        ("ready filter, page 1", &ready),
    ] {
        let plan = explain(&mut connection, filters, Some(0), None).await;
        println!("{}", report(name, &plan));
        if filters.database_id.is_some() {
            let rows = relation_rows(&plan[0]["Plan"], "match_analysis_games");
            assert!(rows <= 10.0 * in_small as f64, "{name}: {rows} rows");
        }
    }
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT position_uuid FROM saved_positions ORDER BY position_uuid LIMIT 100",
    )
    .fetch_all(&mut *connection)
    .await
    .unwrap();
    let mut plan = Value::Null;
    for _ in 0..3 {
        plan = sqlx::query_scalar(&format!(
            "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {}",
            crate::learning::position_cards_sql()
        ))
        .bind(&ids)
        .bind(None::<String>)
        .bind(true)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    }
    println!("{}", report("card content of 100 saved positions", &plan));
    let rows = relation_rows(&plan[0]["Plan"], "match_analysis_games");
    assert!(rows <= 1000.0, "card content reads {rows} rows: {plan:#}");
}
