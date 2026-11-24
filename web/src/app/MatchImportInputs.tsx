import { useI18n } from '../shared/i18n/i18n';
import {
  MATCH_FILE_ACCEPT,
  matchFileEntries,
  selectFolderMatchFiles,
} from '../workspaces/library/import/match-import';
import { useLibraryText } from '../workspaces/library/library-messages';
import type { MatchImporter } from './useMatchImporter';

/** The hidden choosers for match files and match folders. */
export default function MatchImportInputs({
  importer,
}: {
  importer: MatchImporter;
}) {
  const { t } = useI18n();
  const l = useLibraryText();
  return (
    <>
      <input
        ref={importer.input}
        className="sr-only"
        type="file"
        aria-label={t('app.matchFile')}
        accept={MATCH_FILE_ACCEPT}
        multiple
        onChange={(event) => {
          const destination = importer.takeDestination();
          void importer.upload(
            { files: matchFileEntries(event.target.files ?? []) },
            destination,
          );
        }}
      />
      <input
        ref={importer.folderInput}
        className="sr-only"
        type="file"
        aria-label={l('matchFolder')}
        multiple
        onChange={(event) => {
          const destination = importer.takeDestination();
          const { files, skipped } = selectFolderMatchFiles(
            matchFileEntries(event.target.files ?? []),
          );
          // Choosing the same folder again must fire another change.
          event.target.value = '';
          void importer.upload({ files, folder: { skipped } }, destination);
        }}
      />
    </>
  );
}
