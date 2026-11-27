import { useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { useVersionedResource } from '../../shared/hooks/useVersionedResource';
import type { Match } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import type { Database } from '../library/library-api';
import { usePlayerText } from './player-directory-messages';
import { aliasLabel } from './PlayerResults';
import {
  PlayerMatches,
  PlayerRatings,
  PlayerRatingTrends,
  type AnalyzeMatches,
} from './PlayerSummary';
import {
  loadVirtualPlayer,
  removeVirtualPlayerMember,
  type VirtualPlayerDetails,
  type VirtualPlayerProfile,
} from './virtual-player-api';
import { useVirtualPlayerText } from './virtual-player-messages';
import {
  VirtualPlayerDeleteDialog,
  VirtualPlayerMemberDialog,
  VirtualPlayerNameDialog,
} from './VirtualPlayerDialogs';

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
  const { details, setDetails, error, retry } = useVersionedResource(
    id,
    loadVirtualPlayer,
    revision,
  );
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
            className="danger"
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
                  <ActionButton
                    icon="delete"
                    label={v('remove')}
                    className="danger"
                    type="button"
                    disabled={busy}
                    aria-label={v('removeMember', {
                      name: member.display_name,
                      group: details.name,
                    })}
                    onClick={() => void remove(member.id)}
                  />
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
