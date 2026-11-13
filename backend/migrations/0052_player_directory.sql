ALTER TABLE matches ADD COLUMN player_source_platform text
    CHECK (player_source_platform IN ('', 'backgammongalaxy.com', 'opengammon.com'));

CREATE TABLE player_profiles (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source_name text NOT NULL,
    first_name text NOT NULL DEFAULT '' CHECK (char_length(first_name) <= 120),
    last_name text NOT NULL DEFAULT '' CHECK (char_length(last_name) <= 120),
    version bigint NOT NULL DEFAULT 0
);

CREATE TABLE player_aliases (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    platform text NOT NULL CHECK (platform IN ('', 'backgammongalaxy.com', 'opengammon.com')),
    name text NOT NULL CHECK (name <> ''),
    profile_id uuid NOT NULL REFERENCES player_profiles(id),
    UNIQUE (platform, name)
);
CREATE INDEX player_aliases_profile ON player_aliases(profile_id, id);

CREATE TABLE match_players (
    match_id uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    side smallint NOT NULL CHECK (side IN (0, 1)),
    alias_id uuid NOT NULL REFERENCES player_aliases(id),
    PRIMARY KEY (match_id, side)
);
CREATE INDEX match_players_alias ON match_players(alias_id, match_id);

CREATE FUNCTION resolve_imported_player(site text, alias_name text) RETURNS uuid
LANGUAGE plpgsql AS $$
DECLARE
    alias_uuid uuid;
    profile_uuid uuid;
BEGIN
    SELECT id INTO alias_uuid FROM player_aliases WHERE platform=site AND name=alias_name;
    IF FOUND THEN RETURN alias_uuid; END IF;

    INSERT INTO player_profiles(source_name) VALUES(alias_name) RETURNING id INTO profile_uuid;
    INSERT INTO player_aliases(platform, name, profile_id) VALUES(site, alias_name, profile_uuid)
        ON CONFLICT(platform, name) DO NOTHING RETURNING id INTO alias_uuid;
    IF NOT FOUND THEN
        DELETE FROM player_profiles WHERE id=profile_uuid;
        SELECT id INTO STRICT alias_uuid FROM player_aliases WHERE platform=site AND name=alias_name;
    END IF;
    RETURN alias_uuid;
END;
$$;

CREATE FUNCTION catalog_match_players() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    site text;
    participant record;
BEGIN
    DELETE FROM match_players WHERE match_id=NEW.id;
    IF NEW.collection_only OR NEW.play_mode IS NOT NULL
       OR NEW.database_id='00000000-0000-0000-0000-000000000005' THEN
        RETURN NEW;
    END IF;

    site := coalesce(NEW.player_source_platform, '');

    -- Use the same lock order when two concurrent imports share both aliases.
    FOR participant IN
        SELECT * FROM (VALUES (0, btrim(NEW.white)), (1, btrim(NEW.black))) AS names(side, name)
        WHERE name<>'' ORDER BY name COLLATE "C", side
    LOOP
        INSERT INTO match_players(match_id, side, alias_id)
        VALUES(NEW.id, participant.side, resolve_imported_player(site, participant.name));
    END LOOP;
    RETURN NEW;
END;
$$;

CREATE FUNCTION set_match_player_platform() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.player_source_platform := coalesce(NEW.player_source_platform, CASE
        WHEN lower(btrim(coalesce(NEW.place, ''))) IN ('backgammongalaxy', 'backgammongalaxy.com')
          OR lower(btrim(coalesce(NEW.tournament, ''))) IN ('backgammongalaxy.com', 'https://www.backgammongalaxy.com/')
        THEN 'backgammongalaxy.com'
        WHEN lower(btrim(coalesce(NEW.place, ''))) IN ('opengammon', 'opengammon.com')
          OR lower(btrim(coalesce(NEW.tournament, ''))) IN ('opengammon.com', 'https://opengammon.com/')
        THEN 'opengammon.com'
        ELSE ''
    END);
    RETURN NEW;
END;
$$;

CREATE TRIGGER set_match_player_platform BEFORE INSERT OR UPDATE OF player_source_platform
    ON matches FOR EACH ROW EXECUTE FUNCTION set_match_player_platform();

CREATE TRIGGER catalog_match_players AFTER INSERT OR UPDATE OF white, black, collection_only, play_mode, player_source_platform
    ON matches FOR EACH ROW EXECUTE FUNCTION catalog_match_players();

UPDATE matches SET player_source_platform=NULL WHERE NOT collection_only AND play_mode IS NULL
    AND database_id<>'00000000-0000-0000-0000-000000000005';
