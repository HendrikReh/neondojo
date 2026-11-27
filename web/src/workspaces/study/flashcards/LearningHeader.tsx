import type { ReactNode, RefObject } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { useCardPracticeText } from './card-practice-messages';
import { useDeckLibraryText } from './deck-library-messages';
import type { NextCard } from './learning-api';
import { useLearningText } from './learning-messages';
import type { LibraryDialogKind } from './LearningDialogs';

export default function LearningHeader({
  reviewing,
  studying,
  inPracticeRound,
  activeNext,
  deckSelector,
  ankiBusy,
  externalAnkiInput,
  onStopStudy,
  onOpenLibraryDialog,
  onImportExternal,
}: {
  reviewing: boolean;
  studying: boolean;
  inPracticeRound: boolean;
  activeNext: NextCard | null;
  deckSelector: ReactNode;
  ankiBusy: boolean;
  externalAnkiInput: RefObject<HTMLInputElement | null>;
  onStopStudy: () => void;
  onOpenLibraryDialog: (dialog: LibraryDialogKind) => void;
  onImportExternal: (files: File[]) => void;
}) {
  const t = useLearningText();
  const practiceT = useCardPracticeText();
  const libraryText = useDeckLibraryText();

  const libraryActions = (
    <div
      className={
        reviewing ? 'learning-controls learning-study-bar' : 'learning-controls'
      }
    >
      {reviewing && (
        <>
          <ActionButton
            icon="back"
            label={t('back')}
            className="subtle"
            onClick={onStopStudy}
          >
            {t('back')}
          </ActionButton>
          {deckSelector}
        </>
      )}
      <ActionButton
        icon="open"
        label={libraryText('decks')}
        onClick={() => onOpenLibraryDialog('decks')}
      >
        {libraryText('decks')}
      </ActionButton>
      {reviewing && activeNext && (
        <p className="learning-counts">
          {t('due', { count: activeNext.counts.due })} ·{' '}
          {t('newCards', { count: activeNext.counts.new })}
          {activeNext.counts.new_available != null &&
            activeNext.counts.new > activeNext.counts.new_available && (
              <>
                {' '}
                (
                {t('newToday', {
                  count: activeNext.counts.new_available,
                })}
                )
              </>
            )}
        </p>
      )}
      {!studying && (
        <ActionButton
          icon="search"
          label={libraryText('duplicates')}
          onClick={() => onOpenLibraryDialog('duplicates')}
        >
          {libraryText('duplicates')}
        </ActionButton>
      )}
      {!studying && !inPracticeRound && (
        <>
          <ActionButton
            icon="refresh"
            label={practiceT('ankiOverview')}
            onClick={() => onOpenLibraryDialog('anki')}
          >
            {practiceT('ankiOverview')}
          </ActionButton>
          <input
            ref={externalAnkiInput}
            type="file"
            hidden
            accept=".apkg"
            multiple
            onChange={(event) => {
              const files = Array.from(event.target.files ?? []);
              event.target.value = '';
              onImportExternal(files);
            }}
          />
          <ActionButton
            icon="open"
            label={practiceT('externalAnkiImport')}
            disabled={ankiBusy}
            onClick={() => externalAnkiInput.current?.click()}
          >
            {practiceT('externalAnkiImport')}
          </ActionButton>
        </>
      )}
    </div>
  );

  if (reviewing) return libraryActions;
  return (
    <div className="learning-heading">
      <div>
        <h1>{t('title')}</h1>
        <p>{t('description')}</p>
      </div>
      {libraryActions}
    </div>
  );
}
