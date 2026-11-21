import { useId } from 'react';
import { ImportWarningList } from './ImportWarningList';
import { useLibraryText, type ImportWarning } from './library-messages';

/** Read-only source irregularities shown above a match or game comment. */
export function ImportNotes({
  warnings,
}: {
  warnings: readonly ImportWarning[];
}) {
  const l = useLibraryText(),
    id = useId();
  if (!warnings.length) return null;
  return (
    <section className="import-notes" aria-labelledby={id}>
      <h3 id={id}>{l('importNotes')}</h3>
      <p className="import-notes-hint">{l('importNotesHint')}</p>
      <ImportWarningList warnings={warnings} />
    </section>
  );
}
