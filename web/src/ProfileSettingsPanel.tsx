import { useState } from 'react';
import { useConfiguration } from './ConfigurationProvider';
import {
  ConfigurationApiError,
  type ConfigurationProfile,
} from './configuration-api';
import { useProfileText } from './profile-messages';
import ProfileActionDialog from './ProfileActionDialog';
import ProfileSummary from './ProfileSummary';
import './profiles.css';

export default function ProfileSettingsPanel() {
  const config = useConfiguration(),
    t = useProfileText();
  const [selectedId, setSelectedId] = useState(config.activeProfile?.id),
    [nameDraft, setNameDraft] = useState<{
      profileId: string;
      value: string;
    } | null>(null),
    [error, setError] = useState<unknown>();
  const [dialog, setDialog] = useState<
    'switch' | 'overwrite' | 'delete' | 'discard' | 'reload'
  >();
  const selected =
    config.profiles.find((p) => p.id === selectedId) ?? config.activeProfile;
  const name =
    nameDraft && nameDraft.profileId === selected?.id
      ? nameDraft.value
      : (selected?.name ?? '');
  const setName = (value: string) => {
    if (selected) setNameDraft({ profileId: selected.id, value });
  };
  const validName =
    !!name.trim() && [...name.trim()].length <= 80 && !/\p{Cc}/u.test(name);
  const run = async (action: () => Promise<void>) => {
    setError(undefined);
    try {
      await action();
      setDialog(undefined);
    } catch (e) {
      setError(e);
    }
  };
  const create = () => {
    // Follow the active profile before the request releases its input lock.
    // A later selection effect must not erase the user's next name edit.
    setSelectedId(undefined);
    void run(() => config.createProfile(name));
  };
  const activate = () => {
    if (!selected) return;
    if (config.dirty) setDialog('switch');
    else void run(() => config.activateProfile(selected, 'keep'));
  };
  const active = selected?.id === config.activeProfile?.id;
  const controlsDisabled = config.busy || Boolean(dialog);
  const report = error ?? config.error;
  if (!config.current || !selected) return <p role="status">{t('loading')}</p>;
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
    discard: {
      title: t('discardTitle'),
      message: t('discardMessage', { name: config.activeProfile!.name }),
      choices: [
        {
          label: t('discard'),
          action: () => void run(() => config.restoreProfile()),
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
      <div className="profile-list" role="list" aria-label={t('profiles')}>
        {config.profiles.map((profile: ConfigurationProfile) => (
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
            {profile.id === config.activeProfile?.id && (
              <span> · {t('active')}</span>
            )}
            {profile.updated_at && (
              <small>{new Date(profile.updated_at).toLocaleString()}</small>
            )}
          </button>
        ))}
      </div>
      <label className="profile-name-label">
        {t('name')}
        <input
          value={name}
          maxLength={160}
          onChange={(e) => setName(e.target.value)}
          disabled={controlsDisabled}
          aria-invalid={
            (report instanceof ConfigurationApiError &&
              ['duplicate_name', 'invalid_name'].includes(report.code)) ||
            undefined
          }
        />
      </label>
      {!validName && <p>{t('invalidName')}</p>}
      <div className="profile-actions">
        <button
          type="button"
          disabled={config.busy || !validName || config.status === 'conflict'}
          onClick={create}
        >
          {t('create')}
        </button>
        <button
          type="button"
          disabled={
            config.busy ||
            !validName ||
            name.trim() === selected.name ||
            config.status === 'conflict'
          }
          onClick={() => void run(() => config.renameProfile(selected, name))}
        >
          {t('rename')}
        </button>
        <button
          type="button"
          disabled={config.busy || active || config.status === 'conflict'}
          onClick={activate}
        >
          {t('activate')}
        </button>
        <button
          type="button"
          disabled={config.busy || config.status === 'conflict'}
          onClick={() => {
            setError(undefined);
            setDialog('overwrite');
          }}
        >
          {t('overwrite')}
        </button>
        <button
          type="button"
          disabled={config.busy || active || config.status === 'conflict'}
          onClick={() => {
            setError(undefined);
            setDialog('delete');
          }}
        >
          {t('delete')}
        </button>
        <button
          type="button"
          disabled={
            config.busy || !config.dirty || config.status === 'conflict'
          }
          onClick={() => {
            setError(undefined);
            setDialog('discard');
          }}
        >
          {t('discardChanges')}
        </button>
      </div>
      {active && <p className="profile-help">{t('deleteActive')}</p>}
      {config.status === 'conflict' && (
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
              disabled={config.busy || !validName}
              onClick={() =>
                void run(() => config.saveConflictAsNewProfile(name))
              }
            >
              {t('saveConflict')}
            </button>
          </div>
        </div>
      )}
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
      {dialog && (
        <ProfileActionDialog
          {...choices[dialog]}
          busy={config.busy}
          error={error}
          onCancel={() => {
            setDialog(undefined);
            setError(undefined);
          }}
        >
          {dialog === 'overwrite' && (
            <ProfileSummary
              configuration={selected.configuration}
              compareTo={config.current}
            />
          )}
        </ProfileActionDialog>
      )}
    </section>
  );
}
