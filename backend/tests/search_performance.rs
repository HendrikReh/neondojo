use sqlx::PgPool;

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
