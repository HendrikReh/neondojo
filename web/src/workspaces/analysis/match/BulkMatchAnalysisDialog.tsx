import { useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import LibraryDialog from '../../../shared/ui/LibraryDialog';
import {
  MatchAnalysisFields,
  useMatchAnalysisOptions,
} from './MatchAnalysisFields';
import type { Job } from '../../../shared/api/api';
import { useI18n } from '../../../shared/i18n/i18n';
import { useLibraryText } from '../../library/library-messages';
import type { LibraryMatch } from '../../library/library-api';
import { submitMatchAnalysis } from './match-analysis-api';

type Failure = { match: LibraryMatch; error: unknown };

export default function BulkMatchAnalysisDialog({
  matches,
  unavailableCount,
  onAccepted,
  onClose,
  ...defaults
}: Parameters<typeof useMatchAnalysisOptions>[0] & {
  matches: readonly LibraryMatch[];
  unavailableCount: number;
  onAccepted: (jobs: Job[], continuesBatch: boolean) => void;
  onClose: () => void;
}) {
  const l = useLibraryText();
  const { t, formatError, formatNumber } = useI18n();
  const options = useMatchAnalysisOptions(defaults);
  const { engineId, plies, analysisConfigured, catalog } = options;
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<{
    accepted: number;
    failures: Failure[];
  }>();
  const pending = result ? result.failures.map(({ match }) => match) : matches;

  async function submit() {
    setSubmitting(true);
    const accepted: Job[] = [];
    const failures: Failure[] = [];
    for (const match of pending) {
      try {
        accepted.push(await submitMatchAnalysis(match.id, engineId, plies));
      } catch (error) {
        failures.push({ match, error });
      }
    }
    onAccepted(accepted, !!result?.accepted);
    setResult({
      accepted: (result?.accepted ?? 0) + accepted.length,
      failures,
    });
    setSubmitting(false);
  }

  return (
    <LibraryDialog
      title={l('analyzeSelection')}
      native
      initialFocus="first-field"
      busy={submitting}
      onClose={onClose}
    >
      <div className="library-dialog-form match-analysis-dialog">
        <p>{l('matchesToAnalyze', { count: formatNumber(matches.length) })}</p>
        {unavailableCount > 0 && (
          <p>
            {l('analysisUnavailable', {
              count: formatNumber(unavailableCount),
            })}
          </p>
        )}
        <div className="match-analysis-options">
          <MatchAnalysisFields
            options={options}
            disabled={submitting || !defaults.configured || !pending.length}
          />
        </div>
        {!!catalog?.error && (
          <p className="error" role="alert">
            {formatError(catalog.error)}
          </p>
        )}
        {result && (
          <p role="status">
            {l('analysisAccepted', { count: formatNumber(result.accepted) })}
          </p>
        )}
        {!!result?.failures.length && (
          <div className="error" role="alert">
            <ul>
              {result.failures.map(({ match, error }) => (
                <li key={match.id}>
                  <strong>
                    {match.display_name.trim() ||
                      `${match.white} – ${match.black}`}
                  </strong>
                  {' · '}
                  {formatError(error)}
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
      <div className="library-dialog-actions">
        <ActionButton
          type="button"
          icon="analysis"
          className="primary"
          label={
            submitting
              ? t('matchAnalysis.submitting')
              : l(
                  result?.failures.length
                    ? 'retryAnalysis'
                    : 'analyzeSelection',
                )
          }
          disabled={submitting || !analysisConfigured || !pending.length}
          onClick={() => void submit()}
        />
        <button type="button" disabled={submitting} onClick={onClose}>
          {l('close')}
        </button>
      </div>
    </LibraryDialog>
  );
}
