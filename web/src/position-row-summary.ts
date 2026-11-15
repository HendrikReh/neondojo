import type { PositionBrowserHit } from './position-browser-api';
import type { Locale } from './i18n-core';
import { localizeDecision } from './i18n-core';
import { positionText } from './position-browser-messages';
import { libraryText } from './library-messages';
import { analysisCatalogText } from './analysis-catalog-messages';

export const positionPlayer = (hit: PositionBrowserHit) =>
  hit.position.player === 0 ? hit.match_info.white : hit.match_info.black;
export const positionOpponent = (hit: PositionBrowserHit) =>
  hit.position.player === 0 ? hit.match_info.black : hit.match_info.white;
const compact = (text: string, limit = 160) => {
  const chars = Array.from(text.replace(/\s+/gu, ' ').trim());
  return chars.length > limit
    ? chars.slice(0, limit).join('') + '…'
    : chars.join('');
};
export function positionAnalysisProvenance(
  error: NonNullable<PositionBrowserHit['native_error']>,
  locale: Locale,
) {
  if (error.job_id && error.plies != null)
    return positionText(locale, 'provenance', {
      engine: error.engine,
      plies: error.plies,
      job: error.job_id,
    });
  return [
    analysisCatalogText(
      locale,
      error.analysis_id?.startsWith('imported:') ? 'imported' : 'native',
    ),
    error.engine,
    error.depth ??
      (error.plies == null
        ? positionText(locale, 'nativeUnknown')
        : positionText(locale, 'depth', { plies: error.plies })),
  ]
    .filter(Boolean)
    .join(' · ');
}

export function createPositionSummaryFormatter(
  locale: Locale,
  showPlayed: boolean,
) {
  const n = new Intl.NumberFormat(locale),
    loss = new Intl.NumberFormat(locale, {
      minimumFractionDigits: 3,
      maximumFractionDigits: 3,
    });
  const p = (
    key: Parameters<typeof positionText>[1],
    params?: Parameters<typeof positionText>[2],
  ) => positionText(locale, key, params);
  const l = (key: Parameters<typeof libraryText>[1]) =>
    libraryText(locale, key);
  return (hit: PositionBrowserHit) =>
    [
      `ID: ${n.format(hit.match_no)} · ${compact(hit.display_name)}`,
      `${p('database')}: ${compact(hit.database_name)}`,
      `${p('player')}: ${compact(positionPlayer(hit), 240)} · ${p('opponent')}: ${compact(positionOpponent(hit), 240)}`,
      `${p('game', { number: hit.position.game_no })} · ${p('ply', { number: hit.position.ply })} · ${p('dice')}: ${hit.position.dice.join('–')}`,
      `${p('score')}: ${hit.white_score == null || hit.black_score == null ? 'n/a' : `${n.format(hit.white_score)}–${n.format(hit.black_score)}`} · ${p('cube')}: ${hit.cube_value == null ? 'n/a' : n.format(hit.cube_value)}`,
      `${p('matchPoints')}: ${hit.match_info.match_length ? n.format(hit.match_info.match_length) : l('money')}`,
      [
        hit.tournament ? `${p('tournament')}: ${compact(hit.tournament)}` : '',
        hit.round ? `${p('round')}: ${compact(hit.round)}` : '',
      ]
        .filter(Boolean)
        .join(' · '),
      `${p('checkerId')}: ${hit.position.position_id}`,
      hit.native_error
        ? `${p('nativeError')}: ${loss.format(hit.native_error.equity_loss)} · ${positionAnalysisProvenance(hit.native_error, locale)}`
        : `${p('nativeError')}: n/a`,
      !hit.card_ready ? p('needed') : '',
      `${p('decks')}: ${hit.decks.length ? hit.decks.map((deck) => deck.name).join(', ') : p('noDecks')}`,
      showPlayed
        ? `${p('playedMove')}: ${localizeDecision(locale, hit.position.notation)}`
        : '',
      hit.tags.length
        ? `${l('tags')}: ${hit.tags
            .slice(0, 6)
            .map((tag) => compact(tag, 64))
            .join(', ')}${hit.tags.length > 6 ? ' …' : ''}`
        : '',
      ...hit.notes
        .filter((note) => note.comment_excerpt.trim())
        .slice(0, 4)
        .map(
          (note) =>
            `${p(({ match: 'scopeMatch', game: 'scopeGame', position: 'scopePosition', move: 'scopeMove' } as const)[note.scope])}: ${compact(note.comment_excerpt)}`,
        ),
    ]
      .filter(Boolean)
      .join('\n');
}
