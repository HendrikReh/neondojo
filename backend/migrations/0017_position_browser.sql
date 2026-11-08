-- GNUbg fields are little-endian bit ranges. Invalid historical fixture IDs
-- stay unknown instead of preventing an otherwise valid library migration.
CREATE FUNCTION position_browser_bits(data bytea, bit_offset integer, bit_width integer)
RETURNS integer LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE AS $$
DECLARE value integer := 0; n integer;
BEGIN
    FOR n IN 0..bit_width-1 LOOP
        value := value | (((get_byte(data,(bit_offset+n)/8) >> ((bit_offset+n)%8)) & 1) << n);
    END LOOP;
    RETURN value;
END $$;

CREATE FUNCTION position_browser_context(match_id text)
RETURNS integer[] LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE AS $$
DECLARE data bytea; owner integer;
BEGIN
    IF match_id !~ '^[A-Za-z0-9+/]{12}$' THEN RETURN NULL; END IF;
    data := decode(match_id,'base64');
    IF octet_length(data) <> 9
       OR position_browser_bits(data,8,3) <> 1
       OR position_browser_bits(data,15,3) NOT BETWEEN 1 AND 6
       OR position_browser_bits(data,18,3) NOT BETWEEN 1 AND 6 THEN
        RETURN NULL;
    END IF;
    owner := position_browser_bits(data,4,2);
    IF owner = 2 THEN RETURN NULL; END IF;
    RETURN ARRAY[position_browser_bits(data,36,15),position_browser_bits(data,51,15),
        position_browser_bits(data,7,1),1 << position_browser_bits(data,0,4),
        CASE WHEN owner=3 THEN -1 ELSE owner END];
EXCEPTION WHEN OTHERS THEN RETURN NULL;
END $$;

ALTER TABLE positions
    ADD COLUMN source_white_score integer GENERATED ALWAYS AS ((position_browser_context(gnubg_match_id))[1]) STORED,
    ADD COLUMN source_black_score integer GENERATED ALWAYS AS ((position_browser_context(gnubg_match_id))[2]) STORED,
    ADD COLUMN source_crawford boolean GENERATED ALWAYS AS ((position_browser_context(gnubg_match_id))[3]=1) STORED,
    ADD COLUMN source_cube_value integer GENERATED ALWAYS AS ((position_browser_context(gnubg_match_id))[4]) STORED,
    ADD COLUMN source_cube_owner integer GENERATED ALWAYS AS ((position_browser_context(gnubg_match_id))[5]) STORED;
CREATE INDEX positions_browser_context ON positions(source_white_score,source_black_score,source_crawford,source_cube_value,id);
CREATE INDEX positions_browser_cube ON positions(source_cube_value,id);
CREATE INDEX positions_browser_dice ON positions((least(dice[1],dice[2])),(greatest(dice[1],dice[2])),id);
CREATE INDEX positions_browser_dice_any ON positions USING gin(dice);
CREATE INDEX positions_browser_order ON positions(match_uuid,ordinal,id);
CREATE INDEX move_analyses_position_checker ON move_analyses(position_uuid,job_id) WHERE kind='checker' AND position_uuid IS NOT NULL;
CREATE INDEX jobs_browser_position_ready ON jobs(position_uuid,plies DESC,created_at DESC,id DESC) WHERE kind='analysis' AND status='completed' AND result IS NOT NULL;

-- Deliberately retain database references inside the definition after deletion:
-- loading a saved definition reports the missing reference, never broadens it.
CREATE TABLE position_filters (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (name=btrim(name) AND char_length(name) BETWEEN 1 AND 120),
    definition jsonb NOT NULL CHECK (jsonb_typeof(definition)='object' AND definition->'schema_version'='1'::jsonb),
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX position_filters_name ON position_filters(lower(name));
