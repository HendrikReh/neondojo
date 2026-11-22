import type { Position } from '../../../shared/http/api';
import type { PlayReviewHistory, PlaySession } from '../../play/play-api';
export type ReviewDecision = NonNullable<
  PlaySession['review']
>['games'][number]['decisions'][number];
export interface ReviewDecisionEntry {
  key: string;
  gameNo: number;
  decision: ReviewDecision;
}
export interface ReviewDecisionPosition {
  position: Position;
  historyIndex: number;
}
export const reviewDecisionKey = (gameNo: number, decisionNo: number) =>
  `${gameNo}:${decisionNo}`;
export function humanReviewDecisions(
  session: PlaySession,
): ReviewDecisionEntry[] {
  return (
    session.review?.games.flatMap((game) =>
      game.decisions
        .filter((d) => d.player === session.human_player)
        .map((decision) => ({
          key: reviewDecisionKey(game.game_no, decision.decision_no),
          gameNo: game.game_no,
          decision,
        })),
    ) ?? []
  );
}
/** Resolve native decision ordinals against immutable source history, never the final board. */
export function buildLearningReviewPositions(
  review: NonNullable<PlaySession['review']>,
  history: PlayReviewHistory,
) {
  const result = new Map<string, ReviewDecisionPosition>();
  for (const game of review.games) {
    const events = history.items.filter(
      (item) => item.game_no === game.game_no,
    );
    const checkers = events.filter((item) => item.kind === 'checker');
    let cursor = 0;
    for (const decision of game.decisions) {
      const key = reviewDecisionKey(game.game_no, decision.decision_no);
      if (
        decision.kind === 'checker' ||
        (decision.kind === 'cube' &&
          decision.played.toLowerCase() === 'no double')
      ) {
        const ordinal = decision.checker_no;
        if (ordinal == null || !Number.isInteger(ordinal) || ordinal < 1) break;
        const event = checkers[ordinal - 1];
        if (!event || event.before.player !== decision.player) break;
        const at = events.indexOf(event);
        if (at < cursor) break;
        if (decision.kind === 'checker') {
          if (
            !decision.dice ||
            decision.dice.length !== 2 ||
            [...decision.dice].sort().join() !==
              [...event.before.dice].sort().join()
          )
            break;
          result.set(key, {
            position: event.before,
            historyIndex: event.index,
          });
          cursor = at + 1;
        } else {
          result.set(key, {
            position: { ...event.before, dice: [] },
            historyIndex: event.index,
          });
          cursor = at;
        }
      } else if (decision.kind === 'cube') {
        const event = events[cursor];
        if (
          !event ||
          event.kind !== 'cube' ||
          event.before.player !== decision.player ||
          event.before.notation.toLowerCase() !== decision.played.toLowerCase()
        )
          break;
        result.set(key, { position: event.before, historyIndex: event.index });
        cursor++;
      } else break;
    }
  }
  return result;
}
