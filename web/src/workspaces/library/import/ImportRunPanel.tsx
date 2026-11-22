import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import { ImportWarningList } from './ImportWarningList';
import { useLibraryText, type ImportWarning } from '../library-messages';

export type ImportResult = {
  /** File name, or the path inside the chosen folder. */
  name: string;
  status: 'importedFile' | 'duplicate' | 'error';
  detail?: unknown;
  warnings?: ImportWarning[];
};

/** Progress and outcomes of a multi-file or folder import. */
export type ImportRun = {
  results: ImportResult[];
  /** Files submitted for import. */
  total: number;
  /** Folder files left out because their format is not supported. */
  skipped: number;
  folder: boolean;
  running: boolean;
  stopping: boolean;
  stopped: boolean;
};

export default function ImportRunPanel({
  run,
  onStop,
  onClose,
}: {
  run: ImportRun;
  onStop: () => void;
  onClose: () => void;
}) {
  const { formatError } = useI18n();
  const l = useLibraryText();
  const done = run.results.length;
  const count = (status: ImportResult['status']) =>
    run.results.filter((result) => result.status === status).length;
  return (
    <section
      className="import-results"
      aria-label={l('importResults')}
      aria-busy={run.running}
    >
      <div className="section-heading">
        <h2>{l('importResults')}</h2>
        {run.running ? (
          <ActionButton
            onClick={onStop}
            disabled={run.stopping}
            icon="pause"
            label={run.stopping ? l('importStopping') : l('importStop')}
          />
        ) : (
          <ActionButton
            onClick={onClose}
            aria-label={l('close')}
            icon="close"
            label={l('close')}
            compact
          ></ActionButton>
        )}
      </div>
      {run.total > 0 && (
        <>
          {run.running && (
            <progress
              className="import-progress"
              value={done}
              max={run.total}
              aria-label={l('importProgress', { done, total: run.total })}
            />
          )}
          <p className="import-summary" role="status">
            {run.running
              ? l('importProgress', { done, total: run.total })
              : run.stopped
                ? l('importStopped', { done, total: run.total })
                : l('importFinished', { total: run.total })}{' '}
            {l('importCounts', {
              imported: count('importedFile'),
              duplicates: count('duplicate'),
              failed: count('error'),
            })}
          </p>
        </>
      )}
      {run.folder && run.total === 0 && (
        <p className="import-summary" role="status">
          {l('importFolderEmpty')}
        </p>
      )}
      {run.folder && run.skipped > 0 && (
        <p className="import-summary">
          {l('importSkipped', { count: run.skipped })}
        </p>
      )}
      {done > 0 && (
        <ul>
          {run.results.map((result, i) => (
            <li key={i} data-status={result.status}>
              {result.name}: {l(result.status)}
              {result.detail ? ` · ${formatError(result.detail)}` : ''}
              <ImportWarningList warnings={result.warnings} />
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
