-- Separate foreign keys preserve target integrity and cascade on library deletion.
CREATE TABLE annotations (
    id uuid PRIMARY KEY,
    match_uuid uuid REFERENCES matches(id) ON DELETE CASCADE,
    position_uuid uuid REFERENCES positions(id) ON DELETE CASCADE,
    target_kind text GENERATED ALWAYS AS (
        CASE WHEN match_uuid IS NOT NULL THEN 'match' ELSE 'position' END
    ) STORED,
    target_id uuid GENERATED ALWAYS AS (coalesce(match_uuid, position_uuid)) STORED,
    tags text[] NOT NULL DEFAULT '{}',
    comment text NOT NULL DEFAULT '' CHECK (char_length(comment) <= 20000),
    version bigint NOT NULL CHECK (version > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    search_text text NOT NULL DEFAULT '',
    CHECK (num_nonnulls(match_uuid, position_uuid) = 1),
    CHECK (cardinality(tags) <= 30),
    UNIQUE (target_kind, target_id)
);

CREATE FUNCTION annotations_search_text() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.search_text := lower(NEW.comment || ' ' || array_to_string(NEW.tags, ' '));
    RETURN NEW;
END
$$;
CREATE TRIGGER annotations_search_text
    BEFORE INSERT OR UPDATE OF comment, tags ON annotations
    FOR EACH ROW EXECUTE FUNCTION annotations_search_text();

CREATE INDEX annotations_search_trgm ON annotations USING gin (search_text gin_trgm_ops);
CREATE INDEX annotations_tags ON annotations USING gin (tags);
CREATE INDEX annotations_match ON annotations(match_uuid) WHERE match_uuid IS NOT NULL;
CREATE INDEX annotations_position ON annotations(position_uuid) WHERE position_uuid IS NOT NULL;
