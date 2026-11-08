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

export default function App() {
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
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [uploading, setUploading] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [exactId, setExactId] = useState('');
  const [found, setFound] = useState<Page<Position> | null>(null);
  const [exactLoading, setExactLoading] = useState(false);
  const [exactError, setExactError] = useState('');
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
    setError('');
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
          if (!abort.signal.aborted) setError(String(error));
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
    navigationVersion.current++;
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
        if (!abort.signal.aborted) setError(String(error));
      } finally {
        if (!abort.signal.aborted) setLoadingPositions(false);
      }
    })();
    return () => abort.abort();
  }, [selectedMatch]);

  async function upload(file: File | undefined) {
    if (!file) return;
    if (file.size > 16 * 1024 * 1024) {
      setError('Die Datei darf höchstens 16 MiB groß sein.');
      return;
    }
    setUploading(true);
    setError('');
    setNotice('');
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
      setNotice(
        imported.duplicate
          ? 'Diese Datei ist bereits in deiner Bibliothek.'
          : `${imported.match.position_count} Positionen importiert.`,
      );
    } catch (error) {
      setError(String(error));
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
      setError(String(error));
    } finally {
      if (version === searchVersion.current) setSearching(false);
    }
  }
  async function nextPosition(loadMore = false) {
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
      if (selection.current !== id) return;
      const firstNew = positions.length;
      setPositions((previous) => [...previous, ...page.items]);
      setPositionCursor(page.next_cursor);
      setIndex(firstNew);
    } catch (error) {
      setError(String(error));
    } finally {
      if (selection.current === id) setLoadingPositions(false);
    }
  }
  async function searchPosition(more = false) {
    setExactLoading(true);
    setExactError('');
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
      setExactError(String(error));
    } finally {
      setExactLoading(false);
    }
  }
  async function openFound(p: Position) {
    navigationVersion.current++;
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
      desiredPosition.current = p.id;
      setSelectedMatch(match);
    } catch (error) {
      setError(String(error));
    }
  }

  async function openAnalyzedPosition(id: string) {
    if (!selectedMatch) return;
    const matchId = selectedMatch.id;
    const version = ++navigationVersion.current;
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
      if (target < 0)
        throw new Error('Die analysierte Position wurde nicht gefunden.');
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
        setError(String(error));
    } finally {
      if (
        selection.current === matchId &&
        navigationVersion.current === version
      )
        setLoadingPositions(false);
    }
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <a className="brand" href="/" aria-label="BG Tutor Startseite">
          <span className="brand-mark">
            b<span>g</span>
          </span>
          <span>
            BG Tutor<small>EVERY MOVE, UNDERSTOOD.</small>
          </span>
        </a>
        <div className="library-heading">
          <span className="eyebrow">DEINE BIBLIOTHEK</span>
          <span className="small-badge">LOKAL</span>
        </div>
        <button
          className="upload-button"
          onClick={() => uploadInput.current?.click()}
          disabled={uploading}
        >
          {uploading ? 'Partie wird importiert …' : '+  Partie importieren'}
          <small>.mat · .xg · .sgf</small>
        </button>
        <input
          ref={uploadInput}
          className="sr-only"
          type="file"
          aria-label="Partiedatei"
          accept=".mat,.sgf,.xg"
          onChange={(e) => void upload(e.target.files?.[0])}
        />
        <label className="search-box">
          <span>⌕</span>
          <input
            aria-label="Partien durchsuchen"
            placeholder="Spieler oder Datei suchen"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        {query.trim().length > 0 && query.trim().length < 3 && (
          <p className="search-hint">Mindestens 3 Zeichen eingeben.</p>
        )}
        <div className="library-count">
          <span>Partien</span>
          <span>
            {searching
              ? 'Suche …'
              : `${matches.length}${matchCursor ? '+' : ''}`}
          </span>
        </div>
        <nav className="match-list" aria-label="Partien">
          {matches.map((m) => (
            <button
              key={m.id}
              className={`match-card ${m.id === selectedMatch?.id ? 'active' : ''}`}
              onClick={() => {
                desiredPosition.current = undefined;
                setSelectedMatch(m);
              }}
            >
              <span className="match-players">
                {m.white}
                <span className="versus">vs.</span>
                {m.black}
              </span>
              <span className="match-meta">
                {m.match_length ? `${m.match_length} Punkte` : 'Money Game'}
                <span>·</span>
                {m.position_count} Züge
              </span>
              <small title={m.filename}>{m.filename}</small>
            </button>
          ))}
          {!matches.length && !searching && (
            <div className="library-empty">
              {query
                ? 'Keine passenden Partien.'
                : 'Deine nächste Erkenntnis beginnt mit einer Partie.'}
            </div>
          )}
        </nav>
        {matchCursor && (
          <button
            className="load-more"
            disabled={searching}
            onClick={() => void loadMatches()}
          >
            Weitere Partien laden
          </button>
        )}
        <div className="sidebar-footer">
          <span className={`status-dot ${health === 'ok' ? 'ready' : ''}`} />
          <span>
            {health === 'ok'
              ? 'GNUbg verbunden'
              : health === 'checking'
                ? 'Backend wird geprüft'
                : 'Backend / GNUbg nicht bereit'}
          </span>
          <a href="/api/openapi.json" target="_blank" rel="noreferrer">
            API ↗
          </a>
        </div>
      </aside>
      <main>
        <header className="topbar">
          <div className="breadcrumb">
            Bibliothek <span>/</span> <strong>Analyse-Workspace</strong>
          </div>
          <span className="privacy-note">
            ↳ Alles bleibt auf deinem Rechner
          </span>
        </header>
        <div className="workspace">
          {error && (
            <div className="error" role="alert">
              {error}
              <button
                aria-label="Fehlermeldung schließen"
                onClick={() => setError('')}
              >
                ×
              </button>
            </div>
          )}
          {notice && (
            <div className="notice" role="status">
              ✓ {notice}
              <button
                aria-label="Hinweis schließen"
                onClick={() => setNotice('')}
              >
                ×
              </button>
            </div>
          )}
          <div className="workspace-title">
            <div>
              <span className="eyebrow">BACKGAMMON, EINEN ZUG WEITER</span>
              <h1>
                {selectedMatch
                  ? 'Kleine Züge. Große Unterschiede.'
                  : 'Dein Spiel. Dein nächster Schritt.'}
              </h1>
            </div>
            <label className="theme-picker">
              Brett{' '}
              <select
                aria-label="Board-Theme"
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
          </div>
          <details className="position-search">
            <summary>
              ⌕ Position in der Bibliothek finden <span>GNUbg Position-ID</span>
            </summary>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void searchPosition();
              }}
            >
              <input
                aria-label="GNUbg Position-ID"
                placeholder="z. B. 4HPwATDgc/ABMA"
                value={exactId}
                onChange={(e) => {
                  setExactId(e.target.value);
                  setFound(null);
                }}
              />
              <button disabled={exactLoading || !exactId.trim()}>
                Exakt suchen
              </button>
              {position && (
                <button
                  type="button"
                  onClick={() => {
                    setExactId(position.position_id);
                    setFound(null);
                  }}
                >
                  Aktuelle Position übernehmen
                </button>
              )}
            </form>
            {exactError && (
              <p role="alert" className="search-hint">
                {exactError}
              </p>
            )}
            {found && (
              <div className="position-results">
                {found.items.length === 0 ? (
                  <p>Keine identische Brettstellung gefunden.</p>
                ) : (
                  found.items.map((p) => (
                    <button key={p.id} onClick={() => void openFound(p)}>
                      Spiel {p.game_no} · Zug {p.ply} · {p.dice.join('–')}{' '}
                      <strong>{p.notation}</strong>
                      <small>{p.match_uuid.slice(0, 8)}…</small>
                    </button>
                  ))
                )}
                {found.next_cursor && (
                  <button
                    onClick={() => void searchPosition(true)}
                    disabled={exactLoading}
                  >
                    Weitere Treffer
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
              <span className="eyebrow">DEIN PERSÖNLICHES ANALYSEBRETT</span>
              <h2>Jede Partie erzählt dir etwas.</h2>
              <p>
                Importiere eine Partie, entdecke entscheidende Positionen und
                finde mit GNUbg heraus, was in deinem nächsten Zug steckt.
              </p>
              <button
                className="primary"
                onClick={() => uploadInput.current?.click()}
                disabled={uploading}
              >
                {uploading ? 'Import läuft …' : 'Erste Partie importieren'}{' '}
                <span>↗</span>
              </button>
              <small>Oder eine MAT-, XG- oder SGF-Datei hier ablegen.</small>
              <div className="welcome-features">
                <span>
                  <b>01</b> Partien sammeln
                </span>
                <span>
                  <b>02</b> Positionen finden
                </span>
                <span>
                  <b>03</b> Züge verstehen
                </span>
              </div>
            </section>
          ) : (
            <>
              <div className="match-title">
                <h2>
                  {selectedMatch.white} <span>vs.</span> {selectedMatch.black}
                </h2>
                <span>
                  {selectedMatch.match_length
                    ? `${selectedMatch.match_length}-Punkte-Match`
                    : 'Money Game'}{' '}
                  <b>·</b> {selectedMatch.format.toUpperCase()}{' '}
                  <a href={`/api/matches/${selectedMatch.id}/source`}>SGF ↓</a>
                </span>
              </div>
              <div className="play-layout">
                <section className="board-panel">
                  {position ? (
                    <>
                      <div className="board-heading">
                        <span>
                          Spiel {position.game_no} <b>·</b> Zug {position.ply}
                        </span>
                        <strong>
                          {position.player === 0
                            ? selectedMatch.white
                            : selectedMatch.black}{' '}
                          am Zug
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
                          aria-label="Vorheriger Zug"
                          disabled={index === 0 || loadingPositions}
                          onClick={() => setIndex(index - 1)}
                        >
                          ←
                        </button>
                        <span>
                          Gespielt <strong>{position.notation}</strong>
                          <small>
                            {position.ordinal + 1} /{' '}
                            {selectedMatch.position_count}
                          </small>
                        </span>
                        <button
                          aria-label="Nächster Zug"
                          disabled={
                            loadingPositions ||
                            (index === positions.length - 1 && !positionCursor)
                          }
                          onClick={() => void nextPosition()}
                        >
                          →
                        </button>
                      </div>
                      <div className="position-code">
                        <span>POSITION-ID</span>
                        <code>{position.position_id}</code>
                        <button
                          title="Positionssuche vorbereiten"
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
                        ? 'Positionen werden geladen …'
                        : 'Keine Positionen vorhanden.'}
                    </div>
                  )}
                </section>
                <aside className="moves-panel">
                  <div className="section-heading">
                    <h3>Zugverlauf</h3>
                    <span>{selectedMatch.position_count}</span>
                  </div>
                  <div className="moves-list">
                    {positions.map((p, i) => (
                      <button
                        key={p.id}
                        className={i === index ? 'current' : ''}
                        onClick={() => {
                          navigationVersion.current++;
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
                        <span className="move-dice">{p.dice.join('–')}</span>
                        <strong>{p.notation}</strong>
                      </button>
                    ))}
                  </div>
                  {positionCursor && (
                    <button
                      disabled={loadingPositions}
                      onClick={() => void nextPosition(true)}
                    >
                      Weiter im Verlauf →
                    </button>
                  )}
                </aside>
              </div>
              <MatchAnalysisPanel
                key={selectedMatch.id}
                match={selectedMatch}
                onPosition={(id) => void openAnalyzedPosition(id)}
              />
              {jobs.error && (
                <div className="error" role="alert">
                  {jobs.error}
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
          <footer className="workspace-footer">
            <span>
              BG TUTOR <b>·</b> Lern aus jeder Position.
            </span>
            <span>Headless Engine / v0.1</span>
          </footer>
        </div>
      </main>
    </div>
  );
}
