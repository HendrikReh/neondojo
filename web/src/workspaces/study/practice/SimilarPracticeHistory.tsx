import { useEffect, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import SimilarPracticeChart from './SimilarPracticeChart';
import { useI18n } from '../../../shared/i18n/i18n';
import { usePositionText } from '../../../position-browser-messages';
import { useStudyText } from '../study-messages';
import {
  loadPracticeHistory,
  retryPendingPracticeSummaries,
  type PracticeHistoryPage,
} from './similar-practice-history-api';

export default function SimilarPracticeHistory() {
  const p = usePositionText();
  const t = useStudyText();
  const { locale, formatError, formatNumber } = useI18n();
  const [page, setPage] = useState<PracticeHistoryPage>();
  const [requested, setRequested] = useState<{ before?: string }>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>();
  const [saveFailed, setSaveFailed] = useState(false);
  const dateFormat = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });

  useEffect(() => {
    const abort = new AbortController();
    setLoading(true);
    setError(undefined);
    async function load() {
      if (!requested.before) {
        const failed = await retryPendingPracticeSummaries();
        if (!abort.signal.aborted) setSaveFailed(failed);
      }
      return loadPracticeHistory(requested.before, abort.signal);
    }
    void load().then(
      (result) => {
        if (abort.signal.aborted) return;
        setPage((previous) => ({
          items: requested.before
            ? [...(previous?.items ?? []), ...result.items]
            : result.items,
          next_cursor: result.next_cursor,
        }));
        setLoading(false);
      },
      (failure) => {
        if (abort.signal.aborted) return;
        setError(failure);
        setLoading(false);
      },
    );
    return () => abort.abort();
  }, [requested]);

  return (
    <section aria-label={p('practiceHistory')}>
      <h2 className="study-practice-subheading">{p('similar')}</h2>
      {saveFailed && (
        <div role="alert">
          <p>{p('practiceSaveFailed')}</p>
          <ActionButton
            type="button"
            icon="refresh"
            label={p('practiceRetrySave')}
            disabled={loading}
            onClick={() => setRequested({})}
          />
        </div>
      )}
      {page?.items.length ? (
        <>
          <SimilarPracticeChart items={page.items} />
          <div className="study-report-table-scroll">
            <table
              className="study-report-table"
              aria-label={p('practiceHistory')}
            >
              <thead>
                <tr>
                  <th scope="col">{p('practiceCompletedAt')}</th>
                  <th scope="col">{p('round')}</th>
                  <th scope="col">{p('practiceScoredMoves')}</th>
                  <th scope="col">{p('practiceAverageLoss')}</th>
                </tr>
              </thead>
              <tbody>
                {page.items.map((round) => (
                  <tr key={round.id}>
                    <td>
                      <time dateTime={round.completed_at}>
                        {dateFormat.format(new Date(round.completed_at))}
                      </time>
                    </td>
                    <td>
                      {p(
                        round.is_retry
                          ? 'practiceRoundRetry'
                          : 'practiceRoundInitial',
                      )}
                    </td>
                    <td>
                      {formatNumber(round.scored_moves)} /{' '}
                      {formatNumber(round.total_positions)}
                    </td>
                    <td>
                      {formatNumber(round.average_equity_loss, {
                        minimumFractionDigits: 3,
                        maximumFractionDigits: 6,
                      })}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      ) : (
        page && (
          <p className="study-report-empty">{p('practiceHistoryEmpty')}</p>
        )
      )}
      {loading && <p role="status">{p('loading')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            type="button"
            icon="refresh"
            label={t('retry')}
            onClick={() => setRequested({ ...requested })}
          />
        </div>
      )}
      {!error && page?.next_cursor && (
        <ActionButton
          type="button"
          icon="next"
          label={p('practiceLoadOlder')}
          disabled={loading}
          onClick={() =>
            setRequested({ before: page.next_cursor ?? undefined })
          }
        />
      )}
    </section>
  );
}
