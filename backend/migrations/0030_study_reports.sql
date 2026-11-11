-- Existing reports become discoverable without copying or recomputing their analysis.
CREATE INDEX play_sessions_study_reports_recent ON play_sessions(updated_at DESC, id DESC)
WHERE state->>'phase'='match_over' AND jsonb_typeof(state->'review')='object';
