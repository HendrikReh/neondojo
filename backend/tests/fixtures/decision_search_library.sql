-- Synthetic library for search_performance::decision_search_*: 10,000
-- matches in four databases, 600,000 positions and 399,000 indexed decisions.
-- 2,000 XG matches carry imported analysis, 7,500 MAT matches a complete
-- native analysis, and 500 MAT matches of Hero no analysis. Hero plays half of
-- the matches, some under a differently cased and spaced name.
INSERT INTO databases(id,name) SELECT md5('db-'||n)::uuid,'Synthetic '||n FROM generate_series(1,3) n;
INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,
                    database_id,played_on,moves_cataloged,imported_analysis_loaded,source_metadata_loaded)
SELECT md5('match-'||n)::uuid,'match-'||n||CASE WHEN n%5=0 THEN '.xg' ELSE '.mat' END,
       CASE WHEN n%5=0 THEN 'xg' ELSE 'mat' END,md5(n::text),
       CASE WHEN n%2=0 THEN CASE WHEN n%10=4 THEN ' HERO ' ELSE 'Hero' END ELSE 'Player '||(n%7) END,
       'Opponent '||(n%40),7,60,''::bytea,'',
       CASE n%4 WHEN 0 THEN '81a18d0f-38c1-4db8-81c0-a743adbc4480'::uuid ELSE md5('db-'||(n%4))::uuid END,
       date '2020-01-01'+(n%2000),true,true,true
FROM generate_series(1,10000) n;
INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford)
SELECT md5('match-'||n)::uuid,g,20,0,0,false FROM generate_series(1,10000) n CROSS JOIN generate_series(1,3) g;
INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation)
SELECT md5('position-'||n||'-'||p)::uuid,md5('match-'||n)::uuid,p,1+(p-1)/20,p,p%2,ARRAY[3,1],array_fill(0,ARRAY[26]),
       substr(md5(n::text||'-'||p),1,14),'MIEEAAAAAAAE','8/5 6/5'
FROM generate_series(1,10000) n CROSS JOIN generate_series(1,60) p;
-- One completed full-match job per analyzed MAT match: three checkpoints, 42 decisions.
INSERT INTO jobs(id,match_uuid,kind,plies,status,result,total_games,completed_games,source_revision,created_at)
SELECT md5('job-'||n)::uuid,md5('match-'||n)::uuid,'match',3,'completed','{"engine":"GNU Backgammon"}',3,3,0,
       timestamptz '2026-01-01'+n*interval '1 minute'
FROM generate_series(1,10000) n WHERE n%5<>0 AND n%20<>2;
-- Position analyses in every 20th match keep the position branch of the catalog realistic.
INSERT INTO jobs(id,position_uuid,kind,plies,status,result,source_revision)
SELECT md5('position-job-'||n)::uuid,md5('position-'||n||'-7')::uuid,'analysis',2,'completed',
       '{"candidates":[{"move":"8/5 6/5","equity":0.1}]}',0
FROM generate_series(1,10000,20) n;
INSERT INTO match_analysis_games(job_id,game_no)
SELECT md5('job-'||n)::uuid,g FROM generate_series(1,10000) n CROSS JOIN generate_series(1,3) g
WHERE n%5<>0 AND n%20<>2;
INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data)
SELECT md5('job-'||n)::uuid,md5('position-'||n||'-'||d)::uuid,1+(d-1)/14,d,d%2,
       CASE WHEN d%5=0 THEN 'cube' ELSE 'checker' END,0.01,
       jsonb_build_object('decision_no',d,'player',d%2,'kind',CASE WHEN d%5=0 THEN 'cube' ELSE 'checker' END,
                          'played','8/5 6/5','best','6/3 6/5','equity_loss',0.01,'evaluation_status','evaluated')
FROM generate_series(1,10000) n CROSS JOIN generate_series(1,42) d WHERE n%5<>0 AND n%20<>2;
INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
SELECT md5('match-'||n)::uuid,md5('position-'||n||'-'||d)::uuid,d,1+(d-1)/14,d,
       CASE WHEN d%5=0 THEN 'cube' ELSE 'checker' END,
       jsonb_build_object('engine','eXtreme Gammon','engine_version','2.19','depth','3-ply',
                          'candidates',jsonb_build_array(jsonb_build_object('rank',1,'equity',0.1)))
FROM generate_series(1,10000) n CROSS JOIN generate_series(1,42) d WHERE n%5=0;
-- 21 decisions per side of each analyzed match, with hits of about 1 kB.
-- Their fingerprints are set by the test once the library is complete.
INSERT INTO decision_index_matches(match_id,fingerprint)
SELECT md5('match-'||n)::uuid,'pending' FROM generate_series(1,10000) n;
INSERT INTO decision_index(match_id,seq,game_no,decision_no,player_name,question_kind,game_plan,opportunity,
                           mistake_kind,equity_loss,match_length,away_own,away_opponent,cube_value,cube_owner,played_on,hit)
SELECT m.id,d-1,1+(d-1)/14,d,lower(btrim(CASE WHEN d%2=1 THEN m.white ELSE m.black END)),
       CASE r%10 WHEN 0 THEN 'cube_offer' WHEN 1 THEN 'cube_response' ELSE 'checker' END,
       (ARRAY['racing','holding','priming','blitz','ace_point','back_game','mutual_holding'])[1+(r/10)%7],
       CASE WHEN r%10<2 THEN 'missed_double' END,
       CASE r%10 WHEN 0 THEN 'premature_double' WHEN 1 THEN 'wrong_take' ELSE 'checker' END,
       CASE WHEN (r/70)%5<3 THEN 0 ELSE ((r/350)%300)/1000.0 END,
       7,1+(r/105000)%7,1+(r/735000)%7,
       CASE (r/5145000)%40 WHEN 0 THEN 8 WHEN 1 THEN 4 WHEN 2 THEN 4 WHEN 3 THEN 4 WHEN 4 THEN 2 WHEN 5 THEN 2
            WHEN 6 THEN 2 WHEN 7 THEN 2 WHEN 8 THEN 2 WHEN 9 THEN 2 ELSE 1 END,
       CASE WHEN (r/5145000)%40>=10 THEN 'centered' WHEN (r/205800000)%2=0 THEN 'self' ELSE 'opponent' END,
       coalesce(m.played_on,m.created_at::date),
       jsonb_build_object('reference',jsonb_build_object('job_id',md5('job-'||n)::uuid,'game_no',1+(d-1)/14,'decision_no',d),
                          'kind','checker','players',jsonb_build_array(m.white,m.black),'engine','GNU Backgammon','plies',3,
                          'position',jsonb_build_object('board',array_fill(0,ARRAY[26]),'position_id',substr(md5(n::text||d),1,14),
                                                        'gnubg_match_id','MIEEAAAAAAAE','dice',ARRAY[3,1],'notation','8/5 6/5',
                                                        'id',md5('position-'||n||'-'||d)::uuid,'match_uuid',m.id),
                          'played','8/5 6/5','best','6/3 6/5','equity_loss',0.1,'match_length',7,'away',ARRAY[7,7],
                          'crawford',false,'cube_value',1,'cube_owner',null,'practicable',true,'game_plan','racing')
FROM generate_series(1,10000) n JOIN matches m ON m.id=md5('match-'||n)::uuid
CROSS JOIN generate_series(1,42) d
CROSS JOIN LATERAL (SELECT abs(hashint8(n::bigint*100+d))::bigint AS r) h
WHERE n%20<>2;
