-- Match-name filtering and ordering use the editable title, falling back to the
-- immutable original filename. Keep the index expression identical to the query.
DROP INDEX matches_database_name;
CREATE INDEX matches_database_name ON matches(database_id,(lower(coalesce(nullif(display_name,''),filename))),id);
CREATE INDEX matches_effective_name_trgm ON matches USING gin((lower(coalesce(nullif(display_name,''),filename))) gin_trgm_ops);
CREATE INDEX matches_database_white_sort ON matches(database_id,lower(white),id);
CREATE INDEX matches_database_black_sort ON matches(database_id,lower(black),id);
CREATE INDEX matches_database_result ON matches(database_id,result_state,id);
CREATE INDEX matches_created ON matches(created_at,id);
CREATE INDEX matches_played ON matches(played_on,id);
