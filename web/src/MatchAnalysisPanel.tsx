import { useEffect, useState } from 'react';
import {
  equity,
  request,
  terminal,
  type Match,
  type MatchAnalysisSummary,
  type MoveAnalysis,
  type MoveAnalysisPage,
} from './api';
import { useMatchAnalysis } from './useMatchAnalysis';

const labels: Record<string, string> = {
  none: 'Gut',
  doubtful: 'Ungenau',
  bad: 'Fehler',
  very_bad: 'Grober Fehler',
};
const actions: Record<string, string> = {
  'No double': 'Nicht doppeln',
  'no double': 'Nicht doppeln',
  double: 'Doppeln',
  take: 'Annehmen',
  drop: 'Passen',
  'Cannot move': 'Aussetzen',
  Double: 'Doppeln',
  Take: 'Annehmen',
  Pass: 'Passen',
  Drop: 'Passen',
  dance: 'Aussetzen',
};
const notation = (value: string) => actions[value] || value;

export default function MatchAnalysisPanel({
  match,
  onPosition,
}: {
  match: Match;
  onPosition: (id: string) => void;
}) {
  const { job, error, submitting, run } = useMatchAnalysis(match.id);
  const [plies, setPlies] = useState(0);
  const [errorsOnly, setErrorsOnly] = useState(false);
  const [player, setPlayer] = useState('');
  const [kind, setKind] = useState('');
  const [rows, setRows] = useState<MoveAnalysis[]>([]);
  const [cursor, setCursor] = useState<number | null>(null);
  const [pageError, setPageError] = useState('');
  const [loading, setLoading] = useState(false);
  const [page, setPage] = useState<number | null>(null);
  const [version, setVersion] = useState(0);
  const active = job && !terminal(job);
  const summary = job?.result as MatchAnalysisSummary | undefined;
  const names = [match.white, match.black];
  const filter = `errors_only=${errorsOnly}${player ? `&player=${player}` : ''}${kind ? `&kind=${kind}` : ''}`;

  // A completed game changes the result set atomically. Start a fresh page chain.
  useEffect(() => {
    setPage(null);
    setRows([]);
    setVersion((n) => n + 1);
  }, [job?.id, job?.completed_games, filter]);
  useEffect(() => {
    if (!job?.id) return;
    const abort = new AbortController();
    setLoading(true);
    setPageError('');
    void request<MoveAnalysisPage>(
      `/jobs/${job.id}/moves?limit=50&${filter}${page ? `&after=${page}` : ''}`,
      { signal: abort.signal },
    )
      .then((result) => {
        if (abort.signal.aborted) return;
        setRows((previous) =>
          page ? [...previous, ...result.items] : result.items,
        );
        setCursor(result.next_cursor ?? null);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setPageError(String(error));
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [job?.id, filter, page, version]);

  return (
    <section className="match-analysis" aria-label="Vollständige Partieanalyse">
      <div className="match-analysis-heading">
        <div>
          <span className="eyebrow">DIE GANZE PARTIE VERSTEHEN</span>
          <h3>Partieanalyse</h3>
          <p>Alle Checker- und Cube-Entscheidungen mit GNUbg.</p>
        </div>
        <div className="match-analysis-controls">
          <label>
            Analysetiefe{' '}
            <select
              aria-label="Partie-Analysetiefe"
              value={plies}
              disabled={!!active || submitting}
              onChange={(event) => setPlies(Number(event.target.value))}
            >
              <option value={0}>0-Ply · Schnell</option>
              <option value={1}>1-Ply</option>
              <option value={2}>2-Ply · Gründlich</option>
            </select>
          </label>
          <button
            className="primary"
            onClick={() => void run(plies)}
            disabled={!!active || submitting}
          >
            {submitting
              ? 'Auftrag wird erstellt …'
              : active
                ? 'Analyse läuft …'
                : job
                  ? 'Partie erneut analysieren'
                  : 'Gesamte Partie analysieren'}
          </button>
        </div>
      </div>
      {error && (
        <p className="analysis-error" role="alert">
          {error}
        </p>
      )}
      {job && (
        <>
          <div
            className="match-analysis-progress"
            role="status"
            aria-live="polite"
          >
            <div>
              <strong>
                {job.status === 'completed'
                  ? 'Partieanalyse abgeschlossen'
                  : job.status === 'failed'
                    ? 'Partieanalyse fehlgeschlagen'
                    : job.status === 'queued'
                      ? 'In der Warteschlange'
                      : 'GNUbg analysiert'}
              </strong>
              <span>
                {job.completed_games} / {job.total_games} Spiele · {job.plies}
                -Ply · {job.progress}%
              </span>
            </div>
            <progress
              aria-label="Fortschritt der Partieanalyse"
              value={job.progress}
              max={100}
            />
            {active && (
              <small>
                Fortschritt und Ergebnisse werden nach jedem Spiel gespeichert.
                Du kannst die Partie währenddessen weiter ansehen.
              </small>
            )}
            {job.error && <p role="alert">{job.error}</p>}
          </div>
          {summary && (
            <>
              <div className="match-analysis-summary">
                {summary.players.map((stats) => (
                  <article key={stats.player}>
                    <h4>{names[stats.player]}</h4>
                    <div className="match-summary-numbers">
                      <span>
                        <b>{stats.doubtful + stats.bad + stats.very_bad}</b>{' '}
                        Auffälligkeiten
                      </span>
                      <span>
                        <b>
                          {(
                            stats.checker_equity_loss + stats.cube_equity_loss
                          ).toFixed(3)}
                        </b>{' '}
                        Equity-Verlust
                      </span>
                    </div>
                    <p>
                      {stats.checker_decisions} Züge · {stats.cube_decisions}{' '}
                      Cube-Entscheidungen
                    </p>
                    <small>
                      {stats.doubtful} ungenau · {stats.bad} Fehler ·{' '}
                      {stats.very_bad} grobe Fehler
                      {stats.unavailable
                        ? ` · ${stats.unavailable} nicht bewertet`
                        : ''}
                    </small>
                  </article>
                ))}
              </div>
              <p className="analysis-method">
                {summary.analyzed_moves} / {summary.total_moves} Züge
                ausgewertet. Equity-Verlust ist die Differenz zur besten
                Entscheidung, kein PR. Ungenau ab 0,04 · Fehler ab 0,08 · grober
                Fehler ab 0,16.
              </p>
            </>
          )}
          <details className="match-move-overview" open>
            <summary>Fehlerübersicht pro Zug</summary>
            <div className="match-analysis-filters">
              <label>
                <input
                  type="checkbox"
                  checked={errorsOnly}
                  onChange={(event) => setErrorsOnly(event.target.checked)}
                />{' '}
                Nur Auffälligkeiten (≥ 0,04)
              </label>
              <select
                aria-label="Analyse nach Spieler filtern"
                value={player}
                onChange={(event) => setPlayer(event.target.value)}
              >
                <option value="">Beide Spieler</option>
                <option value="0">{match.white}</option>
                <option value="1">{match.black}</option>
              </select>
              <select
                aria-label="Analyse nach Entscheidung filtern"
                value={kind}
                onChange={(event) => setKind(event.target.value)}
              >
                <option value="">Checker und Cube</option>
                <option value="checker">Checker</option>
                <option value="cube">Cube</option>
              </select>
            </div>
            {pageError && <p role="alert">{pageError}</p>}
            <div className="match-moves-scroll">
              <table className="match-moves-table">
                <thead>
                  <tr>
                    <th>Spiel · Zug</th>
                    <th>Spieler / Art</th>
                    <th>Gespielt</th>
                    <th>Beste Entscheidung</th>
                    <th>Verlust</th>
                    <th>Bewertung</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id}>
                      <td>
                        {row.position_uuid ? (
                          <button
                            className="position-link"
                            aria-label={`Spiel ${row.game_no}, Zug ${row.checker_no} auf dem Brett öffnen`}
                            onClick={() => onPosition(row.position_uuid!)}
                          >
                            {row.game_no} · {row.checker_no} ↗
                          </button>
                        ) : (
                          `${row.game_no} · Cube ${row.decision_no}`
                        )}
                      </td>
                      <td>
                        {names[row.player]}
                        <small>
                          {row.kind === 'checker'
                            ? `Checker · ${row.dice?.join('–')}`
                            : 'Cube'}
                        </small>
                      </td>
                      <td>
                        {notation(row.played)}
                        <small>
                          {row.played_equity != null
                            ? equity(row.played_equity)
                            : '—'}
                        </small>
                      </td>
                      <td>
                        {notation(row.best)}
                        <small>
                          {row.best_equity != null
                            ? equity(row.best_equity)
                            : '—'}
                        </small>
                      </td>
                      <td className="equity-loss">
                        {row.evaluation_status === 'unavailable'
                          ? '—'
                          : row.equity_loss.toFixed(3)}
                      </td>
                      <td>
                        <span className={`severity severity-${row.severity}`}>
                          {row.evaluation_status === 'forced'
                            ? 'Erzwungen'
                            : row.evaluation_status === 'unavailable'
                              ? 'Nicht bewertet'
                              : labels[row.severity] || row.severity}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {rows.length === 0 && !loading && (
              <p className="analysis-empty">
                {job.completed_games === 0
                  ? 'Die Zugübersicht erscheint nach dem ersten analysierten Spiel.'
                  : 'Keine Entscheidungen für diese Filter.'}
              </p>
            )}
            {loading && <p role="status">Zugübersicht wird geladen …</p>}
            {cursor && (
              <button onClick={() => setPage(cursor)} disabled={loading}>
                Weitere Entscheidungen laden
              </button>
            )}
          </details>
        </>
      )}
    </section>
  );
}
