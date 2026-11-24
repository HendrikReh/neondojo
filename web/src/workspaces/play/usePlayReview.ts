import { useEffect, useRef, useState, useCallback } from 'react';
import { useLatest } from '../../shared/hooks/useLatest';
import {
  loadPlayHistory,
  type PlayReviewHistory,
  type PlaySession,
} from './play-api';
import { AppError } from '../../shared/i18n/i18n-core';
import type { BoardView } from '../../shared/board/move-preview';
import { useBoardReviewShortcuts } from '../../shared/review/useBoardReviewShortcuts';

/** History snapshots never replace the live checkpoint or unconfirmed steps. */
export function usePlayReview(session: PlaySession | null, enabled: boolean) {
  const key = session ? `${session.id}:${session.version}` : '';
  const [selection, setSelection] = useState<{
    key: string;
    index: number;
  } | null>(null);
  const [view, setView] = useState<BoardView>('arrows');
  const active = selection?.key === key && Boolean(session);
  const activeRef = useLatest(active);
  const liveKey = useLatest(key);
  const cache = useRef<{ key: string; data: PlayReviewHistory } | null>(null);
  const [result, setResult] = useState<{
    key: string;
    data?: PlayReviewHistory;
    error?: unknown;
    loading: boolean;
  }>({ key: '', loading: false });
  const select = useCallback(
    (index: number) => {
      if (!enabled || !session || index < 0 || index >= session.history.length)
        return;
      activeRef.current = true;
      setSelection({ key, index });
    },
    [enabled, key, session?.history.length],
  );
  const reset = useCallback(() => {
    activeRef.current = false;
    setSelection(null);
  }, []);
  useEffect(() => {
    if (!active || !enabled || !session) return;
    if (cache.current?.key === key) {
      setResult({ key, data: cache.current.data, loading: false });
      return;
    }
    const controller = new AbortController();
    setResult({ key, loading: true });
    void loadPlayHistory(session, controller.signal)
      .then((data) => {
        if (controller.signal.aborted || liveKey.current !== key) return;
        if (
          data.session_id !== session.id ||
          data.version !== session.version ||
          data.items.length !== session.history.length ||
          data.items.some((entry, index) => entry.index !== index)
        )
          throw new AppError('error.invalidResponse');
        cache.current = { key, data };
        setResult({ key, data, loading: false });
      })
      .catch((error) => {
        if (!controller.signal.aborted && liveKey.current === key)
          setResult({ key, error, loading: false });
      });
    return () => controller.abort();
  }, [active, enabled, key]);
  const index = active ? selection!.index : null;
  const entry =
    index !== null && result.key === key
      ? result.data?.items[index]
      : undefined;
  useBoardReviewShortcuts(
    active && enabled,
    '.play-history .move-history-row',
    setView,
  );
  return {
    active,
    index,
    entry,
    view,
    setView,
    select,
    reset,
    isActive: () => activeRef.current,
    loading: active && !entry && (result.key !== key || result.loading),
    error: active && result.key === key ? result.error : undefined,
  };
}
