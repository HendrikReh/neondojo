import { request } from './api';
import type { Locale } from './i18n';
import type { PlaySession } from './play-api';
type Schema = import('./generated-api').components['schemas'];
export type AssessmentState = Schema['AssessmentState'];
export type AssessmentEntry = { id: string; token: number };

export function matchAssessmentReportId(session: PlaySession): string {
  return session.study_report_id ?? session.id;
}

export function studyAssessment(
  id: string,
  language: Locale,
  signal: AbortSignal,
  start?: Schema['StartAssessment'],
) {
  return request<AssessmentState>(
    `/study/reports/${encodeURIComponent(id)}/assessment?language=${encodeURIComponent(language)}`,
    start
      ? {
          signal,
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(start),
        }
      : { signal },
  );
}
