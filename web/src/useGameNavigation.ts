import { useEffect, useRef, useState } from 'react';
import {
  request,
  type Match,
  type MatchGame,
  type Position,
  type Page,
  type DiceStats,
} from './api';
import { AppError } from './i18n-core';

type Navigation = {
  match: Match | null;
  games: MatchGame[];
  gameNo?: number;
  positions: Position[];
  index: number;
  stats: DiceStats | null;
  loading: boolean;
  error: unknown;
  targetId?: string;
};
type Task = { version: number; controller: AbortController };
const initial = (): Navigation => ({
  match: null,
  games: [],
  positions: [],
  index: 0,
  stats: null,
  loading: false,
  error: null,
});

/** One game at a time. A newer navigation invalidates every older response. */
export function useGameNavigation() {
  const [view, setView] = useState<Navigation>(initial);
  const current = useRef(view);
  const version = useRef(0);
  const active = useRef<AbortController | undefined>(undefined);
  const activeMatch = useRef<string | undefined>(undefined);
  useEffect(
    () => () => {
      version.current++;
      active.current?.abort();
    },
    [],
  );

  function update(patch: Partial<Navigation>) {
    current.current = { ...current.current, ...patch };
    setView(current.current);
  }
  function begin(matchId?: string): Task {
    activeMatch.current = matchId;
    active.current?.abort();
    const controller = new AbortController();
    active.current = controller;
    const task = { version: ++version.current, controller };
    update({ loading: true, error: null });
    return task;
  }
  function valid(task: Task) {
    return version.current === task.version && !task.controller.signal.aborted;
  }
  function ensureCurrent(task: Task) {
    if (!valid(task))
      throw new DOMException('Superseded navigation', 'AbortError');
  }
  function fail(task: Task, error: unknown) {
    if (valid(task)) {
      update({ loading: false, error });
      task.controller.abort();
    }
    return false;
  }

  async function load(
    match: Match,
    gameNo: number | undefined,
    targetId: string | undefined,
    task: Task,
  ) {
    ensureCurrent(task);
    const cached =
      current.current.match?.id === match.id ? current.current.games : [];
    update({
      match,
      games: cached,
      gameNo,
      positions: [],
      index: 0,
      stats: null,
      targetId,
    });
    const games = cached.length
      ? cached
      : await request<MatchGame[]>(`/matches/${match.id}/games`, {
          signal: task.controller.signal,
        });
    ensureCurrent(task);
    const game =
      gameNo === undefined
        ? (games.find((g) => g.position_count > 0) ?? games[0])
        : games.find((g) => g.game_no === gameNo);
    if (!game) throw new AppError('error.positionMissing');
    update({ games, gameNo: game.game_no });
    const scope = `match_id=${match.id}&game_no=${game.game_no}`;
    const positionsPromise = (async () => {
      const all: Position[] = [];
      let cursor: string | null = null;
      do {
        const page: Page<Position> = await request(
          `/positions?${scope}&limit=100${cursor ? `&after=${cursor}` : ''}`,
          { signal: task.controller.signal },
        );
        ensureCurrent(task);
        if (
          page.items.some(
            (p) => p.match_uuid !== match.id || p.game_no !== game.game_no,
          )
        )
          throw new AppError('error.invalidResponse');
        all.push(...page.items);
        cursor = page.next_cursor;
      } while (cursor);
      if (all.length !== game.position_count)
        throw new AppError('error.invalidResponse');
      return all;
    })();
    const [positions, stats] = await Promise.all([
      positionsPromise,
      request<DiceStats>(`/matches/${match.id}/dice?game_no=${game.game_no}`, {
        signal: task.controller.signal,
      }),
    ]);
    ensureCurrent(task);
    const index = targetId ? positions.findIndex((p) => p.id === targetId) : 0;
    if (targetId && index < 0) throw new AppError('error.positionMissing');
    update({ positions, index, stats, loading: false, targetId: undefined });
    return true;
  }
  async function selectMatch(match: Match) {
    const task = begin(match.id);
    try {
      return await load(match, undefined, undefined, task);
    } catch (error) {
      return fail(task, error);
    }
  }
  async function selectGame(gameNo: number) {
    const match = current.current.match;
    if (!match) return false;
    const task = begin(match.id);
    try {
      return await load(match, gameNo, undefined, task);
    } catch (error) {
      return fail(task, error);
    }
  }
  async function openPosition(position: Position | string) {
    const task = begin(
      typeof position === 'string' ? undefined : position.match_uuid,
    );
    try {
      const target =
        typeof position === 'string'
          ? await request<Position>(`/positions/${position}`, {
              signal: task.controller.signal,
            })
          : position;
      ensureCurrent(task);
      activeMatch.current = target.match_uuid;
      const previous = current.current;
      if (
        previous.match?.id === target.match_uuid &&
        previous.gameNo === target.game_no
      ) {
        const index = previous.positions.findIndex((p) => p.id === target.id);
        if (index >= 0) {
          update({ index, loading: false });
          return true;
        }
      }
      const match =
        previous.match?.id === target.match_uuid
          ? previous.match
          : await request<Match>(`/matches/${target.match_uuid}`, {
              signal: task.controller.signal,
            });
      ensureCurrent(task);
      return await load(match, target.game_no, target.id, task);
    } catch (error) {
      return fail(task, error);
    }
  }
  async function openMatch(id: string, gameNo?: number) {
    const task = begin(id);
    try {
      const match =
        current.current.match?.id === id
          ? current.current.match
          : await request<Match>(`/matches/${id}`, {
              signal: task.controller.signal,
            });
      ensureCurrent(task);
      return await load(match!, gameNo, undefined, task);
    } catch (error) {
      return fail(task, error);
    }
  }
  function cancel() {
    activeMatch.current = undefined;
    active.current?.abort();
    version.current++;
    update({ loading: false });
  }
  function selectIndex(index: number) {
    if (index < 0 || index >= current.current.positions.length) return;
    cancel();
    update({ index, error: null });
  }
  async function resume() {
    const previous = current.current;
    if (!previous.match || previous.loading) return;
    const game = previous.games.find((g) => g.game_no === previous.gameNo);
    if (!previous.error && game && previous.stats !== null) return;
    const task = begin(previous.match.id);
    try {
      await load(previous.match, previous.gameNo, previous.targetId, task);
    } catch (error) {
      fail(task, error);
    }
  }
  function invalidateMatchAnalysis(matchId: string) {
    if (current.current.match?.id !== matchId) return;
    if (current.current.loading && activeMatch.current !== matchId) {
      // Remove restricted cached data while allowing a newer, unrelated
      // selection (including a position whose source is still loading) to finish.
      update({
        match: null,
        games: [],
        gameNo: undefined,
        positions: [],
        index: 0,
        stats: null,
        error: null,
        targetId: undefined,
      });
    } else {
      cancel();
      update(initial());
    }
  }
  return {
    ...view,
    invalidateMatchAnalysis,
    game: view.games.find((g) => g.game_no === view.gameNo),
    position: view.positions[view.index],
    selectMatch,
    selectGame,
    openPosition,
    openMatch,
    selectIndex,
    cancel,
    resume,
    next: () => selectIndex(current.current.index + 1),
    previous: () => selectIndex(current.current.index - 1),
    intent: () => version.current,
    clearError: () => update({ error: null }),
    clear: () => {
      cancel();
      update(initial());
    },
  };
}
