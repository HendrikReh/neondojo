-- Native converters previously discarded final-game outcomes. Requeue only
-- source-derived metadata; a user's metadata save owns all result fields.
-- The bounded startup backfill reads immutable originals and leaves canonical
-- SGF, positions, imported analysis, learning data and annotations unchanged.
UPDATE matches
SET source_metadata_loaded = false
WHERE metadata_version = 0 AND format IN ('bgf', 'xg');
