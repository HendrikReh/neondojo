import ActionButton from '../shared/ui/ActionButton';
import { useI18n } from '../shared/i18n/i18n';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import ImportRunPanel from '../workspaces/library/import/ImportRunPanel';
import { ImportWarningList } from '../workspaces/library/import/ImportWarningList';
import type { MatchImporter } from './useMatchImporter';

/** Errors, notices and import progress above every workspace. */
export default function AppNotices({
  error,
  settingsError,
  importer,
  onCloseError,
  onRetrySettings,
}: {
  error: unknown;
  settingsError: unknown;
  importer: MatchImporter;
  onCloseError: () => void;
  onRetrySettings: () => void;
}) {
  const { t, formatError } = useI18n();
  const w = useWorkspaceText();
  return (
    <>
      {!!error && (
        <div className="error" role="alert">
          {formatError(error)}
          <ActionButton
            aria-label={t('app.closeError')}
            onClick={onCloseError}
            icon="close"
            label={t('app.closeError')}
            compact
          ></ActionButton>
        </div>
      )}
      {!!settingsError && (
        <div className="error" role="alert">
          {w('configurationError')} {formatError(settingsError)}
          <ActionButton
            onClick={onRetrySettings}
            icon="refresh"
            label={w('retry')}
            compact
          ></ActionButton>
        </div>
      )}
      {importer.notice && (
        <div className="notice" role="status">
          ✓{' '}
          {importer.notice.duplicate
            ? t('app.importDuplicate')
            : t('app.importCount', { count: importer.notice.count })}
          <ActionButton
            aria-label={t('app.closeNotice')}
            onClick={importer.closeNotice}
            icon="close"
            label={t('app.closeNotice')}
            compact
          ></ActionButton>
          <ImportWarningList warnings={importer.notice.warnings} />
        </div>
      )}
      {importer.run && (
        <ImportRunPanel
          run={importer.run}
          onStop={importer.stop}
          onClose={importer.closeRun}
        />
      )}
    </>
  );
}
