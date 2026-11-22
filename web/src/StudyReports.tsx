import { useEffect, useRef, useState } from 'react';
import type { Theme } from './shared/board/themes';
import ActionButton from './shared/ui/ActionButton';
import StudyMatchReport from './StudyMatchReport';
import LearningReviewDialog from './LearningReviewDialog';
import type { AssessmentEntry } from './match-assessment-api';
import { useI18n } from './shared/i18n/i18n';
import StudyReportTable from './StudyReportTable';
import { groupStudyReports } from './study-report-groups';
import {
  loadStudyReports,
  loadStudyReport,
  type StudyReportPage,
} from './study-api';
import type { PlaySession } from './play-api';
import { usePlayText } from './play-messages';
import { useStudyText } from './study-messages';
export default function StudyReports({
  visible,
  theme,
  requestedAssessment,
}: {
  visible: boolean;
  theme: Theme;
  requestedAssessment?: AssessmentEntry;
}) {
  const gameText = usePlayText();
  const t = useStudyText();
  const { locale, formatError } = useI18n();
  const reportTrigger = useRef<HTMLButtonElement | null>(null);
  const returnReport = useRef<string | undefined>(undefined);
  const [page, setPage] = useState<StudyReportPage>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  const [moreLoading, setMoreLoading] = useState(false);
  const [moreError, setMoreError] = useState<unknown>();
  const listAbort = useRef<AbortController | null>(null),
    moreLocked = useRef(false);
  const [selected, setSelected] = useState<{
    id: string;
    title?: string;
    start: boolean;
    token?: number;
    money?: boolean;
  }>();
  const consumedRequest = useRef<number | undefined>(undefined);
  const restoreFocus = useRef(false);
  const archiveHeading = useRef<HTMLHeadingElement>(null);
  const [moneyReport, setMoneyReport] = useState<PlaySession>();
  const [moneyError, setMoneyError] = useState<unknown>();
  const [moneyRevision, setMoneyRevision] = useState(0);
  const moneyId = selected?.money ? selected.id : undefined;
  useEffect(() => {
    if (!visible || !moneyId) return;
    const abort = new AbortController();
    setMoneyError(undefined);
    setMoneyReport(undefined);
    void loadStudyReport(moneyId, abort.signal)
      .then((report) => {
        if (!abort.signal.aborted) setMoneyReport(report);
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted) setMoneyError(error);
      });
    return () => abort.abort();
  }, [visible, moneyId, moneyRevision]);
  useEffect(() => {
    if (
      !visible ||
      !requestedAssessment ||
      consumedRequest.current === requestedAssessment.token
    )
      return;
    consumedRequest.current = requestedAssessment.token;
    reportTrigger.current = null;
    returnReport.current = undefined;
    setSelected({
      id: requestedAssessment.id,
      start: true,
      token: requestedAssessment.token,
    });
  }, [visible, requestedAssessment]);
  useEffect(() => {
    if (!selected && restoreFocus.current) {
      const button = returnReport.current
        ? document.getElementById(`open-study-report-${returnReport.current}`)
        : undefined;
      if (button || !loading) {
        (button ?? archiveHeading.current)?.focus();
        restoreFocus.current = false;
      }
    }
  }, [selected, loading]);
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    listAbort.current = abort;
    setLoading(true);
    setError(undefined);
    setMoreError(undefined);
    setMoreLoading(false);
    moreLocked.current = false;
    void loadStudyReports(abort.signal)
      .then((result) => {
        if (!abort.signal.aborted) setPage(result);
      })
      .catch((failure) => {
        if (!abort.signal.aborted) setError(failure);
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => {
      abort.abort();
    };
  }, [visible, revision]);
  useEffect(() => {
    if (!visible) setSelected(undefined);
  }, [visible]);
  async function more() {
    const abort = listAbort.current,
      cursor = page?.next_cursor;
    if (!abort || abort.signal.aborted || !cursor || moreLocked.current) return;
    moreLocked.current = true;
    setMoreLoading(true);
    setMoreError(undefined);
    try {
      const result = await loadStudyReports(abort.signal, cursor);
      if (!abort.signal.aborted)
        setPage((previous) => ({
          items: [...(previous?.items ?? []), ...result.items],
          next_cursor: result.next_cursor,
        }));
    } catch (failure) {
      if (!abort.signal.aborted) setMoreError(failure);
    } finally {
      if (!abort.signal.aborted) {
        moreLocked.current = false;
        setMoreLoading(false);
      }
    }
  }
  if (selected && !selected.money && visible)
    return (
      <StudyMatchReport
        key={`${selected.id}:${selected.token ?? 'archive'}`}
        reportId={selected.id}
        title={selected.title}
        start={selected.start}
        theme={theme}
        onClose={() => {
          restoreFocus.current = true;
          setSelected(undefined);
        }}
      />
    );
  return (
    <section className="study-reports" aria-label={t('archive')}>
      <div className="study-reports-heading">
        <div>
          <h1 ref={archiveHeading} tabIndex={-1}>
            {t('reports')}
          </h1>
          <p>{t('description')}</p>
        </div>
        <ActionButton
          icon="refresh"
          label={t('refresh')}
          disabled={loading}
          onClick={() => setRevision((value) => value + 1)}
        >
          {t('refresh')}
        </ActionButton>
      </div>
      {loading && <p role="status">{t('loading')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={t('retry')}
            onClick={() => setRevision((value) => value + 1)}
          >
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {!loading && !error && page && (
        <>
          {!page.items.length ? (
            <div className="study-report-empty">
              <h2>{t('empty')}</h2>
              <p>{t('emptyHelp')}</p>
            </div>
          ) : (
            groupStudyReports(page.items).map((group) => {
              const report = group.reports[0];
              const table = (
                <StudyReportTable
                  key={group.id}
                  reports={group.reports}
                  moneyGames={group.kind === 'money'}
                  selectedId={selected?.id}
                  onOpen={(report, trigger) => {
                    reportTrigger.current = trigger;
                    returnReport.current = report.id;
                    setSelected({
                      id: report.id,
                      title:
                        report.game_no == null
                          ? report.name
                          : `${report.name} · ${gameText('game')} ${report.game_no}`,
                      start: false,
                      money: report.match_length === 0,
                    });
                  }}
                />
              );
              return group.kind === 'money' ? (
                <section
                  key={group.id}
                  className="study-money-session"
                  data-session-id={group.id}
                  aria-labelledby={`study-session-${group.id}`}
                >
                  <header className="study-money-session-header">
                    <div>
                      <h2 id={`study-session-${group.id}`}>
                        {t('moneySession')} · {report.name}
                      </h2>
                      <p>
                        <time dateTime={report.created_at}>
                          {new Date(report.created_at).toLocaleString(locale, {
                            dateStyle: 'medium',
                            timeStyle: 'short',
                          })}
                        </time>{' '}
                        · {t('gameReports')}: {group.reports.length}
                      </p>
                    </div>
                    <div className="study-money-session-total">
                      <span>{t('totalScore')}</span>
                      <strong>
                        {report.human_player === 0
                          ? report.human_name
                          : report.bot_name}{' '}
                        {report.session_scores.join(' : ')}{' '}
                        {report.human_player === 1
                          ? report.human_name
                          : report.bot_name}
                      </strong>
                      <span>
                        {t(report.money_session_ended ? 'ended' : 'ongoing')}
                      </span>
                    </div>
                  </header>
                  {table}
                </section>
              ) : (
                table
              );
            })
          )}
          {page.next_cursor && (
            <ActionButton
              icon="more"
              label={moreError ? t('retry') : t('more')}
              disabled={moreLoading}
              onClick={() => void more()}
            >
              {moreError ? t('retry') : t('more')}
            </ActionButton>
          )}
          {moreLoading && <p role="status">{t('loading')}</p>}
          {!!moreError && <p role="alert">{formatError(moreError)}</p>}
        </>
      )}
      {visible && selected?.money && (
        <LearningReviewDialog
          key={selected.id}
          returnFocus={reportTrigger}
          theme={theme}
          context={selected.title}
          session={
            (moneyReport?.study_report_id ?? moneyReport?.id) === selected.id
              ? moneyReport
              : undefined
          }
          busy={!moneyError && !moneyReport}
          error={moneyError}
          onRetry={() => setMoneyRevision((value) => value + 1)}
          onClose={() => setSelected(undefined)}
        />
      )}
    </section>
  );
}
