import { useEffect, useState } from 'react';
import Board from './Board';
import { request, type MatchGame } from './api';
import {
  loadDecisions,
  movePosition,
  type LibraryMatch,
  type GameDecisions,
  type StoredMove,
} from './library-api';
import type { Theme } from './themes';
import { useLibraryText } from './library-messages';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
import type { components } from './generated-api';
export default function MatchPreview({
  match,
  theme,
  onOpen,
  onEdit,
  onNotes,
}: {
  match?: LibraryMatch;
  theme: Theme;
  onOpen: (m: LibraryMatch, gameNo?: number) => void;
  onEdit: (m: LibraryMatch) => void;
  onNotes: (move: StoredMove, m: LibraryMatch) => void;
}) {
  const l = useLibraryText(),
    { formatError, formatDecision } = useI18n();
  const sourceT = useSourceText();
  const [sourceSummary, setSourceSummary] =
    useState<components['schemas']['ImportedAnalysisSummary']>();
  const [games, setGames] = useState<MatchGame[]>([]),
    [gameNo, setGameNo] = useState(1),
    [catalog, setCatalog] = useState<GameDecisions>(),
    [index, setIndex] = useState(0),
    [loading, setLoading] = useState(false),
    [error, setError] = useState<unknown>();
  useEffect(() => {
    setSourceSummary(undefined);
    setGames([]);
    setGameNo(1);
    setCatalog(undefined);
    setIndex(0);
    setError(undefined);
    if (!match) return;
    const abort = new AbortController();
    void request<components['schemas']['ImportedAnalysisSummary']>(
      `/matches/${match.id}/imported-analysis`,
      { signal: abort.signal },
    )
      .then((data) => {
        if (!abort.signal.aborted) setSourceSummary(data);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    void request<MatchGame[]>(`/matches/${match.id}/games`, {
      signal: abort.signal,
    })
      .then((items) => {
        if (abort.signal.aborted) return;
        setGames(items);
        setGameNo(items[0]?.game_no ?? 1);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => abort.abort();
  }, [match?.id]);
  useEffect(() => {
    setCatalog(undefined);
    setIndex(0);
    if (!match || !games.length) return;
    const abort = new AbortController();
    setLoading(true);
    setError(undefined);
    void loadDecisions(match.id, gameNo, abort.signal)
      .then((value) => {
        if (!abort.signal.aborted) setCatalog(value);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [match?.id, gameNo, games]);
  const move = catalog?.items[index];
  if (!match)
    return (
      <aside className="match-preview preview-empty">
        <h2>{l('preview')}</h2>
        <p>{l('previewHint')}</p>
      </aside>
    );
  return (
    <aside className="match-preview" aria-label={l('preview')}>
      <div className="section-heading">
        <h2>{match.display_name}</h2>
        <button onClick={() => onEdit(match)}>{l('editMatch')}</button>
      </div>
      {sourceSummary?.status === 'available' && (
        <p className="source-provenance">
          <strong>
            {sourceT('imported')}: {sourceSummary.engines.join(', ')}
          </strong>
          <small title={sourceSummary.depths.join(', ')}>
            {sourceSummary.depths.slice(0, 6).join(', ')}
            {sourceSummary.depths.length > 6 ? ' …' : ''}
          </small>
        </p>
      )}
      <p>
        {match.white} — {match.black}
      </p>
      <div className="preview-selectors">
        <label>
          {l('game')}
          <select
            value={gameNo}
            onChange={(e) => setGameNo(Number(e.target.value))}
          >
            {games.map((g) => (
              <option key={g.game_no} value={g.game_no}>
                {g.game_no} · {g.white_score}–{g.black_score}
              </option>
            ))}
          </select>
        </label>
        {catalog && catalog.items.length > 0 && (
          <label>
            {l('sourceMove')}
            <select
              value={index}
              onChange={(e) => setIndex(Number(e.target.value))}
            >
              {catalog.items.map((m, i) => (
                <option key={m.id} value={i}>
                  {m.move_no}. {formatDecision(m.notation)}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      {loading && <p role="status">{l('loading')}</p>}
      {move && (
        <>
          <Board
            position={movePosition(move)}
            white={match.white}
            black={match.black}
            theme={theme}
          />
          <div className="preview-playback">
            <button
              aria-label={l('previous')}
              disabled={index === 0}
              onClick={() => setIndex((i) => i - 1)}
            >
              ←
            </button>
            <span>
              {index + 1} / {catalog?.items.length} ·{' '}
              {formatDecision(move.notation)}
            </span>
            <button
              aria-label={l('next')}
              disabled={index + 1 === (catalog?.items.length ?? 0)}
              onClick={() => setIndex((i) => i + 1)}
            >
              →
            </button>
          </div>
        </>
      )}
      {catalog && !catalog.items.length && <p>{l('emptyGame')}</p>}
      <div className="annotation-tags">
        {match.tags.map((tag) => (
          <span key={tag} className="tag-chip">
            {tag}
          </span>
        ))}
      </div>
      {match.comment && <p className="database-comment">{match.comment}</p>}
      <div className="preview-actions">
        <button className="primary" onClick={() => onOpen(match, gameNo)}>
          {l('open')}
        </button>
        {move && (
          <button onClick={() => onNotes(move, match)}>{l('cubeNotes')}</button>
        )}
      </div>
    </aside>
  );
}
