-- Round filters and free-text search use the same literal substring expression.
CREATE INDEX matches_round_trgm ON matches USING gin(lower(round) gin_trgm_ops);
