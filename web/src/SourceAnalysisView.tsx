import { useEffect, useEffectEvent, useState } from 'react';
import { useI18n } from './shared/i18n/i18n';
import XgPositionView from './XgPositionView';
import EngineLabel from './shared/engine/EngineLabel';
import type { PreviewCandidate } from './shared/review/candidate-preview';
import CandidateTable from './CandidateTable';
import { candidateEquityRanks } from './candidate-comparison';
import type { Match, Position } from './shared/api/api';
import { useSourceText } from './analysis-source-messages';
import { useCandidateText } from './candidate-messages';
import type { SourceAnalysis, BookLookup } from './source-analysis-api';

function OgxmCheckerSummary({ raw }: { raw: unknown }) {
  const l = useSourceText();
  const candidate = useCandidateText();
  const { formatNumber } = useI18n();
  if (typeof raw !== 'object' || raw === null) return null;

  const best = 'best_equity' in raw ? raw.best_equity : null;
  const loss = 'equity_loss' in raw ? raw.equity_loss : null;
  const hasBest = typeof best === 'number' && Number.isFinite(best);
  const hasLoss =
    typeof loss === 'number' && Number.isFinite(loss) && loss >= 0;
  if (!hasBest && !hasLoss) return null;

  // The stored analysis retains the OGXM currency enum (§9.10).
  const currency = 'currency' in raw ? raw.currency : null;
  const mwc = currency === 2 || currency === 3;
  const unit = mwc
    ? 'MWC'
    : currency === 0
      ? 'Cubeless'
      : currency === 1
        ? 'Cubeful'
        : candidate('unknownUnit');
  const precision = {
    minimumFractionDigits: 3,
    maximumFractionDigits: mwc ? 4 : 6,
  };

  return (
    <table className="candidates ogxm-checker-summary">
      <caption>
        {l('ogxmSummary')} · {unit}
      </caption>
      <tbody>
        {hasBest && (
          <tr>
            <th scope="row">{l('bestEquity')}</th>
            <td>
              {formatNumber(best, {
                ...precision,
                style: mwc ? 'percent' : 'decimal',
              })}
            </td>
          </tr>
        )}
        {hasLoss && (
          <tr>
            <th scope="row">{candidate('loss')}</th>
            <td>
              {mwc
                ? candidate('percentagePoints', {
                    value: formatNumber(loss * 100, precision),
                  })
                : formatNumber(loss, precision)}
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

export default function SourceAnalysisView({
  records,
  book,
  source,
  onPreview,
  selectedPreview,
  position,
  playerName,
  players,
  preferredKind,
}: {
  records: SourceAnalysis[];
  book?: BookLookup;
  source: 'file' | 'book';
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  players?: Pick<Match, 'white' | 'black'>;
  preferredKind?: 'checker' | 'cube';
}) {
  const l = useSourceText(),
    { t, formatEquity, formatPercent } = useI18n();
  const [bookChoice, setBookChoice] = useState<'backgammondb' | 'xg'>();
  const [recordId, setRecordId] = useState<string>(),
    [decision, setDecision] = useState<'checker' | 'cube'>('checker');
  const resetPreview = useEffectEvent(() => onPreview?.(null));
  useEffect(() => {
    resetPreview();
    setRecordId(undefined);
    setDecision('checker');
    setBookChoice(undefined);
  }, [records, book, preferredKind]);
  const record =
    records.find((r) => r.id === recordId) ??
    records.find((r) => r.kind === (preferredKind ?? 'checker')) ??
    records[0];
  const xgChecker =
    source === 'file' &&
    record?.source_format === 'xg' &&
    record.kind === 'checker';
  const sourceCandidates =
    source === 'file'
      ? (record?.candidates ?? [])
      : (book?.checker?.candidates.map((c) => ({
          ...c,
          equity_kind: book?.provenance?.equity_kind ?? 'unknown',
          depth:
            book.checker?.plies == null ? null : `${book.checker.plies}-ply`,
          method: 'evaluation',
        })) ?? []);
  const equityRanks = xgChecker ? candidateEquityRanks(sourceCandidates) : null;
  const candidates = equityRanks?.size
    ? Array.from(equityRanks, ([candidate, rank]) => ({ ...candidate, rank }))
    : sourceCandidates;
  const rawCube =
    source === 'file'
      ? record?.cube
      : book?.cube
        ? {
            ...book.cube.equities,
            equity_kind: book?.provenance?.equity_kind ?? 'unknown',
            verdict: book.cube.verdict,
          }
        : null;
  const cube =
    rawCube && typeof rawCube === 'object' && !Array.isArray(rawCube)
      ? (rawCube as Record<string, unknown>)
      : null;
  const cubeAction = source === 'file' ? (record?.played ?? '') : '';
  const cubeResponse = /^(take|drop|pass)$/i.test(cubeAction);
  const cubeBeforeRoll = /^(?:no )?double$/i.test(cubeAction);
  const fromDoubler =
    cube?.perspective === 'doubler' ||
    (cubeResponse &&
      record?.source_format === 'ogxm' &&
      (record.player === 0 || record.player === 1) &&
      cube?.equity_player === 1 - record.player);
  let perspectiveName: string | undefined;
  if ((cubeResponse || cubeBeforeRoll) && record && players) {
    const seat = cube?.equity_player;
    if (seat === 0 || seat === 1) {
      perspectiveName = seat === 0 ? players.white : players.black;
    } else if (fromDoubler && (record.player === 0 || record.player === 1)) {
      const equityPlayer = cubeResponse ? 1 - record.player : record.player;
      perspectiveName = equityPlayer === 0 ? players.white : players.black;
    }
  }
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
  const selectedDecision =
    decision === 'checker' && !book?.checker && book?.cube ? 'cube' : decision;
  const checker =
    source === 'file'
      ? record?.kind === 'checker'
      : selectedDecision === 'checker';
  const ogxmSummaryRecord =
    source === 'file' &&
    checker &&
    record?.source_format === 'ogxm' &&
    candidates.length === 0
      ? record
      : null;
  const ogxmPlaceholder = ogxmSummaryRecord?.evaluated === false;
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
      <div className="source-book-selector text-navigation text-navigation--compact">
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
  const importedSource = (entry: SourceAnalysis) =>
    [
      [entry.engine, entry.engine_version].filter(Boolean).join(' '),
      entry.depth ?? l('unknownDepth'),
      entry.method,
      entry.source_format.toUpperCase(),
    ]
      .filter(Boolean)
      .join(' · ');
  const bookSource = (plies?: number | null) =>
    [
      book?.provenance?.engine ?? 'BackgammonDB',
      book?.provenance && 'BackgammonDB',
      plies == null ? l('unknownDepth') : `${plies}-ply`,
      book?.provenance?.evaluation_kind,
      book?.provenance?.context,
    ]
      .filter(Boolean)
      .join(' · ');
  const previewSource =
    source === 'file' && record
      ? importedSource(record)
      : bookSource(book?.checker?.plies);
  const hasComparison =
    !(source === 'book' && selectedBook === 'xg') &&
    !ogxmPlaceholder &&
    checker &&
    !!(source === 'file' ? record : book?.checker);
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
          {!checker && <p className="fineprint">{l('sourceHelp')}</p>}
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
          {record && !checker ? (
            <p className="source-provenance">
              <strong>
                <EngineLabel details={importedSource(record)} />
              </strong>
            </p>
          ) : (
            !record && <p>{l('noSource')}</p>
          )}
        </>
      ) : (
        <>
          {book?.provenance && !checker && (
            <p className="source-provenance">
              <strong>
                <EngineLabel details={bookSource(book.cube?.plies)} />
              </strong>
            </p>
          )}
          {book?.hit ? (
            <div className="source-decision-buttons text-navigation text-navigation--compact">
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
      {ogxmPlaceholder && (
        <p className="fineprint" role="status">
          {t('matchAnalysis.unavailable')}
        </p>
      )}
      {hasComparison && (
        <CandidateTable
          candidates={candidates}
          source={previewSource}
          showCandidateDepth={xgChecker}
          depth={
            source === 'file'
              ? record?.depth
              : book?.checker?.plies == null
                ? null
                : `${book.checker.plies}-ply`
          }
          played={
            position?.notation ??
            (source === 'file' ? record?.played : undefined)
          }
          position={position}
          playerName={playerName}
          onPreview={onPreview}
          selectedPreview={selectedPreview}
        />
      )}
      {ogxmSummaryRecord && !ogxmPlaceholder && (
        <OgxmCheckerSummary raw={ogxmSummaryRecord.raw} />
      )}
      {!checker && cube && (
        <>
          <p className="fineprint">
            {cubeResponse ? l('cubeResponse') : l('cubeBeforeRoll')}
          </p>
          {(perspectiveName || fromDoubler) && (
            <p className="fineprint">
              {perspectiveName
                ? l('namedCubePerspective', { player: perspectiveName })
                : l('doublerPerspective')}
            </p>
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
