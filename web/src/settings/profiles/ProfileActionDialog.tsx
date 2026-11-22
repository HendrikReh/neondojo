import { createPortal } from 'react-dom';
import type { ReactNode } from 'react';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { useProfileText } from './profile-messages';
import { ConfigurationApiError } from '../configuration/configuration-api';
import { profileFieldKey } from './profile-messages';

export default function ProfileActionDialog({
  title,
  message,
  children,
  choices,
  onCancel,
  busy = false,
  lockScroll = true,
  error,
}: {
  title: string;
  message: string;
  children?: ReactNode;
  /** A primary choice is the recommended answer. */
  choices: { label: string; action: () => void; primary?: boolean }[];
  onCancel: () => void;
  busy?: boolean;
  lockScroll?: boolean;
  error?: unknown;
}) {
  const t = useProfileText();
  return createPortal(
    <LibraryDialog
      title={title}
      onClose={onCancel}
      busy={busy}
      native
      lockScroll={lockScroll}
    >
      <p>{message}</p>
      {children}
      {!!error && (
        <div role="alert" className="error">
          <p>{error instanceof Error ? error.message : String(error)}</p>
          {error instanceof ConfigurationApiError &&
            error.issues.length > 0 && (
              <ul>
                {error.issues.map((issue, index) => (
                  <li key={index}>
                    {profileFieldKey(issue.path)
                      ? t(profileFieldKey(issue.path)!)
                      : issue.path}
                    : {issue.message}
                  </li>
                ))}
              </ul>
            )}
        </div>
      )}
      <div className="profile-actions">
        {choices.map((choice) => (
          <button
            key={choice.label}
            type="button"
            className={choice.primary ? 'primary' : undefined}
            disabled={busy}
            onClick={choice.action}
          >
            {choice.label}
          </button>
        ))}
        <button type="button" disabled={busy} onClick={onCancel}>
          {t('cancel')}
        </button>
      </div>
    </LibraryDialog>,
    document.body,
  );
}
