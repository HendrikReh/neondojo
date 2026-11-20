-- A virtual player profile groups real player profiles without changing them.
-- At most one virtual profile is "me": NeonDojo bot matches count for it.
CREATE TABLE virtual_player_profiles (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name text NOT NULL CHECK (btrim(name) = name AND name <> '' AND char_length(name) <= 120),
    is_me boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX virtual_player_profiles_name ON virtual_player_profiles(lower(name));
CREATE UNIQUE INDEX virtual_player_profiles_me ON virtual_player_profiles(is_me) WHERE is_me;

-- The primary key keeps every real profile in at most one virtual profile.
CREATE TABLE virtual_player_members (
    profile_id uuid PRIMARY KEY REFERENCES player_profiles(id) ON DELETE CASCADE,
    virtual_id uuid NOT NULL REFERENCES virtual_player_profiles(id) ON DELETE CASCADE
);
CREATE INDEX virtual_player_members_virtual ON virtual_player_members(virtual_id, profile_id);

-- The one definition of which match sides a virtual profile played. Members
-- contribute their catalogued sides; bot matches never enter match_players, so
-- the human side of every finished or paused NeonDojo match belongs to "me".
-- `rated` limits performance ratings to unassisted competition play.
CREATE VIEW virtual_player_sides AS
SELECT v.virtual_id, mp.match_id, mp.side, true AS rated
FROM virtual_player_members v
JOIN player_aliases a ON a.profile_id = v.profile_id
JOIN match_players mp ON mp.alias_id = a.id
UNION ALL
SELECT vp.id, m.id, (s.state->>'human_player')::smallint,
       NOT m.play_assisted AND m.play_mode = 'competition'
FROM virtual_player_profiles vp
JOIN play_sessions s ON vp.is_me
JOIN matches m ON m.id = s.library_match_id
WHERE m.play_mode IS NOT NULL AND NOT m.collection_only
  AND s.state->>'human_player' IN ('0', '1')
  AND NOT (coalesce(s.state->>'play_mode', 'competition') = 'competition'
           AND s.state->>'phase' <> 'match_over'
           AND NOT (s.state->>'match_length' = '0' AND s.state->>'phase' = 'game_over'));
