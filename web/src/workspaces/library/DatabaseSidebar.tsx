import Tooltip from '../../shared/ui/Tooltip';
import ActionButton from '../../shared/ui/ActionButton';
import ActionIcon from '../../shared/ui/action-icons';
import { databaseDisplayName, useLibraryText } from './library-messages';
import {
  BOT_DATABASE_ID,
  PAUSED_MATCHES_DATABASE_ID,
  type Database,
} from './library-api';
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
          <span className="database-collection-label">
            <ActionIcon name="deck" className="database-collection-icon" />
            <span>{l('allDatabases')}</span>
          </span>
          <small>{databases.reduce((n, d) => n + d.match_count, 0)}</small>
        </button>
        {databases.map((db) => {
          const description = db.description.trim();
          const repeatsName =
            description.toLowerCase() === db.name.trim().toLowerCase();
          const defaultDescription =
            db.id === PAUSED_MATCHES_DATABASE_ID
              ? l('pausedMatchesDescription')
              : db.id === BOT_DATABASE_ID
                ? l('botMatchesDescription')
                : '';
          const tooltip = [
            db.is_system ? l('systemDatabase') : '',
            (!repeatsName && description) || defaultDescription,
          ]
            .filter(Boolean)
            .join(' · ');
          return (
            <Tooltip
              as="button"
              key={db.id}
              className={selected === db.id ? 'active' : ''}
              aria-pressed={selected === db.id}
              onClick={() => onSelect(db.id)}
              text={tooltip || undefined}
            >
              <span className="database-collection-label">
                <ActionIcon
                  name={db.is_system ? 'system-database' : 'database'}
                  className="database-collection-icon"
                />
                <span>{databaseDisplayName(db, l)}</span>
              </span>
              <small>{db.match_count}</small>
            </Tooltip>
          );
        })}
      </div>
      {active && (
        <div className="database-collection-actions">
          <p>{active.description}</p>
          <div className="database-collection-buttons">
            <ActionButton
              icon="edit"
              label={l('editDatabase')}
              compact
              onClick={() => onEdit(active)}
            />
            {active.id !== PAUSED_MATCHES_DATABASE_ID && (
              <ActionButton
                icon="delete"
                label={l(active.is_system ? 'clearDatabase' : 'deleteDatabase')}
                compact
                className="danger"
                disabled={active.is_system && active.match_count === 0}
                onClick={() => onDelete(active)}
              />
            )}
          </div>
        </div>
      )}
    </aside>
  );
}
