import { matchEventLabels } from '../../shared/i18n/i18n-core';
import {
  defineMessages,
  fromRows,
  type MessageRow,
} from '../../shared/i18n/messages';

const entries = {
  id: ['ID', 'ID', 'ID', 'ID', 'ID', 'ID'],
  length: ['Length', 'Länge', 'Longitud', 'Longueur', 'Długość', 'Lunghezza'],
  result: ['Result', 'Ergebnis', 'Resultado', 'Résultat', 'Wynik', 'Risultato'],
  played: ['Played', 'Gespielt', 'Jugado', 'Joué', 'Rozegrano', 'Giocato'],
  event: [
    matchEventLabels.en,
    matchEventLabels.de,
    matchEventLabels.es,
    matchEventLabels.fr,
    matchEventLabels.pl,
    matchEventLabels.it,
  ],
  whitePr: [
    'PR White',
    'PR Weiß',
    'PR Blancas',
    'PR Blancs',
    'PR Białe',
    'PR Bianchi',
  ],
  blackPr: [
    'PR Black',
    'PR Schwarz',
    'PR Negras',
    'PR Noirs',
    'PR Czarne',
    'PR Neri',
  ],
  finalScore: [
    'Final score',
    'Endstand',
    'Marcador final',
    'Score final',
    'Wynik końcowy',
    'Punteggio finale',
  ],
  ongoing: [
    'Ongoing',
    'Unvollständig',
    'En curso',
    'En cours',
    'W toku',
    'In corso',
  ],
  retainedPosition: [
    'Original match removed; saved position retained',
    'Originalmatch gelöscht; gemerkte Position bleibt erhalten',
    'Match original eliminado; posición guardada conservada',
    'Match original supprimé ; position enregistrée conservée',
    'Oryginalny mecz usunięty; zapisana pozycja zachowana',
    'Match originale eliminato; posizione salvata conservata',
  ],
  loadError: [
    'Could not load match details.',
    'Matchdetails konnten nicht geladen werden.',
    'No se pudieron cargar los detalles del match.',
    'Impossible de charger les détails du match.',
    'Nie udało się wczytać szczegółów meczu.',
    'Impossibile caricare i dettagli del match.',
  ],
  retry: [
    'Retry',
    'Erneut versuchen',
    'Reintentar',
    'Réessayer',
    'Spróbuj ponownie',
    'Riprova',
  ],
} satisfies Record<string, MessageRow>;

const messages = defineMessages('analysis-match-header', fromRows(entries));

export const useAnalysisMatchHeaderText = messages.useText;
