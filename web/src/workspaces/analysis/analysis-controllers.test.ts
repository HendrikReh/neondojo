import { beforeEach, expect, it, vi } from 'vitest';
import type { Job, Match } from '../../shared/http/api';

// Like the other Node hook tests: one slot per hook, effects run on commit.
const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
}));
interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useInsertionEffect(effect: () => void) {
    effect();
  },
  useContext(context: { _currentValue: unknown }) {
    return context._currentValue;
  },
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots))
      hooks.slots[index] = typeof initial === 'function' ? initial() : initial;
    return [
      hooks.slots[index],
      (next: unknown) => {
        hooks.slots[index] =
          typeof next === 'function' ? next(hooks.slots[index]) : next;
      },
    ];
  },
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useCallback(callback: unknown) {
    return callback;
  },
  useMemo(factory: () => unknown) {
    return factory();
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as EffectSlot | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: previous?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
import { catalogFixture } from './analysis-catalog-test-fixtures';
import { useMatchReview } from './match/useMatchReview';
import { usePositionCalculation } from './usePositionCalculation';

function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
});

it('starts every visit of a match with fresh checker filters', () => {
  const review = (id?: string) => {
    hooks.index = 0;
    return useMatchReview({
      match: id ? ({ id, white: 'Ada', black: 'Grace' } as Match) : undefined,
      catalog: catalogFixture(),
      reviewKind: 'checker',
      showResults: false,
      checkerFiltersOpen: true,
      onDecision: () => {},
      onShowReview: () => {},
      onGame: () => {},
    });
  };
  const fresh = { player: '', severity: '', errors_only: true };
  review('first').updateFilter({ player: '1', game_no: 2 });
  expect(review('first').selected).toMatchObject({ player: '1', game_no: 2 });
  expect(review('second').selected).toMatchObject(fresh);
  expect(review('first').selected).toMatchObject(fresh);
  expect(review('first').selected.game_no).toBeUndefined();
  review('first').updateFilter({ player: '1' });
  // A retained position has no match to review.
  expect(review().selected).toMatchObject(fresh);
  expect(review().navigation).toBeUndefined();
  expect(review('first').selected).toMatchObject(fresh);
});

function positionCalculation() {
  const run = vi.fn(async () => {});
  const shown = vi.fn();
  const render = (
    input: { open?: boolean; active?: boolean; job?: Job } = {},
  ) => {
    const once = () => {
      hooks.index = 0;
      return usePositionCalculation({
        positionJobs: {
          jobs: input.job ? { analysis: input.job } : {},
          refresh: () => {},
          removeDice: async () => {},
          error: null,
          submitting: false,
          restoring: false,
          failure: undefined,
          run,
        },
        positionId: 'position',
        defaultPlies: 3,
        disabled: false,
        active: input.active ?? true,
        open: input.open,
        onShowResults: shown,
      });
    };
    once();
    commit();
    return once();
  };
  return { run, shown, render };
}

it('starts the chosen recalculation, shows its tab and keeps its depth', () => {
  const { run, shown, render } = positionCalculation();
  render().openSettings('dice');
  render().setPlies(2);
  expect(render().settings).toMatchObject({ kind: 'dice', plies: 2 });
  render().start();
  expect(run).toHaveBeenCalledExactlyOnceWith('dice', 2);
  expect(shown).toHaveBeenCalledOnce();
  expect(render()).toMatchObject({ view: 'dice', settings: undefined });
  render().openSettings('dice');
  expect(render().settings?.plies).toBe(2);
});

it('closes the recalculation dialog with the board and refuses blocked starts', () => {
  const { run, render } = positionCalculation();
  render().openSettings('analysis');
  expect(render({ active: false }).settings).toBeUndefined();
  expect(render().settings).toBeUndefined();
  render().openSettings('analysis');
  const job = { id: 'job', status: 'running' } as Job;
  expect(render({ job })).toMatchObject({ canOpen: false, blocked: true });
  render({ job }).start();
  expect(run).not.toHaveBeenCalled();
});

it('forgets the tab, the depth and a deleted result when the match closes', () => {
  const { render } = positionCalculation();
  render().openSettings('dice');
  render().setPlies(1);
  render().start();
  render().setDeletedDice({ positionId: 'position', engine: 'gnubg' });
  expect(render()).toMatchObject({ view: 'dice', diceDeleted: true });
  render({ open: false });
  expect(render()).toMatchObject({ view: 'analysis', diceDeleted: false });
  render().openSettings('dice');
  expect(render().settings?.plies).toBe(3);
});
