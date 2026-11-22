import type { LibraryMatch } from './library-api';
import { playModeText } from '../play/play-mode-messages';
import type { Locale } from '../../shared/i18n/i18n-core';
import { libraryText, type LibraryMessageKey } from './library-messages';
import { workspaceText } from '../../shared/i18n/workspace-messages';
import { prDigits } from '../../shared/format/pr-format';
import { dateTimeFormat, numberFormat } from '../../shared/i18n/i18n-core';

export function libraryAnalysisStatus(status: string | null, locale: Locale) {
  const key: LibraryMessageKey =
    status === 'imported'
      ? 'importedAnalysisStatus'
      : status &&
          ['completed', 'queued', 'running', 'failed', 'partial'].includes(
            status,
          )
        ? (status as LibraryMessageKey)
        : 'noAnalysis';
  return libraryText(locale, key);
}

export function isUnfinishedCompetition(
  match: Pick<LibraryMatch, 'play_mode' | 'play_session_id' | 'play_phase'> &
    Partial<Pick<LibraryMatch, 'match_length'>>,
) {
  return (
    match.play_mode === 'competition' &&
    Boolean(match.play_session_id) &&
    match.play_phase !== 'match_over' &&
    !(match.match_length === 0 && match.play_phase === 'game_over')
  );
}
/** Matches a new analysis job can be submitted for. */
export function canAnalyzeMatch(
  match: Parameters<typeof isUnfinishedCompetition>[0] &
    Pick<LibraryMatch, 'source_available'> &
    Partial<Pick<LibraryMatch, 'play_paused'>>,
) {
  return (
    match.source_available &&
    !match.play_paused &&
    !isUnfinishedCompetition(match)
  );
}
export function canShowMatchPr(
  match: Pick<
    LibraryMatch,
    'play_mode' | 'play_session_id' | 'play_phase' | 'play_assisted'
  > &
    Partial<Pick<LibraryMatch, 'match_length'>>,
) {
  return (
    !match.play_assisted &&
    (!match.play_mode || match.play_mode === 'competition') &&
    !isUnfinishedCompetition(match)
  );
}
function excerpt(value: string, limit = 160) {
  const characters = Array.from(value.replace(/\s+/gu, ' ').trim());
  return characters.length > limit
    ? `${characters.slice(0, limit).join('')}…`
    : characters.join('');
}

export function createMatchSummaryFormatter(locale: Locale) {
  const l = (key: LibraryMessageKey) => libraryText(locale, key);
  const numbers = numberFormat(locale);
  const dates = dateTimeFormat(locale, { dateStyle: 'medium' });
  const number = (value: number) => numbers.format(value);
  const date = (value: string) =>
    dates.format(new Date(value.length === 10 ? `${value}T12:00:00` : value));
  const ratings = numberFormat(locale, prDigits);
  const pr = (value: number | null) =>
    value == null ? '—' : ratings.format(value);
  return (match: LibraryMatch) => {
    const name =
      match.display_name !== match.filename ? excerpt(match.display_name) : '';
    const result =
      match.white_score != null && match.black_score != null
        ? `${number(match.white_score)}–${number(match.black_score)}`
        : 'n/a';
    const tags = match.tags
      .slice(0, 6)
      .map((tag) => excerpt(tag, 64))
      .join(', ');
    const comment = excerpt(match.comment, 240);
    return [
      `ID: ${number(match.match_no)}${name ? ` · ${name}` : ''}`,
      `${excerpt(match.white, 240)} — ${excerpt(match.black, 240)}`,
      `${l('length')}: ${match.match_length ? number(match.match_length) : l('money')} · ${l('result')}: ${result}`,
      canShowMatchPr(match) &&
      (match.white_pr != null || match.black_pr != null)
        ? `PR: ${l('white')} ${pr(match.white_pr)} · ${l('black')} ${pr(match.black_pr)}`
        : '',
      match.play_mode
        ? `${playModeText(locale, match.play_mode)} · ${playModeText(locale, match.play_assisted ? 'assisted' : 'independent')}`
        : '',
      `${l('games')}: ${number(match.game_count)} · ${workspaceText(locale, 'positions')}: ${number(match.position_count)}`,
      `${l('analysis')}: ${libraryAnalysisStatus(match.analysis_status, locale)}${match.pr_profile ? ` · ${excerpt(match.pr_profile)}` : ''}`,
      [
        match.played_on ? `${l('played')}: ${date(match.played_on)}` : '',
        `${l('imported')}: ${date(match.created_at)}`,
      ]
        .filter(Boolean)
        .join(' · '),
      [
        match.tournament
          ? `${l('tournament')}: ${excerpt(match.tournament, 120)}`
          : '',
        match.round ? `${l('round')}: ${excerpt(match.round, 120)}` : '',
        match.place ? `${l('place')}: ${excerpt(match.place, 120)}` : '',
      ]
        .filter(Boolean)
        .join(' · '),
      tags
        ? `${l('tags')}: ${tags}${match.tags.length > 6 ? ` … (+${number(match.tags.length - 6)})` : ''}`
        : '',
      comment ? `${l('comment')}: ${comment}` : '',
    ]
      .filter(Boolean)
      .join('\n');
  };
}
