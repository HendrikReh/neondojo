import ActionButton from '../../shared/ui/ActionButton';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import type { LibraryMatch } from './library-api';
import { canAnalyzeMatch } from './library-match-summary';
import { useLibraryText } from './library-messages';

export type LibraryBatchAction = 'copy' | 'move' | 'tags' | 'export' | 'delete';

/** The actions for the selected matches. */
export default function LibraryBatchBar({
  selection,
  current,
  openingDisabled,
  onOpen,
  onResume,
  onEdit,
  onAnalyze,
  onBatch,
  onClear,
}: {
  selection: LibraryMatch[];
  /** The selection belongs to the shown result. */
  current: boolean;
  openingDisabled: (match: LibraryMatch) => boolean;
  onOpen: (match: LibraryMatch) => void;
  onResume?: (sessionId: string) => void;
  onEdit: (match: LibraryMatch) => void;
  onAnalyze?: (matches: LibraryMatch[], unavailableCount: number) => void;
  onBatch: (action: LibraryBatchAction) => void;
  onClear: () => void;
}) {
  const w = useWorkspaceText(),
    l = useLibraryText();
  const hasSelection = selection.length > 0;
  const pausedSelected = selection.some((match) => match.play_paused);
  const singleSelected = selection.length === 1 ? selection[0] : undefined;
  const resumeId =
    singleSelected &&
    (singleSelected.play_paused || singleSelected.play_phase !== 'match_over')
      ? singleSelected.play_session_id
      : undefined;
  const analyzableMatches = selection.filter(canAnalyzeMatch);
  function batchButton(action: LibraryBatchAction) {
    return (
      <ActionButton
        icon={action}
        label={l(action)}
        compact
        key={action}
        className={action === 'delete' ? 'danger' : undefined}
        disabled={
          !hasSelection ||
          !current ||
          (action === 'export' && selection.length > 50) ||
          (pausedSelected && ['copy', 'move', 'delete'].includes(action))
        }
        onClick={() => onBatch(action)}
      />
    );
  }
  return (
    <div
      className="library-batch-bar"
      data-has-selection={hasSelection}
      role="group"
      aria-label={l('actions')}
    >
      <strong role="status">
        {l('selectedMatches', { count: selection.length })}
      </strong>
      <div className="library-batch-group" data-group="single">
        <ActionButton
          type="button"
          icon="open"
          label={w('openMatch')}
          compact
          disabled={
            !singleSelected ||
            !!resumeId ||
            singleSelected.play_paused ||
            openingDisabled(singleSelected)
          }
          onClick={() => {
            if (singleSelected) onOpen(singleSelected);
          }}
        />
        <ActionButton
          type="button"
          icon="resume"
          label={l('resumePlay')}
          compact
          disabled={!current || !resumeId || !onResume}
          onClick={() => {
            if (resumeId) onResume?.(resumeId);
          }}
        />
        <ActionButton
          type="button"
          icon="edit"
          label={l('editMatch')}
          aria-haspopup="dialog"
          compact
          disabled={!current || !singleSelected}
          onClick={(event) => {
            event.currentTarget.focus();
            if (singleSelected) onEdit(singleSelected);
          }}
        />
      </div>
      <div className="library-batch-group" data-group="batch">
        <ActionButton
          type="button"
          icon="analysis"
          label={`${l('analyzeSelection')} …`}
          aria-haspopup="dialog"
          compact
          disabled={!current || !onAnalyze || !analyzableMatches.length}
          onClick={(event) => {
            event.currentTarget.focus();
            onAnalyze?.(
              analyzableMatches,
              selection.length - analyzableMatches.length,
            );
          }}
        />
        {(['copy', 'move', 'tags', 'export'] as const).map(batchButton)}
      </div>
      <div className="library-batch-group" data-group="destructive">
        {batchButton('delete')}
        <ActionButton
          icon="clear"
          label={l('clearSelection')}
          compact
          disabled={!hasSelection}
          onClick={onClear}
          aria-label={l('clearSelection')}
        />
      </div>
    </div>
  );
}
