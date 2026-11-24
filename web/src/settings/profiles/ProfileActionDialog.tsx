import { createPortal } from 'react-dom';
import type { ReactNode } from 'react';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import { useProfileText } from './profile-messages';
import { useConfigurationErrors } from './configuration-error-messages';

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
  const errors = useConfigurationErrors();
  const issues = errors.issues(error);
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
          <p>{errors.message(error)}</p>
          {issues.length > 0 && (
            <ul>
              {issues.map((issue, index) => (
                <li key={index}>{errors.issue(issue)}</li>
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
