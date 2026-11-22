-- Pip count of one side of a stored checker board. Side 0 owns the positive
-- counts and moves toward index 0; side 1 owns the negative counts and moves
-- toward index 25 (the stored layout of positions.board, see
-- backend/src/similar_positions.rs `profile`). A pip count is the sum of a
-- side's cumulative checker profile, so the difference of two positions' pip
-- counts is a lower bound of their similar-position distance.
CREATE FUNCTION public.checker_pips(board integer[], side integer) RETURNS integer
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $$
SELECT coalesce(sum(
    CASE WHEN side = 0 THEN i * greatest(board[i + 1], 0)
         ELSE (25 - i) * greatest(-board[i + 1], 0) END
), 0)::integer
FROM generate_series(0, 25) AS i
$$;

-- Similar-position search reads a band of own and opposing pip counts of the
-- player on roll around its target instead of every stored position.
CREATE INDEX positions_similar_pips_idx
    ON public.positions (public.checker_pips(board, player), public.checker_pips(board, 1 - player));
