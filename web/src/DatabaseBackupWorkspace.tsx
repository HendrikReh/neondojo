import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import LibraryDialog from './LibraryDialog';
import { downloadResponse, requestResponse } from './api';
import { databaseBackupMessages } from './database-backup-messages';
import type { paths } from './generated-api';
import { useI18n } from './i18n';
import './database-backup.css';

const path = '/database-backup';
type Restore = paths['/api/database-backup']['post'];
const mediaType: keyof Restore['requestBody']['content'] =
  'application/vnd.bgtutor.backup';

type TransferState =
  { phase: 'idle' | 'pending' | 'done' } | { phase: 'error'; error: unknown };

export default function DatabaseBackupWorkspace() {
  const { locale, formatError } = useI18n();
  const t = databaseBackupMessages[locale];
  const [file, setFile] = useState<File>();
  const [exportState, setExportState] = useState<TransferState>({
    phase: 'idle',
  });
  const [restoreState, setRestoreState] = useState<TransferState>();
  const fileInput = useRef<HTMLInputElement>(null);
  const restoreTrigger = useRef<HTMLButtonElement>(null);
  const restoreError = useRef<HTMLParagraphElement>(null);
  const reloadTrigger = useRef<HTMLButtonElement>(null);
  const exporting = exportState.phase === 'pending';
  const restoring = restoreState?.phase === 'pending';
  const restored = restoreState?.phase === 'done';
  const busy = exporting || restoring;

  useEffect(() => {
    if (restoreState?.phase === 'error') restoreError.current?.focus();
    if (restoreState?.phase === 'done') reloadTrigger.current?.focus();
  }, [restoreState]);

  function closeConfirmation() {
    setRestoreState(undefined);
    requestAnimationFrame(() => restoreTrigger.current?.focus());
  }

  async function exportBackup() {
    setExportState({ phase: 'pending' });
    try {
      const response = await requestResponse(path);
      await downloadResponse(response, 'bgtutor.bgtutor-backup');
      setExportState({ phase: 'done' });
    } catch (error) {
      setExportState({ phase: 'error', error });
    }
  }

  async function restoreBackup() {
    if (!file) return;
    setRestoreState({ phase: 'pending' });
    const query: Restore['parameters']['query'] = { replace: true };
    try {
      await requestResponse(`${path}?replace=${query.replace}`, {
        method: 'POST',
        headers: { 'Content-Type': mediaType },
        body: file,
      });
      setRestoreState({ phase: 'done' });
    } catch (error) {
      setRestoreState({ phase: 'error', error });
    }
  }

  return (
    <section
      className="database-backup"
      aria-labelledby="database-backup-title"
    >
      <h1 id="database-backup-title">{t.title}</h1>
      <section
        className="database-backup-panel"
        aria-labelledby="database-backup-export"
      >
        <div className="database-backup-export">
          <div>
            <h2 id="database-backup-export">{t.exportTitle}</h2>
            <p className="database-backup-help">{t.exportHelp}</p>
          </div>
          <ActionButton
            type="button"
            icon="export"
            label={t.exportTitle}
            disabled={busy}
            onClick={() => void exportBackup()}
          />
        </div>
        <div className="database-backup-status" role="status">
          {exporting && t.exporting}
          {exportState.phase === 'done' && t.exported}
        </div>
        {exportState.phase === 'error' && (
          <p className="error" role="alert">
            {formatError(exportState.error)}
          </p>
        )}
      </section>
      <section
        className="database-backup-panel"
        aria-labelledby="database-backup-import"
      >
        <h2 id="database-backup-import">{t.importTitle}</h2>
        <p className="database-backup-warning" id="database-backup-warning">
          {t.warning}
        </p>
        <p className="database-backup-help" id="database-backup-format">
          {t.importHelp}
        </p>
        <div className="database-backup-actions">
          <input
            ref={fileInput}
            type="file"
            hidden
            accept={`.bgtutor-backup,${mediaType}`}
            disabled={busy}
            onChange={(event) => {
              const selected = event.currentTarget.files?.[0];
              event.currentTarget.value = '';
              if (selected) setFile(selected);
            }}
          />
          <ActionButton
            type="button"
            icon="open"
            label={t.chooseFile}
            aria-describedby="database-backup-warning database-backup-format"
            disabled={busy}
            onClick={() => fileInput.current?.click()}
          />
          <span
            id="database-backup-selected-file"
            className={
              file ? 'database-backup-filename' : 'database-backup-help'
            }
            aria-live="polite"
          >
            {file?.name ?? t.noFile}
          </span>
        </div>
        <div className="database-backup-review">
          <ActionButton
            type="button"
            icon="import"
            ref={restoreTrigger}
            label={t.review}
            aria-describedby="database-backup-selected-file"
            disabled={!file || busy}
            onClick={() => setRestoreState({ phase: 'idle' })}
          />
        </div>
      </section>
      {restoreState && (
        <LibraryDialog
          native
          initialFocus="dialog"
          busy={restoring || restored}
          title={t.importTitle}
          onClose={closeConfirmation}
        >
          <div className="database-backup-dialog">
            <div className="database-backup-status" role="status">
              {restoring && t.restoring}
              {restored && t.done}
            </div>
            {restored ? (
              <div className="library-dialog-actions database-backup-actions">
                <ActionButton
                  type="button"
                  icon="refresh"
                  ref={reloadTrigger}
                  label={t.reload}
                  onClick={() => window.location.reload()}
                />
              </div>
            ) : (
              <>
                <dl className="database-backup-source">
                  <dt>{t.file}</dt>
                  <dd className="database-backup-filename">{file?.name}</dd>
                </dl>
                <p
                  className="database-backup-warning"
                  id="database-backup-confirm-warning"
                >
                  {t.warning}
                </p>
                <p
                  className="database-backup-help"
                  id="database-backup-validation"
                >
                  {t.validation}
                </p>
                {restoreState.phase === 'error' && (
                  <p
                    className="error"
                    ref={restoreError}
                    role="alert"
                    tabIndex={-1}
                  >
                    {formatError(restoreState.error)}
                  </p>
                )}
                <div className="library-dialog-actions database-backup-actions">
                  <button
                    type="button"
                    disabled={restoring}
                    onClick={closeConfirmation}
                  >
                    {t.cancel}
                  </button>
                  <ActionButton
                    type="button"
                    className="danger"
                    icon="import"
                    label={t.replace}
                    aria-describedby="database-backup-confirm-warning database-backup-validation"
                    disabled={restoring}
                    onClick={() => void restoreBackup()}
                  />
                </div>
              </>
            )}
          </div>
        </LibraryDialog>
      )}
    </section>
  );
}
