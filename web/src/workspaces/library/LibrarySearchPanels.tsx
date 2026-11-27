import { useRef, type RefObject, type SetStateAction } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import ActionIcon from '../../shared/ui/action-icons';
import type { Position } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import AnnotationSearch, {
  type AnnotationHit,
} from '../analysis/notes/AnnotationSearch';
import { useLibraryText } from './library-messages';
import {
  emptyFilters,
  type Database,
  type LibraryFilters as Filters,
  type LibraryMatch,
} from './library-api';
import LibraryFilters, {
  LibraryFilterChips,
  type LibraryFilterKey,
} from './LibraryFilters';
import DuplicateMatchFinder from './DuplicateMatchFinder';

export type SearchPanel = 'filters' | 'notes' | 'position' | 'duplicates';

export default function LibrarySearchPanels({
  searchId,
  visible,
  loading,
  databaseId,
  databases,
  annotationRevision,
  duplicateRefreshToken,
  currentPosition,
  filters,
  draftFilters,
  searchPanel,
  onSearchPanel,
  exactPosition,
  positionInputError,
  onExactPositionChange,
  onSearchPosition,
  onSetFilters,
  onRemoveFilter,
  onAnnotation,
  selected,
  onSelect,
  onOpen,
  openingDisabled,
  filterToggleRef,
  positionToggleRef,
}: {
  searchId: string;
  visible: boolean;
  loading: boolean;
  databaseId: string;
  databases: Database[];
  annotationRevision: number;
  duplicateRefreshToken: number;
  currentPosition?: Position;
  filters: Filters;
  draftFilters: Filters;
  searchPanel?: SearchPanel;
  onSearchPanel: (
    next: SearchPanel | undefined | ((current?: SearchPanel) => SearchPanel | undefined),
  ) => void;
  exactPosition: string;
  positionInputError: boolean;
  onExactPositionChange: (value: string) => void;
  onSearchPosition: () => void;
  onSetFilters: (next: SetStateAction<Filters>) => void;
  onRemoveFilter: (key: LibraryFilterKey) => void;
  onAnnotation: (hit: AnnotationHit) => void;
  selected: ReadonlySet<string>;
  onSelect: (rows: readonly LibraryMatch[], checked: boolean) => void;
  onOpen: (match: LibraryMatch) => void;
  openingDisabled: (match: LibraryMatch) => boolean;
  filterToggleRef: RefObject<HTMLButtonElement | null>;
  positionToggleRef: RefObject<HTMLButtonElement | null>;
}) {
  const { t } = useI18n();
  const w = useWorkspaceText();
  const l = useLibraryText();
  const notesToggle = useRef<HTMLButtonElement>(null);
  const duplicatesToggle = useRef<HTMLButtonElement>(null);

  const searchPanels = [
    {
      id: 'filters',
      label: l('filters'),
      icon: 'filter',
      trigger: filterToggleRef,
    },
    { id: 'notes', label: w('notes'), icon: 'notes', trigger: notesToggle },
    {
      id: 'position',
      label: l('position'),
      icon: 'similar',
      trigger: positionToggleRef,
    },
    {
      id: 'duplicates',
      label: l('duplicates'),
      icon: 'duplicates',
      trigger: duplicatesToggle,
    },
  ] as const;

  return (
    <>
      <div className="library-search-toolbar">
        <label className="search-box">
          <ActionIcon name="search" size={16} />
          <input
            aria-label={t('app.searchMatches')}
            placeholder={t('app.searchPlaceholder')}
            value={draftFilters.q}
            onChange={(e) => onSetFilters((f) => ({ ...f, q: e.target.value }))}
          />
        </label>
        {searchPanels.map(({ id, label, icon, trigger }) => (
          <ActionButton
            key={id}
            ref={trigger}
            icon={icon}
            label={label}
            type="button"
            onClick={() =>
              onSearchPanel((current) => (current === id ? undefined : id))
            }
            aria-controls={`${searchId}-${id}`}
            aria-expanded={searchPanel === id}
          />
        ))}
      </div>
      <div
        onKeyDown={(event) => {
          if (
            event.key !== 'Escape' ||
            !searchPanel ||
            event.defaultPrevented
          )
            return;
          event.preventDefault();
          searchPanels
            .find(({ id }) => id === searchPanel)
            ?.trigger.current?.focus();
          onSearchPanel(undefined);
        }}
      >
        <LibraryFilters
          filters={draftFilters}
          onChange={onSetFilters}
          onClear={() => onSetFilters(emptyFilters())}
          panelId={`${searchId}-filters`}
          open={searchPanel === 'filters'}
        />
        <AnnotationSearch
          revision={annotationRevision}
          databaseId={databaseId}
          onOpen={onAnnotation}
          panelId={`${searchId}-notes`}
          open={searchPanel === 'notes'}
        />
        <div
          className="position-search"
          id={`${searchId}-position`}
          hidden={searchPanel !== 'position'}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              onSearchPosition();
            }}
          >
            <input
              aria-label={t('app.positionId')}
              placeholder={t('app.positionPlaceholder')}
              value={exactPosition}
              aria-invalid={positionInputError || undefined}
              onChange={(e) => onExactPositionChange(e.target.value)}
            />
            <ActionButton
              icon="search"
              label={t('app.exactSearch')}
              type="submit"
              disabled={loading || !exactPosition.trim()}
            />
            {currentPosition && (
              <ActionButton
                icon="view"
                label={t('app.usePosition')}
                type="button"
                onClick={() =>
                  onExactPositionChange(currentPosition.position_id)
                }
              />
            )}
          </form>
          {positionInputError && <p role="alert">{l('invalidPositionId')}</p>}
        </div>
        <DuplicateMatchFinder
          open={visible && searchPanel === 'duplicates'}
          panelId={`${searchId}-duplicates`}
          databaseId={databaseId}
          databases={databases}
          refreshToken={duplicateRefreshToken}
          selected={selected}
          onSelect={onSelect}
          onOpen={onOpen}
          openingDisabled={openingDisabled}
        />
      </div>
      <LibraryFilterChips filters={filters} onRemove={onRemoveFilter} />
    </>
  );
}
