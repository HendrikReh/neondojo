import ActionButton from '../../../shared/ui/ActionButton';
import { TabList } from '../../../shared/ui/TabList';
import { useEffect, useId, useState } from 'react';
import AnnotationPanel from './AnnotationPanel';
import type { AnnotationKind } from './annotation-api';
import { useAnnotationI18n } from './annotation-messages';
import { ImportNotes } from './ImportNotes';
import {
  loadDecisions,
  loadImportWarnings,
  type GameDecisions,
} from '../../library/library-api';
import {
  useLibraryText,
  type ImportWarning,
} from '../../library/library-messages';
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
  return (
    <div className="notes-panel">
      <TabList<(typeof scopes)[number]>
        className="notes-scope-tabs text-navigation text-navigation--compact"
        ariaLabel={t('scopeLabel')}
        active={active as (typeof scopes)[number]}
        onSelect={setPreferred}
        tabs={scopes.map((scope) => ({
          value: scope,
          id: `${htmlId}-${scope}-tab`,
          controls: `${htmlId}-${scope}-panel`,
          disabled: !available.includes(scope),
          label:
            scope === 'game'
              ? l('game')
              : t(scope === 'match' ? 'match' : 'position'),
        }))}
      />
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
