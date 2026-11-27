import { useState, type FormEvent } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useIsMounted } from '../../shared/hooks/useIsMounted';
import { useI18n } from '../../shared/i18n/i18n';
import { AppError } from '../../shared/i18n/i18n-core';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { useLibraryText } from '../library/library-messages';
import { usePlayerText } from './player-directory-messages';
import { usePlayerDetails } from './PlayerProfileDialogs';
import { aliasLabel, PlayerResults } from './PlayerResults';
import {
  addVirtualPlayerMember,
  createVirtualPlayer,
  deleteVirtualPlayer,
  renameVirtualPlayer,
  type VirtualPlayerDetails,
} from './virtual-player-api';
import { useVirtualPlayerText } from './virtual-player-messages';

/** The backend's conflict for a name another virtual profile carries. */
function nameTaken(error: unknown) {
  return (
    error instanceof AppError &&
    error.params.status === '409' &&
    error.diagnostic.includes('already exists')
  );
}

/** Creates a virtual profile, or renames `profile`. */
export function VirtualPlayerNameDialog({
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
          <ActionButton
            icon="cancel"
            label={l('cancel')}
            type="button"
            disabled={busy}
            onClick={onClose}
          />
          <ActionButton
            type="submit"
            icon={profile ? 'save' : 'add'}
            label={profile ? v('save') : v('createAction')}
            className="primary"
            disabled={busy || !valid}
          />
        </div>
      </form>
    </LibraryDialog>
  );
}

export function VirtualPlayerDeleteDialog({
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
        <ActionButton
          icon="cancel"
          label={l('cancel')}
          type="button"
          disabled={busy}
          onClick={onClose}
        />
        <ActionButton
          icon="delete"
          label={v('deleteTitle')}
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
        />
      </div>
    </LibraryDialog>
  );
}

export function VirtualPlayerMemberDialog({
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
  const mounted = useIsMounted();
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
            <ActionButton
              type="button"
              icon="refresh"
              label={t('retry')}
              onClick={retry}
            />
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
          <ActionButton
            icon="cancel"
            label={l('cancel')}
            type="button"
            disabled={busy}
            onClick={onClose}
          />
          <ActionButton
            icon="add"
            label={v('add')}
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
          />
        </div>
      </div>
    </LibraryDialog>
  );
}
