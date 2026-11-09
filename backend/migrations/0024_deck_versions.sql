-- A dialog's version becomes stale after a rename or a membership addition.
ALTER TABLE decks
    ADD COLUMN version bigint NOT NULL DEFAULT 0 CHECK (version >= 0);
