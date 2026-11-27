import { useEffect, useRef, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { TabList } from '../../../shared/ui/TabList';
import MatchAssessmentPanel from './MatchAssessmentPanel';
import PlayLearningReview from './PlayLearningReview';
import { loadStudyReport } from '../study-api';
import type { PlaySession } from '../../play/play-api';
import { useMatchAssessmentText } from './match-assessment-messages';
import { useStudyText } from '../study-messages';
import { useI18n } from '../../../shared/i18n/i18n';
import type { Theme } from '../../../shared/board/themes';
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
            <TabList
              as="nav"
              className="text-navigation study-match-report-tabs"
              ariaLabel={study('decisions')}
              active={tab}
              onSelect={setTab}
              tabs={(['assessment', 'decisions'] as const).map((key) => ({
                value: key,
                id: `match-report-tab-${key}`,
                controls: `match-report-panel-${key}`,
                label: key === 'assessment' ? t('tab') : study('decisions'),
              }))}
            />
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
