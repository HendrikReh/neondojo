import Tooltip from './Tooltip';
import ActionButton from './ActionButton';
import { useLibraryText } from './library-messages';
import type { Database } from './library-api';
export default function DatabaseSidebar({
  databases,
  selected,
  onSelect,
  onNew,
  onEdit,
  onDelete,
}: {
  databases: Database[];
  selected: string;
  onSelect: (id: string) => void;
  onNew: () => void;
  onEdit: (db: Database) => void;
  onDelete: (db: Database) => void;
}) {
  const l = useLibraryText();
  const active = databases.find((db) => db.id === selected);
  return (
    <aside className="database-sidebar" aria-label={l('databases')}>
      <div className="section-heading">
        <h2>{l('databases')}</h2>
        <ActionButton
          icon="add"
          label={l('newDatabase')}
          compact
          onClick={onNew}
          aria-label={l('newDatabase')}
        />
      </div>
      <div className="database-collections text-navigation text-navigation--stacked">
        <button
          className={!selected ? 'active' : ''}
          onClick={() => onSelect('')}
          aria-pressed={!selected}
        >
          <span>▦ {l('allDatabases')}</span>
          <small>{databases.reduce((n, d) => n + d.match_count, 0)}</small>
        </button>
        {databases.map((db) => {
          const description = db.description.trim();
          const repeatsName =
            description.toLowerCase() === db.name.trim().toLowerCase();
          return (
            <Tooltip
              as="button"
              key={db.id}
              className={selected === db.id ? 'active' : ''}
              aria-pressed={selected === db.id}
              onClick={() => onSelect(db.id)}
              text={repeatsName ? undefined : description || undefined}
            >
              <span>▤ {db.name}</span>
              <small>{db.match_count}</small>
            </Tooltip>
          );
        })}
      </div>
      {active && (
        <div className="database-collection-actions">
          <p>{active.description}</p>
          <ActionButton
            icon="edit"
            label={l('editDatabase')}
            compact
            onClick={() => onEdit(active)}
          />
          {!active.is_default && (
            <ActionButton
              icon="delete"
              label={l('deleteDatabase')}
              compact
              onClick={() => onDelete(active)}
            />
          )}
        </div>
      )}
    </aside>
  );
}
