import { useEffect, useRef, useState } from 'react';
import {
  request,
  type Match,
  type Position,
  type Page,
  type DiceStats,
} from './api';
import Board, { themes, type Theme } from './Board';
import AnalysisPanel from './AnalysisPanel';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import { useJobs } from './useJobs';
import AnnotationPanel from './AnnotationPanel';
import AnnotationSearch, { type AnnotationHit } from './AnnotationSearch';
import LearningWorkspace, { AddCardsPanel } from './LearningPanel';
import { useLearningText } from './learning-messages';
import { languages, useI18n, type Locale } from './i18n';
import { AppError } from './i18n-core';

export default function App() {
  const learnT = useLearningText();
  const [view, setView] = useState<'library' | 'learning'>('library');
  const [annotationRevision, setAnnotationRevision] = useState(0);
  const [deckRevision, setDeckRevision] = useState(0);
  const { t, locale, setLocale, formatNumber, formatDecision, formatError } =
    useI18n();
  const [matches, setMatches] = useState<Match[]>([]);
  const [matchCursor, setMatchCursor] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const [selectedMatch, setSelectedMatch] = useState<Match | null>(null);
  const [positions, setPositions] = useState<Position[]>([]);
  const [positionCursor, setPositionCursor] = useState<string | null>(null);
  const [index, setIndex] = useState(0);
  const [loadingPositions, setLoadingPositions] = useState(false);
  const [stats, setStats] = useState<DiceStats | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [notice, setNotice] = useState<{
    duplicate: boolean;
    count: number;
  } | null>(null);
  const [uploading, setUploading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [exactId, setExactId] = useState('');
  const [found, setFound] = useState<Page<Position> | null>(null);
  const [exactLoading, setExactLoading] = useState(false);
  const [exactError, setExactError] = useState<unknown>(null);
  const [health, setHealth] = useState<'checking' | 'ok' | 'degraded'>(
    'checking',
  );
  const [theme, setTheme] = useState<Theme>(() => {
    const stored = localStorage.getItem('bgtutor.theme');
    return themes.find((t) => t.id === stored)?.id || 'forest';
  });
  const uploadInput = useRef<HTMLInputElement>(null);
  const selection = useRef<string | undefined>(undefined);
  const searchVersion = useRef(0);
  const navigationVersion = useRef(0);
  const desiredPosition = useRef<string | undefined>(undefined);
  const position = positions[index];
  const jobs = useJobs(position?.id);
  function invalidateNavigation() {
    navigationVersion.current++;
    setLoadingPositions(false);
    return navigationVersion.current;
  }

  useEffect(() => {
    localStorage.setItem('bgtutor.theme', theme);
  }, [theme]);
  useEffect(() => {
    const abort = new AbortController();
    request<{ status: string }>('/health', { signal: abort.signal })
      .then((h) => setHealth(h.status === 'ok' ? 'ok' : 'degraded'))
      .catch(() => {
        if (!abort.signal.aborted) setHealth('degraded');
      });
    return () => abort.abort();
  }, []);
  useEffect(() => {
    const abort = new AbortController();
    const version = ++searchVersion.current;
    setSearching(true);
    setError(null);
    setMatchCursor(null);
    const timer = setTimeout(() => {
      if (query.trim().length > 0 && query.trim().length < 3) {
        setSearching(false);
        return;
      }
      request<Page<Match>>(`/matches?q=${encodeURIComponent(query.trim())}`, {
        signal: abort.signal,
      })
        .then((page) => {
          if (version !== searchVersion.current) return;
          setMatches(page.items);
          setMatchCursor(page.next_cursor);
        })
        .catch((error) => {
          if (!abort.signal.aborted) setError(error);
        })
        .finally(() => {
          if (!abort.signal.aborted) setSearching(false);
        });
    }, 250);
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [query, refresh]);

  useEffect(() => {
    const abort = new AbortController();
    selection.current = selectedMatch?.id;
    invalidateNavigation();
    setPositions([]);
    setIndex(0);
    setPositionCursor(null);
    setStats(null);
    if (!selectedMatch) return;
    setLoadingPositions(true);
    const id = selectedMatch.id;
    void (async () => {
      try {
        const all: Position[] = [];
        let cursor: string | null = null;
        do {
          const page: Page<Position> = await request(
            `/positions?match_id=${id}&limit=100${cursor ? `&after=${cursor}` : ''}`,
            { signal: abort.signal },
          );
          all.push(...page.items);
          cursor = page.next_cursor;
        } while (
          desiredPosition.current &&
          !all.some((p) => p.id === desiredPosition.current) &&
          cursor
        );
        if (abort.signal.aborted) return;
        const target = desiredPosition.current;
        desiredPosition.current = undefined;
        setPositions(all);
        setPositionCursor(cursor);
        setIndex(
          Math.max(
            0,
            all.findIndex((p) => p.id === target),
          ),
        );
        setStats(
          await request<DiceStats>(`/matches/${id}/dice`, {
            signal: abort.signal,
          }),
        );
      } catch (error) {
        if (!abort.signal.aborted) setError(error);
      } finally {
        if (!abort.signal.aborted) setLoadingPositions(false);
      }
    })();
    return () => abort.abort();
  }, [selectedMatch]);

  async function upload(file: File | undefined) {
    if (!file) return;
    if (file.size > 16 * 1024 * 1024) {
      setError(new AppError('error.fileSize'));
      return;
    }
    setUploading(true);
    setError(null);
    setNotice(null);
    try {
      const body = new FormData();
      body.append('file', file);
      const imported = await request<{ match: Match; duplicate: boolean }>(
        '/imports',
        { method: 'POST', body },
      );
      setQuery('');
      setRefresh((r) => r + 1);
      setSelectedMatch(imported.match);
      setFound(null);
      setNotice({
        duplicate: imported.duplicate,
        count: imported.match.position_count,
      });
    } catch (error) {
      setError(error);
    } finally {
      setUploading(false);
      if (uploadInput.current) uploadInput.current.value = '';
    }
  }
  async function loadMatches() {
    if (!matchCursor) return;
    const version = searchVersion.current;
    setSearching(true);
    try {
      const page = await request<Page<Match>>(
        `/matches?q=${encodeURIComponent(query.trim())}&after=${matchCursor}`,
      );
      if (version !== searchVersion.current) return;
      setMatches((previous) => [...previous, ...page.items]);
      setMatchCursor(page.next_cursor);
    } catch (error) {
      setError(error);
    } finally {
      if (version === searchVersion.current) setSearching(false);
    }
  }
  async function nextPosition(loadMore = false) {
    const version = invalidateNavigation();
    if (!loadMore && index < positions.length - 1) {
      setIndex(index + 1);
      return;
    }
    if (!positionCursor || !selectedMatch) return;
    const id = selectedMatch.id;
    setLoadingPositions(true);
    try {
      const page = await request<Page<Position>>(
        `/positions?match_id=${id}&limit=100&after=${positionCursor}`,
      );
      if (selection.current !== id || navigationVersion.current !== version)
        return;
      const firstNew = positions.length;
      setPositions((previous) => [...previous, ...page.items]);
      setPositionCursor(page.next_cursor);
      setIndex(firstNew);
    } catch (error) {
      if (navigationVersion.current === version) setError(error);
    } finally {
      if (selection.current === id && navigationVersion.current === version)
        setLoadingPositions(false);
    }
  }
  async function searchPosition(more = false) {
    setExactLoading(true);
    setExactError(null);
    try {
      const page = await request<Page<Position>>(
        `/positions?position_id=${encodeURIComponent(exactId.trim())}&limit=20${more && found?.next_cursor ? `&after=${found.next_cursor}` : ''}`,
      );
      setFound(
        more && found
          ? { ...page, items: [...found.items, ...page.items] }
          : page,
      );
    } catch (error) {
      setExactError(error);
    } finally {
      setExactLoading(false);
    }
  }
  async function openFound(p: Position) {
    const version = invalidateNavigation();
    setLoadingPositions(false);
    try {
      if (
        selectedMatch?.id === p.match_uuid &&
        positions.some((v) => v.id === p.id)
      ) {
        setIndex(positions.findIndex((v) => v.id === p.id));
        return;
      }
      const match = await request<Match>(`/matches/${p.match_uuid}`);
      if (navigationVersion.current !== version) return;
      desiredPosition.current = p.id;
      setSelectedMatch(match);
    } catch (error) {
      if (navigationVersion.current === version) setError(error);
    }
  }

  async function openAnalyzedPosition(id: string) {
    if (!selectedMatch) return;
    const matchId = selectedMatch.id;
    const version = invalidateNavigation();
    setLoadingPositions(true);
    try {
      const all = [...positions];
      let cursor = positionCursor;
      while (!all.some((p) => p.id === id) && cursor) {
        const next: Page<Position> = await request(
          `/positions?match_id=${matchId}&limit=100&after=${cursor}`,
        );
        if (
          selection.current !== matchId ||
          navigationVersion.current !== version
        )
          return;
        all.push(...next.items);
        cursor = next.next_cursor;
      }
      if (
        selection.current !== matchId ||
        navigationVersion.current !== version
      )
        return;
      const target = all.findIndex((p) => p.id === id);
      if (target < 0) throw new AppError('error.positionMissing');
      setPositions(all);
      setPositionCursor(cursor);
      setIndex(target);
      document
        .querySelector('.board-panel')
        ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    } catch (error) {
      if (
        selection.current === matchId &&
        navigationVersion.current === version
      )
        setError(error);
    } finally {
      if (
        selection.current === matchId &&
        navigationVersion.current === version
      )
        setLoadingPositions(false);
    }
  }

  async function openAnnotation(hit: AnnotationHit) {
    setLoadingPositions(false);
    setView('library');
    const version = invalidateNavigation();
    if (
      hit.target_kind === 'position' &&
      selectedMatch?.id === hit.match_uuid &&
      positions.some((p) => p.id === hit.target_id)
    ) {
      setIndex(positions.findIndex((p) => p.id === hit.target_id));
      document
        .querySelector('.board-panel')
        ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      return;
    }
    try {
      const match = await request<Match>(`/matches/${hit.match_uuid}`);
      if (navigationVersion.current !== version) return;
      desiredPosition.current =
        hit.target_kind === 'position' ? hit.target_id : undefined;
      setSelectedMatch(match);
    } catch (error) {
      if (navigationVersion.current === version) setError(error);
    }
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <a className="brand" href="/" aria-label={t('app.home')}>
          <span className="brand-mark">
            b<span>g</span>
          </span>
          <span>
            BG Tutor<small>{t('app.tagline')}</small>
          </span>
        </a>
        <div className="library-heading">
          <span className="eyebrow">{t('app.libraryEyebrow')}</span>
          <span className="small-badge">{t('app.localBadge')}</span>
        </div>
        <button
          className="upload-button"
          onClick={() => uploadInput.current?.click()}
          disabled={uploading}
        >
          {uploading ? t('app.importing') : t('app.import')}
          <small>.mat · .xg · .sgf</small>
        </button>
        <input
          ref={uploadInput}
          className="sr-only"
          type="file"
          aria-label={t('app.matchFile')}
          accept=".mat,.sgf,.xg"
          onChange={(e) => void upload(e.target.files?.[0])}
        />
        <label className="search-box">
          <span>⌕</span>
          <input
            aria-label={t('app.searchMatches')}
            placeholder={t('app.searchPlaceholder')}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        {query.trim().length > 0 && query.trim().length < 3 && (
          <p className="search-hint">{t('app.searchMin')}</p>
        )}
        <div className="library-count">
          <span>{t('app.matches')}</span>
          <span>
            {searching
              ? t('app.searching')
              : `${formatNumber(matches.length)}${matchCursor ? '+' : ''}`}
          </span>
        </div>
        <nav className="match-list" aria-label={t('app.matches')}>
          {matches.map((m) => (
            <button
              key={m.id}
              className={`match-card ${m.id === selectedMatch?.id ? 'active' : ''}`}
              onClick={() => {
                invalidateNavigation();
                desiredPosition.current = undefined;
                setView('library');
                setSelectedMatch(m);
              }}
            >
              <span className="match-players">
                {m.white}
                <span className="versus">{t('common.versus')}</span>
                {m.black}
              </span>
              <span className="match-meta">
                {m.match_length
                  ? t('board.points', { count: m.match_length })
                  : t('common.moneyGame')}
                <span>·</span>
                {t('app.moves', { count: m.position_count })}
              </span>
              <small title={m.filename}>{m.filename}</small>
            </button>
          ))}
          {!matches.length && !searching && (
            <div className="library-empty">
              {query ? t('app.noMatches') : t('app.libraryEmpty')}
            </div>
          )}
        </nav>
        {matchCursor && (
          <button
            className="load-more"
            disabled={searching}
            onClick={() => void loadMatches()}
          >
            {t('app.moreMatches')}
          </button>
        )}
        <div className="sidebar-footer">
          <span className={`status-dot ${health === 'ok' ? 'ready' : ''}`} />
          <span>
            {health === 'ok'
              ? t('app.engineConnected')
              : health === 'checking'
                ? t('app.healthChecking')
                : t('app.healthUnavailable')}
          </span>
          <a href="/api/openapi.json" target="_blank" rel="noreferrer">
            API ↗
          </a>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <div className="breadcrumb">
            {t('app.library')} <span>/</span>{' '}
            <strong>{t('app.workspace')}</strong>
          </div>
          <span className="privacy-note">{t('app.privacy')}</span>
        </header>
        <div className="workspace">
          {!!error && (
            <div className="error" role="alert">
              {formatError(error)}
              <button
                aria-label={t('app.closeError')}
                onClick={() => setError(null)}
              >
                ×
              </button>
            </div>
          )}
          {notice && (
            <div className="notice" role="status">
              ✓{' '}
              {notice.duplicate
                ? t('app.importDuplicate')
                : t('app.importCount', { count: notice.count })}
              <button
                aria-label={t('app.closeNotice')}
                onClick={() => setNotice(null)}
              >
                ×
              </button>
            </div>
          )}
          <div className="workspace-title">
            <div>
              <span className="eyebrow">{t('app.eyebrow')}</span>
              <h1>
                {selectedMatch ? t('app.selectedTitle') : t('app.welcomeTitle')}
              </h1>
            </div>
            <div className="workspace-settings">
              <label className="theme-picker">
                {t('common.board')}{' '}
                <select
                  aria-label={t('app.themeLabel')}
                  value={theme}
                  onChange={(e) => setTheme(e.target.value as Theme)}
                >
                  {themes.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.name}
                    </option>
                  ))}
                </select>
              </label>
              <label className="theme-picker language-picker">
                {t('common.language')}{' '}
                <select
                  aria-label={t('common.language')}
                  value={locale}
                  onChange={(event) => setLocale(event.target.value as Locale)}
                >
                  {languages.map((language) => (
                    <option key={language.id} value={language.id}>
                      {language.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </div>
          <nav className="workspace-views" aria-label={learnT('study')}>
            <button
              aria-pressed={view === 'library'}
              onClick={() => {
                invalidateNavigation();
                setView('library');
              }}
            >
              {learnT('library')}
            </button>
            <button
              aria-pressed={view === 'learning'}
              onClick={() => {
                invalidateNavigation();
                setView('learning');
              }}
            >
              {learnT('study')}
            </button>
          </nav>
          {view === 'learning' ? (
            <LearningWorkspace
              theme={theme}
              revision={deckRevision}
              onChanged={() => setDeckRevision((n) => n + 1)}
            />
          ) : (
            <>
              <AnnotationSearch
                revision={annotationRevision}
                onOpen={(hit) => void openAnnotation(hit)}
              />
              <details className="position-search">
                <summary>
                  ⌕ {t('app.findPosition')} <span>{t('app.positionId')}</span>
                </summary>
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    void searchPosition();
                  }}
                >
                  <input
                    aria-label={t('app.positionId')}
                    placeholder={t('app.positionPlaceholder')}
                    value={exactId}
                    onChange={(e) => {
                      setExactId(e.target.value);
                      setFound(null);
                    }}
                  />
                  <button disabled={exactLoading || !exactId.trim()}>
                    {t('app.exactSearch')}
                  </button>
                  {position && (
                    <button
                      type="button"
                      onClick={() => {
                        setExactId(position.position_id);
                        setFound(null);
                      }}
                    >
                      {t('app.usePosition')}
                    </button>
                  )}
                </form>
                {!!exactError && (
                  <p role="alert" className="search-hint">
                    {formatError(exactError)}
                  </p>
                )}
                {found && (
                  <div className="position-results">
                    {found.items.length === 0 ? (
                      <p>{t('app.noPositionMatches')}</p>
                    ) : (
                      found.items.map((p) => (
                        <button key={p.id} onClick={() => void openFound(p)}>
                          {t('app.positionResult', {
                            game: p.game_no,
                            move: p.ply,
                            dice: p.dice.join('–'),
                          })}{' '}
                          <strong>{formatDecision(p.notation)}</strong>
                          <small>{p.match_uuid.slice(0, 8)}…</small>
                        </button>
                      ))
                    )}
                    {found.next_cursor && (
                      <button
                        onClick={() => void searchPosition(true)}
                        disabled={exactLoading}
                      >
                        {t('app.moreResults')}
                      </button>
                    )}
                  </div>
                )}
              </details>
              {!selectedMatch ? (
                <section
                  className="welcome"
                  onDragOver={(e) => e.preventDefault()}
                  onDrop={(e) => {
                    e.preventDefault();
                    if (!uploading) void upload(e.dataTransfer.files[0]);
                  }}
                >
                  <div className="welcome-board" aria-hidden="true">
                    <span>▲▼▲▼▲▼</span>
                    <div>
                      ● ● <b>⚄ ⚂</b> ○ ○
                    </div>
                    <span>▼▲▼▲▼▲</span>
                  </div>
                  <span className="eyebrow">{t('app.welcomeEyebrow')}</span>
                  <h2>{t('app.welcomeHeading')}</h2>
                  <p>{t('app.welcomeDescription')}</p>
                  <button
                    className="primary"
                    onClick={() => uploadInput.current?.click()}
                    disabled={uploading}
                  >
                    {uploading ? t('app.importRunning') : t('app.importFirst')}{' '}
                    <span>↗</span>
                  </button>
                  <small>{t('app.dropFile')}</small>
                  <div className="welcome-features">
                    <span>
                      <b>01</b> {t('app.collectMatches')}
                    </span>
                    <span>
                      <b>02</b> {t('app.findPositions')}
                    </span>
                    <span>
                      <b>03</b> {t('app.understandMoves')}
                    </span>
                  </div>
                </section>
              ) : (
                <>
                  <div className="match-title">
                    <h2>
                      {selectedMatch.white} <span>{t('common.versus')}</span>{' '}
                      {selectedMatch.black}
                    </h2>
                    <span>
                      {selectedMatch.match_length
                        ? t('app.matchLength', {
                            count: selectedMatch.match_length,
                          })
                        : t('common.moneyGame')}{' '}
                      <b>·</b> {selectedMatch.format.toUpperCase()}{' '}
                      <a href={`/api/matches/${selectedMatch.id}/source`}>
                        SGF ↓
                      </a>
                    </span>
                  </div>
                  <AnnotationPanel
                    kind="match"
                    id={selectedMatch.id}
                    onSaved={() => {
                      setAnnotationRevision((n) => n + 1);
                      setRefresh((n) => n + 1);
                    }}
                  />
                  <div className="play-layout">
                    <section className="board-panel">
                      {position ? (
                        <>
                          <div className="board-heading">
                            <span>
                              {t('app.gameMove', {
                                game: position.game_no,
                                move: position.ply,
                              })}
                            </span>
                            <strong>
                              {t('app.toMove', {
                                player:
                                  position.player === 0
                                    ? selectedMatch.white
                                    : selectedMatch.black,
                              })}
                            </strong>
                          </div>
                          <Board
                            position={position}
                            white={selectedMatch.white}
                            black={selectedMatch.black}
                            theme={theme}
                          />
                          <div className="board-navigation">
                            <button
                              aria-label={t('app.previousMove')}
                              disabled={index === 0 || loadingPositions}
                              onClick={() => {
                                invalidateNavigation();
                                setIndex(index - 1);
                              }}
                            >
                              ←
                            </button>
                            <span>
                              {t('common.played')}{' '}
                              <strong>
                                {formatDecision(position.notation)}
                              </strong>
                              <small>
                                {formatNumber(position.ordinal + 1)} /{' '}
                                {formatNumber(selectedMatch.position_count)}
                              </small>
                            </span>
                            <button
                              aria-label={t('app.nextMove')}
                              disabled={
                                loadingPositions ||
                                (index === positions.length - 1 &&
                                  !positionCursor)
                              }
                              onClick={() => void nextPosition()}
                            >
                              →
                            </button>
                          </div>
                          <div className="position-code">
                            <span>{t('app.positionCode')}</span>
                            <code>{position.position_id}</code>
                            <button
                              title={t('app.prepareSearch')}
                              onClick={() => {
                                setExactId(position.position_id);
                                setFound(null);
                                const details =
                                  document.querySelector<HTMLDetailsElement>(
                                    '.position-search',
                                  );
                                if (details) details.open = true;
                              }}
                            >
                              ⌕
                            </button>
                          </div>
                        </>
                      ) : (
                        <div className="board-loading" role="status">
                          {loadingPositions
                            ? t('app.positionsLoading')
                            : t('app.noPositions')}
                        </div>
                      )}
                    </section>
                    <aside className="moves-panel">
                      <div className="section-heading">
                        <h3>{t('app.moveHistory')}</h3>
                        <span>
                          {formatNumber(selectedMatch.position_count)}
                        </span>
                      </div>
                      <div className="moves-list">
                        {positions.map((p, i) => (
                          <button
                            key={p.id}
                            className={i === index ? 'current' : ''}
                            onClick={() => {
                              invalidateNavigation();
                              setLoadingPositions(false);
                              setIndex(i);
                            }}
                          >
                            <span className="move-no">
                              {p.game_no}.{p.ply}
                            </span>
                            <i
                              className={`player-dot ${p.player === 0 ? 'white' : 'black'}`}
                            />
                            <span className="move-dice">
                              {p.dice.join('–')}
                            </span>
                            <strong>{formatDecision(p.notation)}</strong>
                          </button>
                        ))}
                      </div>
                      {positionCursor && (
                        <button
                          disabled={loadingPositions}
                          onClick={() => void nextPosition(true)}
                        >
                          {t('app.moreHistory')}
                        </button>
                      )}
                    </aside>
                  </div>
                  {position && (
                    <AnnotationPanel
                      kind="position"
                      id={position.id}
                      onSaved={() => {
                        setAnnotationRevision((n) => n + 1);
                        setRefresh((n) => n + 1);
                      }}
                    />
                  )}
                  <AddCardsPanel
                    positionId={position?.id}
                    matchId={selectedMatch.id}
                    revision={deckRevision}
                    onChanged={() => setDeckRevision((n) => n + 1)}
                  />
                  <MatchAnalysisPanel
                    key={selectedMatch.id}
                    match={selectedMatch}
                    onPosition={(id) => void openAnalyzedPosition(id)}
                  />
                  {!!jobs.error && (
                    <div className="error" role="alert">
                      {formatError(jobs.error)}
                    </div>
                  )}
                  <AnalysisPanel
                    jobs={jobs.jobs}
                    stats={stats}
                    disabled={!position || jobs.submitting}
                    onAnalyze={(kind, plies) => void jobs.run(kind, plies)}
                  />
                </>
              )}
            </>
          )}
          <footer className="workspace-footer">
            <span>
              BG TUTOR <b>·</b> {t('app.footer')}
            </span>
            <span>{t('app.engineVersion')}</span>
          </footer>
        </div>
      </main>
    </div>
  );
}
