use neondojo::recurring_mistakes::query_plans;
use serde_json::Value;
use sqlx::PgPool;

/// Every plan node below `node`, including itself.
fn nodes(node: &Value) -> Vec<&Value> {
    let mut found = vec![node];
    for child in node["Plans"].as_array().into_iter().flatten() {
        found.extend(nodes(child));
    }
    found
}

fn uses_index(plan: &Value, index: &str) -> bool {
    nodes(&plan[0]["Plan"])
        .iter()
        .any(|node| node["Index Name"] == index)
}

fn reads_relation(plan: &Value, relation: &str) -> bool {
    nodes(&plan[0]["Plan"])
        .iter()
        .any(|node| node["Relation Name"] == relation)
}

/// Executions of the scans of `index`.
fn index_loops(plan: &Value, index: &str) -> u64 {
    nodes(&plan[0]["Plan"])
        .iter()
        .filter(|node| node["Index Name"] == index)
        .filter_map(|node| node["Actual Loops"].as_u64())
        .sum()
}

fn summary(plan: &Value) -> String {
    let root = &plan[0]["Plan"];
    format!(
        "planning {} ms, execution {} ms, rows {}, shared hit {} read {}",
        plan[0]["Planning Time"],
        plan[0]["Execution Time"],
        root["Actual Rows"],
        root["Shared Hit Blocks"],
        root["Shared Read Blocks"]
    )
}

/// The second, warm run of each query.
async fn decision_search_plans(pool: &PgPool, query: &str) -> Vec<(&'static str, Value)> {
    let explain = || async {
        query_plans::decision_search(pool, query)
            .await
            .unwrap_or_else(|_| panic!("explain {query}"))
    };
    explain().await;
    let plans = explain().await;
    for (step, plan) in &plans {
        println!("{query} {step}: {}", summary(plan));
    }
    plans
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a large synthetic dataset"]
async fn decision_search_stays_index_bound_on_a_large_library(pool: PgPool) {
    sqlx::raw_sql(include_str!("fixtures/decision_search_library.sql"))
        .execute(&pool)
        .await
        .unwrap();
    let adopted = query_plans::adopt_current_fingerprints(&pool)
        .await
        .unwrap_or_else(|_| panic!("adopt fingerprints"));
    assert_eq!(adopted, 10_000);
    sqlx::query("VACUUM ANALYZE").execute(&pool).await.unwrap();
    // Measure with the application's session settings: JIT is off.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::Executor::execute(connection, "SET jit = off").await?;
                Ok(())
            })
        })
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let hero: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE player_name='hero'),
                count(*) FILTER (WHERE player_name='hero' AND equity_loss>=0.05),
                count(*) FILTER (WHERE player_name='hero' AND equity_loss>=0.05
                                   AND question_kind='cube_offer' AND cube_value=4)
         FROM decision_index",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(hero, (94_500, 31_571, 215));

    // Hero's matches in the scope, and those without a match analysis.
    for (query, matches, unanalyzed) in [
        ("player=Hero&min_loss=0.05", 5_000, 500),
        (
            "player=Hero&min_loss=0.05&kind=cube_offer&cube_value=4",
            5_000,
            500,
        ),
        (
            "player=Hero&min_loss=0.05&database_id=81a18d0f-38c1-4db8-81c0-a743adbc4480",
            2_500,
            0,
        ),
        ("player=Hero&page=1801", 5_000, 500),
    ] {
        let database = query.contains("database_id");
        let plans = decision_search_plans(&pool, query).await;
        let plan = |step: &str| &plans.iter().find(|(name, _)| *name == step).unwrap().1;
        for step in ["legacy_size", "legacy_ids"] {
            // Nothing is pending: one probe of the empty partial index.
            assert!(
                uses_index(plan(step), "matches_pending_imported_catalog"),
                "{:#}",
                plan(step)
            );
        }
        let candidates = plan("candidates");
        assert_eq!(candidates[0]["Plan"]["Actual Rows"], matches);
        if !database {
            assert!(uses_index(candidates, "matches_library_white_player"));
            assert!(uses_index(candidates, "matches_library_black_player"));
        }
        // Only matches without a match analysis look for position analyses.
        assert_eq!(index_loops(candidates, "positions_match_id"), unanalyzed);
        let count = plan("count");
        assert!(uses_index(count, "decision_index_player_loss"), "{count:#}");
        let page = plan("page");
        assert!(uses_index(page, "decision_index_player_loss"), "{page:#}");
        assert_eq!(page[0]["Plan"]["Actual Rows"], 50);
        // Only the returned hits are read from the table.
        assert_eq!(index_loops(page, "decision_index_pkey"), 50);
        assert_eq!(reads_relation(count, "matches"), database, "{count:#}");
        assert_eq!(reads_relation(page, "matches"), database, "{page:#}");
        for node in nodes(&count[0]["Plan"])
            .into_iter()
            .chain(nodes(&page[0]["Plan"]))
        {
            assert_ne!(
                (node["Node Type"].as_str(), node["Relation Name"].as_str()),
                (Some("Seq Scan"), Some("decision_index"))
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "creates 50000 synthetic matches and 250000 positions in an isolated test database"]
async fn search_uses_indexes_on_a_large_library(pool: PgPool) {
    sqlx::raw_sql("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf)
        SELECT md5('match-'||n)::uuid,'match-'||n||'.mat','mat',md5(n::text),
        CASE WHEN n=4242 THEN 'Needle Player' ELSE 'Player '||n END,'Opponent',7,5,''::bytea,''
        FROM generate_series(1,50000) n;
        INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation)
        SELECT md5('position-'||n||'-'||ply)::uuid,md5('match-'||n)::uuid,ply,1,ply,0,ARRAY[3,1],array_fill(0,ARRAY[26]),
        CASE WHEN n=4242 AND ply=1 THEN '4HPwATDgc/ABMA' ELSE substr(md5(n::text||ply),1,14) END,'MIEEAAAAAAAE','8/5 6/5'
        FROM generate_series(1,50000) n CROSS JOIN generate_series(1,5) ply;
        ANALYZE matches; ANALYZE positions;")
        .execute(&pool).await.unwrap();
    sqlx::query("VACUUM ANALYZE matches")
        .execute(&pool)
        .await
        .unwrap();
    let match_plan:serde_json::Value=sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id,filename,format,white,black,match_length,position_count,created_at FROM matches WHERE search_text LIKE '%needle player%' ORDER BY id LIMIT 51").fetch_one(&pool).await.unwrap();
    let position_plan:serde_json::Value=sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT * FROM positions WHERE position_id='4HPwATDgc/ABMA' ORDER BY id LIMIT 51").fetch_one(&pool).await.unwrap();
    assert!(
        match_plan.to_string().contains("matches_search_trgm"),
        "{match_plan:#}"
    );
    assert!(
        position_plan.to_string().contains("positions_position_id"),
        "{position_plan:#}"
    );
    assert_eq!(match_plan[0]["Plan"]["Actual Rows"], 1);
    assert_eq!(position_plan[0]["Plan"]["Actual Rows"], 1);
    println!(
        "50000 matches: {} ms via GIN; 250000 positions: {} ms via B-tree",
        match_plan[0]["Execution Time"], position_plan[0]["Execution Time"]
    );
}
