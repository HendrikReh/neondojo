import { useEffect, useState } from 'react';
import { request, type Position } from './shared/api/api';
import type { CheckerTurn } from './shared/board/play-state';

type CheckerContext = Pick<Position, 'board' | 'player' | 'dice'>;

export function useCheckerTurns(position?: CheckerContext) {
  const body =
    position &&
    JSON.stringify({
      board: position.board,
      player: position.player,
      dice: position.dice,
    });
  const [attempt, setAttempt] = useState(0);
  const [result, setResult] = useState<{
    body: string;
    attempt: number;
    turns?: CheckerTurn[];
    error?: unknown;
  }>();
  const current =
    body === result?.body && attempt === result?.attempt ? result : undefined;

  useEffect(() => {
    if (!body || current) return;
    const abort = new AbortController();
    void request<CheckerTurn[]>('/play/checker-turns', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
      signal: abort.signal,
    }).then(
      (turns) => {
        if (!abort.signal.aborted) setResult({ body, attempt, turns });
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setResult({ body, attempt, error });
      },
    );
    return () => abort.abort();
  }, [body, attempt, current]);

  return {
    turns: current?.turns,
    error: current?.error,
    loading: Boolean(body) && !current,
    retry: () => setAttempt((value) => value + 1),
  };
}
