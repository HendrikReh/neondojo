import type { components } from '../../src/generated-api';
import { playSessionFixture } from './play-session-fixture';

type StudyReportSummary = components['schemas']['StudyReportSummary'];
type PlaySession = components['schemas']['PlaySession'];

export const id = '66bb9950-416c-4bde-9241-bc2fd47dc057',
  other = '66bb9950-416c-4bde-9241-bc2fd47dc058';
export const summary = (
  reportId = id,
  name = 'Hendrik · GNUbg',
): StudyReportSummary => ({
  id: reportId,
  session_id: reportId,
  name,
  library_match_id: null,
  created_at: '2026-11-09T12:00:00Z',
  updated_at: '2026-11-10T13:30:00Z',
  human_player: 0,
  human_name: 'Hendrik',
  match_length: 7,
  scores: [7, 3],
  session_scores: [7, 3],
  money_session_ended: false,
  play_mode: 'tutor',
  first_attempt_rating: 4.5,
  assisted: true,
  analysis_plies: 3,
  engine_id: 'gnubg',
  bot_name: 'GNUbg',
});
export function detail(reportId = id): PlaySession {
  return playSessionFixture({
    id: reportId,
    human_name: 'Hendrik',
    engine: { id: 'gnubg', version: 'GNUbg', plies: 3 },
    version: 1,
    match_length: 7,
    phase: 'match_over',
    winner: 0,
    scores: [7, 3],
    history: [],
    human_player: 0,
    play_mode: 'tutor',
    decisions: [
      {
        id: 'decision',
        action: 'move',
        original_action: 'move',
        original_notation: '8/5 6/5',
        notation: '13/10 6/5',
        game_no: 1,
        history_index: 0,
        warning: true,
        revealed: true,
        assisted: true,
        retry_count: 1,
        first_loss: 0.12,
        loss: 0,
        first_severity: 'blunder',
        severity: 'none',
        alternatives: [],
        committed: true,
        attempts: [
          { action: 'move', notation: '8/5 6/5', loss: 0.12, assisted: false },
          { action: 'move', notation: '13/10 6/5', loss: 0, assisted: true },
        ],
      },
    ],
    review: {
      assisted: true,
      analysis_plies: 3,
      first_attempt_loss: 0.12,
      final_choice_loss: 0.08,
      first_attempt_rating: 4.5,
      first_attempt_rating_decisions: 3,
      first_attempt_rating_method:
        'gnubg-original-choices-emg500-reached-positions-v1',
      first_attempt_rating_profile: 'fixture',
      first_attempt_count: 3,
      retry_count: 1,
      independent_first_attempt_count: 2,
      assisted_first_attempt_count: 1,
      games: [
        {
          game_no: 1,
          engine: 'GNUbg',
          decisions: [
            {
              player: 0,
              decision_no: 1,
              kind: 'checker',
              played: '13/10 6/5',
              best: '13/10 6/5',
              equity_loss: 0,
              severity: 'none',
              evaluation_status: 'evaluated',
            },
          ],
        },
      ],
    },
  });
}

/**
 * A completed money game. Study opens point matches in a dedicated report
 * view and keeps the review dialog for money games.
 */
export const moneySummary = (
  reportId = id,
  name = 'Hendrik · GNUbg',
): StudyReportSummary => ({
  ...summary(reportId, name),
  match_length: 0,
  game_no: 1,
  session_id: reportId,
  scores: [2, 0],
  session_scores: [2, 0],
  money_session_ended: true,
});
/** Turn a saved report into the money game that `moneySummary` lists. */
export function asMoneyReport<T extends object>(report: T) {
  return Object.assign(report, { match_length: 0, review_game_no: 1 });
}
