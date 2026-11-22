import { request } from '../../shared/http/api';
import type { PlaySession } from '../play/play-api';
type Schema = import('../../generated-api').components['schemas'];
export type StudyReportSummary = Schema['StudyReportSummary'];
export type StudyReportPage = Schema['StudyReportPage'];
export const loadStudyReports = (signal: AbortSignal, after?: string) =>
  request<StudyReportPage>(
    `/study/reports?group_by=session${after ? `&after=${encodeURIComponent(after)}` : ''}`,
    { signal },
  );
export const loadStudyReport = (id: string, signal: AbortSignal) =>
  request<PlaySession>(`/study/reports/${encodeURIComponent(id)}`, { signal });
