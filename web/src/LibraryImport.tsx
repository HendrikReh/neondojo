import { useState } from 'react';
import ActionButton from './ActionButton';
import { useI18n } from './i18n';
import { acceptsLibraryMatches, type Database } from './library-api';
import { databaseDisplayName, useLibraryText } from './library-messages';

export default function LibraryImport({
  databases,
  databaseId,
  uploading,
  onImport,
}: {
  databases: readonly Database[];
  databaseId: string;
  uploading: boolean;
  onImport: (databaseId: string) => void;
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
      <select
        aria-label={l('importDestination')}
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
    </div>
  );
}
