DO $$
DECLARE target_id uuid;
BEGIN
    SELECT id INTO target_id
    FROM databases
    WHERE name = 'Unsortiert' AND NOT collection_only
    ORDER BY is_default DESC, created_at, id
    LIMIT 1;

    IF target_id IS NULL THEN
        INSERT INTO databases (id, name, description)
        VALUES (gen_random_uuid(), 'Unsortiert', 'Matches ohne konkrete Zuordnung')
        RETURNING id INTO target_id;
    END IF;

    UPDATE databases
    SET is_default = false, version = version + 1, updated_at = now()
    WHERE is_default AND id <> target_id;

    UPDATE databases
    SET is_default = true, version = version + 1, updated_at = now()
    WHERE id = target_id AND NOT is_default;
END $$;
