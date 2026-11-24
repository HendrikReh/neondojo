import { useEffect, useState } from 'react';
import { useI18n } from '../../shared/i18n/i18n';
import {
  matchAssessmentReportId,
  studyAssessment,
} from '../study/reports/match-assessment-api';
import type { PlaySession } from './play-api';

/** Whether the reviewed match already has an assessment in the app language. */
export function useSavedAssessment(
  session: PlaySession | null,
  visible: boolean,
) {
  const { locale } = useI18n();
  const reportId =
    visible &&
    session?.phase === 'match_over' &&
    session.match_length > 0 &&
    session.review
      ? matchAssessmentReportId(session)
      : undefined;
  const [saved, setSaved] = useState<string>();
  const key = reportId ? `${reportId}:${locale}` : undefined;
  useEffect(() => {
    if (!reportId || !key) return;
    const abort = new AbortController();
    void studyAssessment(reportId, locale, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted)
          setSaved(
            result.job && result.job.status !== 'failed' ? key : undefined,
          );
      })
      .catch(() => {
        if (!abort.signal.aborted) setSaved(undefined);
      });
    return () => abort.abort();
  }, [reportId, key, locale]);
  return !!key && saved === key;
}
