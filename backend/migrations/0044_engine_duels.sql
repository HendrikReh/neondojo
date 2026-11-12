INSERT INTO databases (id, name)
VALUES ('00000000-0000-0000-0000-000000000005', 'Engine Duels');

DROP TRIGGER databases_protect_system ON databases;
ALTER TABLE databases
    DROP COLUMN is_system,
    ADD COLUMN is_system boolean GENERATED ALWAYS AS (
        is_default OR id IN ('00000000-0000-0000-0000-000000000003',
                            '00000000-0000-0000-0000-000000000004',
                            '00000000-0000-0000-0000-000000000005')
    ) STORED,
    ADD CONSTRAINT databases_engine_duels_name CHECK (
        id <> '00000000-0000-0000-0000-000000000005'::uuid OR name = 'Engine Duels'
    );
CREATE TRIGGER databases_protect_system
BEFORE DELETE OR UPDATE OF id, is_default ON databases
FOR EACH ROW WHEN (OLD.is_system) EXECUTE FUNCTION protect_system_database();
