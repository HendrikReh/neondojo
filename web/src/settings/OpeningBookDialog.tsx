import { ConfigurationApiError } from './configuration/configuration-api';
import { openingBookConfigurationKey } from './configuration/configuration-model';
import ProfileActionDialog from './profiles/ProfileActionDialog';
import { useOptionalConfiguration } from './configuration/ConfigurationProvider';
import { useProfileText } from './profiles/profile-messages';
import ActionButton from '../shared/ui/ActionButton';
import EngineLabel from '../shared/engine/EngineLabel';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import LibraryDialog from '../shared/ui/LibraryDialog';
import { request } from '../shared/api/api';
import { AppError } from '../shared/i18n/i18n-core';
import { useI18n } from '../shared/i18n/i18n';
import {
  openingBookProviderLabel,
  openingBookProviders,
  useSourceText,
} from '../workspaces/analysis/candidates/analysis-source-messages';
import {
  bookSettings,
  type BookSettings,
  type BookConfig,
  type BookStatus,
  type XgBookStatus,
} from '../workspaces/analysis/candidates/source-analysis-api';
export default function OpeningBookDialog({
  onClose,
  onRefresh,
}: {
  onClose: () => void;
  onRefresh: () => void;
}) {
  const configuration = useOptionalConfiguration();
  const profileT = useProfileText();
  const l = useSourceText(),
    { locale, formatError } = useI18n();
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
      let refresh = true;
      if (configuration) {
        let previousBookKey: string | undefined;
        const applied = await configuration.applyCurrent((current) => {
          previousBookKey = openingBookConfigurationKey(current.opening_book);
          return {
            ...current,
            opening_book: {
              provider: draft.provider as
                'off' | 'automatic' | 'backgammondb' | 'xg',
              path: draft.path,
              xg_path: draft.xg_path,
            },
          };
        });
        // Changed book settings already refresh through the configuration provider.
        refresh =
          previousBookKey === openingBookConfigurationKey(applied.opening_book);
      } else {
        await request<BookSettings>('/opening-book', {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(draft),
          signal: abort.signal,
        });
      }
      if (!abort.signal.aborted) {
        if (refresh) onRefresh();
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
              {openingBookProviders.map((provider) => (
                <option key={provider} value={provider}>
                  {openingBookProviderLabel(locale, provider)}
                </option>
              ))}
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
          <ActionButton
            icon="confirm"
            label={l('check')}
            compact
            type="button"
            onClick={() => void check()}
            disabled={busy || !draft.path.trim()}
          />
          {status && (
            <div className="source-book-status" role="status">
              <strong>
                BackgammonDB ·{' '}
                {status.available
                  ? l('available', { count: status.entries })
                  : l('unavailable')}
              </strong>
              {status.available && (
                <>
                  <p>
                    <EngineLabel
                      details={
                        status.engine_version
                          ? `GNU Backgammon ${status.engine_version}`
                          : 'GNU Backgammon'
                      }
                    />
                    {' · '}
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
          <ActionButton
            icon="confirm"
            label={l('checkXg')}
            compact
            type="button"
            onClick={() => void checkXg()}
            disabled={busy || !draft.xg_path?.trim()}
          />
          {xgStatus && (
            <div className="source-book-status">
              <strong>
                {xgStatus.available
                  ? l('xgReady', { count: xgStatus.entries })
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
            <ActionButton
              icon="refresh"
              label={l('retry')}
              compact
              type="button"
              onClick={() =>
                configuration ? setConfirmReload(true) : setReload((n) => n + 1)
              }
            />
          )}
          <div className="library-dialog-actions">
            <ActionButton
              icon="cancel"
              label={l('cancel')}
              compact
              type="button"
              disabled={busy}
              onClick={onClose}
            />
            <ActionButton
              icon="save"
              label={
                busy
                  ? l('loading')
                  : configuration
                    ? profileT('apply')
                    : l('save')
              }
              className="primary"
              disabled={busy || conflict}
            />
          </div>
        </form>
      ) : (
        <>
          <p role="alert">{formatError(error)}</p>
          <ActionButton
            icon="refresh"
            label={l('retry')}
            compact
            onClick={() => setReload((n) => n + 1)}
          />
        </>
      )}
    </LibraryDialog>
  );
}
