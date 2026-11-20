-- NeonDojo bot matches are catalogued like imports: the human side becomes a
-- real player under the NeonDojo platform; the bot side stays uncatalogued.
-- A virtual profile only groups real profiles and never owns matches itself.
ALTER TABLE player_aliases DROP CONSTRAINT player_aliases_platform_check;
ALTER TABLE player_aliases ADD CONSTRAINT player_aliases_platform_check
    CHECK (platform IN ('', 'backgammongalaxy.com', 'opengammon.com', 'neondojo'));

-- The side the human played; copies and detached matches keep it.
ALTER TABLE matches ADD COLUMN play_human_side smallint
    CHECK (play_human_side IN (0, 1));

CREATE OR REPLACE FUNCTION catalog_match_players() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    site text;
    participant record;
BEGIN
    DELETE FROM match_players WHERE match_id=NEW.id;
    IF NEW.collection_only OR NEW.database_id='00000000-0000-0000-0000-000000000005'
       OR (NEW.play_mode IS NOT NULL AND NEW.play_human_side IS NULL) THEN
        RETURN NEW;
    END IF;

    site := CASE WHEN NEW.play_mode IS NOT NULL THEN 'neondojo'
                 ELSE coalesce(NEW.player_source_platform, '') END;

    -- Use the same lock order when two concurrent imports share both aliases.
    FOR participant IN
        SELECT * FROM (VALUES (0, btrim(NEW.white)), (1, btrim(NEW.black))) AS names(side, name)
        WHERE name<>'' AND (NEW.play_mode IS NULL OR side=NEW.play_human_side)
        ORDER BY name COLLATE "C", side
    LOOP
        INSERT INTO match_players(match_id, side, alias_id)
        VALUES(NEW.id, participant.side, resolve_imported_player(site, participant.name));
    END LOOP;
    RETURN NEW;
END;
$$;

DROP TRIGGER catalog_match_players ON matches;
CREATE TRIGGER catalog_match_players
    AFTER INSERT OR UPDATE OF white, black, collection_only, play_mode, play_human_side,
        player_source_platform
    ON matches FOR EACH ROW EXECUTE FUNCTION catalog_match_players();

-- The one definition of a real profile's match sides and of which of them
-- can enter a performance rating: imported matches, and bot matches played
-- in Competition mode without learning aids once they are finished.
CREATE VIEW player_match_sides AS
SELECT a.profile_id, mp.match_id, mp.side,
       NOT m.play_assisted AND (m.play_mode IS NULL OR (
           m.play_mode = 'competition'
           AND NOT EXISTS (
               SELECT 1 FROM play_sessions s
               WHERE s.library_match_id = m.id
                 AND s.state->>'phase' <> 'match_over'
                 AND NOT (s.state->>'match_length' = '0' AND s.state->>'phase' = 'game_over')
           ))) AS rated
FROM match_players mp
JOIN player_aliases a ON a.id = mp.alias_id
JOIN matches m ON m.id = mp.match_id;

-- A virtual profile's sides are exactly its members' sides.
CREATE OR REPLACE VIEW virtual_player_sides AS
SELECT v.virtual_id, s.match_id, s.side, s.rated
FROM virtual_player_members v
JOIN player_match_sides s ON s.profile_id = v.profile_id;

-- Catalogue existing bot matches from their sessions.
UPDATE matches m SET play_human_side = (s.state->>'human_player')::smallint
FROM play_sessions s
WHERE s.library_match_id = m.id AND m.play_mode IS NOT NULL
  AND s.state->>'human_player' IN ('0', '1');

-- Bot matches used to count for "My player profile" directly. Keep that by
-- adding their new profiles as members, unless already in another group.
WITH added AS (
    INSERT INTO virtual_player_members(profile_id, virtual_id)
    SELECT DISTINCT a.profile_id, vp.id
    FROM virtual_player_profiles vp
    JOIN player_aliases a ON a.platform = 'neondojo'
    WHERE vp.is_me
    ON CONFLICT (profile_id) DO NOTHING
    RETURNING virtual_id
)
UPDATE virtual_player_profiles SET version = version + 1
WHERE id IN (SELECT virtual_id FROM added);
