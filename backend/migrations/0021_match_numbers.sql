-- Library numbers identify stored entries independently of UUIDs and row order.
-- PostgreSQL assigns existing rows a number and advances the sequence for imports
-- and copies; ordinary metadata updates and database moves retain the number.
ALTER TABLE matches ADD COLUMN match_no bigint GENERATED ALWAYS AS IDENTITY;
ALTER TABLE matches ADD CONSTRAINT matches_match_no_key UNIQUE(match_no);
