import type { RefObject } from 'react';
import type { Theme } from '../../../shared/board/themes';
import AnkiSyncOverview from './AnkiSyncOverview';
import type { SpecialPractice } from './CardPracticeActions';
import CardSpecialPractice from './CardSpecialPractice';
import CreateDeckForm from './CreateDeckForm';
import {
  CardBatchDialog,
  DeckLibraryDialog,
  DuplicateCardsDialog,
  type CardBatchAction,
} from './DeckLibraryDialogs';
import DeckManagementDialog from './DeckManagementDialog';
import type { VersionedDeck } from './deck-management-api';
import type { Deck } from './learning-api';

export type LibraryDialogKind = 'decks' | 'duplicates' | 'anki';
export type DeckManagementRequest = {
  mode: 'rename' | 'delete';
  deck: VersionedDeck;
};

export default function LearningDialogs({
  visible,
  theme,
  decks,
  specialPractice,
  specialPracticeTrigger,
  libraryDialog,
  cardAction,
  management,
  onCloseSpecialPractice,
  onCloseLibraryDialog,
  onCreateDeck,
  onOpenDeck,
  onManageDeck,
  onSyncedAnki,
  onCloseCardAction,
  onCompletedCardAction,
  onRefreshCardActions,
  onCloseManagement,
  onRefreshDecks,
  onRenamedDeck,
  onDeletedDeck,
  onChanged,
}: {
  visible: boolean;
  theme: Theme;
  decks: VersionedDeck[];
  specialPractice?: SpecialPractice;
  specialPracticeTrigger: RefObject<HTMLButtonElement | null>;
  libraryDialog?: LibraryDialogKind;
  cardAction?: CardBatchAction;
  management: DeckManagementRequest | null;
  onCloseSpecialPractice: () => void;
  onCloseLibraryDialog: () => void;
  onCreateDeck: (deck: Deck) => void;
  onOpenDeck: (id: string) => void;
  onManageDeck: (mode: 'rename' | 'delete', deck: VersionedDeck) => void;
  onSyncedAnki: () => void;
  onCloseCardAction: () => void;
  onCompletedCardAction: (notice: string) => void;
  onRefreshCardActions: () => void;
  onCloseManagement: () => void;
  onRefreshDecks: () => Promise<VersionedDeck[]>;
  onRenamedDeck: (deck: VersionedDeck) => void;
  onDeletedDeck: (id: string) => void;
  onChanged: () => void;
}) {
  return (
    <>
      {specialPractice && visible && (
        <CardSpecialPractice
          {...specialPractice}
          theme={theme}
          returnFocus={specialPracticeTrigger}
          onClose={onCloseSpecialPractice}
        />
      )}
      {libraryDialog === 'decks' && (
        <DeckLibraryDialog
          decks={decks}
          close={onCloseLibraryDialog}
          create={<CreateDeckForm onCreated={onCreateDeck} />}
          open={onOpenDeck}
          manage={onManageDeck}
        />
      )}
      {libraryDialog === 'duplicates' && (
        <DuplicateCardsDialog
          close={onCloseLibraryDialog}
          onChanged={onChanged}
        />
      )}
      {libraryDialog === 'anki' && visible && (
        <AnkiSyncOverview
          close={onCloseLibraryDialog}
          onSynced={onSyncedAnki}
        />
      )}
      {cardAction && (
        <CardBatchDialog
          action={cardAction}
          decks={decks}
          close={onCloseCardAction}
          completed={onCompletedCardAction}
          refresh={onRefreshCardActions}
        />
      )}
      {management && (
        <DeckManagementDialog
          mode={management.mode}
          deck={management.deck}
          onClose={onCloseManagement}
          onRefresh={onRefreshDecks}
          onRenamed={onRenamedDeck}
          onDeleted={onDeletedDeck}
        />
      )}
    </>
  );
}
