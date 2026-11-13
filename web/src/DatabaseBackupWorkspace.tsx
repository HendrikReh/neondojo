import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import LibraryDialog from './LibraryDialog';
import { downloadResponse, request, requestResponse } from './api';
import { databaseBackupMessages } from './database-backup-messages';
import type { paths } from './generated-api';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import './database-backup.css';

const path = '/database-backup';
const checkPath = '/database-backup/check';
type CheckResponse =
  paths['/api/database-backup/check']['post']['responses'][200]['content']['application/json'];
type Restore = paths['/api/database-backup']['post'];
const mediaType: keyof Restore['requestBody']['content'] =
  'application/vnd.bgtutor.backup';

type TransferState =
  { phase: 'idle' | 'pending' | 'done' } | { phase: 'error'; error: unknown };

type BackupContents = NonNullable<CheckResponse['contents']>;

type BackupCheck = { file: File } & (
  | { phase: 'pending' }
  | { phase: CheckResponse['status']; contents?: BackupContents }
  | { phase: 'error'; error: unknown }
);

export default function DatabaseBackupWorkspace() {
  const { locale, formatError } = useI18n();
  const t = databaseBackupMessages[locale];
  const [check, setCheck] = useState<BackupCheck>();
  const file = check?.file;
  const contents = check && 'contents' in check ? check.contents : undefined;
  const compatible = check?.phase === 'compatible';
  const checking = check?.phase === 'pending';
  const activeCheck = useRef<AbortController>(null);
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

  useEffect(() => () => activeCheck.current?.abort(), []);

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

  async function checkBackup(file: File) {
    activeCheck.current?.abort();
    const controller = new AbortController();
    activeCheck.current = controller;
    setCheck({ file, phase: 'pending' });
    try {
      const result = await request<CheckResponse | null>(checkPath, {
        method: 'POST',
        headers: { 'Content-Type': mediaType },
        body: file,
        signal: controller.signal,
      });
      const status = result?.status;
      if (status !== 'compatible' && status !== 'incompatible_schema') {
        throw new AppError('error.invalidResponse');
      }
      const contents = result?.contents ?? undefined;
      if (
        (status === 'compatible' && !contents) ||
        (contents &&
          (![
            contents.matches,
            contents.saved_positions,
            contents.learning_cards,
          ].every((value) => Number.isSafeInteger(value) && value >= 0) ||
            (contents.created_at != null &&
              (typeof contents.created_at !== 'string' ||
                !Number.isFinite(Date.parse(contents.created_at))))))
      ) {
        throw new AppError('error.invalidResponse');
      }
      if (!controller.signal.aborted)
        setCheck({ file, phase: status, contents });
    } catch (error) {
      if (!controller.signal.aborted) setCheck({ file, phase: 'error', error });
    } finally {
      if (activeCheck.current === controller) activeCheck.current = null;
    }
  }

  async function restoreBackup() {
    if (!file || !compatible) return;
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
            disabled={busy || checking}
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
              if (selected) void checkBackup(selected);
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
        <div
          id="database-backup-check-status"
          className="database-backup-status"
          role="status"
        >
          {checking && t.checking}
          {compatible && (
            <span className="database-backup-compatible">{t.compatible}</span>
          )}
        </div>
        {check?.phase === 'incompatible_schema' && (
          <p className="error" role="alert">
            {t.incompatible}
          </p>
        )}
        {contents && <BackupContentsSummary contents={contents} />}
        {check?.phase === 'error' && (
          <>
            <p className="error" role="alert">
              {t.checkFailure} {formatError(check.error)}
            </p>
            <div className="database-backup-review">
              <button
                type="button"
                disabled={busy}
                onClick={() => void checkBackup(check.file)}
              >
                {t.checkRetry}
              </button>
            </div>
          </>
        )}
        <div className="database-backup-review">
          <ActionButton
            type="button"
            icon="import"
            ref={restoreTrigger}
            label={t.review}
            aria-describedby="database-backup-selected-file database-backup-check-status"
            disabled={!compatible || busy}
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
                  {t.compatible} {t.validation}
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
                    disabled={!compatible || restoring}
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

function BackupContentsSummary({ contents }: { contents: BackupContents }) {
  const { locale, formatNumber } = useI18n();
  const t = databaseBackupMessages[locale];
  return (
    <section className="database-backup-contents" aria-label={t.contents}>
      <dl>
        <div>
          <dt>{t.createdAt}</dt>
          <dd>
            {contents.created_at ? (
              <time dateTime={contents.created_at}>
                {new Intl.DateTimeFormat(locale, {
                  dateStyle: 'medium',
                  timeStyle: 'short',
                }).format(new Date(contents.created_at))}
              </time>
            ) : (
              t.missingDate
            )}
          </dd>
        </div>
        <div>
          <dt>{t.matches}</dt>
          <dd>{formatNumber(contents.matches)}</dd>
        </div>
        <div>
          <dt>{t.savedPositions}</dt>
          <dd>{formatNumber(contents.saved_positions)}</dd>
        </div>
        <div>
          <dt>{t.learningCards}</dt>
          <dd>{formatNumber(contents.learning_cards)}</dd>
        </div>
      </dl>
    </section>
  );
}
