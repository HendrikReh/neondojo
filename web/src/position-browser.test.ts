import { expect, test } from 'vitest';
import {
  emptyPositionFilters,
  positionDefinition,
  positionDraft,
  positionParams,
  samePositionDefinition,
  togglePositionSelection,
  mergePositionPages,
  type PositionBrowserHit,
} from './position-browser-api';

test('position search defaults to every database and a versioned definition', () => {
  expect(positionDefinition(emptyPositionFilters())).toEqual({
    schema_version: 1,
    sort: 'match',
  });
  expect(emptyPositionFilters().database_id).toBe('');
});
test('serializes literal strings, unknown score zero and explicit non-Crawford', () => {
  const definition = positionDefinition({
    ...emptyPositionFilters(),
    q: '  50%_\\ test  ',
    white_score: '0',
    crawford: 'false',
    die1: '6',
    analysis_state: 'ready',
    sort: 'loss',
  });
  expect(definition).toEqual({
    schema_version: 1,
    sort: 'loss',
    q: '50%_\\ test',
    white_score: 0,
    crawford: false,
    die1: 6,
    analysis_state: 'ready',
  });
  const params = positionParams(definition, 'opaque + cursor');
  expect(JSON.parse(params.get('filters')!)).toEqual(definition);
  expect(params.get('after')).toBe('opaque + cursor');
});
test('saved definitions roundtrip without silently dropping missing database references', () => {
  const saved = {
    schema_version: 1 as const,
    sort: 'match' as const,
    database_id: 'deleted',
    crawford: false,
    match_points_min: 0,
    tag: 'prime',
  };
  expect(positionDefinition(positionDraft(saved))).toEqual(saved);
  expect(samePositionDefinition(saved, { ...saved, q: null })).toBe(true);
  expect(samePositionDefinition(saved, { ...saved, database_id: null })).toBe(
    false,
  );
});
test('rejects invalid ranges and invalid numeric filters instead of widening a search', () => {
  expect(() =>
    positionDefinition({
      ...emptyPositionFilters(),
      match_points_min: '7',
      match_points_max: '3',
    }),
  ).toThrow();
  expect(() =>
    positionDefinition({ ...emptyPositionFilters(), die1: '7' }),
  ).toThrow();
  expect(() =>
    positionDefinition({ ...emptyPositionFilters(), white_score: '0.5' }),
  ).toThrow();
  expect(() =>
    positionDefinition({ ...emptyPositionFilters(), min_loss: 'NaN' }),
  ).toThrow();
});
test('page selection preserves earlier choices and enforces the 200 position limit', () => {
  const original = new Set(['earlier']);
  const selected = togglePositionSelection(
    original,
    Array.from({ length: 205 }, (_, n) => `p${n}`),
    true,
  );
  expect(selected.size).toBe(200);
  expect(selected.has('earlier')).toBe(true);
  expect(original.size).toBe(1);
  expect(togglePositionSelection(selected, ['p0', 'p1'], false).size).toBe(198);
});
test('merges repeated UUIDs while retaining identical boards in distinct game contexts', () => {
  const hit = (id: string, ply: number) =>
    ({
      position: { id, position_id: 'same-board', ply },
      card_ready: false,
    }) as PositionBrowserHit;
  const first = hit('one', 1),
    second = hit('two', 2),
    refreshed = { ...first, card_ready: true };
  expect(
    mergePositionPages([first, second], [refreshed, hit('three', 3)]),
  ).toEqual([refreshed, second, hit('three', 3)]);
});

test('saved filter equality follows backend tag and dice canonicalization', () => {
  const first = positionDefinition({
    ...emptyPositionFilters(),
    tag: ' PRIME ',
    die1: '6',
    die2: '1',
  });
  expect(first.tag).toBe('prime');
  expect([first.die1, first.die2]).toEqual([1, 6]);
  expect(
    samePositionDefinition(first, { ...first, tag: 'Prime', die1: 6, die2: 1 }),
  ).toBe(true);
});
