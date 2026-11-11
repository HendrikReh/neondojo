import { describe, expect, it } from 'vitest';
import {
  defaultConfiguration,
  normalizeConfiguration,
} from './configuration-model';
import {
  profileChangeRows,
  restoreConfigurationField,
} from './profile-changes';

describe('restoring individual profile settings', () => {
  for (const fixture of [
    {
      path: 'positions.definition.dice',
      parent: 'positions',
      keys: ['die1', 'die2'],
      saved: [5, 6],
      current: [1, 2],
    },
    {
      path: 'positions.definition.match_points_range',
      parent: 'positions',
      keys: ['match_points_min', 'match_points_max'],
      saved: [5, 7],
      current: [1, 3],
    },
    {
      path: 'positions.definition.loss_range',
      parent: 'positions',
      keys: ['min_loss', 'max_loss'],
      saved: [0.5, 1],
      current: [0.1, 0.2],
    },
    {
      path: 'library.filters.match_points_range',
      parent: 'library',
      keys: ['match_points_min', 'match_points_max'],
      saved: ['5', '7'],
      current: ['1', '3'],
    },
    {
      path: 'library.filters.date_range',
      parent: 'library',
      keys: ['date_from', 'date_to'],
      saved: ['2026-06-01', '2026-06-30'],
      current: ['2026-01-01', '2026-01-31'],
    },
  ]) {
    it(`explicitly restores coupled filter ${fixture.path} as one setting`, () => {
      const saved = structuredClone(defaultConfiguration);
      const current = structuredClone(defaultConfiguration);
      const savedParent = (fixture.parent === 'positions'
        ? saved.positions.definition
        : saved.library.filters) as unknown as Record<string, unknown>;
      const currentParent = (fixture.parent === 'positions'
        ? current.positions.definition
        : current.library.filters) as unknown as Record<string, unknown>;
      fixture.keys.forEach((key, index) => {
        savedParent[key] = fixture.saved[index];
        currentParent[key] = fixture.current[index];
      });
      expect(profileChangeRows(saved, current)).toEqual([
        { path: fixture.path, saved: fixture.saved, current: fixture.current },
      ]);
      const result = restoreConfigurationField(saved, current, fixture.path);
      expect(profileChangeRows(saved, result)).toEqual([]);
      expect(
        restoreConfigurationField(
          saved,
          current,
          `${fixture.path.slice(0, fixture.path.lastIndexOf('.'))}.${fixture.keys[0]}`,
        ),
      ).toEqual(normalizeConfiguration(current));
    });
  }
  it('removes missing endpoints when restoring a partially bounded range', () => {
    const saved = structuredClone(defaultConfiguration);
    saved.positions.definition.max_loss = 0.2;
    const current = structuredClone(defaultConfiguration);
    current.positions.definition.min_loss = 0.5;
    current.positions.definition.max_loss = 1;
    const result = restoreConfigurationField(
      saved,
      current,
      'positions.definition.loss_range',
    );
    expect(result.positions.definition).not.toHaveProperty('min_loss');
    expect(result.positions.definition.max_loss).toBe(0.2);
  });
  it('restores one scalar without mutating inputs or other changes', () => {
    const saved = structuredClone(defaultConfiguration);
    const current = structuredClone(saved);
    current.analysis.plies = 4;
    current.general.player_name = 'Keep me';
    const previous = structuredClone(current);
    const result = restoreConfigurationField(saved, current, 'analysis.plies');
    expect(result.analysis.plies).toBe(2);
    expect(result.general.player_name).toBe('Keep me');
    expect(current).toEqual(previous);
    expect(saved).toEqual(defaultConfiguration);
  });
  it('deletes an optional field that has no saved value and retains its siblings', () => {
    const current = structuredClone(defaultConfiguration);
    current.positions.definition.tag = 'extra';
    current.positions.definition.q = 'retained';
    const result = restoreConfigurationField(
      defaultConfiguration,
      current,
      'positions.definition.tag',
    );
    expect(result.positions.definition).not.toHaveProperty('tag');
    expect(result.positions.definition.q).toBe('retained');
  });
  it('restores a saved optional value that is currently absent', () => {
    const saved = structuredClone(defaultConfiguration);
    saved.positions.definition.q = 'saved query';
    expect(
      restoreConfigurationField(
        saved,
        defaultConfiguration,
        'positions.definition.q',
      ).positions.definition.q,
    ).toBe('saved query');
  });
  for (const restoreNull of [true, false])
    it(`restores a whole nullable object to ${restoreNull ? 'null' : 'its saved contents'}`, () => {
      const withFilters = structuredClone(defaultConfiguration);
      withFilters.annotations.filters = {
        q: 'notes',
        tag: 'memo',
        kind: 'match',
      };
      const [saved, current] = restoreNull
        ? [defaultConfiguration, withFilters]
        : [withFilters, defaultConfiguration];
      const result = restoreConfigurationField(
        saved,
        current,
        'annotations.filters',
      );
      expect(result.annotations.filters).toEqual(saved.annotations.filters);
      if (!restoreNull)
        expect(result.annotations.filters).not.toBe(saved.annotations.filters);
    });
  it('deletes only an extra width override from an existing width map', () => {
    const saved = structuredClone(defaultConfiguration);
    saved.library.column_widths = { white: 211 };
    const current = structuredClone(saved);
    current.library.column_widths = { white: 233, black: 240 };
    const result = restoreConfigurationField(
      saved,
      current,
      'library.column_widths.black',
    );
    expect(result.library.column_widths).toEqual({ white: 233 });
  });
  it('restores automatic widths as null', () => {
    const current = structuredClone(defaultConfiguration);
    current.library.column_widths = { white: 211 };
    expect(
      restoreConfigurationField(
        defaultConfiguration,
        current,
        'library.column_widths',
      ).library.column_widths,
    ).toBeNull();
  });
  it('ignores paths that do not identify an actual difference', () => {
    for (const path of [
      'unknown',
      '__proto__.polluted',
      'schema_version',
      'analysis.plies',
    ])
      expect(
        restoreConfigurationField(
          defaultConfiguration,
          defaultConfiguration,
          path,
        ),
      ).toEqual(normalizeConfiguration(defaultConfiguration));
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
  });
  it('does not show a redundant visibility difference caused only by column order', () => {
    const current = structuredClone(defaultConfiguration);
    current.library.column_order.reverse();
    expect(
      profileChangeRows(defaultConfiguration, current).map((d) => d.path),
    ).toEqual(['library.column_order']);
  });
  it('restores visible column membership while retaining the independent current column order', () => {
    const current = structuredClone(defaultConfiguration);
    current.library.column_order.reverse();
    current.library.visible_columns = current.library.visible_columns.filter(
      (id) => id !== 'white',
    );
    const result = restoreConfigurationField(
      defaultConfiguration,
      current,
      'library.visible_columns',
    );
    expect(result.library.column_order).toEqual(current.library.column_order);
    expect(new Set(result.library.visible_columns)).toEqual(
      new Set(defaultConfiguration.library.visible_columns),
    );
    expect(
      profileChangeRows(defaultConfiguration, result).map((d) => d.path),
    ).toEqual(['library.column_order']);
  });
  it('restores column order without changing which columns are visible', () => {
    const current = structuredClone(defaultConfiguration);
    current.library.column_order.reverse();
    current.library.visible_columns = current.library.visible_columns.filter(
      (id) => id !== 'white',
    );
    const result = restoreConfigurationField(
      defaultConfiguration,
      current,
      'library.column_order',
    );
    expect(result.library.column_order).toEqual(
      defaultConfiguration.library.column_order,
    );
    expect(new Set(result.library.visible_columns)).toEqual(
      new Set(current.library.visible_columns),
    );
    expect(
      profileChangeRows(defaultConfiguration, result).map((d) => d.path),
    ).toEqual(['library.visible_columns']);
  });
});
