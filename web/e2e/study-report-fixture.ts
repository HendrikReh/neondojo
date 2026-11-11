export const id = '66bb9950-416c-4bde-9241-bc2fd47dc057',
  other = '66bb9950-416c-4bde-9241-bc2fd47dc058';
export const summary = (reportId = id, name = 'Hendrik · GNUbg') => ({
  id: reportId,
  name,
  library_match_id: null,
  created_at: '2026-11-09T12:00:00Z',
  updated_at: '2026-11-10T13:30:00Z',
  human_player: 0,
  human_name: 'Hendrik',
  match_length: 7,
  scores: [7, 3],
  play_mode: 'tutor',
  first_attempt_rating: 4.5,
  assisted: true,
  analysis_plies: 2,
});
export function detail(reportId = id) {
  return {
    id: reportId,
    human_player: 0,
    play_mode: 'tutor',
    decisions: [
      {
        id: 'decision',
        original_action: 'move',
        original_notation: '8/5 6/5',
        notation: '13/10 6/5',
        game_no: 1,
        history_index: 0,
        first_loss: 0.12,
        attempts: [
          { action: 'move', notation: '8/5 6/5', loss: 0.12, assisted: false },
          { action: 'move', notation: '13/10 6/5', loss: 0, assisted: true },
        ],
      },
    ],
    review: {
      assisted: true,
      analysis_plies: 2,
      first_attempt_loss: 0.12,
      first_attempt_rating: 4.5,
      first_attempt_count: 3,
      retry_count: 1,
      independent_first_attempt_count: 2,
      assisted_first_attempt_count: 1,
      games: [
        {
          game_no: 1,
          decisions: [
            {
              player: 0,
              decision_no: 1,
              played: '13/10 6/5',
              best: '13/10 6/5',
              equity_loss: 0,
            },
          ],
        },
      ],
    },
  };
}
