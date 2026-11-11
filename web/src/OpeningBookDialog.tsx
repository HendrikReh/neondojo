import { ConfigurationApiError } from './configuration-api';
import ProfileActionDialog from './ProfileActionDialog';
import { useOptionalConfiguration } from './ConfigurationProvider';
import { useProfileText } from './profile-messages';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import LibraryDialog from './LibraryDialog';
import { request } from './api';
import { AppError } from './i18n-core';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
import {
  bookSettings,
  type BookSettings,
  type BookConfig,
  type BookStatus,
  type XgBookStatus,
} from './source-analysis-api';
export default function OpeningBookDialog({
  onClose,
  onSaved,
}: {
  onClose: () => void;
  onSaved: () => void;
}) {
  const configuration = useOptionalConfiguration();
  const profileT = useProfileText();
  const l = useSourceText(),
    { formatError, formatNumber } = useI18n();
  const [xgChecked, setXgChecked] = useState<XgBookStatus>();
  const [saved, setSaved] = useState<BookSettings>(),
    [draft, setDraft] = useState<BookConfig>(),
    [checked, setChecked] = useState<BookStatus>(),
    [reload, setReload] = useState(0),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>();
  const [confirmReload, setConfirmReload] = useState(false);
  const task = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    const abort = new AbortController();
    setLoading(true);
    setError(undefined);
    void bookSettings(abort.signal)
      .then((data) => {
        if (abort.signal.aborted) return;
        setSaved(data);
        setDraft(
          configuration?.current
            ? { ...data.config, ...configuration.current.opening_book }
            : data.config,
        );
        setChecked(undefined);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => {
      abort.abort();
      task.current?.abort();
    };
  }, [reload]);
  const status =
    checked?.path === draft?.path
      ? checked
      : saved && saved.status.path === draft?.path
        ? saved.status
        : undefined;
  const xgStatus =
    xgChecked?.path === draft?.xg_path
      ? xgChecked
      : saved?.xg_status?.path === draft?.xg_path
        ? saved?.xg_status
        : undefined;
  const conflict =
    (error instanceof AppError && String(error.params.status) === '409') ||
    (error instanceof ConfigurationApiError && error.status === 409);
  async function check() {
    if (!draft || busy) return;
    const abort = new AbortController();
    task.current = abort;
    setBusy(true);
    setError(undefined);
    try {
      const result = await request<BookStatus>('/opening-book/check', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: draft.path }),
        signal: abort.signal,
      });
      if (!abort.signal.aborted) setChecked(result);
    } catch (e) {
      if (!abort.signal.aborted) setError(e);
    } finally {
      if (!abort.signal.aborted) setBusy(false);
    }
  }
  async function checkXg() {
    if (!draft?.xg_path || busy) return;
    const abort = new AbortController();
    task.current = abort;
    setBusy(true);
    setError(undefined);
    try {
      const result = await request<XgBookStatus>('/opening-book/xg/check', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: draft.xg_path }),
        signal: abort.signal,
      });
      if (!abort.signal.aborted) setXgChecked(result);
    } catch (e) {
      if (!abort.signal.aborted) setError(e);
    } finally {
      if (!abort.signal.aborted) setBusy(false);
    }
  }
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!draft || busy || loading || conflict) return;
    const abort = new AbortController();
    task.current = abort;
    setBusy(true);
    setError(undefined);
    try {
      if (configuration) {
        await configuration.applyCurrent((c) => ({
          ...c,
          opening_book: {
            provider: draft.provider as
              'off' | 'automatic' | 'backgammondb' | 'xg',
            path: draft.path,
            xg_path: draft.xg_path,
          },
        }));
      } else {
        await request<BookSettings>('/opening-book', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(draft),
          signal: abort.signal,
        });
      }
      if (!abort.signal.aborted) {
        onSaved();
        onClose();
      }
    } catch (e) {
      if (!abort.signal.aborted) setError(e);
    } finally {
      if (!abort.signal.aborted) setBusy(false);
    }
  }
  return (
    <LibraryDialog native title={l('books')} onClose={onClose} busy={busy}>
      {confirmReload && (
        <ProfileActionDialog
          title={profileT('reloadServer')}
          message={profileT('reloadConfirm')}
          busy={busy}
          onCancel={() => setConfirmReload(false)}
          choices={[
            {
              label: profileT('discard'),
              action: () => {
                setBusy(true);
                void configuration
                  ?.reloadServer()
                  .then(() => {
                    setConfirmReload(false);
                    setReload((n) => n + 1);
                  })
                  .catch(setError)
                  .finally(() => setBusy(false));
              },
            },
          ]}
        />
      )}
      {loading ? (
        <p role="status">{l('loading')}</p>
      ) : draft ? (
        <form className="library-dialog-form" onSubmit={(e) => void submit(e)}>
          <p>{l('configurationHelp')}</p>
          <label>
            {l('provider')}
            <select
              aria-label={l('provider')}
              value={draft.provider}
              disabled={busy}
              onChange={(e) => setDraft({ ...draft, provider: e.target.value })}
            >
              <option value="automatic">{l('automatic')}</option>
              <option value="backgammondb">BackgammonDB</option>
              <option value="xg">eXtreme Gammon (XG)</option>
              <option value="off">{l('off')}</option>
            </select>
          </label>
          <label>
            {l('path')}
            <input
              value={draft.path}
              required
              spellCheck={false}
              disabled={busy}
              onChange={(e) => {
                setDraft({ ...draft, path: e.target.value });
                setChecked(undefined);
              }}
            />
          </label>
          <button
            type="button"
            onClick={() => void check()}
            disabled={busy || !draft.path.trim()}
          >
            {l('check')}
          </button>
          {status && (
            <div className="source-book-status" role="status">
              <strong>
                BackgammonDB ·{' '}
                {status.available
                  ? l('available', { count: formatNumber(status.entries) })
                  : l('unavailable')}
              </strong>
              {status.available && (
                <>
                  <p>
                    GNU Backgammon {status.engine_version} ·{' '}
                    {l('contextCount', { count: status.contexts.length })}
                  </p>
                  <p>
                    {status.attribution} · {status.license}
                  </p>
                </>
              )}
              {status.error && <p>{status.error}</p>}
            </div>
          )}
          <label>
            {l('xgPath')}
            <input
              aria-label={l('xgPath')}
              value={draft.xg_path ?? ''}
              required={draft.provider === 'xg'}
              disabled={busy}
              spellCheck={false}
              onChange={(e) => {
                setDraft({ ...draft, xg_path: e.target.value });
                setXgChecked(undefined);
              }}
            />
          </label>
          <button
            type="button"
            onClick={() => void checkXg()}
            disabled={busy || !draft.xg_path?.trim()}
          >
            {l('checkXg')}
          </button>
          {xgStatus && (
            <div className="source-book-status">
              <strong>
                {xgStatus.available
                  ? l('xgReady', { count: formatNumber(xgStatus.entries) })
                  : l('unavailable')}
              </strong>
              {xgStatus.book_version && (
                <p>eXtreme Gammon Opening Book {xgStatus.book_version}</p>
              )}
              <p>{xgStatus.license}</p>
              {xgStatus.error && <p>{xgStatus.error}</p>}
            </div>
          )}
          <p className="fineprint">{l('versionHelp')}</p>
          <p className="fineprint">{l('bookStructure')}</p>
          {!!error && (
            <p role="alert" className="error">
              {conflict ? l('conflict') : formatError(error)}
            </p>
          )}
          {conflict && (
            <button
              type="button"
              onClick={() =>
                configuration ? setConfirmReload(true) : setReload((n) => n + 1)
              }
            >
              {l('retry')}
            </button>
          )}
          <div className="library-dialog-actions">
            <button type="button" disabled={busy} onClick={onClose}>
              {l('cancel')}
            </button>
            <button className="primary" disabled={busy || conflict}>
              {busy
                ? l('loading')
                : configuration
                  ? profileT('apply')
                  : l('save')}
            </button>
          </div>
        </form>
      ) : (
        <>
          <p role="alert">{formatError(error)}</p>
          <button onClick={() => setReload((n) => n + 1)}>{l('retry')}</button>
        </>
      )}
    </LibraryDialog>
  );
}
