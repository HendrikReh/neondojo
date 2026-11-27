import type { ReactNode } from 'react';
import ActionButton from '../shared/ui/ActionButton';
import LibraryDialog from '../shared/ui/LibraryDialog';
import { useProfileText } from './profiles/profile-messages';
import { useSettingsText } from './settings-messages';

export type GuardedDestination = 'close' | 'profiles' | 'book' | 'benchmark';

export default function SettingsDraftGuardDialog({
  id,
  destination,
  saving,
  canApply,
  applyHint,
  saveFeedback,
  onCancel,
  onDiscard,
  onApplyAndContinue,
}: {
  id: string;
  destination: GuardedDestination;
  saving: boolean;
  canApply: boolean;
  applyHint: string;
  saveFeedback: ReactNode;
  onCancel: () => void;
  onDiscard: () => void;
  onApplyAndContinue: () => void;
}) {
  const t = useSettingsText();
  const profileT = useProfileText();

  return (
    <LibraryDialog
      native
      lockScroll={false}
      title={profileT('draftGuardTitle')}
      onClose={onCancel}
      busy={saving}
      initialFocus="dialog"
    >
      <p>{profileT('draftGuardMessage')}</p>
      {!!applyHint && (
        <p id={`${id}-guard-target`} className="settings-help">
          {applyHint}
        </p>
      )}
      {saveFeedback}
      <div className="library-dialog-actions settings-draft-actions">
        <ActionButton
          icon="edit"
          label={t('keepEditing')}
          type="button"
          disabled={saving}
          onClick={onCancel}
        />
        <ActionButton
          icon="clear"
          label={profileT('discardDraft')}
          type="button"
          disabled={saving}
          onClick={onDiscard}
        />
        <ActionButton
          icon="save"
          label={
            saving
              ? t('saving')
              : destination === 'close'
                ? profileT('apply')
                : profileT('applyAndContinue')
          }
          type="button"
          className="settings-save"
          aria-describedby={applyHint ? `${id}-guard-target` : undefined}
          disabled={!canApply}
          onClick={onApplyAndContinue}
        />
      </div>
    </LibraryDialog>
  );
}
