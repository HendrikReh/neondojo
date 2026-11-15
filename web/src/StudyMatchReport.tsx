import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import ActionButton from './ActionButton';
import MatchAssessmentPanel from './MatchAssessmentPanel';
import PlayLearningReview from './PlayLearningReview';
import { loadStudyReport } from './study-api';
import type { PlaySession } from './play-api';
import { useMatchAssessmentText } from './match-assessment-messages';
import { useStudyText } from './study-messages';
import { useI18n } from './i18n';
import type { Theme } from './themes';
import './learning-review-dialog.css';
import './match-assessment.css';

export default function StudyMatchReport({
  reportId,
  title,
  start,
  theme,
  onClose,
}: {
  reportId: string;
  title?: string;
  start: boolean;
  theme: Theme;
  onClose: () => void;
}) {
  const t = useMatchAssessmentText();
  const study = useStudyText();
  const { formatError } = useI18n();
  const heading = useRef<HTMLHeadingElement>(null);
  const tabs = useRef<
    Record<'assessment' | 'decisions', HTMLButtonElement | null>
  >({
    assessment: null,
    decisions: null,
  });
  const [tab, setTab] = useState<'assessment' | 'decisions'>(
    start ? 'assessment' : 'decisions',
  );
  const [session, setSession] = useState<PlaySession>();
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    heading.current?.focus();
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    setError(undefined);
    void loadStudyReport(reportId, abort.signal)
      .then((report) => {
        if (abort.signal.aborted) return;
        // Legacy archives use the session ID, but their board still belongs
        // to the archived, versioned history route.
        setSession({ ...report, study_report_id: reportId });
      })
      .catch((failure: unknown) => {
        if (!abort.signal.aborted) setError(failure);
      });
    return () => abort.abort();
  }, [reportId, revision]);

  function navigate(event: KeyboardEvent<HTMLButtonElement>) {
    let next: typeof tab;
    if (event.key === 'Home') next = 'assessment';
    else if (event.key === 'End') next = 'decisions';
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight')
      next = tab === 'assessment' ? 'decisions' : 'assessment';
    else return;
    event.preventDefault();
    setTab(next);
    tabs.current[next]?.focus();
  }

  return (
    <section
      className="study-match-report"
      aria-labelledby="study-match-report-heading"
      onKeyDown={(event) => {
        if (event.key === 'Escape' && !event.defaultPrevented) {
          event.preventDefault();
          onClose();
        }
      }}
    >
      <header className="study-match-report-heading">
        <div>
          <h1 ref={heading} id="study-match-report-heading" tabIndex={-1}>
            {title ?? session?.human_name ?? study('decisions')}
          </h1>
          {session && <p>{session.scores.join(' : ')}</p>}
        </div>
        <ActionButton icon="back" label={t('back')} onClick={onClose}>
          {t('back')}
        </ActionButton>
      </header>
      {!session && !error && <p role="status">{study('loadingReport')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={study('retry')}
            onClick={() => setRevision((value) => value + 1)}
          >
            {study('retry')}
          </ActionButton>
        </div>
      )}
      {session && (
        <>
          {session.match_length > 0 && (
            <nav
              className="text-navigation study-match-report-tabs"
              role="tablist"
              aria-label={study('decisions')}
            >
              {(['assessment', 'decisions'] as const).map((key) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  id={`match-report-tab-${key}`}
                  ref={(node) => {
                    tabs.current[key] = node;
                  }}
                  aria-controls={`match-report-panel-${key}`}
                  aria-selected={tab === key}
                  tabIndex={tab === key ? 0 : -1}
                  onKeyDown={navigate}
                  onClick={() => setTab(key)}
                >
                  {key === 'assessment' ? t('tab') : study('decisions')}
                </button>
              ))}
            </nav>
          )}
          {session.match_length > 0 && (
            <div
              id="match-report-panel-assessment"
              role="tabpanel"
              aria-labelledby="match-report-tab-assessment"
              hidden={tab !== 'assessment'}
            >
              <MatchAssessmentPanel
                reportId={reportId}
                session={session}
                theme={theme}
                start={start}
                visible={tab === 'assessment'}
              />
            </div>
          )}
          {session.match_length > 0 ? (
            <div
              id="match-report-panel-decisions"
              role="tabpanel"
              aria-labelledby="match-report-tab-decisions"
              hidden={tab !== 'decisions'}
            >
              <PlayLearningReview session={session} theme={theme} />
            </div>
          ) : (
            <PlayLearningReview session={session} theme={theme} />
          )}
        </>
      )}
    </section>
  );
}
