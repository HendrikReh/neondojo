-- A null summary identifies a legacy checkpoint: its decisions remain usable,
-- but native GNUbg denominators cannot be recovered from decision rows.
ALTER TABLE match_analysis_games ADD COLUMN summary jsonb;
