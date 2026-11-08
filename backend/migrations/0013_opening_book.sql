-- A book has its own provenance and settings; it never changes native engine jobs.
CREATE TABLE opening_book_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    provider text NOT NULL DEFAULT 'automatic'
        CHECK (provider IN ('off','backgammondb','automatic')),
    path text NOT NULL
);
