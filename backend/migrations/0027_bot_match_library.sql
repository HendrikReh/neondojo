-- A session keeps its durable checkpoint when its library entry is removed.
ALTER TABLE play_sessions ADD COLUMN library_match_id uuid UNIQUE REFERENCES matches(id) ON DELETE SET NULL;
ALTER TABLE play_sessions ADD COLUMN library_archived boolean NOT NULL DEFAULT false;
-- Growing sources retain historical analysis; only matching revisions are current.
ALTER TABLE matches ADD COLUMN source_revision bigint NOT NULL DEFAULT 0;
ALTER TABLE jobs ADD COLUMN source_revision bigint NOT NULL DEFAULT 0;
-- -1 is an insert-only sentinel: explicit revisions on copied historical jobs
-- remain unchanged. Constraints are checked after the BEFORE INSERT trigger.
ALTER TABLE jobs ALTER COLUMN source_revision SET DEFAULT -1;
ALTER TABLE jobs ADD CONSTRAINT jobs_source_revision_nonnegative CHECK(source_revision >= 0);
CREATE FUNCTION snapshot_job_source_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.source_revision = -1 THEN
        NEW.source_revision := 0;
        IF NEW.match_uuid IS NOT NULL THEN
            SELECT source_revision INTO NEW.source_revision FROM matches WHERE id=NEW.match_uuid;
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER jobs_source_revision BEFORE INSERT ON jobs FOR EACH ROW EXECUTE FUNCTION snapshot_job_source_revision();
