import { useEffect, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import Board from './Board';
import { request, type Position } from './api';
import { matchContext } from './boardMath';
import type { components } from './generated-api';
import { useI18n } from './i18n';
import { usePositionText } from './position-browser-messages';
import type { Theme } from './themes';

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
  const { t, formatError } = useI18n();
  const heading = useRef<HTMLHeadingElement>(null);
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
  const [selected, setSelected] = useState<string>();
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
  const preview = hits.find((hit) => hit.position.id === selected) ?? hits[0];
  const context = preview && matchContext(preview.position.gnubg_match_id);
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
          <div className="position-table-scroll">
            <table className="similar-position-table" aria-label={p('results')}>
              <thead>
                <tr>
                  <th scope="col">{p('similarMatch')}</th>
                  <th scope="col">{p('player')}</th>
                  <th scope="col">{p('gameNumber')}</th>
                  <th scope="col">{p('moveNumber')}</th>
                  <th scope="col">{p('checkerDistance')}</th>
                </tr>
              </thead>
              <tbody>
                {hits.map((hit) => (
                  <tr
                    key={hit.position.id}
                    className={
                      hit === preview ? 'similar-position-selected' : undefined
                    }
                  >
                    <td>
                      <button
                        type="button"
                        aria-pressed={hit === preview}
                        onClick={() => setSelected(hit.position.id)}
                      >
                        {hit.match_name}
                      </button>
                    </td>
                    <td>{hit.position.player === 0 ? hit.white : hit.black}</td>
                    <td>{hit.position.game_no}</td>
                    <td>{hit.position.ply}</td>
                    <td>{hit.distance}</td>
                  </tr>
                ))}
              </tbody>
            </table>
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
    </section>
  );
}
