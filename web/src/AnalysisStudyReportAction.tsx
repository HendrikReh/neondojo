import { useEffect, useRef, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import LearningReviewDialog from './LearningReviewDialog';
import { loadStudyReport } from './study-api';
import type { PlaySession } from './workspaces/play/play-api';
import type { Theme } from './shared/board/themes';
import { usePlayModeText } from './workspaces/play/play-mode-messages';
import { AppError } from './shared/i18n/i18n-core';

/** A header-scoped, read-only report. Parent keys it by match and report identity. */
export default function AnalysisStudyReportAction({
  matchId,
  reportId,
  theme,
}: {
  matchId: string;
  reportId: string;
  theme: Theme;
}) {
  const modeT = usePlayModeText();
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [session, setSession] = useState<PlaySession>();
  const [error, setError] = useState<unknown>();
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!open || session) return;
    const abort = new AbortController();
    setBusy(true);
    setError(undefined);
    void loadStudyReport(reportId, abort.signal)
      .then((report) => {
        if (abort.signal.aborted) return;
        if (
          (report.study_report_id ?? report.id) !== reportId ||
          report.library_match_id !== matchId ||
          report.phase !== 'match_over' ||
          !report.review
        )
          throw new AppError('error.invalidResponse');
        setSession(report);
        setBusy(false);
      })
      .catch((failure) => {
        if (!abort.signal.aborted) {
          setError(failure);
          setBusy(false);
        }
      });
    return () => abort.abort();
  }, [open, session, revision, reportId, matchId]);
  return (
    <>
      <ActionButton
        ref={trigger}
        icon="analysis"
        label={modeT('reviewTooltip')}
        className="analysis-match-report-action"
        onClick={() => {
          setError(undefined);
          setBusy(!session);
          setOpen(true);
        }}
      >
        {modeT('reviewTooltip')}
      </ActionButton>
      {open && (
        <LearningReviewDialog
          returnFocus={trigger}
          session={session}
          theme={theme}
          busy={busy}
          error={error}
          onRetry={() => setRevision((value) => value + 1)}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
