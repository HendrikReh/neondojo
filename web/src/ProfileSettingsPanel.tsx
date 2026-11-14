import { createPortal } from 'react-dom';
import { useId, useRef, useState } from 'react';
import ActionButton from './ActionButton';
import { useConfiguration } from './ConfigurationProvider';
import {
  ConfigurationApiError,
  type ConfigurationProfile,
} from './configuration-api';
import { useProfileText } from './profile-messages';
import { useI18n } from './i18n';
import LibraryDialog from './LibraryDialog';
import ProfileActionDialog from './ProfileActionDialog';
import ProfileSummary from './ProfileSummary';
import { profileChangeRows } from './profile-changes';
import './profiles.css';

export default function ProfileSettingsPanel() {
  const config = useConfiguration(),
    t = useProfileText(),
    { locale, formatNumber } = useI18n();
  const activeHeading = useId(),
    actionsId = useId(),
    saveTargetId = useId();
  const actions = useRef<HTMLDivElement>(null),
    actionsButton = useRef<HTMLButtonElement>(null),
    renameButton = useRef<HTMLButtonElement>(null);
  const [selectedId, setSelectedId] = useState<string>(),
    [error, setError] = useState<unknown>();
  const [dialog, setDialog] = useState<
    'switch' | 'overwrite' | 'delete' | 'restore' | 'reload'
  >();
  const [nameAction, setNameAction] = useState<
    | { kind: 'create' | 'recover' }
    | { kind: 'rename'; profile: ConfigurationProfile }
  >();
  const activeProfile = config.activeProfile;
  const selected =
    config.profiles.find((p) => p.id === selectedId) ?? activeProfile;
  const run = async (action: () => Promise<void>) => {
    setError(undefined);
    try {
      await action();
      setDialog(undefined);
    } catch (e) {
      setError(e);
    }
  };
  const activate = () => {
    if (!selected) return;
    if (config.dirty) setDialog('switch');
    else void run(() => config.activateProfile(selected, 'keep'));
  };
  function closeActions() {
    actions.current?.hidePopover();
    actionsButton.current?.focus();
  }
  function confirm(next: typeof dialog) {
    setError(undefined);
    setDialog(next);
  }
  function dismissDialog() {
    setDialog(undefined);
    setError(undefined);
  }
  const conflict = config.status === 'conflict';
  const controlsDisabled = config.busy || conflict;
  const report = conflict ? undefined : (error ?? config.error);
  if (!config.current || !selected || !activeProfile)
    return <p role="status">{t('loading')}</p>;
  const active = selected.id === activeProfile.id;
  const changeCount = profileChangeRows(
    activeProfile.configuration,
    config.current,
  ).length;
  const dateFormat = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
  const choices: Record<
    NonNullable<typeof dialog>,
    {
      title: string;
      message: string;
      choices: { label: string; action: () => void }[];
    }
  > = {
    switch: {
      title: t('switchTitle'),
      message: t('switchMessage', { name: selected.name }),
      choices: [
        {
          label: t('saveSwitch'),
          action: () =>
            void run(() => config.activateProfile(selected, 'save_current')),
        },
        {
          label: t('discardSwitch'),
          action: () =>
            void run(() => config.activateProfile(selected, 'keep')),
        },
      ],
    },
    overwrite: {
      title: t('overwriteTitle'),
      message: t('overwriteMessage', { name: selected.name }),
      choices: [
        {
          label: t('overwrite'),
          action: () => void run(() => config.overwriteProfile(selected)),
        },
      ],
    },
    delete: {
      title: t('deleteTitle'),
      message: t('deleteMessage', { name: selected.name }),
      choices: [
        {
          label: t('delete'),
          action: () => void run(() => config.deleteProfile(selected)),
        },
      ],
    },
    restore: {
      title: t('restoreSaved'),
      message: t('discardMessage', { name: activeProfile.name }),
      choices: [
        {
          label: t('restoreSaved'),
          action: () => void run(() => config.restoreProfile(activeProfile)),
        },
      ],
    },
    reload: {
      title: t('reloadServer'),
      message: t('reloadConfirm'),
      choices: [
        {
          label: t('discard'),
          action: () => void run(() => config.reloadServer()),
        },
      ],
    },
  };
  return (
    <section className="profile-settings" aria-label={t('profiles')}>
      <section className="profile-overview" aria-labelledby={activeHeading}>
        <div>
          <h3 id={activeHeading}>{t('activeProfile')}</h3>
          <strong className="profile-overview-name">
            {activeProfile.name}
          </strong>
          <p className="profile-help">
            {t('changeCount', { count: formatNumber(changeCount) })}
          </p>
        </div>
        {config.dirty && (
          <ActionButton
            type="button"
            icon="undo"
            label={t('restoreSaved')}
            disabled={controlsDisabled}
            onClick={() => confirm('restore')}
          />
        )}
      </section>
      {conflict && (
        <div role="alert" className="profile-conflict">
          <p>{t('conflict')}</p>
          <div className="profile-actions">
            <button
              type="button"
              disabled={config.busy}
              onClick={() => setDialog('reload')}
            >
              {t('reloadServer')}
            </button>
            <button
              type="button"
              disabled={config.busy}
              onClick={() => setNameAction({ kind: 'recover' })}
            >
              {t('saveConflict')}
            </button>
          </div>
        </div>
      )}
      <div className="profile-list" role="list" aria-label={t('profiles')}>
        {config.profiles.map((profile) => (
          <button
            key={profile.id}
            type="button"
            disabled={config.busy}
            aria-pressed={profile.id === selected.id}
            onClick={() => {
              setSelectedId(profile.id);
              setError(undefined);
            }}
          >
            <strong>{profile.name}</strong>
            {profile.id === activeProfile.id && <span> · {t('active')}</span>}
            {profile.updated_at && (
              <time dateTime={profile.updated_at}>
                {dateFormat.format(new Date(profile.updated_at))}
              </time>
            )}
          </button>
        ))}
      </div>
      <div className="profile-actions profile-settings-actions">
        <ActionButton
          type="button"
          icon="save"
          className="primary"
          label={t('overwrite')}
          aria-describedby={saveTargetId}
          disabled={controlsDisabled}
          onClick={() => confirm('overwrite')}
        />
        <ActionButton
          type="button"
          icon="add"
          label={t('create')}
          disabled={controlsDisabled}
          onClick={() => setNameAction({ kind: 'create' })}
        />
        {!active && (
          <ActionButton
            type="button"
            icon="confirm"
            label={t('activate')}
            disabled={controlsDisabled}
            onClick={activate}
          />
        )}
        <ActionButton
          ref={actionsButton}
          type="button"
          icon="more"
          label={t('profileActions')}
          popoverTarget={actionsId}
          disabled={controlsDisabled}
        />
        <div
          id={actionsId}
          ref={actions}
          popover="auto"
          role="group"
          aria-label={t('profileActions')}
          className="profile-settings-menu"
          onToggle={(event) => {
            if (event.newState === 'open') renameButton.current?.focus();
          }}
          onKeyDown={(event) => {
            if (event.key !== 'Escape') return;
            event.preventDefault();
            event.stopPropagation();
            closeActions();
          }}
        >
          <ActionButton
            ref={renameButton}
            type="button"
            icon="edit"
            label={t('rename')}
            disabled={controlsDisabled}
            onClick={() => {
              closeActions();
              setNameAction({ kind: 'rename', profile: selected });
            }}
          />
          <ActionButton
            type="button"
            icon="delete"
            label={t('delete')}
            disabled={controlsDisabled || active}
            onClick={() => {
              closeActions();
              confirm('delete');
            }}
          />
          {active && <p className="profile-help">{t('deleteActive')}</p>}
        </div>
      </div>
      <p className="profile-help profile-save-target" id={saveTargetId}>
        {t('saveTarget', { name: selected.name })}
      </p>
      {!!report && (
        <div role="alert" className="error">
          <p>{report instanceof Error ? report.message : String(report)}</p>
          {report instanceof ConfigurationApiError &&
            report.issues.length > 0 && (
              <ul>
                {report.issues.map((issue, index) => (
                  <li key={index}>
                    {issue.path}: {issue.message}
                  </li>
                ))}
              </ul>
            )}
          {config.status === 'error' && (
            <button
              type="button"
              disabled={config.busy}
              onClick={() => void run(() => config.flush())}
            >
              {t('retry')}
            </button>
          )}
        </div>
      )}
      {config.storageError && <p role="status">{t('recoveryUnavailable')}</p>}
      <ProfileSummary
        configuration={selected.configuration}
        compareTo={config.current}
      />
      {nameAction && (
        <ProfileNameDialog
          title={t(
            nameAction.kind === 'recover'
              ? 'saveConflict'
              : nameAction.kind === 'rename'
                ? 'renameTitle'
                : 'create',
          )}
          actionLabel={t(nameAction.kind === 'rename' ? 'rename' : 'create')}
          profile={
            nameAction.kind === 'rename' ? nameAction.profile : undefined
          }
          busy={config.busy}
          onClose={() => setNameAction(undefined)}
          onSave={async (name) => {
            if (nameAction.kind === 'rename') {
              await config.renameProfile(nameAction.profile, name);
            } else {
              if (nameAction.kind === 'recover')
                await config.saveConflictAsNewProfile(name);
              else await config.createProfile(name);
              setSelectedId(undefined);
            }
            setError(undefined);
            setNameAction(undefined);
            if (nameAction.kind === 'rename')
              requestAnimationFrame(() => actionsButton.current?.focus());
          }}
        />
      )}
      {dialog && (
        <ProfileActionDialog
          {...choices[dialog]}
          busy={config.busy}
          error={error}
          lockScroll={false}
          onCancel={dismissDialog}
        >
          {dialog === 'overwrite' && (
            <ProfileSummary
              configuration={selected.configuration}
              compareTo={config.current}
              differencesOnly
            />
          )}
        </ProfileActionDialog>
      )}
    </section>
  );
}

function ProfileNameDialog({
  title,
  actionLabel,
  profile,
  busy,
  onClose,
  onSave,
}: {
  title: string;
  actionLabel: string;
  profile?: ConfigurationProfile;
  busy: boolean;
  onClose: () => void;
  onSave: (name: string) => Promise<void>;
}) {
  const t = useProfileText();
  const feedbackId = useId();
  const [name, setName] = useState(profile?.name ?? '');
  const [error, setError] = useState<unknown>();
  const trimmed = name.trim();
  const valid = !!trimmed && [...trimmed].length <= 80 && !/\p{Cc}/u.test(name);
  const nameError =
    error instanceof ConfigurationApiError &&
    ['duplicate_name', 'invalid_name'].includes(error.code);
  const invalid = !!name && !valid;
  return createPortal(
    <LibraryDialog
      title={title}
      onClose={onClose}
      native
      lockScroll={false}
      initialFocus="first-field"
      busy={busy}
    >
      <form
        className="library-dialog-form"
        onSubmit={async (event) => {
          event.preventDefault();
          if (!valid || trimmed === profile?.name || busy) return;
          setError(undefined);
          try {
            await onSave(name);
          } catch (failure) {
            setError(failure);
          }
        }}
      >
        <label className="profile-name-label">
          {t('name')}
          <input
            value={name}
            maxLength={160}
            disabled={busy}
            aria-invalid={invalid || nameError || undefined}
            aria-describedby={invalid || nameError ? feedbackId : undefined}
            onChange={(event) => {
              setName(event.target.value);
              setError(undefined);
            }}
          />
        </label>
        {(invalid || Boolean(error)) && (
          <p id={feedbackId} role="alert" className="error">
            {invalid
              ? t('invalidName')
              : error instanceof Error
                ? error.message
                : String(error)}
          </p>
        )}
        <div className="library-dialog-actions">
          <button type="button" onClick={onClose} disabled={busy}>
            {t('cancel')}
          </button>
          <ActionButton
            type="submit"
            icon="save"
            className="primary"
            label={actionLabel}
            disabled={busy || !valid || trimmed === profile?.name}
          />
        </div>
      </form>
    </LibraryDialog>,
    document.body,
  );
}
