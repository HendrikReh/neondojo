import { request } from './api';
import type { components } from './generated-api';

type Schema = components['schemas'];
export type PracticeSummaryInput = Schema['SaveSimilarPracticeSummary'];
export type PracticeHistoryPage = Schema['SimilarPracticeHistory'];

const path = '/study/similar-practice';
const pendingPrefix = 'bgtutor.similarPractice.pending.';
const activeSaves = new Map<
  string,
  Promise<Schema['SimilarPracticeSummary']>
>();

export function savePracticeSummary(summary: PracticeSummaryInput) {
  const active = activeSaves.get(summary.id);
  if (active) return active;
  const save = submitPracticeSummary(summary).finally(() =>
    activeSaves.delete(summary.id),
  );
  activeSaves.set(summary.id, save);
  return save;
}

async function submitPracticeSummary(summary: PracticeSummaryInput) {
  const key = pendingPrefix + summary.id;
  const body = JSON.stringify(summary);
  try {
    localStorage.setItem(key, body);
  } catch {
    // The request and in-dialog retry still work when storage is unavailable.
  }
  const saved = await request<Schema['SimilarPracticeSummary']>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
    keepalive: true,
  });
  try {
    localStorage.removeItem(key);
  } catch {
    // The stable round ID makes a later resend safe.
  }
  return saved;
}

export async function retryPendingPracticeSummaries() {
  let keys: string[];
  try {
    keys = Object.keys(localStorage).filter((key) =>
      key.startsWith(pendingPrefix),
    );
  } catch {
    return false;
  }
  let failed = false;
  for (const key of keys) {
    try {
      const summary = JSON.parse(
        localStorage.getItem(key) ?? 'null',
      ) as PracticeSummaryInput | null;
      if (summary?.id === key.slice(pendingPrefix.length))
        await savePracticeSummary(summary);
    } catch {
      failed = true;
    }
  }
  return failed;
}

export function loadPracticeHistory(
  before: string | undefined,
  signal: AbortSignal,
) {
  const query = before ? `?${new URLSearchParams({ before })}` : '';
  return request<PracticeHistoryPage>(path + query, { signal });
}
