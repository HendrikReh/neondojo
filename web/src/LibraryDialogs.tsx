import ActionButton from './ActionButton';
import { useState, type FormEvent } from 'react';
import { request } from './api';
import LibraryDialog from './LibraryDialog';
import {
  databaseDisplayName,
  useLibraryText,
  type LibraryMessageKey,
} from './library-messages';
import {
  acceptsLibraryMatches,
  exportMatches,
  type ExportFormat,
  type Database,
  type LibraryMatch,
  type BatchAction,
  type ActionResponse,
} from './library-api';
import { useI18n } from './i18n';

export type LibraryDialogState =
  | { kind: 'database'; database?: Database }
  | { kind: 'deleteDatabase'; database: Database }
  | { kind: 'metadata'; match: LibraryMatch }
  | {
      kind: 'batch';
      action: BatchAction | 'export';
      ids: string[];
      source: string;
    };
export default function LibraryDialogs({
  state,
  databases,
  onClose,
  onChanged,
  onAction,
}: {
  state: LibraryDialogState;
  databases: Database[];
  onClose: () => void;
  onChanged: (databaseId?: string, validateSelection?: boolean) => void;
  onAction: (result: ActionResponse) => void;
}) {
  const l = useLibraryText(),
    { formatError } = useI18n();
  const db = state.kind === 'database' ? state.database : undefined;
  const clearDatabase =
    state.kind === 'deleteDatabase' && state.database.is_system;
  const match = state.kind === 'metadata' ? state.match : undefined;
  const [name, setName] = useState(db?.name ?? match?.display_name ?? ''),
    [description, setDescription] = useState(db?.description ?? '');
  const targets = databases.filter(
    (d) =>
      acceptsLibraryMatches(d.id) &&
      (state.kind !== 'batch' || d.id !== state.source),
  );
  const [target, setTarget] = useState(targets[0]?.id ?? '');
  const [add, setAdd] = useState(''),
    [remove, setRemove] = useState(''),
    [format, setFormat] = useState<ExportFormat>('sgf');
  const [played, setPlayed] = useState(match?.played_on ?? ''),
    [tournament, setTournament] = useState(match?.tournament ?? ''),
    [round, setRound] = useState(match?.round ?? ''),
    [place, setPlace] = useState(match?.place ?? ''),
    [result, setResult] = useState(match?.result_state ?? 'unknown'),
    [white, setWhite] = useState(match?.white_score?.toString() ?? ''),
    [black, setBlack] = useState(match?.black_score?.toString() ?? '');
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<unknown>();
  const title =
    state.kind === 'database'
      ? l(db ? 'editDatabase' : 'newDatabase')
      : state.kind === 'deleteDatabase'
        ? l(clearDatabase ? 'clearDatabase' : 'deleteDatabase')
        : state.kind === 'metadata'
          ? l('editMatch')
          : l(state.action);
  const action = state.kind === 'batch' ? state.action : undefined;
  const limit = action === 'export' ? 50 : 100;
  const permitted = state.kind !== 'batch' || state.ids.length <= limit;
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!permitted || busy) return;
    setBusy(true);
    setError(undefined);
    try {
      if (state.kind === 'database') {
        const saved = await request<Database>(
          `/databases${db ? `/${db.id}` : ''}`,
          {
            method: db ? 'PUT' : 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              name,
              description,
              ...(db ? { version: db.version } : {}),
            }),
          },
        );
        onChanged(saved.id);
      } else if (state.kind === 'deleteDatabase') {
        const path = `/databases/${state.database.id}`;
        const version = state.database.version;
        await request<void>(
          clearDatabase
            ? `${path}/matches?version=${version}`
            : `${path}?version=${version}&delete_matches=true`,
          { method: 'DELETE' },
        );
        onChanged(clearDatabase ? state.database.id : '', true);
      } else if (state.kind === 'metadata') {
        await request(`/matches/${state.match.id}/metadata`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            version: state.match.version,
            display_name: name,
            played_on: played || null,
            tournament: tournament || null,
            round: round || null,
            place: place || null,
            result_state: result,
            white_score: white === '' ? null : Number(white),
            black_score: black === '' ? null : Number(black),
          }),
        });
        onChanged();
      } else if (state.action === 'export') {
        await exportMatches(state.ids, format);
      } else {
        const result = await request<ActionResponse>('/library/actions', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            action: state.action,
            match_ids: state.ids,
            ...(state.source ? { database_id: state.source } : {}),
            ...(['copy', 'move'].includes(state.action)
              ? { target_database_id: target }
              : {}),
            ...(state.action === 'tags'
              ? {
                  add_tags: add
                    .split(',')
                    .map((s) => s.trim())
                    .filter(Boolean),
                  remove_tags: remove
                    .split(',')
                    .map((s) => s.trim())
                    .filter(Boolean),
                }
              : {}),
          }),
        });
        onAction(result);
      }
      onClose();
    } catch (e) {
      setError(e);
    } finally {
      setBusy(false);
    }
  }
  return (
    <LibraryDialog title={title} onClose={onClose} busy={busy}>
      <form onSubmit={(e) => void submit(e)} className="library-dialog-form">
        {(state.kind === 'database' || state.kind === 'metadata') && (
          <label>
            {l('name')}
            <input
              required
              maxLength={state.kind === 'database' ? 120 : 240}
              value={db?.is_system ? databaseDisplayName(db, l) : name}
              readOnly={db?.is_system}
              onChange={(e) => setName(e.target.value)}
              disabled={busy}
            />
          </label>
        )}
        {state.kind === 'database' && (
          <label>
            {l('description')}
            <textarea
              value={description}
              maxLength={2000}
              onChange={(e) => setDescription(e.target.value)}
              disabled={busy}
            />
          </label>
        )}
        {state.kind === 'metadata' && (
          <>
            <label>
              {l('played')}
              <input
                type="date"
                value={played}
                onChange={(e) => setPlayed(e.target.value)}
                disabled={busy}
              />
            </label>
            <label>
              {l('tournament')}
              <input
                maxLength={240}
                value={tournament}
                onChange={(e) => setTournament(e.target.value)}
                disabled={busy}
              />
            </label>
            <div className="library-form-pair">
              <label>
                {l('round')}
                <input
                  value={round}
                  maxLength={120}
                  onChange={(e) => setRound(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label>
                {l('place')}
                <input
                  maxLength={240}
                  value={place}
                  onChange={(e) => setPlace(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
            <label>
              {l('result')}
              <select
                aria-label={l('result')}
                value={result}
                onChange={(e) => setResult(e.target.value)}
                disabled={busy}
              >
                {['unknown', 'ongoing', 'completed'].map((v) => (
                  <option value={v} key={v}>
                    {l(v as LibraryMessageKey)}
                  </option>
                ))}
              </select>
            </label>
            <div className="library-form-pair">
              <label>
                {l('white')} · {l('score')}
                <input
                  type="number"
                  min="0"
                  max="32767"
                  value={white}
                  onChange={(e) => setWhite(e.target.value)}
                  disabled={busy}
                />
              </label>
              <label>
                {l('black')} · {l('score')}
                <input
                  type="number"
                  min="0"
                  max="32767"
                  value={black}
                  onChange={(e) => setBlack(e.target.value)}
                  disabled={busy}
                />
              </label>
            </div>
          </>
        )}
        {state.kind === 'deleteDatabase' && (
          <p>
            {l(clearDatabase ? 'clearDatabaseWarning' : 'databaseWarning', {
              count: state.database.match_count,
            })}{' '}
            <strong>{state.database.name}</strong>
          </p>
        )}
        {state.kind === 'batch' && (
          <>
            <p>{l('selected', { count: state.ids.length })}</p>
            {['copy', 'move'].includes(state.action) && (
              <>
                <label>
                  {l('target')}
                  <select
                    aria-label={l('target')}
                    value={target}
                    onChange={(e) => setTarget(e.target.value)}
                    disabled={busy}
                    required
                  >
                    <option value="" disabled>
                      —
                    </option>
                    {targets.map((d) => (
                      <option key={d.id} value={d.id}>
                        {d.name}
                      </option>
                    ))}
                  </select>
                </label>
                {state.action === 'copy' && <p>{l('copyHelp')}</p>}
              </>
            )}
            {state.action === 'delete' && (
              <p className="destructive-copy">
                {l('deleteWarning', { count: state.ids.length })}
              </p>
            )}
            {state.action === 'tags' && (
              <>
                <label>
                  {l('addTags')}
                  <input
                    maxLength={2000}
                    value={add}
                    onChange={(e) => setAdd(e.target.value)}
                    disabled={busy}
                  />
                </label>
                <label>
                  {l('removeTags')}
                  <input
                    maxLength={2000}
                    value={remove}
                    onChange={(e) => setRemove(e.target.value)}
                    disabled={busy}
                  />
                </label>
              </>
            )}
            {state.action === 'export' && (
              <>
                <label>
                  {l('exportFormat')}
                  <select
                    aria-label={l('exportFormat')}
                    value={format}
                    onChange={(e) => {
                      const value = e.target.value;
                      if (
                        value === 'sgf' ||
                        value === 'xg' ||
                        value === 'original'
                      )
                        setFormat(value);
                    }}
                    disabled={busy}
                  >
                    <option value="sgf">SGF</option>
                    <option value="xg">XG</option>
                    <option value="original">{l('original')}</option>
                  </select>
                </label>
                <p>{l('exportHelp')}</p>
              </>
            )}
            {!permitted && (
              <p role="alert" className="error">
                {l('batchLimit')}
              </p>
            )}
          </>
        )}
        {!!error && (
          <p role="alert" className="error">
            {formatError(error)}
          </p>
        )}
        <div className="library-dialog-actions">
          <ActionButton
            icon="cancel"
            label={l('cancel')}
            type="button"
            disabled={busy}
            onClick={onClose}
          />
          <ActionButton
            icon={
              busy
                ? 'loading'
                : state.kind === 'deleteDatabase'
                  ? 'delete'
                  : action === 'tags'
                    ? 'edit'
                    : (action ?? 'save')
            }
            label={
              busy
                ? l('loading')
                : state.kind === 'deleteDatabase'
                  ? l(clearDatabase ? 'clearDatabase' : 'delete')
                  : action
                    ? l(action)
                    : l('save')
            }
            className={
              state.kind === 'deleteDatabase' || action === 'delete'
                ? 'danger'
                : 'primary'
            }
            disabled={
              busy ||
              !permitted ||
              (action === 'tags' && !add.trim() && !remove.trim()) ||
              (!!action && ['copy', 'move'].includes(action) && !target)
            }
          />
        </div>
      </form>
    </LibraryDialog>
  );
}
