-- Keep the established database identities and all existing match assignments.
UPDATE databases
SET name = 'Unsorted', version = version + 1, updated_at = now()
WHERE is_default AND name <> 'Unsorted';

INSERT INTO databases (id, name)
VALUES ('00000000-0000-0000-0000-000000000003', 'Bot Matches')
ON CONFLICT (id) DO UPDATE
SET name = EXCLUDED.name, version = databases.version + 1, updated_at = now()
WHERE databases.name <> EXCLUDED.name;

ALTER TABLE databases
    ADD COLUMN is_system boolean GENERATED ALWAYS AS (
        is_default OR id = '00000000-0000-0000-0000-000000000003'::uuid
    ) STORED,
    ADD CONSTRAINT databases_system_names CHECK (
        (NOT is_default OR name = 'Unsorted')
        AND (id <> '00000000-0000-0000-0000-000000000003'::uuid OR name = 'Bot Matches')
    );

CREATE FUNCTION protect_system_database() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'System databases cannot be deleted' USING ERRCODE = '23514';
    END IF;
    IF NEW.id <> OLD.id OR NEW.is_default <> OLD.is_default THEN
        RAISE EXCEPTION 'System database identities cannot be changed' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;

CREATE TRIGGER databases_protect_system
BEFORE DELETE OR UPDATE OF id, is_default ON databases
FOR EACH ROW WHEN (OLD.is_system) EXECUTE FUNCTION protect_system_database();

-- Migration 0035 established the application default but left the old SQL default.
DO $$
DECLARE default_id uuid;
BEGIN
    SELECT id INTO STRICT default_id FROM databases WHERE is_default;
    EXECUTE format('ALTER TABLE matches ALTER COLUMN database_id SET DEFAULT %L::uuid', default_id);
END $$;
