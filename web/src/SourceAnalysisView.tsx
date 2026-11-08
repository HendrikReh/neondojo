import { useEffect, useState } from 'react';
import { useI18n } from './i18n';
import XgPositionView from './XgPositionView';
import { useBoardReviewText } from './board-review-messages';
import type { PreviewCandidate } from './candidate-preview';
import { useSourceText } from './analysis-source-messages';
import type { SourceAnalysis, BookLookup } from './source-analysis-api';
export default function SourceAnalysisView({
  records,
  book,
  source,
  onPreview,
  selectedPreview,
}: {
  records: SourceAnalysis[];
  book?: BookLookup;
  source: 'file' | 'book';
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
}) {
  const review = useBoardReviewText();
  const l = useSourceText(),
    { t, formatEquity, formatPercent, formatDecision } = useI18n();
  const [bookChoice, setBookChoice] = useState<'backgammondb' | 'xg'>();
  const [recordId, setRecordId] = useState<string>(),
    [decision, setDecision] = useState<'checker' | 'cube'>('checker');
  useEffect(() => {
    onPreview?.(null);
    setRecordId(undefined);
    setDecision('checker');
    setBookChoice(undefined);
  }, [records, book, onPreview]);
  const record =
    records.find((r) => r.id === recordId) ??
    records.find((r) => r.kind === 'checker') ??
    records[0];
  const candidates =
    source === 'file'
      ? (record?.candidates ?? [])
      : (book?.checker?.candidates.map((c) => ({
          ...c,
          equity_kind: 'cubeful',
          depth:
            book.checker?.plies == null ? null : `${book.checker.plies}-ply`,
          method: 'evaluation',
        })) ?? []);
  const rawCube =
    source === 'file'
      ? record?.cube
      : book?.cube
        ? {
            ...book.cube.equities,
            equity_kind: 'cubeful',
            verdict: book.cube.verdict,
          }
        : null;
  const cube =
    rawCube && typeof rawCube === 'object' && !Array.isArray(rawCube)
      ? (rawCube as Record<string, unknown>)
      : null;
  const cubeKind =
    typeof cube?.equity_kind === 'string' ? cube.equity_kind : 'unknown';
  const unit = (kind: string) =>
    kind === 'mwc'
      ? 'MWC'
      : kind === 'cubeful'
        ? 'Cubeful'
        : kind === 'cubeless'
          ? 'Cubeless'
          : kind;
  const eq = (value: unknown, kind: string) =>
    typeof value === 'number' && Number.isFinite(value)
      ? kind === 'mwc'
        ? formatPercent(value, 3)
        : formatEquity(value)
      : '—';
  const win = (probabilities: unknown) => {
    const p = Array.isArray(probabilities)
      ? probabilities[0]
      : probabilities &&
          typeof probabilities === 'object' &&
          'win' in probabilities
        ? probabilities.win
        : undefined;
    return typeof p === 'number' && p >= 0 && p <= 1 ? formatPercent(p) : '—';
  };
  const selectedDecision =
    decision === 'checker' && !book?.checker && book?.cube ? 'cube' : decision;
  const checker =
    source === 'file'
      ? record?.kind === 'checker'
      : selectedDecision === 'checker';
  const selectedBook =
    book?.provider === 'xg'
      ? 'xg'
      : book?.provider === 'backgammondb'
        ? 'backgammondb'
        : (bookChoice ??
          (book?.xg_position && !book.checker && !book.cube
            ? 'xg'
            : 'backgammondb'));
  const bookControls =
    source === 'book' ? (
      <div className="source-book-selector">
        <button
          type="button"
          aria-pressed={selectedBook === 'backgammondb'}
          disabled={book?.provider === 'xg'}
          onClick={() => {
            onPreview?.(null);
            setBookChoice('backgammondb');
          }}
        >
          BackgammonDB
        </button>
        <button
          type="button"
          aria-pressed={selectedBook === 'xg'}
          disabled={book?.provider === 'backgammondb'}
          onClick={() => {
            onPreview?.(null);
            setBookChoice('xg');
          }}
        >
          {l('xgPosition')}
        </button>
      </div>
    ) : null;
  const previewSource =
    source === 'file'
      ? `${record?.engine ?? ''} · ${record?.depth ?? l('unknownDepth')}`
      : `BackgammonDB · ${book?.provenance?.engine ?? ''} · ${book?.checker?.plies ?? '—'}-ply`;
  const preview = (candidate: { notation: string; rank: number }) =>
    onPreview?.({ ...candidate, source: previewSource });
  if (source === 'book' && selectedBook === 'xg')
    return (
      <div className="source-analysis">
        {bookControls}
        <XgPositionView position={book?.xg_position} />
      </div>
    );
  return (
    <div className="source-analysis">
      {bookControls}
      {source === 'file' ? (
        <>
          <p className="fineprint">{l('sourceHelp')}</p>
          {records.length > 1 && (
            <label className="source-record">
              {l('decision')}
              <select
                aria-label={l('decision')}
                value={record?.id ?? ''}
                onChange={(e) => {
                  onPreview?.(null);
                  setRecordId(e.target.value);
                }}
              >
                {records.map((r) => (
                  <option key={r.id} value={r.id}>
                    {r.kind === 'cube' ? l('cube') : l('checker')} · {r.engine}{' '}
                    · {r.depth ?? l('unknownDepth')}
                  </option>
                ))}
              </select>
            </label>
          )}
          {record ? (
            <p className="source-provenance">
              <strong>
                {record.engine} {record.engine_version}
              </strong>
              <span>{record.depth ?? l('unknownDepth')}</span>
              <small>
                {record.method} · {record.source_format.toUpperCase()}
              </small>
            </p>
          ) : (
            <p>{l('noSource')}</p>
          )}
        </>
      ) : (
        <>
          {book?.provenance && (
            <p className="source-provenance">
              <strong>BackgammonDB · {book.provenance.engine}</strong>
              <span>
                {(selectedDecision === 'checker'
                  ? book.checker?.plies
                  : book.cube?.plies) ?? '—'}
                -ply · {book.provenance.evaluation_kind}
              </span>
              <small>{book.provenance.context}</small>
            </p>
          )}
          {book?.hit ? (
            <div className="source-decision-buttons">
              <button
                aria-pressed={selectedDecision === 'checker'}
                disabled={!book.checker}
                onClick={() => {
                  onPreview?.(null);
                  setDecision('checker');
                }}
              >
                {l('checker')}
              </button>
              <button
                aria-pressed={selectedDecision === 'cube'}
                disabled={!book.cube}
                onClick={() => {
                  onPreview?.(null);
                  setDecision('cube');
                }}
              >
                {l('cube')}
              </button>
            </div>
          ) : (
            <p>{l('noBook')}</p>
          )}
        </>
      )}
      {checker && candidates.length > 0 && (
        <div className="source-candidates">
          {onPreview && (
            <button
              type="button"
              className="preview-best"
              onClick={() => preview(candidates[0])}
            >
              {review('best')}
            </button>
          )}
          <table className="candidates">
            <thead>
              <tr>
                <th>{t('analysis.rank')}</th>
                <th>{t('common.move')}</th>
                <th>{t('common.equity')}</th>
                <th>{t('analysis.win')}</th>
              </tr>
            </thead>
            <tbody>
              {candidates.map((c) => (
                <tr key={c.rank} className={c.rank === 1 ? 'best-move' : ''}>
                  <td>{c.rank}</td>
                  <td>
                    {onPreview ? (
                      <button
                        type="button"
                        className="candidate-preview-button"
                        aria-label={review('previewMove', {
                          rank: c.rank,
                          move: formatDecision(c.notation),
                        })}
                        aria-pressed={
                          selectedPreview?.notation === c.notation &&
                          selectedPreview?.source === previewSource
                        }
                        onClick={() => preview(c)}
                      >
                        {formatDecision(c.notation)}
                      </button>
                    ) : (
                      formatDecision(c.notation)
                    )}
                    <small>{c.depth ?? '—'}</small>
                  </td>
                  <td title={unit(c.equity_kind)}>
                    {eq(c.equity, c.equity_kind)}
                    <small>{unit(c.equity_kind)}</small>
                  </td>
                  <td>{win(c.probabilities)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {!checker && cube && (
        <>
          <p className="fineprint">
            {source === 'file' &&
            /^(take|drop|pass)$/i.test(record?.played ?? '')
              ? l('cubeResponse')
              : l('cubeBeforeRoll')}
          </p>
          {cube.perspective === 'doubler' && (
            <p className="fineprint">{l('doublerPerspective')}</p>
          )}
          <p className="fineprint">
            {l('equityUnit')}: {unit(cubeKind)}
            {typeof cube.verdict === 'string' && cube.verdict
              ? ` · ${cube.verdict}`
              : ''}
          </p>
          {['no_double', 'double_take', 'double_pass'].some(
            (key) => typeof cube[key as keyof typeof cube] === 'number',
          ) ? (
            <table className="candidates">
              <tbody>
                {(['no_double', 'double_take', 'double_pass'] as const).map(
                  (key, i) => (
                    <tr key={key}>
                      <td>
                        {l(
                          (['noDouble', 'doubleTake', 'doublePass'] as const)[
                            i
                          ],
                        )}
                      </td>
                      <td>{eq(cube[key], cubeKind)}</td>
                    </tr>
                  ),
                )}
              </tbody>
            </table>
          ) : (
            <p>{l('noCubeValues')}</p>
          )}
        </>
      )}
      {source === 'book' && book?.provenance && (
        <p className="fineprint source-attribution">
          <a
            href={
              book.provenance.source_url &&
              /^https?:\/\//.test(book.provenance.source_url)
                ? book.provenance.source_url
                : 'https://backgammondb.com/opening-book'
            }
            target="_blank"
            rel="noreferrer"
          >
            {book.provenance.attribution}
          </a>{' '}
          · {book.provenance.license}
        </p>
      )}
    </div>
  );
}
