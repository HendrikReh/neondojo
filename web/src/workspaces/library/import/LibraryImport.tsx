import { useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import { acceptsLibraryMatches, type Database } from '../library-api';
import { databaseDisplayName, useLibraryText } from '../library-messages';

export default function LibraryImport({
  databases,
  databaseId,
  uploading,
  onImport,
  onImportFolder,
}: {
  databases: readonly Database[];
  databaseId: string;
  uploading: boolean;
  onImport: (databaseId: string) => void;
  onImportFolder: (databaseId: string) => void;
}) {
  const { t } = useI18n();
  const l = useLibraryText();
  const [chosen, setChosen] = useState('');
  const destinations = databases.filter((database) =>
    acceptsLibraryMatches(database.id),
  );
  const target =
    destinations.find((database) => database.id === chosen) ??
    destinations.find((database) => database.id === databaseId) ??
    destinations.find((database) => database.is_default);

  return (
    <div className="library-import">
      <ActionButton
        icon="import"
        label={uploading ? t('app.importRunning') : t('app.importPlain')}
        className="primary"
        type="button"
        disabled={uploading || !target}
        onClick={() => target && onImport(target.id)}
      />
      <ActionButton
        icon="import-folder"
        label={l('importFolder')}
        tooltip={l('importFolderHelp')}
        type="button"
        disabled={uploading || !target}
        onClick={() => target && onImportFolder(target.id)}
      />
      <label className="library-import-destination">
        <span aria-hidden="true">{l('importInto')}</span>
        {/* The accessible name contains the visible label's word. */}
        <select
          aria-label={l('importIntoName')}
          value={target?.id ?? ''}
          disabled={uploading || !destinations.length}
          onChange={(event) => setChosen(event.target.value)}
        >
          {!target && <option value="">{l('importDestination')}</option>}
          {destinations.map((database) => (
            <option key={database.id} value={database.id}>
              {databaseDisplayName(database, l)}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
