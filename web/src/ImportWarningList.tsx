import {
  importWarningText,
  useLibraryText,
  type ImportWarning,
} from './library-messages';

/** Source irregularities an import resolved, listed under its result. */
export function ImportWarningList({
  warnings,
}: {
  warnings: readonly ImportWarning[] | undefined;
}) {
  const l = useLibraryText();
  if (!warnings?.length) return null;
  return (
    <ul className="import-warnings">
      {warnings.map((warning, i) => (
        <li key={i}>{importWarningText(warning, l)}</li>
      ))}
    </ul>
  );
}
