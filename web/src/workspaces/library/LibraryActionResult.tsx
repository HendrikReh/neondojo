import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import type { ActionResponse } from './library-api';
import { useLibraryText, type LibraryMessageKey } from './library-messages';

const resultKey: Record<string, LibraryMessageKey> = {
  copied: 'copied',
  moved: 'moved',
  deleted: 'deleted',
  tagged: 'tagged',
  duplicate: 'duplicate',
  same_database: 'sameDatabase',
  not_found: 'notFound',
  busy: 'busy',
  wrong_database: 'notFound',
  duplicate_request: 'duplicate',
};
/** What an action did with each match. */
export default function LibraryActionResult({
  result,
  names,
}: {
  result: ActionResponse;
  /** The names of the matches when the action ran. */
  names: Readonly<Record<string, string>>;
}) {
  const w = useWorkspaceText(),
    l = useLibraryText();
  return (
    <div className="library-action-result" role="status">
      {Object.entries(
        result.results.reduce<Record<string, number>>(
          (totals, r) => ({
            ...totals,
            [r.status]: (totals[r.status] ?? 0) + 1,
          }),
          {},
        ),
      ).map(([status, count]) => (
        <span key={status}>
          {l(resultKey[status] ?? 'error')}: {count}{' '}
        </span>
      ))}
      {result.results.length > 0 && (
        <details>
          <summary>{w('details')}</summary>
          <ul>
            {result.results.map((r, i) => (
              <li key={`${r.match_id}:${i}`}>
                <strong>{names[r.match_id] ?? r.match_id.slice(0, 8)}</strong>:{' '}
                {l(resultKey[r.status] ?? 'error')}
                {r.error ? ` · ${r.error}` : ''}
                {(r.created_id || r.existing_id) && (
                  <small title={r.created_id ?? r.existing_id ?? ''}>
                    {' '}
                    · {l('target')}:{' '}
                    {(r.created_id ?? r.existing_id)?.slice(0, 8)}
                  </small>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
