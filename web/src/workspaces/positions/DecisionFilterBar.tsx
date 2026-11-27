import { useId } from 'react';
import { useDecimalInput } from '../../shared/format/decimal-input';
import ActionButton from '../../shared/ui/ActionButton';
import {
  SearchFilterInput,
  SearchFilterSelect,
} from '../../shared/ui/SearchFilterForm';
import type { GamePlan } from '../analysis/game-plans/game-plan-api';
import { useGamePlanText } from '../analysis/game-plans/game-plan-messages';
import type { Database } from '../library/library-api';
import MistakePlayerSelect from '../study/mistakes/MistakePlayerSelect';
import {
  mistakeGamePlans,
  type DecisionFilters,
  type MistakeKind,
} from '../study/mistakes/recurring-mistakes-api';
import { useMistakeText } from '../study/mistakes/recurring-mistakes-messages';
import { useDecisionSearchText } from './decision-search-messages';
import { usePositionText } from './position-browser-messages';

const PAGE_SIZE = 50;
const kinds = ['checker', 'cube_offer', 'cube_response'] as const;
const mistakeKinds: MistakeKind[] = [
  'checker',
  'missed_double',
  'premature_double',
  'wrong_take',
  'wrong_pass',
  'unclassified_cube',
];
const owners = ['self', 'opponent', 'centered'] as const;

export type DecisionSearchDraft = {
  player: string;
  database: string;
  kind: string;
  mistake: string;
  plan: string;
  minLoss?: string;
  maxLoss: string;
  away: string;
  opponentAway: string;
  cubeValue: string;
  cubeOwner: string;
};

export const emptyDecisionDraft: DecisionSearchDraft = {
  player: '',
  database: '',
  kind: '',
  mistake: '',
  plan: '',
  maxLoss: '',
  away: '',
  opponentAway: '',
  cubeValue: '',
  cubeOwner: '',
};

const optional = (text: string) =>
  text.trim() === '' ? undefined : Number(text);

/** Draft text to request filters; undefined while a number is not usable. */
export function decisionFilters(
  draft: DecisionSearchDraft,
  profileLoss: number,
): DecisionFilters | undefined {
  if (!draft.player) return undefined;
  const numbers = {
    min_loss:
      draft.minLoss === undefined ? profileLoss : optional(draft.minLoss),
    max_loss: optional(draft.maxLoss),
    away: optional(draft.away),
    opponent_away: optional(draft.opponentAway),
    cube_value: optional(draft.cubeValue),
  };
  if (Object.values(numbers).some((value) => Number.isNaN(value)))
    return undefined;
  return {
    player: draft.player,
    database_id: draft.database || undefined,
    kind: (draft.kind || undefined) as DecisionFilters['kind'],
    mistake_kind: (draft.mistake || undefined) as MistakeKind | undefined,
    game_plan: (draft.plan || undefined) as GamePlan | undefined,
    cube_owner: (draft.cubeOwner || undefined) as DecisionFilters['cube_owner'],
    ...numbers,
    limit: PAGE_SIZE,
  };
}

export default function DecisionFilterBar({
  draft,
  players,
  databases,
  profileLoss,
  canSearch,
  onFieldChange,
  onSubmit,
  onClear,
}: {
  draft: DecisionSearchDraft;
  players: readonly string[];
  databases: readonly Database[];
  profileLoss: number;
  canSearch: boolean;
  onFieldChange: (key: keyof DecisionSearchDraft, value: string) => void;
  onSubmit: () => void;
  onClear: () => void;
}) {
  const t = useDecisionSearchText();
  const p = usePositionText();
  const mistake = useMistakeText();
  const plan = useGamePlanText();
  const hintId = useId();
  const minLoss = useDecimalInput({
    value: draft.minLoss ?? String(profileLoss),
    onChange: (value) => onFieldChange('minLoss', value),
    step: 0.005,
    min: 0,
    max: 1000,
  });
  const maxLoss = useDecimalInput({
    value: draft.maxLoss,
    onChange: (value) => onFieldChange('maxLoss', value),
    step: 0.005,
    min: 0,
    max: 1000,
  });
  const set = (key: keyof DecisionSearchDraft) => (value: string) =>
    onFieldChange(key, value);

  return (
    <div className="position-controls position-controls--compact">
      <form
        className="position-filter-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <div className="position-search-row decision-search-row">
          <MistakePlayerSelect
            players={[...players]}
            value={draft.player}
            onChange={set('player')}
          />
          <SearchFilterSelect
            label={t('database')}
            value={draft.database}
            onChange={set('database')}
          >
            <option value="">{t('allDatabases')}</option>
            {databases.map((item) => (
              <option key={item.id} value={item.id}>
                {item.name}
              </option>
            ))}
          </SearchFilterSelect>
          <SearchFilterSelect
            label={t('kind')}
            value={draft.kind}
            onChange={set('kind')}
          >
            <option value="">{t('any')}</option>
            {kinds.map((kind) => (
              <option key={kind} value={kind}>
                {mistake(kind)}
              </option>
            ))}
          </SearchFilterSelect>
          <SearchFilterSelect
            label={t('mistake')}
            value={draft.mistake}
            onChange={set('mistake')}
          >
            <option value="">{t('any')}</option>
            {mistakeKinds.map((kind) => (
              <option key={kind} value={kind}>
                {mistake(kind)}
              </option>
            ))}
          </SearchFilterSelect>
          <div className="position-filter-pair">
            <SearchFilterInput
              label={t('minLoss')}
              {...minLoss.inputProps}
              onChange={minLoss.change}
            />
            <SearchFilterInput
              label={t('maxLoss')}
              {...maxLoss.inputProps}
              onChange={maxLoss.change}
            />
          </div>
          <ActionButton
            type="submit"
            className="primary"
            icon="search"
            label={t('search')}
            disabled={!canSearch}
            aria-describedby={draft.player ? undefined : hintId}
          />
          <ActionButton
            icon="clear"
            label={p('clear')}
            compact
            type="button"
            onClick={onClear}
          />
        </div>
        <div className="position-filter-toolbar">
          <details className="position-filter-details">
            <summary>{p('filters')}</summary>
            <div className="position-filter-grid">
              <SearchFilterSelect
                label={t('plan')}
                value={draft.plan}
                onChange={set('plan')}
              >
                <option value="">{t('any')}</option>
                {mistakeGamePlans.map((name) => (
                  <option key={name} value={name}>
                    {plan(name)}
                  </option>
                ))}
              </SearchFilterSelect>
              <SearchFilterInput
                label={t('away')}
                type="number"
                min={1}
                max={64}
                step={1}
                value={draft.away}
                onChange={set('away')}
              />
              <SearchFilterInput
                label={t('opponentAway')}
                type="number"
                min={1}
                max={64}
                step={1}
                value={draft.opponentAway}
                onChange={set('opponentAway')}
              />
              <SearchFilterSelect
                label={t('cubeValue')}
                value={draft.cubeValue}
                onChange={set('cubeValue')}
              >
                <option value="">{t('any')}</option>
                {[1, 2, 4, 8, 16, 32, 64].map((value) => (
                  <option key={value} value={value}>
                    {value}
                  </option>
                ))}
              </SearchFilterSelect>
              <SearchFilterSelect
                label={t('cubeOwner')}
                value={draft.cubeOwner}
                onChange={set('cubeOwner')}
              >
                <option value="">{t('any')}</option>
                {owners.map((owner) => (
                  <option key={owner} value={owner}>
                    {t(
                      owner === 'self'
                        ? 'ownerSelf'
                        : owner === 'opponent'
                          ? 'ownerOpponent'
                          : 'ownerCentered',
                    )}
                  </option>
                ))}
              </SearchFilterSelect>
            </div>
          </details>
          {!draft.player && (
            <p id={hintId} className="decision-search-hint">
              {t('choosePlayer')}
            </p>
          )}
        </div>
      </form>
    </div>
  );
}
