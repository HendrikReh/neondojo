INSERT INTO databases (id, name)
VALUES ('00000000-0000-0000-0000-000000000004', 'Paused Matches');

DROP TRIGGER databases_protect_system ON databases;
ALTER TABLE databases
    DROP COLUMN is_system,
    ADD COLUMN is_system boolean GENERATED ALWAYS AS (
        is_default OR id IN ('00000000-0000-0000-0000-000000000003',
                            '00000000-0000-0000-0000-000000000004')
    ) STORED,
    ADD CONSTRAINT databases_paused_matches_name CHECK (
        id <> '00000000-0000-0000-0000-000000000004'::uuid OR name = 'Paused Matches'
    );
CREATE TRIGGER databases_protect_system
BEFORE DELETE OR UPDATE OF id, is_default ON databases
FOR EACH ROW WHEN (OLD.is_system) EXECUTE FUNCTION protect_system_database();

-- Also marks the session as paused. The former database can be deleted while
-- the session is paused, so this is intentionally not a foreign key.
ALTER TABLE play_sessions ADD COLUMN paused_from_database_id uuid;
