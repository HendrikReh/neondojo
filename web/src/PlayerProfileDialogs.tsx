import { useEffect, useState } from 'react';
import { request } from './api';
import type { components } from './generated-api';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import LibraryDialog from './LibraryDialog';
import { useLibraryText } from './library-messages';
import { usePlayerText } from './player-directory-messages';
import { aliasLabel, PlayerResults } from './PlayerResults';

type Schema = components['schemas'];
type Details = Schema['PlayerDetails'];
type PlayerWrite =
  | {
      path: '';
      method: 'PUT';
      body: Omit<Schema['PlayerNameWrite'], 'version'>;
    }
  | {
      path: '/aliases';
      method: 'POST';
      body: Omit<Schema['PlayerAliasWrite'], 'version'>;
    }
  | {
      path: '/merge';
      method: 'POST';
      body: Omit<Schema['PlayerMerge'], 'version'>;
    };

export function usePlayerDetails(id?: string, revision = 0) {
  const [details, setDetails] = useState<Details>();
  const [failure, setFailure] = useState<{ id: string; error: unknown }>();
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!id) return;
    const abort = new AbortController();
    setFailure(undefined);
    request<Details>(`/players/${id}`, { signal: abort.signal }).then(
      (row) => {
        if (!abort.signal.aborted)
          setDetails((previous) =>
            previous?.id === id && previous.version > row.version
              ? previous
              : row,
          );
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setFailure({ id, error });
      },
    );
    return () => abort.abort();
  }, [id, revision, attempt]);

  return {
    details: id && details?.id === id ? details : undefined,
    setDetails,
    error: failure?.id === id ? failure?.error : undefined,
    retry: () => setAttempt((n) => n + 1),
  };
}

function usePlayerWrite(profile: Details) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const [conflict, setConflict] = useState(false);

  async function run(change?: PlayerWrite) {
    if (busy) return;
    setBusy(true);
    setError(undefined);
    try {
      const result = await request<Details>(
        `/players/${profile.id}${change?.path ?? ''}`,
        change && {
          method: change.method,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ...change.body, version: profile.version }),
        },
      );
      setConflict(false);
      return result;
    } catch (error) {
      setError(error);
      if (
        error instanceof AppError &&
        ['404', '409'].includes(String(error.params.status))
      )
        setConflict(true);
    } finally {
      setBusy(false);
    }
  }

  return { busy, error, conflict, run };
}

function PlayerAliases({ aliases }: { aliases: Schema['PlayerAlias'][] }) {
  const t = usePlayerText();
  return (
    <ul className="player-aliases">
      {aliases.map((alias) => (
        <li key={`${alias.platform}:${alias.name}`}>
          {aliasLabel(alias, t('local'))}
        </li>
      ))}
    </ul>
  );
}

export function PlayerEditDialog({
  profile: initialProfile,
  onChanged,
  onClose,
}: {
  profile: Details;
  onChanged: (profile: Details) => void;
  onClose: () => void;
}) {
  const t = usePlayerText();
  const { formatError } = useI18n();
  const [profile, setProfile] = useState(initialProfile);
  const { busy, error, conflict, run } = usePlayerWrite(profile);
  const [first, setFirst] = useState(profile.first_name);
  const [last, setLast] = useState(profile.last_name);
  const [alias, setAlias] = useState('');
  const [platform, setPlatform] = useState('');
  const [feedback, setFeedback] = useState<'saved' | 'aliasAdded'>();
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const nameChanged =
    first !== profile.first_name || last !== profile.last_name;

  function close() {
    if (confirmDiscard) setConfirmDiscard(false);
    else if (nameChanged || alias.trim()) setConfirmDiscard(true);
    else onClose();
  }

  return (
    <LibraryDialog title={t('edit')} busy={busy} onClose={close} native>
      {confirmDiscard ? (
        <>
          <p role="alert">{t('discardQuestion')}</p>
          <div className="library-dialog-actions">
            <button
              type="button"
              autoFocus
              onClick={() => setConfirmDiscard(false)}
            >
              {t('keepEditing')}
            </button>
            <button type="button" onClick={onClose}>
              {t('discard')}
            </button>
          </div>
        </>
      ) : (
        <div className="player-profile-editor">
          <form
            className="player-form"
            onSubmit={async (event) => {
              event.preventDefault();
              setFeedback(undefined);
              const saved = await run({
                path: '',
                method: 'PUT',
                body: { first_name: first, last_name: last },
              });
              if (saved) {
                setProfile(saved);
                setFirst(saved.first_name);
                setLast(saved.last_name);
                onChanged(saved);
                setFeedback('saved');
              }
            }}
          >
            <label>
              {t('first')}
              <input
                value={first}
                maxLength={120}
                disabled={busy}
                onChange={(event) => setFirst(event.target.value)}
              />
            </label>
            <label>
              {t('last')}
              <input
                value={last}
                maxLength={120}
                disabled={busy}
                onChange={(event) => setLast(event.target.value)}
              />
            </label>
            <button disabled={busy || !nameChanged}>{t('save')}</button>
          </form>
          <section>
            <h3>{t('aliases')}</h3>
            <PlayerAliases aliases={profile.aliases} />
            <form
              className="player-form"
              onSubmit={async (event) => {
                event.preventDefault();
                setFeedback(undefined);
                const saved = await run({
                  path: '/aliases',
                  method: 'POST',
                  body: { name: alias, platform },
                });
                if (saved) {
                  setProfile(saved);
                  onChanged(saved);
                  setAlias('');
                  setFeedback('aliasAdded');
                }
              }}
            >
              <label>
                {t('source')}
                <select
                  value={platform}
                  disabled={busy}
                  onChange={(event) => setPlatform(event.target.value)}
                >
                  <option value="">{t('local')}</option>
                  <option>backgammongalaxy.com</option>
                  <option>opengammon.com</option>
                </select>
              </label>
              <label>
                {t('alias')}
                <input
                  value={alias}
                  maxLength={1024}
                  disabled={busy}
                  required
                  onChange={(event) => setAlias(event.target.value)}
                />
              </label>
              <button disabled={busy || !alias.trim()}>{t('add')}</button>
            </form>
          </section>
          {feedback && <p role="status">{t(feedback)}</p>}
          {error !== undefined && <p role="alert">{formatError(error)}</p>}
          {conflict && (
            <button
              type="button"
              disabled={busy}
              onClick={async () => {
                const latest = await run();
                if (latest) {
                  setProfile(latest);
                  onChanged(latest);
                }
              }}
            >
              {t('reload')}
            </button>
          )}
        </div>
      )}
    </LibraryDialog>
  );
}

export function PlayerMergeDialog({
  profile: initialProfile,
  revision,
  onClose,
  onRefreshed,
  onMerged,
}: {
  profile: Details;
  revision: number;
  onClose: () => void;
  onRefreshed: (profile: Details) => void;
  onMerged: (profile: Details) => void;
}) {
  const t = usePlayerText();
  const l = useLibraryText();
  const { formatError, formatNumber } = useI18n();
  const [profile, setProfile] = useState(initialProfile);
  const { busy, error, conflict, run } = usePlayerWrite(profile);
  const [search, setSearch] = useState('');
  const [sourceId, setSourceId] = useState<string>();
  const [refresh, setRefresh] = useState(0);
  const {
    details: candidate,
    setDetails: setCandidate,
    error: sourceError,
    retry,
  } = usePlayerDetails(sourceId);

  return (
    <LibraryDialog
      title={t('merge')}
      busy={busy}
      onClose={onClose}
      native
      initialFocus="dialog"
    >
      <div className="player-merge position-browser">
        <label className="player-search">
          {t('mergeWith')}
          <input
            type="search"
            maxLength={240}
            value={search}
            disabled={busy}
            onChange={(event) => {
              setSearch(event.target.value);
              setSourceId(undefined);
              setCandidate(undefined);
            }}
          />
        </label>
        {search.trim() && (
          <div inert={busy}>
            <PlayerResults
              search={search}
              excludeId={profile.id}
              revision={revision + refresh}
              selected={sourceId}
              onSelect={(id) => {
                if (id === sourceId) return;
                setCandidate(undefined);
                setSourceId(id);
              }}
            />
          </div>
        )}
        {sourceError !== undefined ? (
          <p role="alert">
            {formatError(sourceError)}{' '}
            <button onClick={retry}>{t('retry')}</button>
          </p>
        ) : sourceId && !candidate ? (
          <p role="status">{t('loading')}</p>
        ) : null}
        {candidate && (
          <>
            <div className="player-merge-profiles">
              {(
                [
                  ['mergeTarget', profile],
                  ['mergeSource', candidate],
                ] as const
              ).map(([role, player]) => (
                <section
                  key={role}
                  className="player-merge-profile"
                  aria-label={t(role)}
                >
                  <h3>{t(role)}</h3>
                  <strong>{player.display_name}</strong>
                  <p>
                    {t('counts', {
                      matches: formatNumber(player.match_count),
                      games: formatNumber(player.game_count),
                    })}
                  </p>
                  <PlayerAliases aliases={player.aliases} />
                </section>
              ))}
            </div>
            <p>{t('mergeHint')}</p>
          </>
        )}
        {error !== undefined && <p role="alert">{formatError(error)}</p>}
        {conflict && (
          <button
            type="button"
            disabled={busy}
            onClick={async () => {
              const latest = await run();
              if (latest) {
                setProfile(latest);
                onRefreshed(latest);
                setSourceId(undefined);
                setCandidate(undefined);
                setRefresh((n) => n + 1);
              }
            }}
          >
            {t('reload')}
          </button>
        )}
        <div className="library-dialog-actions">
          <button type="button" disabled={busy} onClick={onClose}>
            {l('cancel')}
          </button>
          <button
            type="button"
            className="danger"
            disabled={
              busy || conflict || !candidate || sourceError !== undefined
            }
            onClick={async () => {
              if (!candidate || candidate.id === profile.id) return;
              const merged = await run({
                path: '/merge',
                method: 'POST',
                body: {
                  source_id: candidate.id,
                  source_version: candidate.version,
                },
              });
              if (merged) onMerged(merged);
            }}
          >
            {t('merge')}
          </button>
        </div>
      </div>
    </LibraryDialog>
  );
}
