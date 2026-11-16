-- Match the default library order, including its stable UUID tie breaker.
CREATE INDEX matches_library_recent ON matches (created_at DESC NULLS LAST, id)
    WHERE NOT collection_only;
CREATE INDEX matches_library_database_recent ON matches (database_id, created_at DESC NULLS LAST, id)
    WHERE NOT collection_only;
