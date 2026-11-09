-- Collection-only hosts retain selected occurrences after their source is removed.
-- They have no original artifact and never belong to the match library.
ALTER TABLE databases ADD COLUMN collection_only boolean NOT NULL DEFAULT false;
INSERT INTO databases(id,name,description,collection_only)
VALUES ('00000000-0000-0000-0000-000000000002','Saved positions','Internal storage for independent saved occurrences',true);
ALTER TABLE databases ADD CONSTRAINT databases_collection_identity CHECK (
    collection_only = (id='00000000-0000-0000-0000-000000000002'::uuid)
    AND NOT (collection_only AND is_default)
);

ALTER TABLE matches
    ADD COLUMN collection_only boolean NOT NULL DEFAULT false,
    ADD COLUMN source_match_id uuid,
    ADD COLUMN source_match_no bigint,
    ADD COLUMN source_database_id uuid,
    ADD COLUMN source_database_name text;
ALTER TABLE matches ADD CONSTRAINT matches_collection_source CHECK (
    collection_only = (database_id='00000000-0000-0000-0000-000000000002'::uuid)
    AND (NOT collection_only OR (
        source_match_id IS NOT NULL AND source_match_no>0
        AND source_database_id IS NOT NULL AND source_database_name IS NOT NULL
        AND octet_length(original)=0 AND octet_length(sgf)=0
        AND moves_cataloged AND imported_analysis_loaded AND source_metadata_loaded
    ))
);
