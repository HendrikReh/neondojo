import { cursorSearchParams } from '../../shared/http/cursor-search';
import { request } from '../../shared/http/api';
import type { GamePlan } from '../analysis/game-plans/game-plan-api';
import { mistakeGamePlans } from '../study/mistakes/recurring-mistakes-api';

import type { components } from '../../generated-api';
type Schema = components['schemas'];
export type PositionFilterDefinition = Schema['PositionFilterDefinition'];
export type PositionBrowserHit = Schema['PositionBrowserHit'];
export type PositionSort = Schema['PositionBrowserSort'];
export type PositionDirection = Schema['PositionBrowserDirection'];
export const positionSorts = [
  'match',
  'loss',
  'match_id',
  'database',
  'game',
  'ply',
  'player',
  'opponent',
  'dice',
  'score',
  'cube',
  'played_move',
  'decks',
  'tags',
  'comment',
] as const;
export type SavedPositionFilter = Schema['SavedPositionFilter'];
export type PositionFilterDraft = Record<
  Exclude<keyof PositionFilterDefinition, 'schema_version'>,
  string
>;
export type PositionBrowserPage = Required<Schema['PositionBrowserPage']>;
export type PositionTransferStatus = Schema['PositionAddStatus'];
export type PositionTransferResult = Schema['AddPositionsResult'];
export type PositionAnalysisReadiness = Schema['PositionAnalysisReadiness'];
export const positionSelectionLimit = 200;
export const POSITION_PAGE_SIZE = 25;
const textFields = [
  'database_id',
  'q',
  'player',
  'match_name',
  'tournament',
  'round',
  'tag',
  'comment',
  'position_id',
] as const;
const numberFields = [
  'match_points_min',
  'match_points_max',
  'die1',
  'die2',
  'white_score',
  'black_score',
  'cube_value',
  'min_loss',
  'max_loss',
] as const;
export const positionFilterKeys = [
  ...textFields,
  'game_plan',
  ...numberFields,
  'crawford',
  'analysis_state',
] as const satisfies readonly (keyof PositionFilterDraft)[];
export type PositionFilterKey = (typeof positionFilterKeys)[number];

export function emptyPositionFilters(): PositionFilterDraft {
  return {
    ...(Object.fromEntries(
      positionFilterKeys.map((key) => [key, '']),
    ) as Record<PositionFilterKey, string>),
    sort: 'match',
    direction: '',
  };
}
export function positionDefinition(
  draft: PositionFilterDraft,
): PositionFilterDefinition {
  const sort = draft.sort || 'match';
  if (!(positionSorts as readonly string[]).includes(sort))
    throw new Error('invalidFilter');
  const definition: PositionFilterDefinition = {
    schema_version: 1,
    sort: sort as PositionSort,
  };
  if (draft.game_plan) {
    if (!mistakeGamePlans.includes(draft.game_plan as GamePlan))
      throw new Error('invalidFilter');
    definition.game_plan = draft.game_plan as GamePlan;
  }
  if (draft.direction) {
    if (draft.direction !== 'asc' && draft.direction !== 'desc')
      throw new Error('invalidFilter');
    definition.direction = draft.direction;
  }
  for (const key of textFields)
    if (draft[key]?.trim()) definition[key] = draft[key].trim();
  for (const key of numberFields) {
    const raw = draft[key]?.trim();
    if (!raw) continue;
    const value = Number(raw),
      loss = key === 'min_loss' || key === 'max_loss';
    if (
      !Number.isFinite(value) ||
      value < 0 ||
      (!loss && !Number.isInteger(value)) ||
      ((key === 'die1' || key === 'die2') && (value < 1 || value > 6)) ||
      (key === 'cube_value' &&
        (value < 1 || value > 32768 || (value & (value - 1)) !== 0))
    )
      throw new Error('invalidFilter');
    definition[key] = value;
  }
  if (definition.tag) definition.tag = definition.tag.toLowerCase();
  if (
    definition.die1 != null &&
    definition.die2 != null &&
    definition.die1 > definition.die2
  )
    [definition.die1, definition.die2] = [definition.die2, definition.die1];
  if (draft.crawford) {
    if (!['true', 'false'].includes(draft.crawford))
      throw new Error('invalidFilter');
    definition.crawford = draft.crawford === 'true';
  }
  if (draft.analysis_state) {
    if (draft.analysis_state !== 'ready' && draft.analysis_state !== 'missing')
      throw new Error('invalidFilter');
    definition.analysis_state = draft.analysis_state;
  }
  if (
    (definition.match_points_min ?? 0) >
      (definition.match_points_max ?? Infinity) ||
    (definition.min_loss ?? 0) > (definition.max_loss ?? Infinity)
  )
    throw new Error('invalidFilter');
  return definition;
}
export function positionDraft(
  definition: PositionFilterDefinition,
): PositionFilterDraft {
  const draft = emptyPositionFilters();
  for (const key of Object.keys(draft) as (keyof PositionFilterDraft)[]) {
    const value = definition[key];
    if (value !== null && value !== undefined) draft[key] = String(value);
  }
  return draft;
}
export function positionParams(
  definition: PositionFilterDefinition,
  after?: string,
  limit = POSITION_PAGE_SIZE,
) {
  return cursorSearchParams(
    limit,
    { filters: JSON.stringify(positionDefinition(positionDraft(definition))) },
    after,
  );
}
export function samePositionDefinition(
  a: PositionFilterDefinition,
  b: PositionFilterDefinition,
) {
  return (
    JSON.stringify(positionDefinition(positionDraft(a))) ===
    JSON.stringify(positionDefinition(positionDraft(b)))
  );
}
export function togglePositionSelection(
  selection: ReadonlySet<string>,
  ids: readonly string[],
  checked: boolean,
) {
  const next = new Set(selection);
  for (const id of ids)
    if (!checked) next.delete(id);
    else if (next.size < positionSelectionLimit) next.add(id);
  return next;
}
export const loadPositionPage = (
  definition: PositionFilterDefinition,
  signal: AbortSignal,
  page: number,
) => {
  const params = positionParams(definition);
  params.set('page', String(page));
  return request<PositionBrowserPage>(`/position-browser?${params}`, {
    signal,
  });
};
export const loadPositionFilters = (signal: AbortSignal) =>
  request<SavedPositionFilter[]>('/position-filters', { signal });
export const loadPositionAnalysisReadiness = (
  ids: readonly string[],
  signal: AbortSignal,
) =>
  request<PositionAnalysisReadiness>('/position-browser/analysis-readiness', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ position_ids: ids }),
    signal,
  });
export const savePositionFilter = (
  name: string,
  definition: PositionFilterDefinition,
  existing?: SavedPositionFilter,
) =>
  request<SavedPositionFilter>(
    `/position-filters${existing ? `/${existing.id}` : ''}`,
    {
      method: existing ? 'PUT' : 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name,
        definition,
        ...(existing ? { expected_version: existing.version } : {}),
      }),
    },
  );
export const deletePositionFilter = (existing: SavedPositionFilter) =>
  request<void>(
    `/position-filters/${existing.id}?expected_version=${existing.version}`,
    { method: 'DELETE' },
  );
export const transferPositions = (deckId: string, ids: string[]) =>
  request<PositionTransferResult>(`/decks/${deckId}/positions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ position_ids: ids }),
  });
