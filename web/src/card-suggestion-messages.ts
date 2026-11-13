import { useI18n } from './i18n';
import type { Locale } from './i18n-core';

const messages = {
  en: {
    suggest: 'Suggest positions',
    loading: 'Finding positions for this match…',
    noAnalysis: 'This match has no completed local analysis yet.',
    empty: 'No decisions in this match meet the selected filters.',
    largestLoss: 'High equity loss in this match',
    recurring: 'Recurring in {count} matches',
    historyUnavailable:
      'The database is too large to include all recurring mistakes. The selection also uses equity loss in this match.',
    decision: 'Game {game} · Decision {decision}',
    inDeck: 'Already in this deck',
    known: 'Already in your card collection',
    add: 'Add selection to deck',
    changed: 'The match analysis changed. Load new suggestions.',
  },
  de: {
    suggest: 'Positionen vorschlagen',
    loading: 'Positionen für dieses Match werden gesucht …',
    noAnalysis:
      'Für dieses Match ist noch keine abgeschlossene lokale Analyse verfügbar.',
    empty: 'Keine Entscheidung in diesem Match erfüllt die gewählten Filter.',
    largestLoss: 'Hoher Equity-Verlust in diesem Match',
    recurring: 'Wiederkehrend in {count} Matches',
    historyUnavailable:
      'Die Datenbank ist zu groß, um alle wiederkehrenden Fehler zu berücksichtigen. Die Auswahl nutzt auch den Equity-Verlust in diesem Match.',
    decision: 'Spiel {game} · Entscheidung {decision}',
    inDeck: 'Bereits in diesem Deck',
    known: 'Bereits in deiner Kartensammlung',
    add: 'Auswahl ins Deck übernehmen',
    changed: 'Die Matchanalyse hat sich geändert. Lade neue Vorschläge.',
  },
  es: {
    suggest: 'Sugerir posiciones',
    loading: 'Buscando posiciones de este partido…',
    noAnalysis: 'Este partido aún no tiene un análisis local completo.',
    empty: 'Ninguna decisión de este partido cumple los filtros seleccionados.',
    largestLoss: 'Pérdida de equidad alta en este partido',
    recurring: 'Se repite en {count} partidos',
    historyUnavailable:
      'La base de datos es demasiado grande para incluir todos los errores recurrentes. La selección también usa la pérdida de equidad en este partido.',
    decision: 'Juego {game} · Decisión {decision}',
    inDeck: 'Ya está en este mazo',
    known: 'Ya está en tu colección de tarjetas',
    add: 'Añadir la selección al mazo',
    changed: 'El análisis del partido ha cambiado. Carga nuevas sugerencias.',
  },
  fr: {
    suggest: 'Suggérer des positions',
    loading: 'Recherche de positions dans ce match…',
    noAnalysis: 'Ce match ne dispose pas encore d’une analyse locale terminée.',
    empty: 'Aucune décision de ce match ne correspond aux filtres choisis.',
    largestLoss: 'Forte perte d’équité dans ce match',
    recurring: 'Se répète dans {count} matchs',
    historyUnavailable:
      'La base est trop volumineuse pour inclure toutes les erreurs récurrentes. La sélection tient aussi compte de la perte d’équité dans ce match.',
    decision: 'Partie {game} · Décision {decision}',
    inDeck: 'Déjà dans ce paquet',
    known: 'Déjà dans votre collection de cartes',
    add: 'Ajouter la sélection au paquet',
    changed: 'L’analyse du match a changé. Chargez de nouvelles suggestions.',
  },
  pl: {
    suggest: 'Zaproponuj pozycje',
    loading: 'Wyszukiwanie pozycji z tego meczu…',
    noAnalysis: 'Ten mecz nie ma jeszcze ukończonej analizy lokalnej.',
    empty: 'Żadna decyzja w tym meczu nie spełnia wybranych filtrów.',
    largestLoss: 'Duża strata equity w tym meczu',
    recurring: 'Powtarza się w {count} meczach',
    historyUnavailable:
      'Baza danych jest zbyt duża, aby uwzględnić wszystkie powtarzające się błędy. Wybór uwzględnia także stratę equity w tym meczu.',
    decision: 'Gra {game} · Decyzja {decision}',
    inDeck: 'Już w tej talii',
    known: 'Już w Twojej kolekcji fiszek',
    add: 'Dodaj wybrane pozycje do talii',
    changed: 'Analiza meczu uległa zmianie. Wczytaj nowe propozycje.',
  },
  it: {
    suggest: 'Suggerisci posizioni',
    loading: 'Ricerca di posizioni in questa partita…',
    noAnalysis:
      'Per questa partita non è ancora disponibile un’analisi locale completata.',
    empty: 'Nessuna decisione di questa partita corrisponde ai filtri scelti.',
    largestLoss: 'Perdita elevata di equity in questa partita',
    recurring: 'Si ripete in {count} partite',
    historyUnavailable:
      'Il database è troppo grande per includere tutti gli errori ricorrenti. La selezione usa anche la perdita di equity in questa partita.',
    decision: 'Gioco {game} · Decisione {decision}',
    inDeck: 'Già in questo mazzo',
    known: 'Già nella tua raccolta di schede',
    add: 'Aggiungi la selezione al mazzo',
    changed: 'L’analisi della partita è cambiata. Carica nuovi suggerimenti.',
  },
} satisfies Record<Locale, Record<string, string>>;

export function useCardSuggestionText() {
  const { locale } = useI18n();
  return messages[locale];
}
