import { useCallback, useEffect, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { request, type Match } from '../../shared/api/api';
import { SearchResultList } from '../../shared/ui/SearchResultList';
import { SearchResultPreview } from '../../shared/ui/SearchResultPreview';
import { useI18n } from '../../shared/i18n/i18n';
import type { Database } from '../library/library-api';
import { usePlayerText } from './player-directory-messages';
import { aliasLabel, PlayerResults, type PlayerOrder } from './PlayerResults';
import {
  PlayerEditDialog,
  PlayerMergeDialog,
  usePlayerDetails,
} from './PlayerProfileDialogs';
import {
  PlayerMatches,
  PlayerRatings,
  PlayerRatingTrends,
  type AnalyzeMatches,
} from './PlayerSummary';
import {
  loadVirtualPlayers,
  type VirtualPlayerProfile,
} from './virtual-player-api';
import { useVirtualPlayerText } from './virtual-player-messages';
import { VirtualPlayerCard, VirtualPlayerStrip } from './VirtualPlayerProfiles';
import './player-directory.css';

// The most active player leads the list, usually the owner of the library.
const DIRECTORY_ORDER: PlayerOrder = { sort: 'matches', direction: 'desc' };

type Selection = { kind: 'player' | 'virtual'; id: string };

function PlayerCard({
  id,
  databases,
  revision,
  onChanged,
  onOpen,
  onProfile,
  onVirtualProfile,
  onAnalyzeMatches,
}: {
  id: string;
  databases: Database[];
  revision: number;
  onChanged: () => void;
  onOpen: (match: Match) => void;
  onProfile: (id: string) => void;
  onVirtualProfile: (id: string) => void;
  onAnalyzeMatches?: AnalyzeMatches;
}) {
  const t = usePlayerText();
  const v = useVirtualPlayerText();
  const { formatError } = useI18n();
  const { details, setDetails, error, retry } = usePlayerDetails(id, revision);
  const [dialog, setDialog] = useState<'edit' | 'merge'>();
  const [matchesRevision, setMatchesRevision] = useState(0);
  const [merged, setMerged] = useState(false);

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

  const aliasNames = [
    ...new Set(details.aliases.map((alias) => alias.name)),
  ].filter((name) => name !== details.display_name);
  const group = details.virtual_profile;

  return (
    <section className="player-card" aria-label={details.display_name}>
      <header className="player-card-header">
        <div>
          <h2>{details.display_name}</h2>
          <p>
            {t('counts', {
              matches: details.match_count,
              games: details.game_count,
            })}
          </p>
          {aliasNames.length > 0 && (
            <p
              className="player-card-aliases"
              title={details.aliases
                .map((alias) => aliasLabel(alias))
                .join('\n')}
            >
              {t('aliases')}: {aliasNames.join(', ')}
            </p>
          )}
          {group && (
            <p className="player-card-group">
              {v('partOf')}:{' '}
              <button
                type="button"
                className="position-preview-link"
                onClick={() => onVirtualProfile(group.id)}
              >
                {group.name}
              </button>
            </p>
          )}
        </div>
        <div className="player-profile-actions">
          <ActionButton
            icon="edit"
            type="button"
            label={t('editShort')}
            aria-haspopup="dialog"
            onClick={() => setDialog('edit')}
          />
          <ActionButton
            icon="merge"
            type="button"
            label={t('mergeShort')}
            aria-haspopup="dialog"
            onClick={() => {
              setMerged(false);
              setDialog('merge');
            }}
          />
        </div>
      </header>
      {merged && (
        <p className="player-merge-status" role="status">
          {t('merged')}
        </p>
      )}
      {error !== undefined && (
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
      )}
      <PlayerRatings
        scope={{ player_id: id }}
        ratings={details.ratings}
        matchCount={details.rateable_match_count}
        onAnalyzeMatches={onAnalyzeMatches}
      />
      <PlayerMatches
        scope={{ player_id: id }}
        databases={databases}
        onOpen={onOpen}
        onProfile={onProfile}
        revision={revision + matchesRevision}
      />
      <PlayerRatingTrends
        ratings={details.ratings}
        history={details.rating_history ?? []}
      />
      {dialog === 'edit' && (
        <PlayerEditDialog
          profile={details}
          onClose={() => setDialog(undefined)}
          onChanged={(profile) => {
            setDetails(profile);
            onChanged();
          }}
        />
      )}
      {dialog === 'merge' && (
        <PlayerMergeDialog
          profile={details}
          revision={revision}
          onClose={() => setDialog(undefined)}
          onRefreshed={setDetails}
          onMerged={(profile) => {
            setDetails(profile);
            onChanged();
            setMatchesRevision((n) => n + 1);
            setMerged(true);
            setDialog(undefined);
          }}
        />
      )}
    </section>
  );
}

export default function PlayerDirectory({
  revision,
  onOpen,
  onAnalyzeMatches,
}: {
  revision: number;
  onOpen: (match: Match) => void;
  onAnalyzeMatches?: AnalyzeMatches;
}) {
  const t = usePlayerText();
  const { formatError, formatNumber } = useI18n();
  const [total, setTotal] = useState(0);
  const [search, setSearch] = useState('');
  const [selection, setSelection] = useState<Selection>();
  const [databases, setDatabases] = useState<Database[]>([]);
  const [error, setError] = useState<unknown>();
  const [listRevision, setListRevision] = useState(0);
  const [groups, setGroups] = useState<{
    items?: VirtualPlayerProfile[];
    error?: unknown;
  }>({});
  const [groupAttempt, setGroupAttempt] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    setError(undefined);
    request<Database[]>('/databases', { signal: abort.signal }).then(
      (items) => {
        if (!abort.signal.aborted) setDatabases(items);
      },
      (err: unknown) => {
        if (!abort.signal.aborted) setError(err);
      },
    );
    return () => abort.abort();
  }, [revision]);
  useEffect(() => {
    const abort = new AbortController();
    loadVirtualPlayers(abort.signal).then(
      (items) => {
        if (abort.signal.aborted) return;
        setGroups({ items });
        // "Me" opens first, before any real profile.
        const me = items.find((item) => item.is_me);
        setSelection(
          (current) => current ?? (me && { kind: 'virtual', id: me.id }),
        );
      },
      (err: unknown) => {
        if (!abort.signal.aborted)
          setGroups((current) => ({ items: current.items ?? [], error: err }));
      },
    );
    return () => abort.abort();
  }, [revision, listRevision, groupAttempt]);
  const changed = useCallback(() => setListRevision((n) => n + 1), []);
  const selectPlayer = useCallback(
    (id: string) => setSelection({ kind: 'player', id }),
    [],
  );
  const selectGroup = useCallback(
    (id: string) => setSelection({ kind: 'virtual', id }),
    [],
  );
  return (
    <section
      className="player-directory position-browser"
      aria-label={t('title')}
    >
      <div className="position-browser-heading">
        <h1>{t('title')}</h1>
      </div>
      <VirtualPlayerStrip
        items={groups.items}
        error={groups.error}
        onRetry={() => setGroupAttempt((n) => n + 1)}
        selected={selection?.kind === 'virtual' ? selection.id : undefined}
        onSelect={selectGroup}
        onCreated={(id) => {
          changed();
          selectGroup(id);
        }}
      />
      <label className="player-search">
        <input
          type="search"
          aria-label={t('search')}
          maxLength={240}
          placeholder={t('searchHint')}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </label>
      {error !== undefined && <p role="alert">{formatError(error)}</p>}
      <SearchResultList
        listClassName="player-list"
        title={t('player')}
        count={
          <span aria-label={t('playerCount', { count: total })}>
            {formatNumber(total)}
          </span>
        }
        preview={
          <SearchResultPreview
            className="player-preview"
            title={t('profile')}
            hint={t('choose')}
          >
            {selection?.kind === 'player' ? (
              <PlayerCard
                key={selection.id}
                id={selection.id}
                databases={databases}
                revision={revision + listRevision}
                onOpen={onOpen}
                onProfile={selectPlayer}
                onVirtualProfile={selectGroup}
                onAnalyzeMatches={onAnalyzeMatches}
                onChanged={changed}
              />
            ) : selection?.kind === 'virtual' ? (
              <VirtualPlayerCard
                key={selection.id}
                id={selection.id}
                databases={databases}
                revision={revision + listRevision}
                onOpen={onOpen}
                onProfile={selectPlayer}
                onAnalyzeMatches={onAnalyzeMatches}
                onChanged={changed}
                onDeleted={() => {
                  setSelection(undefined);
                  changed();
                }}
              />
            ) : null}
          </SearchResultPreview>
        }
      >
        <PlayerResults
          search={search}
          revision={revision + listRevision}
          selected={selection?.kind === 'player' ? selection.id : undefined}
          onSelect={selectPlayer}
          reportTotal={setTotal}
          initialOrder={DIRECTORY_ORDER}
          // Waits for the virtual profiles, so "me" can open first.
          selectFirst={groups.items !== undefined && !selection}
        />
      </SearchResultList>
    </section>
  );
}
