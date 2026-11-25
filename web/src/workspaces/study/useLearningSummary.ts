import { useEffect, useState } from 'react';
import {
  loadLearningSummary,
  type LearningSummary,
} from './flashcards/learning-api';

type Summary =
  | { status: 'loading' }
  | { status: 'ready'; data: LearningSummary }
  | { status: 'error'; error: unknown };

export function useLearningSummary(visible: boolean, revision = 0) {
  const [summary, setSummary] = useState<Summary>({ status: 'loading' });
  const [cardRefresh, setCardRefresh] = useState(0);
  useEffect(() => {
    setSummary({ status: 'loading' });
    if (!visible) return;
    const abort = new AbortController();
    let timer: number | undefined;
    void loadLearningSummary(abort.signal)
      .then((data) => {
        if (abort.signal.aborted) return;
        setSummary({ status: 'ready', data });
        if (data.next_due_at) {
          const delay = Date.parse(data.next_due_at) - Date.now();
          timer = window.setTimeout(
            () => setCardRefresh((value) => value + 1),
            delay < 0 ? 15_000 : Math.min(delay + 100, 2_147_483_647),
          );
        }
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setSummary({ status: 'error', error });
      });
    return () => {
      abort.abort();
      window.clearTimeout(timer);
    };
  }, [visible, revision, cardRefresh]);

  return { summary, refresh: () => setCardRefresh((value) => value + 1) };
}
