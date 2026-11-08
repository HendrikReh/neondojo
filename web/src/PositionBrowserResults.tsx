import Board from './Board';
import { useI18n } from './i18n';
import type { PositionBrowserHit } from './position-browser-api';
import {
  usePositionText,
  type PositionMessageKey,
} from './position-browser-messages';
import type { Theme } from './themes';
export interface PositionResultsProps {
  items: PositionBrowserHit[];
  selected: ReadonlySet<string>;
  previewId?: string;
  onPreview: (id: string) => void;
  onToggle: (ids: string[], checked: boolean) => void;
  onOpen: (id: string) => void;
  busy: boolean;
}
export const positionPlayer = (hit: PositionBrowserHit) =>
  hit.position.player === 0 ? hit.match_info.white : hit.match_info.black;
const scopeKeys: Record<
  PositionBrowserHit['notes'][number]['scope'],
  PositionMessageKey
> = {
  match: 'scopeMatch',
  game: 'scopeGame',
  position: 'scopePosition',
  move: 'scopeMove',
};
function HitNotes({ hit }: { hit: PositionBrowserHit }) {
  const p = usePositionText();
  return (
    <div className="position-hit-notes">
      {!!hit.tags.length && (
        <div className="annotation-tags">
          {hit.tags.map((tag) => (
            <span className="tag-chip" key={tag}>
              {tag}
            </span>
          ))}
        </div>
      )}
      {hit.notes
        .filter((note) => note.comment_excerpt)
        .map((note, index) => (
          <p key={`${note.scope}-${index}`}>
            <small>{p(scopeKeys[note.scope])}</small> {note.comment_excerpt}
          </p>
        ))}
    </div>
  );
}
function NativeError({ hit }: { hit: PositionBrowserHit }) {
  const p = usePositionText(),
    { formatNumber } = useI18n();
  return hit.native_error ? (
    <span className="position-native-error">
      <strong>
        {formatNumber(hit.native_error.equity_loss, {
          minimumFractionDigits: 3,
          maximumFractionDigits: 3,
        })}
      </strong>
      <small
        title={p('provenance', {
          engine: hit.native_error.engine,
          plies: hit.native_error.plies,
          job: hit.native_error.job_id,
        })}
      >
        {hit.native_error.engine} ·{' '}
        {p('depth', { plies: hit.native_error.plies })}
      </small>
    </span>
  ) : (
    <span className="position-unknown">{p('nativeUnknown')}</span>
  );
}
export function PositionBrowserResults({
  items,
  selected,
  previewId,
  onPreview,
  onToggle,
  onOpen,
  busy,
}: PositionResultsProps) {
  const p = usePositionText(),
    { formatNumber } = useI18n();
  const ids = items.map((hit) => hit.position.id),
    all = ids.length > 0 && ids.every((id) => selected.has(id));
  return (
    <div className="position-browser-results" aria-busy={busy}>
      <div className="position-page-select">
        <label>
          <input
            type="checkbox"
            checked={all}
            disabled={!ids.length || busy || (!all && selected.size >= 200)}
            onChange={(event) => onToggle(ids, event.target.checked)}
          />
          {p('selectPage')}
        </label>
      </div>
      <div className="position-table-scroll">
        <table className="position-table" aria-label={p('results')}>
          <thead>
            <tr>
              <th>
                <span className="visually-hidden">
                  {p('selected', { count: selected.size })}
                </span>
              </th>
              <th scope="col">{p('origin')}</th>
              <th scope="col">
                {p('player')} / {p('dice')}
              </th>
              <th scope="col">{p('notes')}</th>
              <th scope="col">{p('nativeError')}</th>
              <th scope="col">
                <span className="visually-hidden">{p('openAnalysis')}</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {items.map((hit) => (
              <tr
                key={hit.position.id}
                className={
                  previewId === hit.position.id
                    ? 'position-preview-selected'
                    : ''
                }
                data-position-id={hit.position.id}
              >
                <td>
                  <input
                    type="checkbox"
                    aria-label={p('selectRow', {
                      game: hit.position.game_no,
                      ply: hit.position.ply,
                      player: positionPlayer(hit),
                    })}
                    checked={selected.has(hit.position.id)}
                    disabled={
                      busy ||
                      (!selected.has(hit.position.id) && selected.size >= 200)
                    }
                    onChange={(event) =>
                      onToggle([hit.position.id], event.target.checked)
                    }
                  />
                </td>
                <td>
                  <button
                    className="position-preview-link"
                    aria-pressed={previewId === hit.position.id}
                    onClick={() => onPreview(hit.position.id)}
                  >
                    <strong>{hit.display_name}</strong>
                    <span>{hit.database_name}</span>
                    <small>
                      {p('game', { number: hit.position.game_no })} ·{' '}
                      {p('ply', { number: hit.position.ply })}
                    </small>
                  </button>
                </td>
                <td>
                  <strong>{positionPlayer(hit)}</strong>
                  <span>
                    {hit.position.dice
                      .map((die) => formatNumber(die))
                      .join(' · ')}
                  </span>
                  <small>
                    {hit.white_score == null
                      ? '?'
                      : formatNumber(hit.white_score)}
                    –
                    {hit.black_score == null
                      ? '?'
                      : formatNumber(hit.black_score)}{' '}
                    · {p('cube')}{' '}
                    {hit.cube_value == null
                      ? '?'
                      : formatNumber(hit.cube_value)}
                  </small>
                </td>
                <td>
                  <HitNotes hit={hit} />
                </td>
                <td>
                  <NativeError hit={hit} />
                </td>
                <td>
                  <button onClick={() => onOpen(hit.position.id)}>
                    {p('openAnalysis')}
                  </button>
                  {!hit.card_ready && (
                    <small className="position-analysis-needed">
                      {p('analysis_required')}
                    </small>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
export function PositionBrowserPreview({
  hit,
  theme,
  onOpen,
}: {
  hit?: PositionBrowserHit;
  theme: Theme;
  onOpen: (id: string) => void;
}) {
  const p = usePositionText(),
    { formatNumber } = useI18n();
  return (
    <aside className="position-preview" aria-label={p('preview')}>
      <h2>{p('preview')}</h2>
      {!hit ? (
        <p>{p('previewHint')}</p>
      ) : (
        <>
          <h3>{hit.display_name}</h3>
          <p className="position-preview-context">
            {hit.database_name} · {p('game', { number: hit.position.game_no })}{' '}
            · {p('ply', { number: hit.position.ply })}
          </p>
          <Board
            position={hit.position}
            white={hit.match_info.white}
            black={hit.match_info.black}
            theme={theme}
            previewLabel={p('beforeMove')}
          />
          <p className="position-before-label">{p('beforeMove')}</p>
          <dl className="position-context-details">
            <div>
              <dt>{p('checkerId')}</dt>
              <dd>{hit.position.position_id}</dd>
            </div>
            <div>
              <dt>{p('matchPoints')}</dt>
              <dd>{formatNumber(hit.match_info.match_length)}</dd>
            </div>
            {hit.crawford != null && (
              <div>
                <dt>{p('crawford')}</dt>
                <dd>{p(hit.crawford ? 'yes' : 'no')}</dd>
              </div>
            )}
            {hit.tournament && (
              <div>
                <dt>{p('tournament')}</dt>
                <dd>{hit.tournament}</dd>
              </div>
            )}
            {hit.round && (
              <div>
                <dt>{p('round')}</dt>
                <dd>{hit.round}</dd>
              </div>
            )}
          </dl>
          <HitNotes hit={hit} />
          <p>
            {p('nativeError')}: <NativeError hit={hit} />
          </p>
          {!hit.card_ready && (
            <p className="position-analysis-needed">{p('needed')}</p>
          )}
          <button className="primary" onClick={() => onOpen(hit.position.id)}>
            {p('openAnalysis')}
          </button>
        </>
      )}
    </aside>
  );
}
