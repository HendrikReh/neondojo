import { createPortal } from 'react-dom';
import { useId, useRef, useState } from 'react';
import LazyPanel from '../../app/LazyPanel';
import ActionButton from '../../shared/ui/ActionButton';
import { useConfiguration } from '../configuration/ConfigurationProvider';
import type { ConfigurationProfile } from '../configuration/configuration-api';
import {
  hasNameIssue,
  useConfigurationErrors,
} from './configuration-error-messages';
import { useProfileText } from './profile-messages';
import { useI18n } from '../../shared/i18n/i18n';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import ProfileActionDialog, {
  type ProfileActionChoice,
} from './ProfileActionDialog';
import ProfileSummary from './ProfileSummary';
import { profileChangeRows } from './profile-changes';
import './profiles.css';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';

export default function ProfileSettingsPanel() {
  const config = useConfiguration(),
    t = useProfileText(),
    errors = useConfigurationErrors(),
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
  const reportIssues = errors.issues(report);
  if (!config.current || !selected || !activeProfile)
    return report ? (
      <div>
        <p role="alert">{errors.message(report)}</p>
        <ActionButton
          icon="refresh"
          label={t('retry')}
          type="button"
          onClick={() => void config.refresh()}
        />
      </div>
    ) : (
      <p role="status">{t('loading')}</p>
    );
  const active = selected.id === activeProfile.id;
  const changeCount = profileChangeRows(
    activeProfile.configuration,
    config.current,
  ).length;
  const dateFormat = dateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
  const choices: Record<
    NonNullable<typeof dialog>,
    {
      title: string;
      message: string;
      choices: ProfileActionChoice[];
    }
  > = {
    switch: {
      title: t('switchTitle'),
      message: t('switchMessage', { name: selected.name }),
      choices: [
        {
          icon: 'save',
          label: t('saveSwitch'),
          primary: true,
          action: () =>
            void run(() => config.activateProfile(selected, 'save_current')),
        },
        {
          icon: 'clear',
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
          icon: 'save',
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
          icon: 'delete',
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
          icon: 'undo',
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
          icon: 'clear',
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
            <ActionButton
              icon="refresh"
              label={t('reloadServer')}
              type="button"
              disabled={config.busy}
              onClick={() => setDialog('reload')}
            />
            <ActionButton
              icon="save"
              label={t('saveConflict')}
              type="button"
              disabled={config.busy}
              onClick={() => setNameAction({ kind: 'recover' })}
            />
          </div>
        </div>
      )}
      <div className="profile-list" role="list" aria-label={t('profiles')}>
        {config.profiles.map((profile) => (
          <ActionButton
            key={profile.id}
            type="button"
            icon="view"
            label={
              profile.id === activeProfile.id
                ? `${profile.name} · ${t('active')}`
                : profile.name
            }
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
          </ActionButton>
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
          <p>{errors.message(report)}</p>
          {reportIssues.length > 0 && (
            <ul>
              {reportIssues.map((issue, index) => (
                <li key={index}>{errors.issue(issue)}</li>
              ))}
            </ul>
          )}
          {config.status === 'error' && (
            <ActionButton
              icon="refresh"
              label={t('retry')}
              type="button"
              disabled={config.busy}
              onClick={() => void run(() => config.flush())}
            />
          )}
        </div>
      )}
      {config.storageError && <p role="status">{t('recoveryUnavailable')}</p>}
      <LazyPanel key={selected.id}>
        <ProfileSummary
          configuration={selected.configuration}
          compareTo={config.current}
        />
      </LazyPanel>
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
            <LazyPanel>
              <ProfileSummary
                configuration={selected.configuration}
                compareTo={config.current}
                differencesOnly
              />
            </LazyPanel>
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
  const errors = useConfigurationErrors();
  const feedbackId = useId();
  const [name, setName] = useState(profile?.name ?? '');
  const [error, setError] = useState<unknown>();
  const trimmed = name.trim();
  const valid = !!trimmed && [...trimmed].length <= 80 && !/\p{Cc}/u.test(name);
  const nameError = hasNameIssue(error);
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
              : errors.message(error)}
          </p>
        )}
        <div className="library-dialog-actions">
          <ActionButton
            icon="cancel"
            label={t('cancel')}
            type="button"
            onClick={onClose}
            disabled={busy}
          />
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
