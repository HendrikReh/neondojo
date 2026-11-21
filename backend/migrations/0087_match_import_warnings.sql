-- Irregularities an import resolved in the source file, such as a game ended
-- before a rejected MAT move. They describe the original file, travel with
-- its copies and stay apart from the user's notes.
ALTER TABLE matches ADD COLUMN import_warnings jsonb NOT NULL DEFAULT '[]'::jsonb
    CHECK (jsonb_typeof(import_warnings) = 'array');
