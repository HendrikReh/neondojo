import { useEffect, useRef, useState } from 'react';
import type { Theme } from './themes';
import ActionButton from './ActionButton';
import LearningReviewDialog from './LearningReviewDialog';
import { useI18n } from './i18n';
import StudyReportTable from './StudyReportTable';
import { groupStudyReports } from './study-report-groups';
import type { PlaySession } from './play-api';
import {
  loadStudyReport,
  loadStudyReports,
  type StudyReportPage,
  type StudyReportSummary,
} from './study-api';
import { usePlayText } from './play-messages';
import { useStudyText } from './study-messages';
export default function StudyReports({
  visible,
  theme,
}: {
  visible: boolean;
  theme: Theme;
}) {
  const gameText = usePlayText();
  const t = useStudyText();
  const { locale, formatError } = useI18n();
  const reportTrigger = useRef<HTMLButtonElement | null>(null);
  const [page, setPage] = useState<StudyReportPage>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  const [moreLoading, setMoreLoading] = useState(false);
  const [moreError, setMoreError] = useState<unknown>();
  const listAbort = useRef<AbortController | null>(null),
    moreLocked = useRef(false);
  const [selected, setSelected] = useState<StudyReportSummary>();
  const [detail, setDetail] = useState<PlaySession>();
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<unknown>();
  const [detailRevision, setDetailRevision] = useState(0);
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
    if (!visible || !selected) return;
    const abort = new AbortController();
    setDetailLoading(true);
    setDetailError(undefined);
    setDetail(undefined);
    void loadStudyReport(selected.id, abort.signal)
      .then((result) => {
        if (!abort.signal.aborted) setDetail(result);
      })
      .catch((failure) => {
        if (!abort.signal.aborted) setDetailError(failure);
      })
      .finally(() => {
        if (!abort.signal.aborted) setDetailLoading(false);
      });
    return () => abort.abort();
  }, [visible, selected?.id, detailRevision]);
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
  return (
    <section className="study-reports" aria-label={t('archive')}>
      <div className="study-reports-heading">
        <div>
          <h1>{t('reports')}</h1>
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
                    setSelected(report);
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
                          : 'GNUbg'}{' '}
                        {report.session_scores.join(' : ')}{' '}
                        {report.human_player === 1
                          ? report.human_name
                          : 'GNUbg'}
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
      {visible && selected && (
        <LearningReviewDialog
          returnFocus={reportTrigger}
          theme={theme}
          context={`${selected.name}${selected.game_no != null ? ` · ${gameText('game')} ${selected.game_no}` : ''}`}
          session={
            (detail?.study_report_id ?? detail?.id) === selected.id
              ? detail
              : undefined
          }
          busy={detailLoading}
          error={detailError}
          onRetry={() => setDetailRevision((value) => value + 1)}
          onClose={() => setSelected(undefined)}
        />
      )}
    </section>
  );
}
