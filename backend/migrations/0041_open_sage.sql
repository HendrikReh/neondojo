-- Local executable identity is deliberately excluded from portable profiles.
CREATE TABLE engine_installations (
    engine_id text PRIMARY KEY CHECK(engine_id='bgsage'),
    version bigint NOT NULL DEFAULT 0 CHECK(version>=0),
    python_path text NOT NULL DEFAULT '',
    snapshot jsonb
);
INSERT INTO engine_installations(engine_id) VALUES('bgsage');
ALTER TABLE app_settings DROP CONSTRAINT app_settings_analysis_engine_check;
ALTER TABLE app_settings DROP CONSTRAINT app_settings_play_engine_check;
ALTER TABLE app_settings ADD CHECK(analysis_engine IN ('gnubg','bgsage'));
ALTER TABLE app_settings ADD CHECK(play_engine IN ('gnubg','bgsage'));
ALTER TABLE app_settings ADD CHECK(analysis_engine<>'bgsage' OR analysis_plies=2);
ALTER TABLE app_settings ADD CHECK(play_engine<>'bgsage' OR play_plies=2);
ALTER TABLE benchmark_runs ADD COLUMN engine_snapshot jsonb;
-- One definition for automatic result selection. Explicit job reads stay on jobs.
CREATE VIEW selected_engine_jobs AS
SELECT j.* FROM jobs j
WHERE coalesce(j.engine_snapshot->>'engine_id','gnubg') =
      coalesce((SELECT analysis_engine FROM app_settings WHERE singleton=true),'gnubg');
ALTER TABLE benchmark_runs ADD COLUMN startup_elapsed_ms double precision CHECK(startup_elapsed_ms>=0);
CREATE INDEX jobs_engine_position_ready
    ON jobs(position_uuid,(coalesce(engine_snapshot->>'engine_id','gnubg')),plies DESC,created_at DESC,id DESC)
    WHERE kind='analysis' AND status='completed' AND result IS NOT NULL;
CREATE INDEX jobs_engine_match_current
    ON jobs(match_uuid,(coalesce(engine_snapshot->>'engine_id','gnubg')),source_revision,plies DESC,created_at DESC,id DESC)
    WHERE kind='match' AND status='completed';
