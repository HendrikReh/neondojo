import { useEffect, useRef, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import {
  downloadResponse,
  request,
  requestResponse,
} from '../../shared/api/api';
import { databaseBackupMessages } from './database-backup-messages';
import type { paths } from '../../generated-api';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import './database-backup.css';

const path = '/database-backup';
const checkPath = '/database-backup/check';
const targetCountsPath = '/database-backup/target-counts';
type CheckResponse =
  paths['/api/database-backup/check']['post']['responses'][200]['content']['application/json'];
type Restore = paths['/api/database-backup']['post'];
const mediaType: keyof Restore['requestBody']['content'] =
  'application/vnd.neondojo.backup';

type TransferState =
  { phase: 'idle' | 'pending' | 'done' } | { phase: 'error'; error: unknown };

type BackupContents = NonNullable<CheckResponse['contents']>;
type TargetCounts =
  paths['/api/database-backup/target-counts']['get']['responses'][200]['content']['application/json'];

type TargetCountsRequest =
  | { status: 'loading' }
  | { status: 'ready'; counts: TargetCounts }
  | { status: 'error'; error: unknown };

const countLabels = {
  matches: 'matches',
  saved_positions: 'savedPositions',
  learning_cards: 'learningCards',
} as const;
const countKeys = ['matches', 'saved_positions', 'learning_cards'] as const;

function validCounts(counts: TargetCounts) {
  return countKeys.every(
    (key) => Number.isSafeInteger(counts[key]) && counts[key] >= 0,
  );
}

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
  const [targetCounts, setTargetCounts] = useState<TargetCountsRequest>();
  const activeTargetRequest = useRef<AbortController>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const restoreTrigger = useRef<HTMLButtonElement>(null);
  const restoreError = useRef<HTMLParagraphElement>(null);
  const reloadTrigger = useRef<HTMLButtonElement>(null);
  const exporting = exportState.phase === 'pending';
  const restoring = restoreState?.phase === 'pending';
  const restored = restoreState?.phase === 'done';
  const busy = exporting || restoring;

  useEffect(
    () => () => {
      activeCheck.current?.abort();
      activeTargetRequest.current?.abort();
    },
    [],
  );

  useEffect(() => {
    if (restoreState?.phase === 'error') restoreError.current?.focus();
    if (restoreState?.phase === 'done') reloadTrigger.current?.focus();
  }, [restoreState]);

  function closeConfirmation() {
    activeTargetRequest.current?.abort();
    setRestoreState(undefined);
    setTargetCounts(undefined);
    requestAnimationFrame(() => restoreTrigger.current?.focus());
  }

  async function loadTargetCounts() {
    activeTargetRequest.current?.abort();
    const controller = new AbortController();
    activeTargetRequest.current = controller;
    setTargetCounts({ status: 'loading' });
    try {
      const counts = await request<TargetCounts | null>(targetCountsPath, {
        signal: controller.signal,
        cache: 'no-store',
      });
      if (!counts || !validCounts(counts)) {
        throw new AppError('error.invalidResponse');
      }
      if (!controller.signal.aborted)
        setTargetCounts({ status: 'ready', counts });
    } catch (error) {
      if (!controller.signal.aborted)
        setTargetCounts({ status: 'error', error });
    } finally {
      if (activeTargetRequest.current === controller)
        activeTargetRequest.current = null;
    }
  }

  async function exportBackup() {
    setExportState({ phase: 'pending' });
    try {
      const response = await requestResponse(path);
      await downloadResponse(response, 'neondojo.neondojo-backup');
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
          (!validCounts(contents) ||
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
    if (!file || !compatible || targetCounts?.status !== 'ready') return;
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
      <h1 id="database-backup-title" className="sr-only">
        {t.title}
      </h1>
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
        <p className="database-backup-help" id="database-backup-format">
          {t.importHelp}
        </p>
        <p className="database-backup-warning" id="database-backup-warning">
          {t.warning}
        </p>
        <div className="database-backup-actions">
          <input
            ref={fileInput}
            type="file"
            hidden
            accept={`.neondojo-backup,${mediaType}`}
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
        {contents && (
          <BackupContentsSummary contents={contents} filename={file?.name} />
        )}
        {check?.phase === 'error' && (
          <>
            <p className="error" role="alert">
              {t.checkFailure} {formatError(check.error)}
            </p>
            <div className="database-backup-review">
              <ActionButton
                icon="refresh"
                type="button"
                label={t.checkRetry}
                disabled={busy}
                onClick={() => void checkBackup(check.file)}
              />
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
            onClick={() => {
              setRestoreState({ phase: 'idle' });
              void loadTargetCounts();
            }}
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
                {contents && (
                  <BackupContentsSummary
                    contents={contents}
                    filename={file?.name}
                    target={targetCounts}
                  />
                )}
                {targetCounts?.status === 'loading' && (
                  <p role="status">{t.targetLoading}</p>
                )}
                {targetCounts?.status === 'error' && (
                  <>
                    <p className="error" role="alert">
                      {t.targetError} {formatError(targetCounts.error)}
                    </p>
                    <ActionButton
                      icon="refresh"
                      type="button"
                      label={t.targetRetry}
                      onClick={() => void loadTargetCounts()}
                    />
                  </>
                )}
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
                  <ActionButton
                    icon="cancel"
                    type="button"
                    label={t.cancel}
                    disabled={restoring}
                    onClick={closeConfirmation}
                  />
                  <ActionButton
                    type="button"
                    className="danger"
                    icon="import"
                    label={t.replace}
                    aria-describedby="database-backup-confirm-warning database-backup-validation"
                    disabled={
                      !compatible ||
                      restoring ||
                      targetCounts?.status !== 'ready'
                    }
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

function backupFilenameTimestamp(filename: string): string | undefined {
  // The backend formats its export filenames in UTC.
  const match =
    /^neondojo-(\d{4}-\d{2}-\d{2})-(\d{2})(\d{2})(\d{2})\.neondojo-backup$/.exec(
      filename,
    );
  if (!match || match[0] !== filename) return undefined;
  const [, day, hours, minutes, seconds] = match;
  const time = `${day}T${hours}:${minutes}:${seconds}Z`;
  const date = new Date(time);
  // A round trip rejects normalized dates such as February 30 or 24:00.
  if (
    Number.isFinite(date.getTime()) &&
    date.toISOString() === time.replace('Z', '.000Z')
  ) {
    return time;
  }
}

function BackupContentsSummary({
  contents,
  filename,
  target,
}: {
  contents: BackupContents;
  filename?: string;
  target?: TargetCountsRequest;
}) {
  const { locale, formatNumber } = useI18n();
  const t = databaseBackupMessages[locale];
  const targetCounts = target?.status === 'ready' ? target.counts : undefined;
  const filenameTimestamp =
    contents.created_at == null && filename
      ? backupFilenameTimestamp(filename)
      : undefined;
  const createdAt = contents.created_at ?? filenameTimestamp;
  const creationTime = createdAt ? (
    <>
      <time dateTime={createdAt}>
        {new Intl.DateTimeFormat(locale, {
          dateStyle: 'medium',
          timeStyle: 'short',
        }).format(new Date(createdAt))}
      </time>
      {filenameTimestamp && ` (${t.dateFromFilename})`}
    </>
  ) : (
    t.missingDate
  );

  return (
    <section
      className="database-backup-contents"
      aria-label={target ? t.compareContents : t.contents}
    >
      <dl>
        <div>
          <dt>{t.createdAt}</dt>
          <dd>{creationTime}</dd>
        </div>
        {!target &&
          countKeys.map((key) => (
            <div key={key}>
              <dt>{t[countLabels[key]]}</dt>
              <dd>{formatNumber(contents[key])}</dd>
            </div>
          ))}
      </dl>
      {target && (
        <table
          aria-label={t.compareContents}
          className="database-backup-comparison"
        >
          <thead>
            <tr>
              <td />
              <th scope="col">{t.backupColumn}</th>
              <th scope="col">{t.targetColumn}</th>
              <th scope="col">{t.differenceColumn}</th>
            </tr>
          </thead>
          <tbody>
            {countKeys.map((key) => {
              const backupCount = contents[key];
              const targetCount = targetCounts?.[key];
              const difference =
                targetCount === undefined
                  ? undefined
                  : backupCount - targetCount;
              const direction =
                difference === undefined || difference === 0
                  ? undefined
                  : difference > 0
                    ? 'increase'
                    : 'decrease';
              return (
                <tr key={key}>
                  <th scope="row">{t[countLabels[key]]}</th>
                  <td>{formatNumber(backupCount)}</td>
                  <td>
                    {targetCount === undefined
                      ? '—'
                      : formatNumber(targetCount)}
                  </td>
                  <td data-difference={direction}>
                    {difference === undefined
                      ? '—'
                      : formatNumber(difference, { signDisplay: 'exceptZero' })}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
  );
}
