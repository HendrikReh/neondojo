import ActionButton from '../shared/ui/ActionButton';
import { useI18n } from '../shared/i18n/i18n';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import MatchDropZone from '../workspaces/library/import/MatchDropZone';
import type { MatchImporter } from './useMatchImporter';

/** Analysis without a match: open one or import one. */
export default function AppWelcome({
  importer,
  restoring,
  onOpenLibrary,
}: {
  importer: MatchImporter;
  restoring: boolean;
  onOpenLibrary: () => void;
}) {
  const { t } = useI18n();
  const w = useWorkspaceText();
  return (
    <section
      className="welcome analysis-welcome"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        importer.drop(event.dataTransfer);
      }}
    >
      <span className="eyebrow">NEONDOJO</span>
      <h1>{w('welcomeTitle')}</h1>
      <p>{w('welcomeDescription')}</p>
      <div className="welcome-actions">
        <ActionButton
          className="primary"
          onClick={onOpenLibrary}
          icon="open"
          label={w('openMatch')}
        >
          {w('openMatch')}
        </ActionButton>
        <ActionButton
          onClick={() => importer.chooseFiles()}
          disabled={importer.uploading}
          icon="import"
          label={
            importer.uploading ? t('app.importRunning') : t('app.importPlain')
          }
        >
          {importer.uploading ? t('app.importRunning') : t('app.importPlain')}
        </ActionButton>
        <MatchDropZone disabled={importer.uploading} onDrop={importer.drop} />
      </div>
      {restoring && <p role="status">{w('lastMatch')}</p>}
    </section>
  );
}
