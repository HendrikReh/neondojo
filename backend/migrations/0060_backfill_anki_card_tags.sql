-- Older APKG imports stored their tags in the card payload.
UPDATE learning_cards AS card
SET tags = merged.tags,
    tags_version = card.tags_version + 1
FROM (
    SELECT old.id, ARRAY(
        SELECT DISTINCT value
        FROM (
            SELECT unnest(old.tags) AS value
            UNION ALL
            SELECT entry #>> '{}'
            FROM jsonb_array_elements(
                CASE WHEN jsonb_typeof(old.payload->'anki_tags') = 'array'
                     THEN old.payload->'anki_tags' ELSE '[]'::jsonb END
            ) AS entries(entry)
            WHERE jsonb_typeof(entry) = 'string'
        ) AS combined
        ORDER BY value
    ) AS tags
    FROM learning_cards AS old
    WHERE jsonb_typeof(old.payload->'anki_tags') = 'array'
) AS merged
WHERE card.id = merged.id
  AND NOT card.tags @> merged.tags;
