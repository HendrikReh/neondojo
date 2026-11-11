import { useEffect, useMemo, useState } from 'react';
import {
  loadLearningReviewHistory,
  type PlaySession,
  type PlayReviewHistory,
} from './play-api';
import { AppError } from './i18n-core';
import { buildLearningReviewPositions } from './learning-review-positions';
export function useLearningReviewPositions(
  session: PlaySession,
  enabled: boolean,
) {
  const version = session.study_report_version ?? session.version;
  const key = `${session.study_report_id ?? session.id}:${version}`;
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<{
    key: string;
    loading: boolean;
    data?: PlayReviewHistory;
    error?: unknown;
  }>({ key: '', loading: false });
  useEffect(() => {
    if (!enabled) return;
    const abort = new AbortController();
    setResult({ key, loading: true });
    void loadLearningReviewHistory(session, abort.signal)
      .then((data) => {
        if (abort.signal.aborted) return;
        if (
          data.session_id !== session.id ||
          data.version !== version ||
          !Array.isArray(data.items) ||
          data.items.length !== session.history?.length ||
          data.items.some(
            (entry, index) =>
              entry.index !== index ||
              entry.before.board.length !== 26 ||
              ![0, 1].includes(entry.before.player),
          )
        )
          throw new AppError('error.invalidResponse');
        setResult({ key, loading: false, data });
      })
      .catch((error) => {
        if (!abort.signal.aborted) setResult({ key, loading: false, error });
      });
    return () => abort.abort();
  }, [key, enabled, revision]);
  const positions = useMemo(
    () =>
      result.key === key && result.data && session.review
        ? buildLearningReviewPositions(session.review, result.data)
        : new Map(),
    [result, key, session.review],
  );
  return {
    positions,
    loading: enabled && (result.key !== key || result.loading),
    error: result.key === key ? result.error : undefined,
    retry: () => setRevision((value) => value + 1),
  };
}
