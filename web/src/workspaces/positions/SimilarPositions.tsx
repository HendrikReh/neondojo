import { useEffect, useId, useRef, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import SearchPagination from '../../shared/ui/SearchPagination';
import { SearchResultRow } from '../../shared/ui/SearchResultList';
import { lastSearchPage } from '../../shared/hooks/useSearchPages';
import Board from '../../shared/board/Board';
import { request, type Position } from '../../shared/api/api';
import { matchContext } from '../../shared/board/boardMath';
import type { components } from '../../generated-api';
import { useI18n } from '../../shared/i18n/i18n';
import { usePositionText } from './position-browser-messages';
import { useDecisionSearchText } from './decision-search-messages';
import SimilarPositionPractice from './SimilarPositionPractice';
import type { Theme } from '../../shared/board/themes';

type SimilarResponse = components['schemas']['SimilarPositions'];

export default function SimilarPositions({
  source,
  matchName,
  visible,
  revision,
  theme,
  onClose,
  onOpen,
}: {
  source: Position;
  matchName: string;
  visible: boolean;
  revision: number;
  theme: Theme;
  onClose: () => void;
  onOpen: (id: string) => void;
}) {
  const p = usePositionText();
  const decisions = useDecisionSearchText();
  const { t, formatError, formatDecision, formatNumber } = useI18n();
  const heading = useRef<HTMLHeadingElement>(null);
  const practiceButton = useRef<HTMLButtonElement>(null);
  const [filters, setFilters] = useState({
    same_dice: false,
    same_score: false,
    same_cube: false,
    include_current_match: false,
  });
  const [retry, setRetry] = useState(0);
  const [response, setResponse] = useState<{
    key: string;
    result?: SimilarResponse;
    error?: unknown;
  }>();
  const [practiceItems, setPracticeItems] =
    useState<SimilarResponse['items']>();
  const [selected, setSelected] = useState<string>();
  const rowPrefix = `${useId()}-`;
  const [resultPage, setResultPage] = useState({ key: '', number: 1 });
  const params = new URLSearchParams(
    Object.entries(filters)
      .filter(([, enabled]) => enabled)
      .map(([name]) => [name, 'true']),
  ).toString();
  const searchKey = `${source.id}:${revision}:${retry}:${params}`;
  const filterLabels = {
    same_dice: p('sameDice'),
    same_score: p(
      matchContext(source.gnubg_match_id).length
        ? 'sameMatchScore'
        : 'sameMoneyGame',
    ),
    same_cube: p('sameCube'),
    include_current_match: p('includeCurrentMatch'),
  };
  useEffect(() => heading.current?.focus(), []);
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    setResponse(undefined);
    void request<SimilarResponse>(
      `/positions/${source.id}/similar${params ? `?${params}` : ''}`,
      { signal: abort.signal },
    ).then(
      (result) => {
        if (!abort.signal.aborted) setResponse({ key: searchKey, result });
      },
      (error) => {
        if (!abort.signal.aborted) setResponse({ key: searchKey, error });
      },
    );
    return () => abort.abort();
  }, [source.id, visible, params, searchKey]);

  const { result, error } = response?.key === searchKey ? response : {};
  const hits = result?.items ?? [];
  const pageSize = 25;
  const page = resultPage.key === searchKey ? resultPage.number : 1;
  const rows = hits.slice((page - 1) * pageSize, page * pageSize);
  const rowIds = rows.map((hit) => hit.position.id);
  const preview = hits.find((hit) => hit.position.id === selected) ?? hits[0];
  const context = preview && matchContext(preview.position.gnubg_match_id);
  // Score from the mover's side, so equal boards at different scores stand out.
  const scoreLabel = (hit: (typeof rows)[number]) => {
    const match = matchContext(hit.position.gnubg_match_id);
    if (!match.length) return t('common.moneyGame');
    const own = hit.position.player;
    return decisions('scoreAway', {
      own: match.length - match.scores[own],
      opponent: match.length - match.scores[1 - own],
    });
  };
  const cubeLabel = (hit: (typeof rows)[number]) => {
    const match = matchContext(hit.position.gnubg_match_id);
    return match.owner == null
      ? String(match.cube)
      : `${match.cube} · ${match.owner === 0 ? hit.white : hit.black}`;
  };
  return (
    <section
      className="position-browser similar-positions"
      aria-labelledby="similar-positions-heading"
    >
      <div className="position-browser-heading">
        <h1 ref={heading} tabIndex={-1} id="similar-positions-heading">
          {p('similar')}
        </h1>
        <p>
          {matchName} · {p('gameNumber')} {source.game_no} · {p('moveNumber')}{' '}
          {source.ply}
        </p>
      </div>
      <div className="similar-position-controls">
        <ActionButton
          type="button"
          icon="back"
          label={p('backToSaved')}
          onClick={onClose}
        />
        {(Object.keys(filters) as (keyof typeof filters)[]).map((name) => (
          <label key={name}>
            <input
              type="checkbox"
              checked={filters[name]}
              onChange={(event) =>
                setFilters({ ...filters, [name]: event.target.checked })
              }
            />
            {filterLabels[name]}
          </label>
        ))}
        <ActionButton
          ref={practiceButton}
          type="button"
          icon="cards"
          label={p('practiceResults')}
          disabled={!hits.length}
          onClick={() => setPracticeItems(hits)}
        />
      </div>
      {!result && !error && <p role="status">{p('loading')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton
            type="button"
            icon="refresh"
            label={p('retrySimilar')}
            onClick={() => setRetry((n) => n + 1)}
          />
        </div>
      )}
      {result && !hits.length && (
        <p className="position-empty">{p('similarEmpty')}</p>
      )}
      {preview && context && (
        <div className="position-browser-layout">
          <div>
            <div className="position-table-scroll">
              <table
                className="similar-position-table"
                aria-label={p('results')}
              >
                <thead>
                  <tr>
                    <th scope="col">{p('similarMatch')}</th>
                    <th scope="col">{p('player')}</th>
                    <th scope="col">{p('gameNumber')}</th>
                    <th scope="col">{p('moveNumber')}</th>
                    <th scope="col">{decisions('score')}</th>
                    <th scope="col">{decisions('cube')}</th>
                    <th scope="col">{decisions('played')}</th>
                    <th scope="col">{decisions('loss')}</th>
                    <th scope="col">{p('checkerDistance')}</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((hit, index) => (
                    <SearchResultRow
                      key={hit.position.id}
                      ids={rowIds}
                      index={index}
                      idPrefix={rowPrefix}
                      active={hit === preview}
                      selectedClassName="similar-position-selected"
                      aria-selected={hit === preview}
                      onPreview={setSelected}
                      onOpen={onOpen}
                      jumpToEdges
                    >
                      <td>{hit.match_name}</td>
                      <td>
                        {hit.position.player === 0 ? hit.white : hit.black}
                      </td>
                      <td>{hit.position.game_no}</td>
                      <td>{hit.position.ply}</td>
                      <td>{scoreLabel(hit)}</td>
                      <td>{cubeLabel(hit)}</td>
                      <td>{formatDecision(hit.position.notation) || '—'}</td>
                      <td>
                        {hit.equity_loss == null
                          ? '—'
                          : formatNumber(hit.equity_loss, {
                              minimumFractionDigits: 3,
                              maximumFractionDigits: 3,
                            })}
                      </td>
                      <td>{hit.distance}</td>
                    </SearchResultRow>
                  ))}
                </tbody>
              </table>
            </div>
            <SearchPagination
              page={page}
              lastPage={lastSearchPage(hits.length, pageSize)}
              onPageChange={(number) =>
                setResultPage({ key: searchKey, number })
              }
            />
          </div>
          <aside className="position-preview" aria-label={p('preview')}>
            <h3>{preview.match_name}</h3>
            <p className="position-preview-context">
              {context.length
                ? t('app.matchLength', { count: context.length })
                : `${t('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}
            </p>
            <Board
              position={preview.position}
              white={preview.white}
              black={preview.black}
              theme={theme}
            />
            <ActionButton
              type="button"
              icon="analysis"
              label={p('openAnalysis')}
              onClick={() => onOpen(preview.position.id)}
            />
          </aside>
        </div>
      )}
      {visible && practiceItems && (
        <SimilarPositionPractice
          items={practiceItems}
          theme={theme}
          returnFocus={practiceButton}
          onClose={() => setPracticeItems(undefined)}
        />
      )}
    </section>
  );
}
