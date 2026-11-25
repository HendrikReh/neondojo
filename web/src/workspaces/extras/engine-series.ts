import options from '../../../../shared/configuration-options.json' with { type: 'json' };
import type { EngineInfo } from '../../settings/settings-api';
import type { CreateSeries, Series } from './engine-match-api';
import type { EngineMatchLabel } from './engine-match-messages';

export type Competitor = CreateSeries['a'];
export type Mode = NonNullable<CreateSeries['mode']>;
export type Format = NonNullable<CreateSeries['format']>;
export type Text = (key: EngineMatchLabel) => string;
export type SeriesDraft = {
  a: Competitor;
  b: Competitor;
  format: Format;
  mode: Mode;
  length: string;
  blockGames: string;
  count: string;
  seed: string;
};

export const initialDraft: SeriesDraft = {
  a: { engine_id: 'gnubg', plies: 2 },
  b: { engine_id: 'gnubg', plies: 1 },
  format: 'match',
  mode: 'paired',
  length: '5',
  blockGames: '100',
  count: '1',
  seed: '',
};
export const MAX_BLOCK_GAMES = 1000;
export const isMoney = (config: { format?: Format }) =>
  config.format === 'money_cubeless';
/** Label for one stored unit (match or block), or a pair of them. */
export const unitLabel = (money: boolean, paired: boolean): EngineMatchLabel =>
  money ? (paired ? 'blockPairs' : 'blocks') : paired ? 'pairs' : 'matches';

export const isActive = (status: string) =>
  status === 'queued' || status === 'running';
export const statusLabel = (status: string, text: Text) => {
  switch (status) {
    case 'queued':
    case 'running':
    case 'completed':
    case 'failed':
    case 'cancelled':
      return text(status);
    default:
      return status;
  }
};
export const duelDepths = (id: string, engines: EngineInfo[]) =>
  id === 'gnubg' || id === 'bgsage'
    ? options.engine_profiles[id].duel.filter(
        (depth) =>
          id !== 'bgsage' ||
          depth !== 1 ||
          engines.find((e) => e.id === id)?.plies.includes(1),
      )
    : [];

export const validNumber = (value: string, max: number) =>
  Number.isInteger(Number(value)) && Number(value) >= 1 && Number(value) <= max;
export const progressLabel = (
  run: Series,
  text: Text,
  number: (value: number) => string,
) =>
  `${number(run.completed_matches)} / ${number(run.total_matches)} ${text(isMoney(run.configuration) ? 'blocks' : 'matches')}`;
