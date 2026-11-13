ALTER TABLE matches
    ADD COLUMN native_match_context_pending boolean NOT NULL DEFAULT false;

-- A legacy manual save owns the whole metadata form, including empty fields.
UPDATE matches
SET native_match_context_pending = true
WHERE format IN ('xg', 'bgf')
    AND NOT collection_only
    AND metadata_version = 0
    AND (played_on IS NULL OR tournament IS NULL OR round IS NULL OR place IS NULL);

CREATE INDEX matches_pending_source_metadata ON matches(id)
    WHERE NOT source_metadata_loaded OR native_match_context_pending;
