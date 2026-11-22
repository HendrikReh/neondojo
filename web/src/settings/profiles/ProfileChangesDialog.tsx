import { useRef, useState } from 'react';
import { useConfiguration } from '../configuration/ConfigurationProvider';
import {
  ConfigurationApiError,
  type ConfigurationProfile,
} from '../configuration/configuration-api';
import ActionButton from '../../shared/ui/ActionButton';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import ProfileSummary from './ProfileSummary';
import { profileFieldKey, useProfileText } from './profile-messages';

export default function ProfileChangesDialog({
  onClose,
}: {
  onClose: () => void;
}) {
  const config = useConfiguration(),
    t = useProfileText();
  const [error, setError] = useState<unknown>(),
    [restored, setRestored] = useState<'field' | 'all'>(),
    [confirmation, setConfirmation] =
      useState<Pick<ConfigurationProfile, 'id' | 'revision' | 'name'>>();
  const content = useRef<HTMLDivElement>(null),
    bulkButton = useRef<HTMLButtonElement>(null),
    cancelButton = useRef<HTMLButtonElement>(null);
  const focusDialog = () =>
    requestAnimationFrame(() =>
      content.current?.closest<HTMLElement>('[role="dialog"]')?.focus(),
    );
  function confirmAll() {
    if (!config.activeProfile || config.busy || config.status === 'conflict')
      return;
    setError(undefined);
    setRestored(undefined);
    const { id, revision, name } = config.activeProfile;
    setConfirmation({ id, revision, name });
    requestAnimationFrame(() => cancelButton.current?.focus());
  }
  function cancelAll() {
    setConfirmation(undefined);
    requestAnimationFrame(() => {
      if (bulkButton.current) bulkButton.current.focus();
      else content.current?.closest<HTMLElement>('[role="dialog"]')?.focus();
    });
  }
  async function restoreAll() {
    if (!confirmation || config.busy || config.status === 'conflict') return;
    setError(undefined);
    setRestored(undefined);
    try {
      await config.restoreProfile(confirmation);
      setConfirmation(undefined);
      setRestored('all');
    } catch (failure) {
      setError(failure);
    } finally {
      focusDialog();
    }
  }
  async function restore(path: string) {
    if (
      !config.activeProfile ||
      config.busy ||
      confirmation ||
      config.status === 'conflict'
    )
      return;
    setError(undefined);
    setRestored(undefined);
    try {
      await config.restoreProfileField(config.activeProfile, path);
      setRestored('field');
    } catch (failure) {
      setError(failure);
    } finally {
      // The selected row may have disappeared; keep keyboard focus in the dialog.
      focusDialog();
    }
  }
  if (!config.activeProfile || !config.current) return null;
  const report =
    error ?? (config.status === 'conflict' ? config.error : undefined);
  return (
    <LibraryDialog
      title={`${t('profileChanges')}: ${config.activeProfile.name}`}
      onClose={confirmation ? cancelAll : onClose}
      busy={config.busy}
      native
      initialFocus="dialog"
    >
      <div ref={content} aria-busy={config.busy}>
        {config.busy && <p role="status">{t('saving')}</p>}
        {restored && !config.busy && (
          <p role="status">
            {t(restored === 'all' ? 'allRestored' : 'fieldRestored')}
          </p>
        )}
        {!!report && (
          <div role="alert" className="error">
            <p>{report instanceof Error ? report.message : String(report)}</p>
            {report instanceof ConfigurationApiError &&
              report.issues.map((issue, index) => (
                <p key={index}>
                  {profileFieldKey(issue.path)
                    ? t(profileFieldKey(issue.path)!)
                    : issue.path}
                  : {issue.message}
                </p>
              ))}
            {config.status === 'conflict' && <p>{t('conflict')}</p>}
          </div>
        )}
        {confirmation ? (
          <section
            role="group"
            aria-label={t('discardTitle')}
            className="profile-reset-all-confirmation"
          >
            <p>{t('discardMessage', { name: confirmation.name })}</p>
            <div className="profile-actions">
              <ActionButton
                type="button"
                icon="undo"
                label={t('resetAll')}
                disabled={config.busy || config.status === 'conflict'}
                onClick={() => void restoreAll()}
              />
              <button
                ref={cancelButton}
                type="button"
                disabled={config.busy}
                onClick={cancelAll}
              >
                {t('cancel')}
              </button>
            </div>
          </section>
        ) : (
          config.dirty && (
            <div className="profile-actions">
              <ActionButton
                ref={bulkButton}
                type="button"
                icon="undo"
                label={t('resetAll')}
                disabled={config.busy || config.status === 'conflict'}
                onClick={confirmAll}
              />
            </div>
          )
        )}
        <ProfileSummary
          configuration={config.activeProfile.configuration}
          compareTo={config.current}
          differencesOnly
          onResetField={(path) => void restore(path)}
          resetDisabled={
            config.busy || Boolean(confirmation) || config.status === 'conflict'
          }
        />
      </div>
    </LibraryDialog>
  );
}
