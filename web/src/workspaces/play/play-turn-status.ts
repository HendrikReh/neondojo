import { matchContext } from '../../shared/board/boardMath';
import type { useI18n } from '../../shared/i18n/i18n';
import type { PlayAction, PlaySession } from './play-api';
import type { usePlayText } from './play-messages';
import type { usePlayModeText } from './play-mode-messages';
import type { usePlayResignText } from './play-resign-messages';
import type { PlayScene } from './usePlayAnimation';

export interface PlayTurnStatusContext {
  session: PlaySession | null;
  reviewActive: boolean;
  paused: boolean;
  scene: PlayScene | null;
  openingScene: Extract<PlayScene, { kind: 'dice' }> | null;
  openingPhase: boolean;
  pendingRoll: PlayAction | null;
  busy: boolean;
  hintOpen: boolean;
  awaitingFeedback: boolean;
  forcedPass: boolean;
  name: (player: number) => string;
}

/** Resolves the live status message for the current turn, animation, or phase. */
export function computePlayTurnStatus(
  ctx: PlayTurnStatusContext,
  text: {
    t: ReturnType<typeof usePlayText>;
    appT: ReturnType<typeof useI18n>['t'];
    modeT: ReturnType<typeof usePlayModeText>;
    resignT: ReturnType<typeof usePlayResignText>;
  },
): string {
  const { session, scene, openingScene, openingPhase, name } = ctx;
  const { t, appT, modeT, resignT } = text;
  if (!session) return '';
  if (ctx.reviewActive) return t('reviewing');
  if (ctx.paused) return t('matchPaused');
  if (scene?.kind === 'checker')
    return `${name(scene.player)} · ${t('watchMove')}`;
  if (openingScene && !openingScene.rolling)
    return openingScene.tied
      ? t('openingTie')
      : appT('app.toMove', { player: name(openingScene.player) });
  if (ctx.pendingRoll || scene?.kind === 'dice')
    return openingPhase
      ? t('openingRolling')
      : `${name(scene?.player ?? session.human_player)} · ${t('rolling')}`;
  if (ctx.busy && !ctx.hintOpen)
    return openingPhase ? t('openingPreparing') : t('thinking');
  if (ctx.awaitingFeedback) return modeT('unconfirmed');
  if (session.money_session_ended) return t('sessionEnded');
  if (session.match_forfeited) return resignT('matchForfeited');
  if (ctx.forcedPass) return t('pass');
  if (session.phase === 'move' || session.phase === 'roll')
    return session.phase === 'roll' ||
      session.position.player === session.human_player
      ? t('yourTurn')
      : appT('app.toMove', { player: name(session.position.player) });
  return t(session.phase);
}

/** Explains why the doubling cube is currently unavailable. */
export function doubleUnavailableReasonKey(session: PlaySession | null) {
  const cubeContext = session
    ? matchContext(session.position.gnubg_match_id)
    : null;
  if (cubeContext?.crawford) return 'doubleCrawford' as const;
  if (cubeContext?.owner != null && cubeContext.owner !== session?.human_player)
    return 'doubleOpponentCube' as const;
  if (
    session &&
    session.match_length > 0 &&
    cubeContext &&
    cubeContext.cube >=
      session.match_length - session.scores[session.human_player]
  )
    return 'doubleMatchCovered' as const;
  return 'cubeUnavailable' as const;
}
