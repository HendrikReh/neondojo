import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useConfiguration } from './ConfigurationProvider';
import {
  ConfigurationApiError,
  type ConfigurationProfile,
} from './configuration-api';
import { useProfileText } from './profile-messages';
import { clippedTooltipText } from './overflow-title';
import Tooltip from './Tooltip';
import ProfileActionDialog from './ProfileActionDialog';
import ProfileChangesDialog from './ProfileChangesDialog';
import './profiles.css';

export default function ProfileSelector({
  onManage,
}: {
  onManage: () => void;
}) {
  const config = useConfiguration(),
    t = useProfileText();
  const [open, setOpen] = useState(false),
    [changesOpen, setChangesOpen] = useState(false),
    [pending, setPending] = useState<ConfigurationProfile>(),
    [actionError, setActionError] = useState<unknown>(),
    [warningsDismissed, setWarningsDismissed] = useState(false);
  const host = useRef<HTMLDivElement>(null),
    button = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    host.current?.querySelector<HTMLElement>('[role=menuitemradio]')?.focus();
    const outside = (e: PointerEvent) => {
      if (!host.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open]);
  const close = () => {
    setOpen(false);
    button.current?.focus();
  };
  const closeChanges = () => {
    setChangesOpen(false);
    // The native modal must leave the top layer before its trigger can focus.
    requestAnimationFrame(() => button.current?.focus());
  };
  const perform = async (
    profile: ConfigurationProfile,
    save: 'keep' | 'save_current',
  ) => {
    setActionError(undefined);
    try {
      await config.activateProfile(profile, save);
      setPending(undefined);
    } catch (error) {
      setActionError(error);
    }
  };
  const select = (profile: ConfigurationProfile) => {
    close();
    if (profile.id === config.activeProfile?.id) return;
    if (config.dirty) {
      setPending(profile);
      setActionError(undefined);
    } else void perform(profile, 'keep');
  };
  const key = (event: KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    }
    if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
      event.preventDefault();
      const items = Array.from(
        host.current?.querySelectorAll<HTMLButtonElement>('[role^=menuitem]') ??
          [],
      );
      const index = items.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? items.length - 1
            : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) %
              items.length;
      items[next]?.focus();
    }
  };
  if (!config.ready)
    return (
      <div className="profile-selector">
        {config.error ? (
          <>
            <span role="alert">
              {config.error instanceof Error
                ? config.error.message
                : t('failed')}
            </span>
            <button type="button" onClick={() => void config.refresh()}>
              {t('retry')}
            </button>
          </>
        ) : (
          <span role="status">{t('loading')}</span>
        )}
      </div>
    );
  return (
    <div className="profile-selector" ref={host}>
      <Tooltip
        as="button"
        text={clippedTooltipText}
        ref={button}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={config.busy}
        onClick={() => setOpen(!open)}
      >
        {t('profile')}:{' '}
        <span className="profile-name" data-tooltip-overflow>
          {config.activeProfile?.name}
        </span>
        {config.dirty && <span> · {t('modified')}</span>}
      </Tooltip>
      {config.status === 'saving' && <small role="status">{t('saving')}</small>}
      {(config.status === 'error' || config.status === 'conflict') && (
        <button type="button" onClick={onManage}>
          {t('failed')}
        </button>
      )}
      {!warningsDismissed && config.initializationWarnings.length > 0 && (
        <div role="status">
          {config.initializationWarnings.map((warning, index) => (
            <p key={index}>{warning.message}</p>
          ))}
          <button type="button" onClick={() => setWarningsDismissed(true)}>
            {t('acknowledge')}
          </button>
        </div>
      )}
      {config.availabilityIssues.length > 0 && (
        <div role="status" className="profile-availability">
          {config.availabilityIssues.map((issue, index) => (
            <p key={index}>
              {issue.path}: {issue.message}
            </p>
          ))}
        </div>
      )}
      {open && (
        <div
          role="menu"
          aria-label={t('profiles')}
          className="profile-menu"
          onKeyDown={key}
        >
          {config.profiles.map((profile) => (
            <button
              role="menuitemradio"
              aria-checked={profile.id === config.activeProfile?.id}
              key={profile.id}
              type="button"
              onClick={() => select(profile)}
            >
              {profile.name}
            </button>
          ))}
          {config.dirty && (
            <button
              role="menuitem"
              type="button"
              onClick={() => {
                close();
                setChangesOpen(true);
              }}
            >
              {t('showChanges')}
            </button>
          )}
          <button
            role="menuitem"
            type="button"
            onClick={() => {
              close();
              onManage();
            }}
          >
            {t('manage')}
          </button>
        </div>
      )}
      {changesOpen &&
        config.activeProfile &&
        config.current &&
        createPortal(
          <ProfileChangesDialog onClose={closeChanges} />,
          document.body,
        )}
      {!!actionError && !pending && (
        <div role="alert">
          <p>
            {actionError instanceof Error
              ? actionError.message
              : String(actionError)}
          </p>
          {actionError instanceof ConfigurationApiError &&
            actionError.issues.map((issue, index) => (
              <p key={index}>
                {issue.path}: {issue.message}
              </p>
            ))}
        </div>
      )}
      {pending && (
        <ProfileActionDialog
          title={t('switchTitle')}
          message={t('switchMessage', { name: pending.name })}
          busy={config.busy}
          error={actionError}
          onCancel={() => setPending(undefined)}
          choices={[
            {
              label: t('saveSwitch'),
              action: () => void perform(pending, 'save_current'),
            },
            {
              label: t('discardSwitch'),
              action: () => void perform(pending, 'keep'),
            },
          ]}
        />
      )}
    </div>
  );
}
