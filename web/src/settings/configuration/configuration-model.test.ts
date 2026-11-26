import { describe, expect, it } from 'vitest';
import {
  configurationDifferences,
  defaultConfiguration,
  isConfigurationDirty,
  normalizeConfiguration,
} from './configuration-model';
import vectors from '../../../../tests/fixtures/configuration-normalization.json';

describe('configuration comparison', () => {
  it.each(['es', 'fr', 'it', 'pl'])(
    'loads cached settings in English for retired locale %s',
    (locale) => {
      const expected = structuredClone(defaultConfiguration);
      expected.general = {
        ...expected.general,
        player_name: 'Ada',
        locale: 'en',
      };
      expect(
        normalizeConfiguration({
          ...expected,
          general: { ...expected.general, locale },
        }),
      ).toEqual(normalizeConfiguration(expected));
    },
  );

  it('matches the board for fresh dice settings and the checkers for missing or null legacy settings', () => {
    expect(defaultConfiguration.appearance.dice_theme).toBe('match-board');
    for (const overrides of [{}, { dice_theme: null }]) {
      const legacy = {
        ...vectors.base,
        appearance: { ...vectors.base.appearance, ...overrides },
      };
      expect(normalizeConfiguration(legacy).appearance.dice_theme).toBeNull();
    }
  });
  it('defaults legacy profiles to the four analysis thresholds', () => {
    expect(
      normalizeConfiguration(vectors.base).analysis.error_thresholds,
    ).toEqual({
      questionable: 0.005,
      error: 0.02,
      blunder: 0.08,
      large_blunder: 0.12,
    });
  });
  it.each(['checker_theme', 'cube_theme', 'dice_theme'])(
    'rejects an unknown %s',
    (field) => {
      expect(() =>
        normalizeConfiguration({
          ...vectors.base,
          appearance: { ...vectors.base.appearance, [field]: 'unknown' },
        }),
      ).toThrow(`appearance.${field}`);
    },
  );
  it('normalizes legacy and independently saved board widths', () => {
    const legacy = structuredClone(vectors.base);
    expect(normalizeConfiguration(legacy).appearance).toMatchObject({
      analysis_board_width: null,
      analysis_overview_board_width: null,
      analysis_errors_board_width: null,
      analysis_statistics_board_width: null,
      analysis_notes_board_width: null,
    });
    const appearance = { ...legacy.appearance, analysis_board_width: 540 };
    expect(
      normalizeConfiguration({ ...legacy, appearance }).appearance,
    ).toMatchObject({
      analysis_board_width: 540,
      analysis_overview_board_width: null,
      analysis_errors_board_width: null,
      analysis_statistics_board_width: null,
      analysis_notes_board_width: null,
    });
    expect(
      normalizeConfiguration({
        ...legacy,
        appearance: { ...appearance, analysis_overview_board_width: 1680 },
      }).appearance,
    ).toMatchObject({
      analysis_board_width: 540,
      analysis_overview_board_width: 1680,
    });
    for (const [field, max] of [
      ['analysis_board_width', 1680],
      ['analysis_overview_board_width', 1680],
      ['analysis_errors_board_width', 1680],
      ['analysis_statistics_board_width', 1680],
      ['analysis_notes_board_width', 1680],
    ] as const)
      for (const width of [0, 319, max + 1, 400.5, Number.NaN])
        expect(() =>
          normalizeConfiguration({
            ...legacy,
            appearance: { ...appearance, [field]: width },
          }),
        ).toThrow(`appearance.${field}`);
  });
  it('supports independent verified NeonGammon profiles and rejects unverified depths', () => {
    for (const area of ['analysis', 'play'] as const) {
      const current = structuredClone(defaultConfiguration);
      const input = {
        ...current,
        [area]: { ...current[area], engine: 'bgsage', plies: 2 },
      };
      expect(normalizeConfiguration(input)[area].engine).toBe('bgsage');
      expect(
        normalizeConfiguration({
          ...input,
          [area]: { ...input[area], plies: 1 },
        })[area].plies,
      ).toBe(1);
      if (area === 'analysis')
        for (const plies of [3, 4])
          expect(
            normalizeConfiguration({
              ...input,
              analysis: { ...input.analysis, plies },
            }).analysis.plies,
          ).toBe(plies);
      for (const plies of area === 'analysis' ? [0, 5] : [0, 3, 4])
        expect(() =>
          normalizeConfiguration({
            ...input,
            [area]: { ...input[area], plies },
          }),
        ).toThrow();
    }
  });
  it('represents missing optional field values as explicit null in differences', () => {
    const current = structuredClone(defaultConfiguration);
    current.positions.definition.tag = 'retained';
    expect(configurationDifferences(defaultConfiguration, current)).toEqual([
      { path: 'positions.definition.tag', saved: null, current: 'retained' },
    ]);
  });
  for (const vector of vectors.cases)
    it(`matches the shared Rust vector: ${vector.name}`, () => {
      const current = structuredClone(vectors.base) as Record<string, unknown>;
      for (const change of vector.changes) {
        const keys = change.path.split('.');
        let target = current;
        for (const key of keys.slice(0, -1))
          target = target[key] as Record<string, unknown>;
        target[keys.at(-1)!] = change.value;
      }
      expect(
        configurationDifferences(vectors.base, current).map((d) => d.path),
      ).toEqual(vector.expected_paths);
    });
  it('compares semantic values independently of object key order', () => {
    const reversed = Object.fromEntries(
      Object.entries(defaultConfiguration).reverse(),
    );
    expect(isConfigurationDirty(defaultConfiguration, reversed)).toBe(false);
  });
  it('returns only changed fields and preserves meaningful column ordering', () => {
    const changed = structuredClone(defaultConfiguration);
    changed.analysis.plies = 4;
    expect(
      configurationDifferences(defaultConfiguration, changed).map(
        (d) => d.path,
      ),
    ).toEqual(['analysis.plies']);
    changed.library.column_order.reverse();
    expect(isConfigurationDirty(defaultConfiguration, changed)).toBe(true);
  });
  it('normalizes visible columns in their configured order', () => {
    const changed = structuredClone(defaultConfiguration);
    changed.library.visible_columns.reverse();
    expect(isConfigurationDirty(defaultConfiguration, changed)).toBe(false);
  });
  it('rejects unknown fields, schema versions and invalid values', () => {
    expect(() =>
      normalizeConfiguration({ ...defaultConfiguration, unknown: true }),
    ).toThrow();
    expect(() =>
      normalizeConfiguration({ ...defaultConfiguration, schema_version: 2 }),
    ).toThrow();
    const changed = structuredClone(defaultConfiguration);
    changed.play.tutor_threshold = -1;
    expect(() => normalizeConfiguration(changed)).toThrow();
    changed.play.tutor_threshold = 0.04;
    changed.library.column_order.pop();
    expect(() => normalizeConfiguration(changed)).toThrow();
  });
});

it('preserves each position-occurrence sort choice in a profile', () => {
  for (const sort of [
    'occurrence_database',
    'occurrence_game',
    'occurrence_move',
  ]) {
    const configuration = structuredClone(defaultConfiguration);
    const input = {
      ...configuration,
      library: { ...configuration.library, sort },
    };
    expect(normalizeConfiguration(input).library.sort).toBe(sort);
  }
});

it('preserves position-search widths in old and new profiles and rejects invalid widths', () => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.library.column_widths = { white: 211 };
  expect(normalizeConfiguration(configuration).library.column_widths).toEqual({
    white: 211,
  });
  configuration.library.column_widths = {
    white: 211,
    occurrence_database: 240,
    occurrence_game: 90,
    occurrence_move: 100,
  };
  expect(normalizeConfiguration(configuration).library.column_widths).toEqual(
    configuration.library.column_widths,
  );
  for (const key of [
    'occurrence_database',
    'occurrence_game',
    'occurrence_move',
  ]) {
    for (const width of [0, 2401, 99.5]) {
      const invalid = structuredClone(configuration);
      invalid.library.column_widths![key] = width;
      expect(() => normalizeConfiguration(invalid)).toThrow();
    }
  }
});

it('persists money-game selection without permitting negative or excessive targets', () => {
  const c = structuredClone(defaultConfiguration);
  c.play.match_length = 0;
  expect(normalizeConfiguration(c).play.match_length).toBe(0);
  for (const length of [-1, 26, 1.5]) {
    c.play.match_length = length;
    expect(() => normalizeConfiguration(c)).toThrow();
  }
});

it('preserves optional game plans through profile normalization without changing old definitions', () => {
  const current = structuredClone(defaultConfiguration);
  expect(normalizeConfiguration(current).positions.definition).toEqual({
    schema_version: 1,
    sort: 'match',
  });
  current.positions.definition.game_plan = 'priming';
  expect(normalizeConfiguration(current).positions.definition.game_plan).toBe(
    'priming',
  );
  expect(configurationDifferences(defaultConfiguration, current)).toEqual([
    { path: 'positions.definition.game_plan', saved: null, current: 'priming' },
  ]);
  expect(() =>
    normalizeConfiguration({
      ...current,
      positions: {
        ...current.positions,
        definition: { ...current.positions.definition, game_plan: 'invalid' },
      },
    }),
  ).toThrow();
});

it('upgrades the column order and action widths from an existing version 1 profile', () => {
  const legacy = structuredClone(defaultConfiguration);
  legacy.library.column_order = legacy.library.column_order.filter(
    (id) => id !== 'play_mode',
  );
  legacy.library.visible_columns = ['white', 'score'];
  legacy.library.column_widths = { actions: 110, white: 211 };
  legacy.positions.column_widths = { actions: 40 };
  const upgraded = normalizeConfiguration(legacy);
  expect(upgraded.library.column_order).toEqual(
    defaultConfiguration.library.column_order,
  );
  expect(upgraded.library.visible_columns).toEqual(['white', 'score']);
  expect(upgraded.library.column_widths).toEqual({ actions: 160, white: 211 });
  expect(upgraded.positions.column_widths).toEqual({ actions: 60 });
  expect(normalizeConfiguration(upgraded)).toEqual(upgraded);
});
