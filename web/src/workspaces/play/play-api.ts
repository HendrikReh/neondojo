import { request } from '../../shared/api/api';
import type { BearoffDirection } from '../../shared/board/play-state';
import type { PlayMode } from './play-mode-messages';
type Schema = import('../../generated-api').components['schemas'];
export type PlaySeverity = Schema['MoveSeverity'];
export type SageDifficulty = Schema['SageDifficulty'];
import { errorGrades } from '../../shared/review/move-severity';

export const playSeverity = (value: string | null | undefined): PlaySeverity =>
  errorGrades.find((grade) => grade === value) ?? 'none';
export type PlayAlternative = Schema['PlayAlternative'];
export type PlayDecision = Schema['PlayDecision'];
// Legacy saved sessions and older cached snapshots have no feedback profile.
export type PlaySession = Omit<
  Schema['PlaySession'],
  | 'bearoff_direction'
  | 'play_mode'
  | 'analysis_plies'
  | 'tutor_threshold'
  | 'training_auto_analysis'
  | 'pending_decision'
  | 'decisions'
  | 'review'
  | 'learning_used'
  | 'game_results'
  | 'money_session_ended'
  | 'game_learning_used'
> & {
  bearoff_direction: BearoffDirection;
  game_learning_used?: boolean;
  money_session_ended?: boolean;
  game_results?: import('./GameResultSummary').PlayGameResult[];
  library_match_id?: string | null;
  play_mode?: PlayMode;
  learning_used?: boolean;
  analysis_plies?: number;
  tutor_threshold?: number;
  training_auto_analysis?: boolean;
  pending_decision?: PlayDecision | null;
  decisions?: PlayDecision[];
  review?: Schema['PlayReview'] | null;
};
export type PlayAction = Schema['PlayAction'];
const json = (body: unknown): RequestInit => ({
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(body),
});
export const listPlaySessions = (signal?: AbortSignal) =>
  request<PlaySession[]>('/play/sessions', { signal });
export const loadPlaySession = (id: string, signal?: AbortSignal) =>
  request<PlaySession>(`/play/sessions/${encodeURIComponent(id)}`, { signal });
export const createPlaySession = (
  human_player: number,
  match_length: number,
  preferences?: Pick<
    Schema['CreatePlaySession'],
    | 'play_mode'
    | 'tutor_threshold'
    | 'training_auto_analysis'
    | 'human_name'
    | 'play_engine'
    | 'play_plies'
    | 'play_top_two'
    | 'play_difficulty'
  >,
) =>
  request<PlaySession>(
    '/play/sessions',
    json({ human_player, match_length, ...preferences }),
  );
export const actInPlaySession = (
  session: PlaySession,
  action: PlayAction,
  move?: number,
  decision_id?: string,
  resign_level?: 1 | 2 | 3,
) =>
  request<PlaySession>(
    `/play/sessions/${encodeURIComponent(session.id)}/actions`,
    json({
      version: session.version,
      action,
      ...(move === undefined ? {} : { move }),
      ...(decision_id === undefined ? {} : { decision_id }),
      ...(resign_level === undefined ? {} : { resign_level }),
    }),
  );

export type PlayReviewEntry = Schema['PlayHistorySnapshot'];
export type PlayReviewHistory = Schema['PlayHistorySnapshots'];
export const loadPlayHistory = (session: PlaySession, signal: AbortSignal) =>
  request<PlayReviewHistory>(
    `/play/sessions/${encodeURIComponent(session.id)}/history?version=${session.version}`,
    { signal },
  );

export const loadLearningReviewHistory = (
  session: PlaySession,
  signal: AbortSignal,
) =>
  session.study_report_id
    ? request<PlayReviewHistory>(
        `/study/reports/${encodeURIComponent(session.study_report_id)}/history?version=${session.study_report_version ?? session.version}`,
        { signal },
      )
    : loadPlayHistory(session, signal);

export const updatePlayFeedback = (
  session: PlaySession,
  operation: 'hint' | 'reveal' | 'retry',
) =>
  request<PlaySession>(
    `/play/sessions/${encodeURIComponent(session.id)}/feedback`,
    json({
      version: session.version,
      operation,
      ...(session.pending_decision
        ? { decision_id: session.pending_decision.id }
        : {}),
    }),
  );

export const reviewPlaySession = (session: PlaySession) =>
  request<PlaySession>(
    `/play/sessions/${encodeURIComponent(session.id)}/review`,
    json({ version: session.version }),
  );

export const changePlayMode = (
  session: PlaySession,
  play_mode: PlayMode,
  tutor_threshold?: number,
) =>
  request<PlaySession>(
    `/play/sessions/${encodeURIComponent(session.id)}/mode`,
    json({
      version: session.version,
      play_mode,
      ...(tutor_threshold === undefined ? {} : { tutor_threshold }),
    }),
  );
