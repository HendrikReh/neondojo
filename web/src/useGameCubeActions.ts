import { useEffect, useState } from 'react';
import { request } from './shared/api/api';
import type { components } from './generated-api';

type GameDecisions = components['schemas']['GameDecisions'];
export type CubeAction = { id: string; player: number; notation: string };
export type GameCubeActions = {
  /** Cube actions recorded directly before a checker move, by its position. */
  before: Map<string, CubeAction[]>;
  /** Cube actions after the last checker move, such as a declined double. */
  trailing: CubeAction[];
};

/** Place each recorded cube action before the checker move that follows it. */
export function groupCubeActions(
  items: GameDecisions['items'],
): GameCubeActions {
  const before = new Map<string, CubeAction[]>();
  let pending: CubeAction[] = [];
  for (const item of [...items].sort((a, b) => a.move_no - b.move_no)) {
    if (item.kind === 'cube')
      pending.push({
        id: item.id,
        player: item.player,
        notation: item.notation,
      });
    else if (item.kind === 'checker' && item.position_uuid) {
      if (pending.length) before.set(item.position_uuid, pending);
      pending = [];
    }
  }
  return { before, trailing: pending };
}

/** Doubles, takes and passes of one game, for display between checker moves. */
export function useGameCubeActions(matchId?: string, gameNo?: number) {
  const key = matchId && gameNo ? `${matchId}:${gameNo}` : '';
  const [state, setState] = useState<{
    key: string;
    actions: GameCubeActions;
  }>();
  useEffect(() => {
    if (!key) return;
    const abort = new AbortController();
    void request<GameDecisions>(
      `/matches/${matchId}/games/${gameNo}/decisions`,
      { signal: abort.signal },
    )
      .then((game) => {
        if (!abort.signal.aborted)
          setState({ key, actions: groupCubeActions(game.items) });
      })
      // Checker rows stay usable without the cube markers.
      .catch(() => undefined);
    return () => abort.abort();
  }, [key, matchId, gameNo]);
  return state?.key === key ? state.actions : undefined;
}
