import { useState } from 'react';
import { request } from '../../../shared/http/api';
import { AppError } from '../../../shared/i18n/i18n-core';
import { useI18n } from '../../../shared/i18n/i18n';
import { useCardPracticeText } from './card-practice-messages';
import { isCollectionScope } from './card-search-api';
import { sendDeckToAnki, syncAnkiReviews } from './learning-api';
import type { AddCardsResult, Deck, ImportedAnkiDeck } from './learning-api';

export type AnkiTransfers = ReturnType<typeof useAnkiTransfers>;

/** Imports Anki packages, sends decks to Anki and syncs reviews, one at a time. */
export function useAnkiTransfers({
  deckId,
  onImported,
  onReviewsSynced,
}: {
  deckId: string;
  onImported: () => void;
  /** Also after a failure, which can follow saved reviews. */
  onReviewsSynced: () => void;
}) {
  const practiceT = useCardPracticeText();
  const { locale, formatError, formatNumber } = useI18n();
  const [ankiImportProgress, setAnkiImportProgress] = useState<{
    file: string;
    current: number;
    total: number;
  }>();
  const [ankiNotice, setAnkiNotice] = useState<string>();
  const [ankiError, setAnkiError] = useState<string>();
  const [sendingAnki, setSendingAnki] = useState(false);
  const [syncingAnki, setSyncingAnki] = useState(false);
  const ankiBusy = !!ankiImportProgress || sendingAnki || syncingAnki;
  async function importAnki(files: File[], external = false) {
    if (!files.length || ankiBusy || (!external && isCollectionScope(deckId)))
      return;
    setAnkiNotice(undefined);
    setAnkiError(undefined);

    const path = external
      ? '/study/anki-imports'
      : `/decks/${deckId}/import.apkg`;
    const failures: string[] = [];
    const importedDecks = new Set<string>();
    let cards = 0;
    let added = 0;
    let existing = 0;

    try {
      for (const [index, file] of files.entries()) {
        setAnkiImportProgress({
          file: file.name,
          current: index + 1,
          total: files.length,
        });
        try {
          const body = new FormData();
          body.set('file', file);
          const options = { method: 'POST', body };
          if (external) {
            const imported = await request<ImportedAnkiDeck[]>(path, options);
            for (const deck of imported) {
              importedDecks.add(deck.name);
              cards += deck.cards;
            }
          } else {
            const result = await request<AddCardsResult>(path, options);
            added += result.added;
            existing += result.updated;
          }
        } catch (failure) {
          failures.push(
            practiceT('ankiImportFailed', {
              file: file.name,
              error: formatError(failure),
            }),
          );
        }
      }

      const successful = files.length - failures.length;
      const summary = [];
      if (files.length > 1) {
        summary.push(
          practiceT('ankiPackagesImported', {
            count: formatNumber(successful),
            total: formatNumber(files.length),
          }),
        );
      }
      if (successful) {
        summary.push(
          external
            ? practiceT('externalAnkiImported', {
                decks: formatNumber(importedDecks.size),
                cards: formatNumber(cards),
              })
            : practiceT('ankiImported', {
                added,
                existing: formatNumber(existing),
              }),
        );
        onImported();
      }
      if (summary.length) setAnkiNotice(summary.join(' · '));
      if (failures.length) setAnkiError(failures.join('\n'));
    } finally {
      setAnkiImportProgress(undefined);
    }
  }
  async function sendToAnki(deck: Deck) {
    if (ankiBusy) return;
    setSendingAnki(true);
    setAnkiNotice(undefined);
    setAnkiError(undefined);
    try {
      await sendDeckToAnki(deck.id, { locale });
      setAnkiNotice(practiceT('ankiSent', { deck: deck.name }));
    } catch (failure) {
      const status =
        failure instanceof AppError ? failure.params.status : undefined;
      setAnkiError(
        status === '503'
          ? practiceT('ankiUnavailable')
          : status === '504'
            ? practiceT('ankiUnconfirmed')
            : formatError(failure),
      );
    } finally {
      setSendingAnki(false);
    }
  }
  async function syncReviews(deck: Deck) {
    if (ankiBusy) return;
    setSyncingAnki(true);
    setAnkiNotice(undefined);
    setAnkiError(undefined);
    try {
      const result = await syncAnkiReviews(deck.id);
      setAnkiNotice(
        practiceT('ankiReviewsSynced', {
          deck: deck.name,
          profile: result.profile,
          received: result.received_reviews,
          sent: result.sent_reviews,
          matched: result.matched_cards,
          unmatched: formatNumber(result.unmatched_cards),
        }),
      );
    } catch (failure) {
      const status =
        failure instanceof AppError ? failure.params.status : undefined;
      setAnkiError(
        status === '503' ? practiceT('ankiUnavailable') : formatError(failure),
      );
    } finally {
      // Anki may have stopped responding after NeonDojo saved incoming reviews.
      onReviewsSynced();
      setSyncingAnki(false);
    }
  }
  return {
    busy: ankiBusy,
    progress: ankiImportProgress,
    sending: sendingAnki,
    syncing: syncingAnki,
    notice: ankiNotice,
    error: ankiError,
    importPackages: importAnki,
    send: sendToAnki,
    sync: syncReviews,
  };
}
