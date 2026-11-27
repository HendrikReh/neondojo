import ActionButton from '../../shared/ui/ActionButton';
import { useState } from 'react';
import { useVersionedResource } from '../../shared/hooks/useVersionedResource';
import { request } from '../../shared/http/api';
import type { components } from '../../generated-api';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { useLibraryText } from '../library/library-messages';
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

const loadPlayerDetails = (id: string, signal: AbortSignal) =>
  request<Details>(`/players/${id}`, { signal });

export function usePlayerDetails(id?: string, revision = 0) {
  return useVersionedResource(id, loadPlayerDetails, revision);
}

function usePlayerWrite(profile: Details) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const [conflict, setConflict] = useState(false);

  /** `version` continues from a write of the same submission. */
  async function run(change?: PlayerWrite, version = profile.version) {
    if (busy) return;
    setBusy(true);
    setError(undefined);
    try {
      const result = await request<Details>(
        `/players/${profile.id}${change?.path ?? ''}`,
        change && {
          method: change.method,
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ...change.body, version }),
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

/** First meaningful word of a name, to offer likely duplicates of a profile. */
function nameSeed(name: string) {
  return name.split(/[^\p{L}\p{N}]+/u).find((word) => word.length >= 3) ?? '';
}

function PlayerAliases({ aliases }: { aliases: Schema['PlayerAlias'][] }) {
  return (
    <ul className="player-aliases">
      {aliases.map((alias) => (
        <li key={`${alias.platform}:${alias.name}`}>{aliasLabel(alias)}</li>
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
  const [saved, setSaved] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const nameChanged =
    first !== profile.first_name || last !== profile.last_name;

  /** Saves the name first; a new alias continues from the saved version. */
  async function save() {
    setSaved(false);
    let current = profile;
    if (nameChanged) {
      const named = await run({
        path: '',
        method: 'PUT',
        body: { first_name: first, last_name: last },
      });
      if (!named) return;
      current = named;
      setProfile(named);
      setFirst(named.first_name);
      setLast(named.last_name);
      onChanged(named);
    }
    if (alias.trim()) {
      const added = await run(
        {
          path: '/aliases',
          method: 'POST',
          body: { name: alias, platform },
        },
        current.version,
      );
      if (!added) return;
      setProfile(added);
      setAlias('');
      onChanged(added);
    }
    setSaved(true);
  }

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
            <ActionButton
              icon="edit"
              label={t('keepEditing')}
              type="button"
              autoFocus
              onClick={() => setConfirmDiscard(false)}
            />
            <ActionButton
              icon="clear"
              label={t('discard')}
              type="button"
              onClick={onClose}
            />
          </div>
        </>
      ) : (
        <form
          className="player-profile-editor"
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
        >
          <div className="player-form">
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
          </div>
          <section>
            <h3>{t('aliases')}</h3>
            <PlayerAliases aliases={profile.aliases} />
            <fieldset className="player-form">
              <legend>{t('add')}</legend>
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
                  <option>neondojo</option>
                </select>
              </label>
              <label>
                {t('alias')}
                <input
                  value={alias}
                  maxLength={1024}
                  disabled={busy}
                  onChange={(event) => setAlias(event.target.value)}
                />
              </label>
            </fieldset>
          </section>
          {saved && <p role="status">{t('saved')}</p>}
          {error !== undefined && <p role="alert">{formatError(error)}</p>}
          {conflict && (
            <ActionButton
              icon="refresh"
              label={t('reload')}
              type="button"
              disabled={busy}
              onClick={async () => {
                const latest = await run();
                if (latest) {
                  setProfile(latest);
                  onChanged(latest);
                }
              }}
            />
          )}
          <div className="library-dialog-actions">
            <ActionButton
              type="submit"
              icon="save"
              label={t('save')}
              className="primary"
              disabled={busy || !(nameChanged || alias.trim())}
            />
          </div>
        </form>
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
  const { formatError } = useI18n();
  const [profile, setProfile] = useState(initialProfile);
  const { busy, error, conflict, run } = usePlayerWrite(profile);
  const [search, setSearch] = useState('');
  const [sourceId, setSourceId] = useState<string>();
  const [refresh, setRefresh] = useState(0);
  const typed = search.trim();
  const query = typed || nameSeed(profile.display_name);
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
      initialFocus="first-field"
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
        {query && (
          <div inert={busy}>
            {!typed && (
              <p className="player-merge-suggestions">{t('suggestions')}</p>
            )}
            <PlayerResults
              search={query}
              quietWhenEmpty={!typed}
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
            <ActionButton
              type="button"
              icon="refresh"
              label={t('retry')}
              onClick={retry}
            />
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
                      matches: player.match_count,
                      games: player.game_count,
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
          <ActionButton
            icon="refresh"
            label={t('reload')}
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
          />
        )}
        <div className="library-dialog-actions">
          <ActionButton
            icon="cancel"
            label={l('cancel')}
            type="button"
            disabled={busy}
            onClick={onClose}
          />
          <ActionButton
            icon="merge"
            label={t('merge')}
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
          />
        </div>
      </div>
    </LibraryDialog>
  );
}
