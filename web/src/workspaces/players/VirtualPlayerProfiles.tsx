import { useEffect, useRef, useState, type FormEvent } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import type { Match } from '../../shared/api/api';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import type { Database } from '../library/library-api';
import { useLibraryText } from '../library/library-messages';
import { usePlayerText } from './player-directory-messages';
import { usePlayerDetails } from './PlayerProfileDialogs';
import { aliasLabel, PlayerResults } from './PlayerResults';
import {
  PlayerMatches,
  PlayerRatings,
  PlayerRatingTrends,
  type AnalyzeMatches,
} from './PlayerSummary';
import {
  addVirtualPlayerMember,
  createVirtualPlayer,
  deleteVirtualPlayer,
  loadVirtualPlayer,
  removeVirtualPlayerMember,
  renameVirtualPlayer,
  type VirtualPlayerDetails,
  type VirtualPlayerProfile,
} from './virtual-player-api';
import { useVirtualPlayerText } from './virtual-player-messages';

/** The virtual profiles above the player list, "me" first. */
export function VirtualPlayerStrip({
  items,
  error,
  onRetry,
  selected,
  onSelect,
  onCreated,
}: {
  items?: VirtualPlayerProfile[];
  error?: unknown;
  onRetry: () => void;
  selected?: string;
  onSelect: (id: string) => void;
  onCreated: (id: string) => void;
}) {
  const v = useVirtualPlayerText();
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const [creating, setCreating] = useState(false);
  return (
    <section className="virtual-player-strip" aria-labelledby="virtual-players">
      <h2 id="virtual-players">{v('title')}</h2>
      {error !== undefined && (
        <p role="alert">
          {formatError(error)}{' '}
          <ActionButton
            icon="refresh"
            label={t('retry')}
            compact
            type="button"
            onClick={onRetry}
          />
        </p>
      )}
      {items === undefined && error === undefined && (
        <p role="status">{t('loading')}</p>
      )}
      {items?.length === 0 && (
        <p className="virtual-player-empty">{v('empty')}</p>
      )}
      {!!items?.length && (
        <ul>
          {items.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                aria-pressed={item.id === selected}
                title={`${item.name} · ${t('counts', {
                  matches: item.match_count,
                  games: item.game_count,
                })}`}
                onClick={() => onSelect(item.id)}
              >
                <span className="virtual-player-name">{item.name}</span>
                {item.is_me && (
                  <span className="virtual-player-me">{v('me')}</span>
                )}
                <span className="virtual-player-count">
                  {formatNumber(item.match_count)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <ActionButton
        icon="add"
        type="button"
        label={v('create')}
        aria-haspopup="dialog"
        onClick={() => setCreating(true)}
      />
      {creating && (
        <VirtualPlayerNameDialog
          onClose={() => setCreating(false)}
          onSaved={(profile) => {
            setCreating(false);
            onCreated(profile.id);
          }}
        />
      )}
    </section>
  );
}

function useVirtualPlayer(id: string, revision: number) {
  const [details, setDetails] = useState<VirtualPlayerDetails>();
  const [failure, setFailure] = useState<{ id: string; error: unknown }>();
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    setFailure(undefined);
    loadVirtualPlayer(id, abort.signal).then(
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
    details: details?.id === id ? details : undefined,
    setDetails,
    error: failure?.id === id ? failure.error : undefined,
    retry: () => setAttempt((n) => n + 1),
  };
}

export function VirtualPlayerCard({
  id,
  databases,
  revision,
  onOpen,
  onProfile,
  onAnalyzeMatches,
  onChanged,
  onDeleted,
}: {
  id: string;
  databases: Database[];
  revision: number;
  onOpen: (match: Match) => void;
  onProfile: (id: string) => void;
  onAnalyzeMatches?: AnalyzeMatches;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  const v = useVirtualPlayerText();
  const t = usePlayerText();
  const { formatError } = useI18n();
  const { details, setDetails, error, retry } = useVirtualPlayer(id, revision);
  const [dialog, setDialog] = useState<'rename' | 'member' | 'delete'>();
  const [busy, setBusy] = useState(false);
  const [writeError, setWriteError] = useState<unknown>();

  if (!details)
    return error ? (
      <p role="alert">
        {formatError(error)}{' '}
        <ActionButton
          icon="refresh"
          label={t('retry')}
          compact
          type="button"
          onClick={retry}
        />
      </p>
    ) : (
      <p role="status">{t('loading')}</p>
    );

  const changed = (profile: VirtualPlayerDetails) => {
    setDetails(profile);
    onChanged();
  };

  async function remove(memberId: string) {
    if (!details || busy) return;
    setBusy(true);
    setWriteError(undefined);
    try {
      changed(await removeVirtualPlayerMember(details, memberId));
    } catch (failure) {
      setWriteError(failure);
      retry();
    } finally {
      setBusy(false);
    }
  }

  return (
    <section
      className="player-card virtual-player-card"
      aria-label={details.name}
    >
      <header className="player-card-header">
        <div>
          <h2>
            {details.name}
            {details.is_me && (
              <span className="virtual-player-me">{v('me')}</span>
            )}
          </h2>
          <p>
            {v('kind')} · {v('memberCount', { count: details.member_count })} ·{' '}
            {t('counts', {
              matches: details.match_count,
              games: details.game_count,
            })}
          </p>
          {details.is_me && (
            <p className="virtual-player-note">{v('botMatches')}</p>
          )}
        </div>
        <div className="player-profile-actions">
          <ActionButton
            icon="add"
            type="button"
            label={v('addMember')}
            aria-haspopup="dialog"
            onClick={() => setDialog('member')}
          />
          <ActionButton
            icon="edit"
            type="button"
            label={v('rename')}
            aria-haspopup="dialog"
            onClick={() => setDialog('rename')}
          />
          <ActionButton
            icon="delete"
            type="button"
            label={v('delete')}
            aria-haspopup="dialog"
            onClick={() => setDialog('delete')}
          />
        </div>
      </header>
      <section className="virtual-player-members" aria-label={v('members')}>
        <h3>{v('members')}</h3>
        {details.members.length ? (
          <ul>
            {details.members.map((member) => {
              const sources = member.aliases.map(aliasLabel).join(', ');
              return (
                <li key={member.id}>
                  <button
                    type="button"
                    className="position-preview-link"
                    title={t('openProfile')}
                    onClick={() => onProfile(member.id)}
                  >
                    {member.display_name}
                  </button>
                  <span className="virtual-player-sources" title={sources}>
                    {sources}
                  </span>
                  <span className="virtual-player-count">
                    {v('matchCount', { count: member.match_count })}
                  </span>
                  <button
                    type="button"
                    disabled={busy}
                    aria-label={v('removeMember', {
                      name: member.display_name,
                      group: details.name,
                    })}
                    onClick={() => void remove(member.id)}
                  >
                    {v('remove')}
                  </button>
                </li>
              );
            })}
          </ul>
        ) : (
          <p>{v('noMembers')}</p>
        )}
        {writeError !== undefined && (
          <p role="alert">{formatError(writeError)}</p>
        )}
      </section>
      <PlayerRatings
        scope={{ virtual_player_id: id }}
        ratings={details.ratings}
        matchCount={details.rateable_match_count}
        onAnalyzeMatches={onAnalyzeMatches}
      />
      <PlayerMatches
        scope={{ virtual_player_id: id }}
        databases={databases}
        onOpen={onOpen}
        onProfile={onProfile}
        revision={revision + details.version}
      />
      <PlayerRatingTrends
        ratings={details.ratings}
        history={details.rating_history}
      />
      {dialog === 'rename' && (
        <VirtualPlayerNameDialog
          profile={details}
          onClose={() => setDialog(undefined)}
          onSaved={(profile) => {
            changed(profile);
            setDialog(undefined);
          }}
        />
      )}
      {dialog === 'member' && (
        <VirtualPlayerMemberDialog
          profile={details}
          revision={revision}
          onClose={() => setDialog(undefined)}
          onAdded={(profile) => {
            changed(profile);
            setDialog(undefined);
          }}
        />
      )}
      {dialog === 'delete' && (
        <VirtualPlayerDeleteDialog
          profile={details}
          onClose={() => setDialog(undefined)}
          onDeleted={onDeleted}
        />
      )}
    </section>
  );
}

/** The backend's conflict for a name another virtual profile carries. */
function nameTaken(error: unknown) {
  return (
    error instanceof AppError &&
    error.params.status === '409' &&
    error.diagnostic.includes('already exists')
  );
}

/** Creates a virtual profile, or renames `profile`. */
function VirtualPlayerNameDialog({
  profile,
  onClose,
  onSaved,
}: {
  profile?: VirtualPlayerDetails;
  onClose: () => void;
  onSaved: (profile: VirtualPlayerDetails) => void;
}) {
  const v = useVirtualPlayerText();
  const l = useLibraryText();
  const { formatError } = useI18n();
  const [name, setName] = useState(profile?.name ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const valid = name.trim() !== '' && name.trim() !== profile?.name;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!valid || busy) return;
    setBusy(true);
    setError(undefined);
    try {
      onSaved(
        profile
          ? await renameVirtualPlayer(profile, name)
          : await createVirtualPlayer(name),
      );
    } catch (failure) {
      setError(failure);
      setBusy(false);
    }
  }

  return (
    <LibraryDialog
      title={profile ? v('renameTitle') : v('createTitle')}
      busy={busy}
      onClose={onClose}
      native
      initialFocus="first-field"
    >
      <form className="player-profile-editor" onSubmit={submit}>
        <label className="virtual-player-field">
          {v('name')}
          <input
            value={name}
            maxLength={120}
            required
            disabled={busy}
            onChange={(event) => setName(event.target.value)}
          />
        </label>
        {error !== undefined && (
          <p role="alert">
            {nameTaken(error) ? v('nameTaken') : formatError(error)}
          </p>
        )}
        <div className="library-dialog-actions">
          <button type="button" disabled={busy} onClick={onClose}>
            {l('cancel')}
          </button>
          <button className="primary" disabled={busy || !valid}>
            {profile ? v('save') : v('createAction')}
          </button>
        </div>
      </form>
    </LibraryDialog>
  );
}

function VirtualPlayerDeleteDialog({
  profile,
  onClose,
  onDeleted,
}: {
  profile: VirtualPlayerDetails;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const v = useVirtualPlayerText();
  const l = useLibraryText();
  const { formatError } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  return (
    <LibraryDialog
      title={v('deleteTitle')}
      busy={busy}
      onClose={onClose}
      native
    >
      <p>{v('deleteHint', { name: profile.name })}</p>
      {error !== undefined && <p role="alert">{formatError(error)}</p>}
      <div className="library-dialog-actions">
        <button type="button" disabled={busy} onClick={onClose}>
          {l('cancel')}
        </button>
        <button
          type="button"
          className="danger"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError(undefined);
            try {
              await deleteVirtualPlayer(profile);
              onDeleted();
            } catch (failure) {
              setError(failure);
              setBusy(false);
            }
          }}
        >
          {v('deleteTitle')}
        </button>
      </div>
    </LibraryDialog>
  );
}

function VirtualPlayerMemberDialog({
  profile,
  revision,
  onClose,
  onAdded,
}: {
  profile: VirtualPlayerDetails;
  revision: number;
  onClose: () => void;
  onAdded: (profile: VirtualPlayerDetails) => void;
}) {
  const v = useVirtualPlayerText();
  const t = usePlayerText();
  const l = useLibraryText();
  const { formatError } = useI18n();
  const [search, setSearch] = useState('');
  const [candidateId, setCandidateId] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const {
    details: candidate,
    error: candidateError,
    retry,
  } = usePlayerDetails(candidateId);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const group = candidate?.virtual_profile;
  const member = group?.id === profile.id;
  const elsewhere = group != null && !member;

  return (
    <LibraryDialog
      title={v('addMemberTitle')}
      busy={busy}
      onClose={onClose}
      native
      initialFocus="first-field"
    >
      <div className="player-merge position-browser">
        <p className="player-merge-suggestions">{v('addMemberHint')}</p>
        <label className="player-search">
          {v('memberSearch')}
          <input
            type="search"
            maxLength={240}
            value={search}
            disabled={busy}
            onChange={(event) => {
              setSearch(event.target.value);
              setCandidateId(undefined);
            }}
          />
        </label>
        {search.trim() && (
          <div inert={busy}>
            <PlayerResults
              search={search}
              revision={revision}
              selected={candidateId}
              onSelect={setCandidateId}
            />
          </div>
        )}
        {candidateError !== undefined ? (
          <p role="alert">
            {formatError(candidateError)}{' '}
            <button onClick={retry}>{t('retry')}</button>
          </p>
        ) : candidateId && !candidate ? (
          <p role="status">{t('loading')}</p>
        ) : null}
        {candidate && (
          <section
            className="player-merge-profile"
            aria-label={v('memberSearch')}
          >
            <strong>{candidate.display_name}</strong>
            <p>
              {t('counts', {
                matches: candidate.match_count,
                games: candidate.game_count,
              })}
            </p>
            <ul className="player-aliases">
              {candidate.aliases.map((alias) => (
                <li key={`${alias.platform}:${alias.name}`}>
                  {aliasLabel(alias)}
                </li>
              ))}
            </ul>
            {member && <p role="status">{v('alreadyMember')}</p>}
            {elsewhere && group && (
              <p role="status">{v('otherGroup', { name: group.name })}</p>
            )}
          </section>
        )}
        {error !== undefined && <p role="alert">{formatError(error)}</p>}
        <div className="library-dialog-actions">
          <button type="button" disabled={busy} onClick={onClose}>
            {l('cancel')}
          </button>
          <button
            type="button"
            className="primary"
            disabled={busy || !candidate || group != null}
            onClick={async () => {
              if (!candidate) return;
              setBusy(true);
              setError(undefined);
              try {
                const added = await addVirtualPlayerMember(
                  profile,
                  candidate.id,
                );
                if (mounted.current) onAdded(added);
              } catch (failure) {
                if (mounted.current) {
                  setError(failure);
                  setBusy(false);
                }
              }
            }}
          >
            {v('add')}
          </button>
        </div>
      </div>
    </LibraryDialog>
  );
}
