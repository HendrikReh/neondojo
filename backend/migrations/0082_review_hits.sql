-- Derived, narrow list of the validated decisions of one saved analysis of a
-- match. Numbered review pages count and select their decisions here and
-- project only the decisions of the requested page from the full analysis
-- data. A set is rebuilt from the source analyses whenever its fingerprint
-- changes, so it holds no information that they do not.
CREATE TABLE review_hit_sets (
    match_id uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    -- 'job:<id>' or 'imported:<match id>', as in the analysis catalog.
    analysis_id text NOT NULL,
    -- Projection version, source revision, catalog state and saved decisions.
    fingerprint text NOT NULL,
    indexed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (match_id, analysis_id)
);

CREATE TABLE review_hits (
    match_id uuid NOT NULL,
    analysis_id text NOT NULL,
    game_no integer NOT NULL,
    decision_no integer NOT NULL,
    -- Cube decisions precede the checker decision with the same number.
    checker boolean NOT NULL,
    -- Source-qualified review decision identity.
    id text NOT NULL,
    -- Player of the source row, which the review's player filter uses.
    player integer NOT NULL CHECK (player IN (0, 1)),
    -- Only evaluated decisions are classified by the error thresholds.
    evaluated boolean NOT NULL,
    equity_loss double precision NOT NULL,
    PRIMARY KEY (match_id, analysis_id, game_no, decision_no, checker, id),
    FOREIGN KEY (match_id, analysis_id)
        REFERENCES review_hit_sets(match_id, analysis_id) ON DELETE CASCADE
);

-- Inserted or deleted analysis rows change the fingerprint. An in-place
-- update, as in data migrations and corrections, drops the affected sets.
CREATE FUNCTION review_hits_drop_updated_native() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM review_hit_sets x
    USING jobs j
    WHERE x.match_id = j.match_uuid
      AND j.id IN (SELECT job_id FROM old_rows UNION SELECT job_id FROM new_rows);
    RETURN NULL;
END
$$;
CREATE TRIGGER move_analyses_drop_review_hits
    AFTER UPDATE ON move_analyses
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION review_hits_drop_updated_native();

CREATE FUNCTION review_hits_drop_updated_imported() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM review_hit_sets x
    WHERE x.match_id IN (SELECT match_uuid FROM old_rows UNION SELECT match_uuid FROM new_rows);
    RETURN NULL;
END
$$;
CREATE TRIGGER imported_analyses_drop_review_hits
    AFTER UPDATE ON imported_analyses
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION review_hits_drop_updated_imported();
