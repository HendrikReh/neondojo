import ActionButton from '../../shared/ui/ActionButton';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useConfiguration } from '../configuration/ConfigurationProvider';
import type { ConfigurationProfile } from '../configuration/configuration-api';
import { useConfigurationErrors } from './configuration-error-messages';
import { useProfileText } from './profile-messages';
import { clippedTooltipText } from '../../shared/format/overflow-title';
import Tooltip from '../../shared/ui/Tooltip';
import ActionIcon from '../../shared/ui/action-icons';
import { useDetailsDismiss } from '../../shared/hooks/useDetailsDismiss';
import ProfileActionDialog from './ProfileActionDialog';
import ProfileChangesDialog from './ProfileChangesDialog';
import './profiles.css';

export default function ProfileSelector({
  onManage,
}: {
  onManage: () => void;
}) {
  const config = useConfiguration(),
    t = useProfileText(),
    errors = useConfigurationErrors();
  const [open, setOpen] = useState(false),
    [changesOpen, setChangesOpen] = useState(false),
    [pending, setPending] = useState<ConfigurationProfile>(),
    [actionError, setActionError] = useState<unknown>(),
    [warningsDismissed, setWarningsDismissed] = useState(false);
  const host = useRef<HTMLDivElement>(null),
    button = useRef<HTMLButtonElement>(null),
    notices = useDetailsDismiss();
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
    mode: 'keep' | 'save_current' = 'keep',
  ) => {
    setActionError(undefined);
    try {
      await config.activateProfile(profile, mode);
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
    } else void perform(profile);
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
  // Warnings and failed switches collapse into one badge beside the profile
  // so the header keeps a single row.
  const warnings = warningsDismissed ? [] : config.initializationWarnings;
  const headerError = !pending ? actionError : undefined;
  const noticeCount =
    warnings.length + config.availabilityIssues.length + (headerError ? 1 : 0);
  // A failed switch answers the user's action, so its notice opens.
  useEffect(() => {
    if (headerError && notices.ref.current) notices.ref.current.open = true;
  }, [headerError]);
  if (!config.ready)
    return (
      <div className="profile-selector">
        {config.error ? (
          <>
            <span role="alert">{errors.message(config.error)}</span>
            <ActionButton
              icon="refresh"
              label={t('retry')}
              type="button"
              onClick={() => void config.refresh()}
            />
            <ActionButton
              icon="settings"
              label={t('manage')}
              type="button"
              onClick={onManage}
            />
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
        <ActionButton
          icon="warning"
          label={t('failed')}
          type="button"
          onClick={onManage}
        />
      )}
      {noticeCount > 0 && (
        <details
          className="profile-notices"
          data-severity={headerError ? 'error' : 'warning'}
          ref={notices.ref}
          onKeyDown={notices.onKeyDown}
        >
          <Tooltip
            as="summary"
            text={t('notices', { count: noticeCount })}
            aria-label={t('notices', { count: noticeCount })}
          >
            <ActionIcon name="warning" size={16} />
            <span aria-hidden="true">{noticeCount}</span>
          </Tooltip>
          <div
            className="profile-notices-panel"
            role="group"
            aria-label={t('notices', { count: noticeCount })}
          >
            {warnings.length > 0 && (
              <div role="status">
                {warnings.map((warning, index) => (
                  <p key={index}>{errors.issue(warning)}</p>
                ))}
                <ActionButton
                  icon="confirm"
                  label={t('acknowledge')}
                  type="button"
                  onClick={() => setWarningsDismissed(true)}
                />
              </div>
            )}
            {config.availabilityIssues.length > 0 && (
              <div role="status" className="profile-availability">
                {config.availabilityIssues.map((issue, index) => (
                  <p key={index}>{errors.issue(issue)}</p>
                ))}
              </div>
            )}
            {!!headerError && (
              <div role="alert">
                <p>{errors.message(headerError)}</p>
                {errors.issues(headerError).map((issue, index) => (
                  <p key={index}>{errors.issue(issue)}</p>
                ))}
              </div>
            )}
          </div>
        </details>
      )}
      {open && (
        <div
          role="menu"
          aria-label={t('profiles')}
          className="profile-menu"
          onKeyDown={key}
        >
          {config.profiles.map((profile) => (
            <ActionButton
              role="menuitemradio"
              icon="settings"
              label={profile.name}
              aria-checked={profile.id === config.activeProfile?.id}
              key={profile.id}
              type="button"
              onClick={() => select(profile)}
            />
          ))}
          {config.dirty && (
            <ActionButton
              icon="view"
              label={t('showChanges')}
              role="menuitem"
              type="button"
              onClick={() => {
                close();
                setChangesOpen(true);
              }}
            />
          )}
          <ActionButton
            icon="settings"
            label={t('manage')}
            role="menuitem"
            type="button"
            onClick={() => {
              close();
              onManage();
            }}
          />
        </div>
      )}
      {changesOpen &&
        config.activeProfile &&
        config.current &&
        createPortal(
          <ProfileChangesDialog onClose={closeChanges} />,
          document.body,
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
              icon: 'save',
              label: t('saveSwitch'),
              primary: true,
              action: () => void perform(pending, 'save_current'),
            },
            {
              icon: 'settings',
              label: t('manage'),
              action: () => {
                setPending(undefined);
                onManage();
              },
            },
            {
              icon: 'clear',
              label: t('discardSwitch'),
              action: () => void perform(pending),
            },
          ]}
        />
      )}
    </div>
  );
}
