import {
  emptyFilters,
  type LibraryFilters,
  type LibrarySort,
} from './library-api';
import {
  ALL_CARDS,
  isCollectionScope,
  emptyCardFilters,
  type CardFilters,
} from './card-search-api';
import {
  DEFAULT_LIBRARY_COLUMNS,
  LIBRARY_DATA_COLUMNS,
  type LibraryDataColumn,
} from './library-column-preferences';
import { LIBRARY_WIDTH_COLUMNS } from './library-columns';
import { POSITION_COLUMNS } from './position-table-columns';
import {
  positionDefinition,
  positionDraft,
  type PositionFilterDefinition,
} from './position-browser-api';
import type { AnnotationFilters } from './annotation-api';
import { isLocale, type Locale } from './i18n-core';
import { isPlayMode, type PlayMode } from './play-mode-messages';
import { uiThemes, type UiTheme, type UiMode } from './ui-appearance';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };

export interface ProfileConfiguration {
  schema_version: 1;
  general: { player_name: string; locale: Locale; play_sounds: boolean };
  analysis: {
    engine: 'gnubg' | 'bgsage';
    plies: number;
    scope: 'game' | 'match';
    search_filter: 'engine_default';
  };
  play: {
    engine: 'gnubg' | 'bgsage';
    plies: number;
    default_mode: PlayMode;
    tutor_threshold: number;
    training_auto_analysis: boolean;
    human_player: 0 | 1;
    match_length: number;
  };
  appearance: {
    ui_theme: UiTheme;
    mode: UiMode;
    board_theme: string;
    bearoff_direction: 'left' | 'right';
    board_view: 'before' | 'arrows' | 'after';
    analysis_board_width: number | null;
    analysis_overview_board_width: number | null;
  };
  opening_book: {
    provider: 'off' | 'automatic' | 'backgammondb' | 'xg';
    path: string;
    xg_path: string;
  };
  library: {
    database_id: string;
    filters: LibraryFilters;
    sort: LibrarySort;
    direction: 'asc' | 'desc';
    visible_columns: LibraryDataColumn[];
    column_order: LibraryDataColumn[];
    column_widths: Record<string, number> | null;
  };
  positions: {
    definition: PositionFilterDefinition;
    show_played: boolean;
    column_widths: Record<string, number> | null;
  };
  learning: {
    deck_id: string;
    filters: CardFilters;
    generation: { min_loss: number; player: string };
  };
  annotations: { filters: AnnotationFilters | null };
  review: {
    player: string;
    kind: string;
    severity: string;
    errors_only: boolean;
  };
}

export const defaultConfiguration: ProfileConfiguration = {
  schema_version: 1,
  general: { player_name: '', locale: 'en', play_sounds: true },
  analysis: {
    engine: 'gnubg',
    plies: 2,
    scope: 'game',
    search_filter: 'engine_default',
  },
  play: {
    engine: 'gnubg',
    plies: 0,
    default_mode: 'coach',
    tutor_threshold: 0.04,
    training_auto_analysis: true,
    human_player: 0,
    match_length: 7,
  },
  appearance: {
    ui_theme: 'graphite',
    mode: 'system',
    board_theme: 'forest',
    bearoff_direction: 'right',
    board_view: 'arrows',
    analysis_board_width: null,
    analysis_overview_board_width: null,
  },
  opening_book: { provider: 'automatic', path: '', xg_path: '' },
  library: {
    database_id: '',
    filters: emptyFilters(),
    sort: 'created_at',
    direction: 'desc',
    visible_columns: [...DEFAULT_LIBRARY_COLUMNS],
    column_order: [...LIBRARY_DATA_COLUMNS],
    column_widths: null,
  },
  positions: {
    definition: { schema_version: 1, sort: 'match' },
    show_played: false,
    column_widths: null,
  },
  learning: {
    deck_id: ALL_CARDS,
    filters: emptyCardFilters(),
    generation: { min_loss: 0.04, player: '' },
  },
  annotations: { filters: null },
  review: { player: '', kind: '', severity: '', errors_only: true },
};

export interface ConfigurationDifference {
  path: string;
  saved: unknown;
  current: unknown;
}
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const invalid = (path: string): never => {
  throw new Error(`Invalid configuration: ${path}`);
};
function exactKeys(
  value: unknown,
  required: readonly string[],
  optional: readonly string[] = [],
  path = '',
): asserts value is Record<string, unknown> {
  if (
    !object(value) ||
    required.some((key) => !Object.hasOwn(value, key)) ||
    Object.keys(value).some(
      (key) => !required.includes(key) && !optional.includes(key),
    )
  )
    invalid(path);
}
function strings(value: unknown, keys: readonly string[], path: string) {
  exactKeys(value, keys, [], path);
  if (Object.values(value).some((v) => typeof v !== 'string')) invalid(path);
}
function widths(
  value: unknown,
  columns: readonly { id: string; min: number }[],
  path: string,
) {
  if (value === null) return;
  if (!object(value)) invalid(path);
  for (const [id, width] of Object.entries(value as Record<string, unknown>)) {
    const column = columns.find((c) => c.id === id);
    if (
      !column ||
      typeof width !== 'number' ||
      !Number.isInteger(width) ||
      width < column.min ||
      width > 2400
    )
      invalid(`${path}.${id}`);
  }
}
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (object(value))
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .filter((k) => value[k] !== undefined)
        .map((key) => [key, canonical(value[key])]),
    );
  return value;
}
const boardWidthFields = [
  'analysis_board_width',
  'analysis_overview_board_width',
] as const;

export function normalizeConfiguration(value: unknown): ProfileConfiguration {
  exactKeys(value, Object.keys(defaultConfiguration), [], 'configuration');
  for (const key of Object.keys(defaultConfiguration).filter(
    (k) => k !== 'schema_version',
  )) {
    const optional: readonly string[] =
      key === 'appearance' ? boardWidthFields : [];
    exactKeys(
      value[key],
      Object.keys(
        defaultConfiguration[key as keyof ProfileConfiguration] as object,
      ).filter((field) => !optional.includes(field)),
      optional,
      key,
    );
  }
  const c = structuredClone(value) as unknown as ProfileConfiguration;
  if (c.schema_version !== 1) invalid('schema_version');
  if (
    typeof c.general.player_name !== 'string' ||
    [...c.general.player_name].length > 120 ||
    !isLocale(c.general.locale) ||
    typeof c.general.play_sounds !== 'boolean'
  )
    invalid('general');
  if (
    !['gnubg', 'bgsage'].includes(c.analysis.engine) ||
    !Number.isInteger(c.analysis.plies) ||
    !configurationOptions.engine_profiles[c.analysis.engine].analysis.includes(
      c.analysis.plies,
    ) ||
    !['game', 'match'].includes(c.analysis.scope) ||
    c.analysis.search_filter !== 'engine_default'
  )
    invalid('analysis');
  if (
    !['gnubg', 'bgsage'].includes(c.play.engine) ||
    !Number.isInteger(c.play.plies) ||
    !configurationOptions.engine_profiles[c.play.engine].play.includes(
      c.play.plies,
    ) ||
    !isPlayMode(c.play.default_mode) ||
    !Number.isFinite(c.play.tutor_threshold) ||
    c.play.tutor_threshold <= 0 ||
    c.play.tutor_threshold > 1 ||
    typeof c.play.training_auto_analysis !== 'boolean' ||
    ![0, 1].includes(c.play.human_player) ||
    !Number.isInteger(c.play.match_length) ||
    c.play.match_length < 0 ||
    c.play.match_length > 25
  )
    invalid('play');
  if (
    !uiThemes.includes(c.appearance.ui_theme) ||
    !['light', 'dark', 'system'].includes(c.appearance.mode) ||
    !configurationOptions.board_themes.includes(c.appearance.board_theme) ||
    !['left', 'right'].includes(c.appearance.bearoff_direction) ||
    !['before', 'arrows', 'after'].includes(c.appearance.board_view)
  )
    invalid('appearance');
  const { min, max } = configurationOptions.analysis_board_width;
  for (const field of boardWidthFields) {
    const width = (c.appearance[field] ??= null);
    if (
      width !== null &&
      (!Number.isInteger(width) || width < min || width > max)
    )
      invalid(`appearance.${field}`);
  }
  if (
    !['off', 'automatic', 'backgammondb', 'xg'].includes(
      c.opening_book.provider,
    ) ||
    typeof c.opening_book.path !== 'string' ||
    typeof c.opening_book.xg_path !== 'string'
  )
    invalid('opening_book');
  const uuid = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i;
  if (
    (c.library.database_id !== '' && !uuid.test(c.library.database_id)) ||
    !['asc', 'desc'].includes(c.library.direction)
  )
    invalid('library');
  const sorts = [
    'occurrence_database',
    'occurrence_game',
    'occurrence_move',
    'id',
    'name',
    'white',
    'white_pr',
    'black',
    'black_pr',
    'match_length',
    'result',
    'score',
    'played_at',
    'created_at',
    'tournament',
    'format',
    'position_count',
    'analysis_status',
    'tags',
    'comment',
  ];
  if (!sorts.includes(c.library.sort)) invalid('library.sort');
  strings(c.library.filters, Object.keys(emptyFilters()), 'library.filters');
  for (const key of Object.keys(c.library.filters) as (keyof LibraryFilters)[])
    c.library.filters[key] = c.library.filters[key].trim();
  c.library.filters.tag = c.library.filters.tag
    .replace(/\s+/gu, ' ')
    .toLowerCase();
  const order = c.library.column_order;
  if (
    Array.isArray(order) &&
    order.length === LIBRARY_DATA_COLUMNS.length - 1 &&
    !order.includes('play_mode')
  ) {
    const index = order.indexOf('match_length');
    if (index >= 0) order.splice(index, 0, 'play_mode');
  }
  if (
    !Array.isArray(c.library.column_order) ||
    c.library.column_order.length !== LIBRARY_DATA_COLUMNS.length ||
    new Set(c.library.column_order).size !== LIBRARY_DATA_COLUMNS.length ||
    c.library.column_order.some((id) => !LIBRARY_DATA_COLUMNS.includes(id))
  )
    invalid('library.column_order');
  if (
    !Array.isArray(c.library.visible_columns) ||
    !c.library.visible_columns.length ||
    c.library.visible_columns.some((id) => !LIBRARY_DATA_COLUMNS.includes(id))
  )
    invalid('library.visible_columns');
  c.library.visible_columns = c.library.column_order.filter((id) =>
    c.library.visible_columns.includes(id),
  );
  for (const [saved, old, current] of [
    [c.library.column_widths, 88, 160],
    [c.positions.column_widths, 40, 60],
  ] as const) {
    const actionWidth = saved?.actions;
    if (
      saved &&
      typeof actionWidth === 'number' &&
      Number.isInteger(actionWidth) &&
      actionWidth >= old &&
      actionWidth < current
    )
      saved.actions = current;
  }
  widths(
    c.library.column_widths,
    LIBRARY_WIDTH_COLUMNS,
    'library.column_widths',
  );
  widths(
    c.positions.column_widths,
    POSITION_COLUMNS,
    'positions.column_widths',
  );
  if (typeof c.positions.show_played !== 'boolean')
    invalid('positions.show_played');
  const positionFields = [
    'database_id',
    'q',
    'player',
    'match_name',
    'tournament',
    'round',
    'tag',
    'comment',
    'position_id',
    'match_points_min',
    'match_points_max',
    'die1',
    'die2',
    'white_score',
    'black_score',
    'cube_value',
    'min_loss',
    'max_loss',
    'crawford',
    'analysis_state',
    'direction',
    'game_plan',
  ];
  exactKeys(
    c.positions.definition,
    ['schema_version', 'sort'],
    positionFields,
    'positions.definition',
  );
  if (c.positions.definition.schema_version !== 1)
    invalid('positions.definition.schema_version');
  c.positions.definition = positionDefinition(
    positionDraft(c.positions.definition),
  );
  exactKeys(
    c.learning.filters,
    Object.keys(emptyCardFilters()),
    ['topics', 'game_plan'],
    'learning.filters',
  );
  if (
    Object.values(c.learning.filters).some((value) => typeof value !== 'string')
  )
    invalid('learning.filters');
  for (const key of Object.keys(c.learning.filters) as (keyof CardFilters)[])
    c.learning.filters[key] = c.learning.filters[key]?.trim() as never;
  if (
    !['', 'checker', 'cube_offer', 'cube_response'].includes(
      c.learning.filters.kind,
    ) ||
    !['', 'new', 'due', 'reviewed'].includes(c.learning.filters.status)
  )
    invalid('learning.filters');
  c.learning.filters.tag = c.learning.filters.tag.toLowerCase();
  if (
    !['', 'pure_race', 'racing', 'attacking', 'priming', 'anchoring'].includes(
      c.learning.filters.game_plan ?? '',
    )
  )
    invalid('learning.filters.game_plan');
  if (!isCollectionScope(c.learning.deck_id) && !uuid.test(c.learning.deck_id))
    invalid('learning.deck_id');
  exactKeys(
    c.learning.generation,
    ['min_loss', 'player'],
    [],
    'learning.generation',
  );
  if (
    !Number.isFinite(c.learning.generation.min_loss) ||
    c.learning.generation.min_loss < 0 ||
    typeof c.learning.generation.player !== 'string'
  )
    invalid('learning.generation');
  c.learning.generation.player = c.learning.generation.player.trim();
  if (!['', '0', '1'].includes(c.learning.generation.player))
    invalid('learning.generation.player');
  if (c.annotations.filters !== null) {
    exactKeys(
      c.annotations.filters,
      ['q', 'tag', 'kind'],
      ['database_id'],
      'annotations.filters',
    );
    if (Object.values(c.annotations.filters).some((v) => typeof v !== 'string'))
      invalid('annotations.filters');
    if (
      !['', 'match', 'game', 'position', 'move'].includes(
        c.annotations.filters.kind,
      )
    )
      invalid('annotations.filters.kind');
    for (const key of Object.keys(
      c.annotations.filters,
    ) as (keyof AnnotationFilters)[])
      c.annotations.filters[key] = c.annotations.filters[key]?.trim() as never;
    c.annotations.filters.tag = c.annotations.filters.tag
      .replace(/\s+/gu, ' ')
      .toLowerCase();
  }
  if (
    typeof c.review.player !== 'string' ||
    typeof c.review.kind !== 'string' ||
    typeof c.review.severity !== 'string' ||
    typeof c.review.errors_only !== 'boolean'
  )
    invalid('review');
  return canonical(c) as ProfileConfiguration;
}
export function configurationDifferences(
  saved: unknown,
  current: unknown,
): ConfigurationDifference[] {
  const a = normalizeConfiguration(saved),
    b = normalizeConfiguration(current);
  const output: ConfigurationDifference[] = [];
  function visit(left: unknown, right: unknown, path: string) {
    if (JSON.stringify(left) === JSON.stringify(right)) return;
    if (object(left) && object(right)) {
      for (const key of new Set([...Object.keys(left), ...Object.keys(right)]))
        visit(left[key], right[key], path ? `${path}.${key}` : key);
    } else output.push({ path, saved: left ?? null, current: right ?? null });
  }
  visit(a, b, '');
  return output;
}
export function isConfigurationDirty(saved: unknown, current: unknown) {
  return configurationDifferences(saved, current).length > 0;
}
