import { beforeEach, expect, it, vi } from 'vitest';
import { defaultConfiguration } from './configuration-model';

const harness = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  current: null as unknown,
}));
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useState(initial: unknown) {
    const index = harness.index++;
    if (!(index in harness.slots))
      harness.slots[index] =
        typeof initial === 'function' ? initial() : initial;
    return [harness.slots[index], () => {}];
  },
  useRef(initial: unknown) {
    return (harness.slots[harness.index++] ??= { current: initial });
  },
}));
vi.mock('./ConfigurationProvider', () => ({
  useOptionalConfiguration: () => harness.current,
}));
import { useProfileValue } from './useProfileValue';

beforeEach(() => {
  harness.index = 0;
  harness.slots = [];
});
it('does not reset a query when an unrelated setting is normalized into fresh objects', () => {
  const state = {
    current: structuredClone(defaultConfiguration),
    updateCurrent: () => {},
  };
  harness.current = state;
  const render = () => {
    harness.index = 0;
    return useProfileValue(
      (c) => c.library.filters,
      (c, filters) => ({ ...c, library: { ...c.library, filters } }),
      defaultConfiguration.library.filters,
    )[0];
  };
  const filters = render();
  state.current = structuredClone(state.current);
  state.current.appearance.mode = 'dark';
  expect(render()).toBe(filters);
  state.current = structuredClone(state.current);
  state.current.library.filters.q = 'new query';
  expect(render()).not.toBe(filters);
  expect(render().q).toBe('new query');
});
