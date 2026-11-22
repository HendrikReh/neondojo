import type { StudyReportSummary } from './study-api';
import ActionButton from './shared/ui/ActionButton';
import { useI18n } from './shared/i18n/i18n';
import { usePlayText } from './play-messages';
import { useStudyText } from './study-messages';
import { isPlayMode, usePlayModeText } from './play-mode-messages';
import { prDigits } from './shared/format/pr-format';

export default function StudyReportTable({
  reports,
  selectedId,
  moneyGames = false,
  onOpen,
}: {
  reports: StudyReportSummary[];
  selectedId?: string;
  moneyGames?: boolean;
  onOpen: (report: StudyReportSummary, trigger: HTMLButtonElement) => void;
}) {
  const t = useStudyText(),
    gameText = usePlayText(),
    modeT = usePlayModeText();
  const { locale, formatNumber, t: appT } = useI18n();
  return (
    <div className="study-report-table-scroll">
      <table className="study-report-table">
        <thead>
          <tr>
            {(
              [
                'date',
                'match',
                'score',
                'mode',
                'assistance',
                'depth',
                'pr',
              ] as const
            ).map((key) => (
              <th key={key} scope="col">
                {moneyGames && key === 'match' ? gameText('game') : t(key)}
              </th>
            ))}
            <th scope="col">
              <span className="sr-only">{t('open')}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {reports.map((report) => (
            <tr
              key={report.id}
              className={
                selectedId === report.id ? 'study-report-selected' : undefined
              }
            >
              <td>
                <time dateTime={report.updated_at}>
                  {new Date(report.updated_at).toLocaleString(locale, {
                    dateStyle: 'medium',
                    timeStyle: 'short',
                  })}
                </time>
              </td>
              <td>
                {!moneyGames && report.name}
                {report.game_no != null && (
                  <>
                    {' '}
                    {!moneyGames && '· '} {gameText('game')} {report.game_no}
                  </>
                )}
              </td>
              <td>
                {report.scores.join(':')} /{' '}
                {report.match_length || appT('common.moneyGame')}
              </td>
              <td>
                {isPlayMode(report.play_mode)
                  ? modeT(report.play_mode)
                  : report.play_mode}
              </td>
              <td>{modeT(report.assisted ? 'assisted' : 'independent')}</td>
              <td>{report.analysis_plies}-ply</td>
              <td>
                {report.first_attempt_rating == null
                  ? '—'
                  : formatNumber(report.first_attempt_rating, prDigits)}
              </td>
              <td>
                <ActionButton
                  icon="open"
                  id={`open-study-report-${report.id}`}
                  label={t('open')}
                  aria-pressed={selectedId === report.id}
                  onClick={(event) => {
                    onOpen(report, event.currentTarget);
                  }}
                >
                  {t('open')}
                </ActionButton>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
