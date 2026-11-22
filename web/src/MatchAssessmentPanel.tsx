import { useEffect, useMemo, useRef, useState } from 'react';
import { engineDisplayName } from './engine-version';
import ActionButton from './ActionButton';
import LearningDecisionPreview from './LearningDecisionPreview';
import { humanReviewDecisions } from './learning-review-positions';
import { useLearningReviewPositions } from './useLearningReviewPositions';
import { studyAssessment, type AssessmentState } from './match-assessment-api';
import { useMatchAssessmentText } from './match-assessment-messages';
import { useI18n } from './i18n';
import type { PlaySession } from './play-api';
import { useStudyText } from './study-messages';
import type { Theme } from './themes';

function active(state?: AssessmentState) {
  return state?.job?.status === 'queued' || state?.job?.status === 'running';
}

export default function MatchAssessmentPanel({
  reportId,
  session,
  theme,
  start,
  visible,
}: {
  reportId: string;
  session: PlaySession;
  theme: Theme;
  start: boolean;
  visible: boolean;
}) {
  const t = useMatchAssessmentText();
  const study = useStudyText();
  const { locale, formatError, formatDecision, formatNumber } = useI18n();
  const [request, setRequest] = useState<{
    attempt: number;
    language?: typeof locale;
  }>({ attempt: 0 });
  const [state, setState] = useState<AssessmentState>();
  const [error, setError] = useState<unknown>();
  const [pending, setPending] = useState(true);
  const [selected, setSelected] = useState<string>();
  const trigger = useRef<HTMLButtonElement>(null);
  const consumedStart = useRef(0);
  const autoStart = useRef(start ? locale : undefined);
  const reportVersion = session.study_report_version ?? session.version;
  const entries = useMemo(
    () =>
      new Map(humanReviewDecisions(session).map((item) => [item.key, item])),
    [session],
  );
  const entry = selected ? entries.get(selected) : undefined;
  const history = useLearningReviewPositions(
    session,
    visible && Boolean(entry),
  );
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const explicit =
      request.language === locale && request.attempt > consumedStart.current;
    let attempt = explicit || autoStart.current === locale;
    setError(undefined);
    setState(undefined);
    setPending(true);
    async function poll() {
      try {
        let next = await studyAssessment(reportId, locale, abort.signal);
        if (abort.signal.aborted) return;
        const shouldStart =
          attempt &&
          next.availability === 'ready' &&
          (!next.job || (explicit && next.job.status === 'failed'));
        attempt = false;
        consumedStart.current = request.attempt;
        autoStart.current = undefined;
        if (shouldStart)
          next = await studyAssessment(reportId, locale, abort.signal, {
            report_version: reportVersion,
          });
        if (abort.signal.aborted) return;
        setState(next);
        if (active(next)) timer = setTimeout(() => void poll(), 1500);
      } catch (failure) {
        if (!abort.signal.aborted) setError(failure);
      } finally {
        if (!abort.signal.aborted) setPending(false);
      }
    }
    void poll();
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [reportId, locale, request, reportVersion, visible]);

  const startJob = () => {
    setRequest((current) => ({
      attempt: current.attempt + 1,
      language: locale,
    }));
  };
  const inspect = (reference: string, button: HTMLButtonElement) => {
    trigger.current = button;
    setSelected(reference);
  };
  const output = state?.job?.result;
  const bot = engineDisplayName(session.engine.id);
  const failed = state?.job?.status === 'failed' ? state.job.error : undefined;
  const errorKey =
    failed === 'invalid_report'
      ? 'invalid'
      : failed === 'interrupted'
        ? 'interrupted'
        : failed === 'source_unavailable'
          ? 'source'
          : failed === 'not_configured'
            ? 'configuration'
            : 'failed';
  const unavailable =
    state?.availability === 'unsupported'
      ? 'unsupported'
      : state?.availability === 'not_configured'
        ? 'configuration'
        : state?.availability === 'missing_analysis'
          ? 'missing'
          : undefined;
  return (
    <section className="match-assessment" aria-label={t('tab')}>
      {session.match_forfeited && (
        <p className="match-assessment-context">{t('forfeit')}</p>
      )}
      {pending && !output && <p role="status">{t('loading')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={study('retry')}
            onClick={() =>
              setRequest((current) => ({
                attempt: current.attempt + 1,
              }))
            }
          >
            {study('retry')}
          </ActionButton>
        </div>
      )}
      {!error && active(state) && (
        <p role="status">
          {t(state?.job?.status === 'queued' ? 'queued' : 'running')}
        </p>
      )}
      {!error && !pending && !active(state) && !output && (
        <>
          {failed && <p role="alert">{t(errorKey)}</p>}
          {unavailable ? (
            <p>{t(unavailable)}</p>
          ) : (
            <ActionButton
              icon="analysis"
              label={t('create')}
              onClick={startJob}
            >
              {t('create')}
            </ActionButton>
          )}
        </>
      )}
      {output && (
        <div
          className={`match-assessment-layout${entry ? ' match-assessment-layout--preview' : ''}`}
        >
          <div className="match-assessment-text">
            <p className="match-assessment-context">
              {t('basis')} · {state?.job?.model}
              {session.review &&
                ` · ${formatNumber(session.review.analysis_plies)}-ply`}
            </p>
            <p className="match-assessment-summary">{output.summary}</p>
            <section>
              <h2>{t('focus')}</h2>
              <p>{output.next_match_focus}</p>
            </section>
            <section>
              <h2>{t('themes')}</h2>
              <dl className="match-assessment-topics">
                {output.topics.map((topic) => (
                  <div key={topic.topic}>
                    <dt>{t(topic.topic)}</dt>
                    <dd>
                      {topic.observation}
                      {topic.evidence_refs
                        .filter((ref) => entries.has(ref))
                        .map((ref) => (
                          <button
                            type="button"
                            key={ref}
                            className="match-assessment-reference"
                            onClick={(event) =>
                              inspect(ref, event.currentTarget)
                            }
                            aria-label={`${t('preview')} ${ref.replace(':', '.')}`}
                          >
                            {ref.replace(':', '.')}
                          </button>
                        ))}
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
            {output.key_decisions.length > 0 && (
              <section>
                <h2>{t('examples')}</h2>
                {output.key_decisions.map((item) => {
                  const details = entries.get(item.evidence_ref);
                  if (!details) return null;
                  const decision = details.decision;
                  return (
                    <article key={item.evidence_ref}>
                      <h3>
                        {details.gameNo}.{decision.decision_no} ·{' '}
                        {t(decision.kind === 'cube' ? 'cube' : 'checker')}
                      </h3>
                      <p className="match-assessment-context">
                        {formatDecision(decision.played)} →{' '}
                        {formatDecision(decision.best)}
                        {' · Δ '}
                        {formatNumber(decision.equity_loss, {
                          maximumFractionDigits: 3,
                        })}{' '}
                        EMG
                      </p>
                      <p>{item.explanation}</p>
                      <p>{item.next_question}</p>
                      <ActionButton
                        icon="analysis"
                        label={t('preview')}
                        onClick={(event) =>
                          inspect(item.evidence_ref, event.currentTarget)
                        }
                      >
                        {t('preview')}
                      </ActionButton>
                    </article>
                  );
                })}
              </section>
            )}
            {output.training.length > 0 && (
              <section>
                <h2>{t('training')}</h2>
                {output.training.map((item, index) => (
                  <article key={index}>
                    <h3>{item.focus}</h3>
                    <p>{item.exercise}</p>
                    <p>{item.success_criterion}</p>
                    {item.evidence_refs
                      .filter((ref) => entries.has(ref))
                      .map((ref) => (
                        <button
                          type="button"
                          key={ref}
                          className="match-assessment-reference"
                          onClick={(event) => inspect(ref, event.currentTarget)}
                          aria-label={`${t('preview')} ${ref.replace(':', '.')}`}
                        >
                          {ref.replace(':', '.')}
                        </button>
                      ))}
                  </article>
                ))}
              </section>
            )}
            {output.limitations.length > 0 && (
              <section>
                <h2>{t('limitations')}</h2>
                <ul>
                  {output.limitations.map((item, index) => (
                    <li key={index}>{item}</li>
                  ))}
                </ul>
              </section>
            )}
          </div>
          {entry && (
            <div className="match-assessment-preview">
              <LearningDecisionPreview
                key={entry.key}
                entry={entry}
                position={history.positions.get(entry.key)?.position}
                theme={theme}
                white={session.human_player === 0 ? session.human_name : bot}
                black={session.human_player === 1 ? session.human_name : bot}
                loading={history.loading}
                error={history.error}
                onRetry={history.retry}
                onBack={() => {
                  setSelected(undefined);
                  trigger.current?.focus();
                }}
                scrollRequest={0}
              />
            </div>
          )}
        </div>
      )}
    </section>
  );
}
