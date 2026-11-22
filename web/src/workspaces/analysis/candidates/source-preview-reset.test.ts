import { beforeEach, expect, it, vi } from 'vitest';
import type { SourceAnalysis } from './source-analysis-api';
const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: [] as Array<() => void>,
}));
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots)) hooks.slots[index] = initial;
    return [
      hooks.slots[index],
      (value: unknown) => {
        hooks.slots[index] = value;
      },
    ];
  },
  useEffect(effect: () => void, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as unknown[] | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous[i])))
      hooks.pending.push(effect);
    hooks.slots[index] = deps;
  },
  useEffectEvent(callback: (...args: unknown[]) => unknown) {
    const index = hooks.index++;
    type Slot = { callback: typeof callback; event: typeof callback };
    let slot = hooks.slots[index] as Slot | undefined;
    if (!slot) {
      slot = {
        callback,
        event: (...args) => (hooks.slots[index] as Slot).callback(...args),
      };
      hooks.slots[index] = slot;
    }
    slot.callback = callback;
    return slot.event;
  },
}));
vi.mock('../../../shared/i18n/i18n', () => ({
  useI18n: () => ({
    locale: 'en',
    t: String,
    formatNumber: String,
    formatEquity: String,
    formatPercent: String,
  }),
}));
import SourceAnalysisView from './SourceAnalysisView';
const records: SourceAnalysis[] = [];
function render(
  onPreview: NonNullable<Parameters<typeof SourceAnalysisView>[0]['onPreview']>,
  current = records,
) {
  hooks.index = 0;
  SourceAnalysisView({ records: current, source: 'book', onPreview });
  hooks.pending.splice(0).forEach((effect) => effect());
}
beforeEach(() => {
  hooks.index = 0;
  hooks.slots = [];
  hooks.pending = [];
});
it('does not clear a candidate preview just because the parent callback was recreated', () => {
  const before = vi.fn(),
    after = vi.fn();
  render(before);
  expect(before).toHaveBeenCalledWith(null);
  render(after);
  expect(after).not.toHaveBeenCalled();
});
it('still clears preview when the actual reference records change', () => {
  const preview = vi.fn();
  render(preview);
  preview.mockClear();
  render(preview, []);
  expect(preview).toHaveBeenCalledOnce();
  expect(preview).toHaveBeenCalledWith(null);
});
