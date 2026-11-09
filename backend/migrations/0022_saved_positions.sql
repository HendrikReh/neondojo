-- Collection membership is explicit. Imported positions, notes, analyses and
-- learning cards do not imply a bookmark; existing libraries start empty.
CREATE TABLE saved_positions (
    position_uuid uuid PRIMARY KEY REFERENCES positions(id) ON DELETE CASCADE,
    saved_at timestamptz NOT NULL DEFAULT now()
);
