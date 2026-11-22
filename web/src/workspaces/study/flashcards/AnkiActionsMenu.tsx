import { useRef } from 'react';
import { useDetailsDismiss } from '../../../shared/hooks/useDetailsDismiss';
import ActionButton from '../../../shared/ui/ActionButton';
import { useCardPracticeText } from './card-practice-messages';
import type { Deck } from './learning-api';
import { useLearningText } from './learning-messages';

/** The Anki menu of a deck: export, send, sync reviews and import. */
export default function AnkiActionsMenu({
  deck,
  exporting,
  busy,
  onExport,
  onSend,
  onSync,
  onImport,
}: {
  deck: Deck;
  exporting: boolean;
  busy: boolean;
  onExport: () => void;
  onSend: () => void;
  onSync: () => void;
  onImport: (files: File[]) => void;
}) {
  const t = useLearningText();
  const practiceT = useCardPracticeText();
  const ankiMenu = useDetailsDismiss();
  const ankiInput = useRef<HTMLInputElement>(null);
  return (
    <details
      className="learning-anki-menu"
      ref={ankiMenu.ref}
      onKeyDown={ankiMenu.onKeyDown}
    >
      <summary>Anki</summary>
      <div
        className="learning-anki-menu-items"
        role="group"
        aria-label="Anki"
        onClick={(event) => {
          if (
            event.target instanceof Element &&
            event.target.closest('button') &&
            ankiMenu.ref.current
          )
            ankiMenu.ref.current.open = false;
        }}
      >
        <ActionButton
          icon="export"
          label={exporting ? t('exporting') : t('export')}
          disabled={!deck.card_count || exporting}
          onClick={onExport}
        >
          {exporting ? t('exporting') : t('export')}
        </ActionButton>
        <ActionButton
          icon="link"
          label={practiceT('sendAnki')}
          disabled={!deck.card_count || busy}
          onClick={onSend}
        >
          {practiceT('sendAnki')}
        </ActionButton>
        <ActionButton
          icon="refresh"
          label={practiceT('syncAnkiReviews')}
          disabled={!deck.card_count || busy}
          onClick={onSync}
        >
          {practiceT('syncAnkiReviews')}
        </ActionButton>
        <input
          ref={ankiInput}
          type="file"
          hidden
          accept=".apkg"
          multiple
          onChange={(event) => {
            const files = Array.from(event.target.files ?? []);
            event.target.value = '';
            onImport(files);
          }}
        />
        <ActionButton
          icon="open"
          label={practiceT('ankiImport')}
          disabled={busy}
          onClick={() => ankiInput.current?.click()}
        >
          {practiceT('ankiImport')}
        </ActionButton>
      </div>
    </details>
  );
}
