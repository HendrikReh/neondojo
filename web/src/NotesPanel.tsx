import ActionButton from './ActionButton';
import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import AnnotationPanel from './AnnotationPanel';
import type { AnnotationKind } from './annotation-api';
import { useAnnotationI18n } from './annotation-messages';
import { ImportNotes } from './ImportNotes';
import {
  loadDecisions,
  loadImportWarnings,
  type GameDecisions,
} from './library-api';
import { useLibraryText, type ImportWarning } from './library-messages';
interface NotesPanelProps {
  matchId: string;
  suspended?: boolean;
  gameNo?: number;
  positionId?: string;
  onSaved?: () => void;
  scopeRequest?: { kind: AnnotationKind; token: number };
  onMoveNotes: () => void;
}
const scopes = ['match', 'game', 'position'] as const;
export default function NotesPanel({
  matchId,
  suspended,
  gameNo,
  positionId,
  onSaved,
  scopeRequest,
  onMoveNotes,
}: NotesPanelProps) {
  const { t, formatError } = useAnnotationI18n(),
    l = useLibraryText(),
    htmlId = useId();
  const refs = useRef<
    Partial<Record<AnnotationKind, HTMLButtonElement | null>>
  >({});
  const [preferred, setPreferred] = useState<AnnotationKind>('position'),
    [catalog, setCatalog] = useState<GameDecisions>(),
    [error, setError] = useState<unknown>(),
    // Owned per match; the scope tabs only filter it.
    [importWarnings, setImportWarnings] = useState<{
      matchId: string;
      warnings: ImportWarning[];
    }>(),
    [importError, setImportError] = useState<unknown>();
  useEffect(() => {
    if (scopeRequest && scopeRequest.kind !== 'move')
      setPreferred(scopeRequest.kind);
  }, [scopeRequest]);
  useEffect(() => {
    setImportError(undefined);
    const abort = new AbortController();
    void loadImportWarnings(matchId, abort.signal)
      .then((warnings) => {
        if (!abort.signal.aborted) setImportWarnings({ matchId, warnings });
      })
      .catch((e) => {
        if (!abort.signal.aborted) setImportError(e);
      });
    return () => abort.abort();
  }, [matchId]);
  useEffect(() => {
    setCatalog(undefined);
    setError(undefined);
    if (!gameNo) return;
    const abort = new AbortController();
    void loadDecisions(matchId, gameNo, abort.signal)
      .then((value) => {
        if (!abort.signal.aborted) setCatalog(value);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => abort.abort();
  }, [matchId, gameNo]);
  const gameId =
    catalog && catalog.game_no === gameNo ? catalog.game_id : undefined;
  const available = scopes.filter(
    (s) => s === 'match' || (s === 'game' ? !!gameId : !!positionId),
  );
  const active = available.includes(preferred as (typeof scopes)[number])
    ? preferred
    : positionId
      ? 'position'
      : gameId
        ? 'game'
        : 'match';
  const annotationId =
    active === 'position' ? positionId! : active === 'game' ? gameId! : matchId;
  const matchWarnings =
    importWarnings?.matchId === matchId ? importWarnings.warnings : [];
  const scopeWarnings =
    active === 'match'
      ? matchWarnings
      : active === 'game'
        ? matchWarnings.filter((warning) => warning.game_no === gameNo)
        : [];
  function navigate(
    e: KeyboardEvent<HTMLButtonElement>,
    current: AnnotationKind,
  ) {
    const index = available.indexOf(current as (typeof scopes)[number]);
    let next: number;
    if (e.key === 'ArrowLeft')
      next = (index + available.length - 1) % available.length;
    else if (e.key === 'ArrowRight') next = (index + 1) % available.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = available.length - 1;
    else return;
    e.preventDefault();
    setPreferred(available[next]);
    refs.current[available[next]]?.focus();
  }
  return (
    <div className="notes-panel">
      <div
        className="notes-scope-tabs text-navigation text-navigation--compact"
        role="tablist"
        aria-label={t('scopeLabel')}
      >
        {scopes.map((scope) => (
          <button
            key={scope}
            ref={(el) => {
              refs.current[scope] = el;
            }}
            type="button"
            role="tab"
            id={`${htmlId}-${scope}-tab`}
            aria-controls={`${htmlId}-${scope}-panel`}
            aria-selected={active === scope}
            tabIndex={active === scope ? 0 : -1}
            disabled={!available.includes(scope)}
            onClick={() => setPreferred(scope)}
            onKeyDown={(e) => navigate(e, scope)}
          >
            {scope === 'game'
              ? l('game')
              : t(scope === 'match' ? 'matchTab' : 'positionTab')}
          </button>
        ))}
      </div>
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      {!!importError && (
        <p role="alert" className="error">
          {formatError(importError)}
        </p>
      )}
      {scopes.map((scope) => (
        <div
          key={scope}
          className="notes-scope-panel"
          role="tabpanel"
          id={`${htmlId}-${scope}-panel`}
          aria-labelledby={`${htmlId}-${scope}-tab`}
          hidden={active !== scope}
        >
          {active === scope && <ImportNotes warnings={scopeWarnings} />}
          {active === scope && !suspended && (
            <AnnotationPanel
              key={`${active}:${annotationId}`}
              kind={active}
              id={annotationId}
              onSaved={onSaved}
            />
          )}
        </div>
      ))}
      {catalog && (
        <div className="notes-game-actions">
          <ActionButton
            icon="notes"
            label={l('cubeNotes')}
            onClick={onMoveNotes}
          />
        </div>
      )}
    </div>
  );
}
