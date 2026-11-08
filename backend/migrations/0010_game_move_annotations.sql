-- Stable source identities are separate from engine analysis decisions: an
-- engine may add virtual NoDouble decisions that never occurred in the source.
ALTER TABLE match_games ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE match_games ADD CONSTRAINT match_games_id_key UNIQUE(id);
ALTER TABLE matches ADD COLUMN moves_cataloged boolean NOT NULL DEFAULT false;

CREATE TABLE match_moves (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    match_uuid uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    game_no integer NOT NULL,
    move_no integer NOT NULL CHECK(move_no > 0),
    kind text NOT NULL CHECK(kind IN ('checker','cube','resign')),
    player integer NOT NULL CHECK(player IN (0,1)),
    notation text NOT NULL,
    position_uuid uuid REFERENCES positions(id) ON DELETE SET NULL,
    board integer[] NOT NULL CHECK(array_length(board,1)=26),
    dice integer[] NOT NULL CHECK(array_length(dice,1)=2),
    position_id text NOT NULL,
    gnubg_match_id text NOT NULL,
    UNIQUE(match_uuid,game_no,move_no),
    FOREIGN KEY(match_uuid,game_no) REFERENCES match_games(match_uuid,game_no) ON DELETE CASCADE
);
CREATE INDEX match_moves_position ON match_moves(position_uuid) WHERE position_uuid IS NOT NULL;

-- Keep the same generated target interface and optimistic versions for all
-- note scopes, while foreign keys enforce ownership and deletion behavior.
ALTER TABLE annotations DROP CONSTRAINT annotations_check;
ALTER TABLE annotations DROP COLUMN target_kind;
ALTER TABLE annotations DROP COLUMN target_id;
ALTER TABLE annotations ADD COLUMN game_uuid uuid REFERENCES match_games(id) ON DELETE CASCADE;
ALTER TABLE annotations ADD COLUMN move_uuid uuid REFERENCES match_moves(id) ON DELETE CASCADE;
ALTER TABLE annotations ADD COLUMN target_kind text GENERATED ALWAYS AS (
    CASE WHEN match_uuid IS NOT NULL THEN 'match'
         WHEN position_uuid IS NOT NULL THEN 'position'
         WHEN game_uuid IS NOT NULL THEN 'game'
         ELSE 'move' END
) STORED;
ALTER TABLE annotations ADD COLUMN target_id uuid GENERATED ALWAYS AS (coalesce(match_uuid,position_uuid,game_uuid,move_uuid)) STORED;
ALTER TABLE annotations ADD CONSTRAINT annotations_one_target CHECK(num_nonnulls(match_uuid,position_uuid,game_uuid,move_uuid)=1);
ALTER TABLE annotations ADD CONSTRAINT annotations_target_kind_target_id_key UNIQUE(target_kind,target_id);
CREATE INDEX annotations_game ON annotations(game_uuid) WHERE game_uuid IS NOT NULL;
CREATE INDEX annotations_move ON annotations(move_uuid) WHERE move_uuid IS NOT NULL;
