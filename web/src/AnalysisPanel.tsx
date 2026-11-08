import { useState } from 'react';
import {
  equity,
  type Analysis,
  type Distribution,
  type DiceStats,
} from './api';
import type { Jobs } from './useJobs';

export default function AnalysisPanel({
  jobs,
  stats,
  disabled,
  onAnalyze,
}: {
  jobs: Jobs;
  stats: DiceStats | null;
  disabled: boolean;
  onAnalyze: (kind: 'analysis' | 'dice', plies: number) => void;
}) {
  const [plies, setPlies] = useState(0);
  const [view, setView] = useState<'analysis' | 'dice' | 'observed'>(
    'analysis',
  );
  const [selectedRoll, setSelectedRoll] = useState('1-1');
  const job =
    Object.values(jobs).find(
      (j) => j.status === 'queued' || j.status === 'running',
    ) || jobs[view === 'dice' ? 'dice' : 'analysis'];
  const pending =
    !!job && (job.status === 'queued' || job.status === 'running');
  const analysis =
    jobs.analysis?.status === 'completed'
      ? (jobs.analysis.result as Analysis)
      : null;
  const distribution =
    jobs.dice?.status === 'completed'
      ? (jobs.dice.result as Distribution)
      : null;
  const choose = (kind: 'analysis' | 'dice') => {
    setView(kind);
    onAnalyze(kind, plies);
  };
  const best = distribution?.rolls.find(
    (r) => r.dice.join('-') === selectedRoll,
  );
  return (
    <section className="analysis-panel">
      <div className="section-heading">
        <div>
          <span className="eyebrow">DEIN NÄCHSTER SCHRITT</span>
          <h2>Die Position verstehen</h2>
        </div>
        <span className="engine-label">GNUbg · lokal</span>
      </div>
      <div className="analysis-controls">
        <label className="depth-label">
          Suchtiefe{' '}
          <select
            aria-label="Suchtiefe"
            value={plies}
            onChange={(e) => setPlies(Number(e.target.value))}
          >
            <option value={0}>0-ply · schnell</option>
            <option value={1}>1-ply</option>
            <option value={2}>2-ply · genauer</option>
          </select>
        </label>
        <button
          className="primary"
          disabled={disabled || pending}
          onClick={() => choose('analysis')}
        >
          Position analysieren <span>↗</span>
        </button>
        <button disabled={disabled || pending} onClick={() => choose('dice')}>
          Dice Distribution
        </button>
      </div>
      {pending && (
        <div className="job-progress" role="status">
          <div>
            <span>
              {job.status === 'queued'
                ? 'Wartet auf GNUbg …'
                : job.kind === 'dice'
                  ? 'Würfelpaare werden berechnet …'
                  : 'Züge werden analysiert …'}
            </span>
            <b>{job.progress}%</b>
          </div>
          <progress max="100" value={job.progress} />
        </div>
      )}
      {job?.status === 'failed' && (
        <div className="error" role="alert">
          <strong>Analyse fehlgeschlagen.</strong> {job.error} Ein erneuter
          Start legt einen neuen Auftrag an.
        </div>
      )}
      <div className="tabs" role="tablist" aria-label="Auswertungen">
        <button
          role="tab"
          aria-selected={view === 'analysis'}
          onClick={() => setView('analysis')}
        >
          Zugkandidaten
        </button>
        <button
          role="tab"
          aria-selected={view === 'dice'}
          onClick={() => setView('dice')}
        >
          Würfel-Equity
        </button>
        <button
          role="tab"
          aria-selected={view === 'observed'}
          onClick={() => setView('observed')}
        >
          Partiewürfe
        </button>
      </div>
      {view === 'analysis' &&
        (analysis ? (
          <>
            <table className="candidates">
              <thead>
                <tr>
                  <th>Rang</th>
                  <th>Zug</th>
                  <th>Equity</th>
                  <th>Gewinn</th>
                </tr>
              </thead>
              <tbody>
                {analysis.candidates.map((c) => (
                  <tr key={c.rank} className={c.rank === 1 ? 'best-move' : ''}>
                    <td>{String(c.rank).padStart(2, '0')}</td>
                    <td>
                      {c.notation}
                      {c.rank === 1 && (
                        <span className="best-label">BESTER ZUG</span>
                      )}
                    </td>
                    <td>{equity(c.equity)}</td>
                    <td>{(c.probabilities[0] * 100).toFixed(1)}%</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="fineprint">
              {analysis.engine} · {analysis.plies}-ply · Cubeful Equity aus
              Sicht des Spielers am Zug.
            </p>
          </>
        ) : (
          <div className="panel-empty">
            <span className="empty-symbol">↗</span>
            <h3>Aus einem Zug wird eine Erkenntnis.</h3>
            <p>
              Starte eine Analyse und vergleiche die besten legalen Züge, ihre
              Equity und Gewinnchance.
            </p>
          </div>
        ))}
      {view === 'dice' &&
        (distribution ? (
          <div className="distribution">
            <div className="distribution-caption">
              <span>36 Würfe. 21 Kombinationen.</span>
              <strong>Ø {equity(distribution.mean_equity)}</strong>
            </div>
            <div className="dice-grid">
              {Array.from({ length: 36 }, (_, i) => {
                const a = Math.floor(i / 6) + 1,
                  b = (i % 6) + 1;
                const roll = distribution.rolls.find(
                  (r) =>
                    r.dice[0] === Math.min(a, b) &&
                    r.dice[1] === Math.max(a, b),
                );
                return (
                  <button
                    key={i}
                    title={`${a}–${b}: ${roll?.best.notation}`}
                    className={`${roll && roll.best.equity >= distribution.mean_equity ? 'above' : 'below'} ${selectedRoll === [Math.min(a, b), Math.max(a, b)].join('-') ? 'selected' : ''}`}
                    onClick={() =>
                      setSelectedRoll(
                        [Math.min(a, b), Math.max(a, b)].join('-'),
                      )
                    }
                  >
                    <span>
                      {a} · {b}
                    </span>
                    <b>{roll ? equity(roll.best.equity) : '—'}</b>
                  </button>
                );
              })}
            </div>
            {best && (
              <p className="roll-detail">
                <strong>{best.dice.join('–')}</strong>
                <span>{best.best.notation}</span>
                <b>{equity(best.best.equity)}</b>
                <small>
                  {(best.probability * 100).toFixed(2)}% Wahrscheinlichkeit
                </small>
              </p>
            )}
            <p className="fineprint">
              Grün: über der mittleren Equity. Pasch zählt 1/36, andere
              Kombinationen 2/36. {distribution.plies}-ply.
            </p>
          </div>
        ) : (
          <div className="panel-empty">
            <span className="empty-symbol">⚄</span>
            <h3>Was bringt der nächste Wurf?</h3>
            <p>
              „Dice Distribution“ berechnet für diese Position alle Würfelpaare
              mit GNUbg.
            </p>
          </div>
        ))}
      {view === 'observed' &&
        (stats ? (
          <div className="observed">
            <p className="fineprint">
              {stats.total} aufgezeichnete Würfe dieser Partie. Häufigkeiten
              sind keine Equity-Analyse; Eröffnungswürfe enthalten keinen Pasch.
            </p>
            <div className="roll-counts">
              {stats.rolls.map((r) => (
                <div key={r.dice.join('-')}>
                  <span>{r.dice.join('–')}</span>
                  <meter
                    value={r.count}
                    max={Math.max(1, ...stats.rolls.map((r) => r.count))}
                  />
                  <b>{r.count}</b>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="panel-empty">
            <p>Wähle eine Partie aus.</p>
          </div>
        ))}
    </section>
  );
}
