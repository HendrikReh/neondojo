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
        <button onClick={onNew} aria-label={l('newDatabase')}>
          ＋
        </button>
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
        {databases.map((db) => (
          <button
            key={db.id}
            className={selected === db.id ? 'active' : ''}
            aria-pressed={selected === db.id}
            onClick={() => onSelect(db.id)}
            title={db.description}
          >
            <span>▤ {db.name}</span>
            <small>{db.match_count}</small>
          </button>
        ))}
      </div>
      {active && (
        <div className="database-collection-actions">
          <p>{active.description}</p>
          <button onClick={() => onEdit(active)}>{l('editDatabase')}</button>
          {!active.is_default && (
            <button onClick={() => onDelete(active)}>
              {l('deleteDatabase')}
            </button>
          )}
        </div>
      )}
    </aside>
  );
}
