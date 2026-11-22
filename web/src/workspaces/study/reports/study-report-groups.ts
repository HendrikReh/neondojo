import type { StudyReportSummary } from '../study-api';
export type StudyReportGroup = {
  kind: 'money' | 'matches';
  id: string;
  reports: StudyReportSummary[];
};
/** Sessions use their durable UUID even when players match or a library entry is deleted. */
export function groupStudyReports(
  reports: StudyReportSummary[],
): StudyReportGroup[] {
  const groups: StudyReportGroup[] = [];
  const money = new Map<string, StudyReportGroup>();
  for (const report of reports) {
    if (report.match_length === 0) {
      let group = money.get(report.session_id);
      if (!group) {
        group = { kind: 'money', id: report.session_id, reports: [] };
        money.set(report.session_id, group);
        groups.push(group);
      }
      group.reports.push(report);
    } else {
      const last = groups.at(-1);
      if (last?.kind === 'matches') last.reports.push(report);
      else groups.push({ kind: 'matches', id: report.id, reports: [report] });
    }
  }
  for (const group of money.values())
    group.reports.sort((a, b) => (a.game_no ?? 0) - (b.game_no ?? 0));
  return groups;
}
