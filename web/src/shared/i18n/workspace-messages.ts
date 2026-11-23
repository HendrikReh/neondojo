import { useI18n, type Locale } from './i18n';
import { interpolate, type TranslationParams } from './i18n-core';

const en = {
  analysis: 'Analysis',
  database: 'Match database',
  settings: 'Settings',
  openMatch: 'Open match',
  evaluation: 'Evaluation',
  notes: 'Notes',
  cards: 'Study cards',
  nextError: 'Next error',
  databaseTitle: 'Match database',
  databaseDescription:
    'Find matches and positions, read notes and open a match for analysis.',
  welcomeTitle: 'Your next move starts here.',
  welcomeDescription:
    'Open a match from your database or import a file to begin your analysis.',
  details: 'Match details',
  closeDetails: 'Close details',
  importedOn: 'Imported',
  format: 'Format',
  positions: 'Positions',
  matchNotes: 'Match tags and comment',
  noComment: 'No comment yet.',
  noTags: 'No tags yet.',
  loading: 'Loading …',
  engine: 'Local GNU Backgammon',
  navigation: 'Workspaces',
  positionDetails: 'Position details',
  retry: 'Try again',
  lastMatch: 'Restoring your last position …',
  analysisProfile: 'Default analysis profile',
  configurationError: 'Settings could not be loaded.',
  chunkFailed:
    'This part of NeonDojo could not be loaded. If NeonDojo was just updated, reload the page.',
  reloadPage: 'Reload page',
};
type Key = keyof typeof en;
type Catalog = Record<Key, string>;
const de: Catalog = {
  analysis: 'Analyse',
  database: 'Matchdatenbank',
  settings: 'Einstellungen',
  openMatch: 'Match öffnen',
  evaluation: 'Bewertung',
  notes: 'Notizen',
  cards: 'Lernkarten',
  nextError: 'Nächster Fehler',
  databaseTitle: 'Matchdatenbank',
  databaseDescription:
    'Matches und Positionen finden, Notizen lesen und ein Match zur Analyse öffnen.',
  welcomeTitle: 'Dein nächster Zug beginnt hier.',
  welcomeDescription:
    'Öffne ein Match aus deiner Datenbank oder importiere eine Datei für die Analyse.',
  details: 'Matchdetails',
  closeDetails: 'Details schließen',
  importedOn: 'Importiert',
  format: 'Format',
  positions: 'Positionen',
  matchNotes: 'Tags und Kommentar zum Match',
  noComment: 'Noch kein Kommentar.',
  noTags: 'Noch keine Tags.',
  loading: 'Wird geladen …',
  engine: 'Lokales GNU Backgammon',
  navigation: 'Arbeitsbereiche',
  positionDetails: 'Positionsdetails',
  retry: 'Erneut versuchen',
  lastMatch: 'Letzte Position wird wiederhergestellt …',
  analysisProfile: 'Standard-Analyseprofil',
  configurationError: 'Einstellungen konnten nicht geladen werden.',
  chunkFailed:
    'Dieser Teil von NeonDojo konnte nicht geladen werden. Wurde NeonDojo gerade aktualisiert, lade die Seite neu.',
  reloadPage: 'Seite neu laden',
};
const es: Catalog = {
  analysis: 'Análisis',
  database: 'Base de partidas',
  settings: 'Ajustes',
  openMatch: 'Abrir partida',
  evaluation: 'Evaluación',
  notes: 'Notas',
  cards: 'Tarjetas',
  nextError: 'Siguiente error',
  databaseTitle: 'Base de partidas',
  databaseDescription:
    'Busca partidas y posiciones, lee notas y abre una partida para analizar.',
  welcomeTitle: 'Tu próxima jugada empieza aquí.',
  welcomeDescription:
    'Abre una partida de tu base de datos o importa un archivo para empezar.',
  details: 'Detalles de la partida',
  closeDetails: 'Cerrar detalles',
  importedOn: 'Importado',
  format: 'Formato',
  positions: 'Posiciones',
  matchNotes: 'Etiquetas y comentario de la partida',
  noComment: 'Todavía no hay comentario.',
  noTags: 'Todavía no hay etiquetas.',
  loading: 'Cargando …',
  engine: 'GNU Backgammon local',
  navigation: 'Áreas de trabajo',
  positionDetails: 'Detalles de la posición',
  retry: 'Reintentar',
  lastMatch: 'Restaurando tu última posición …',
  analysisProfile: 'Perfil de análisis predeterminado',
  configurationError: 'No se pudieron cargar los ajustes.',
  chunkFailed:
    'No se pudo cargar esta parte de NeonDojo. Si NeonDojo se acaba de actualizar, recarga la página.',
  reloadPage: 'Recargar página',
};
const fr: Catalog = {
  analysis: 'Analyse',
  database: 'Base de matchs',
  settings: 'Paramètres',
  openMatch: 'Ouvrir un match',
  evaluation: 'Évaluation',
  notes: 'Notes',
  cards: 'Cartes',
  nextError: 'Erreur suivante',
  databaseTitle: 'Base de matchs',
  databaseDescription:
    'Recherchez des matchs et positions, lisez les notes et ouvrez un match à analyser.',
  welcomeTitle: 'Votre prochain coup commence ici.',
  welcomeDescription:
    'Ouvrez un match de votre base ou importez un fichier pour commencer.',
  details: 'Détails du match',
  closeDetails: 'Fermer les détails',
  importedOn: 'Importé le',
  format: 'Format',
  positions: 'Positions',
  matchNotes: 'Étiquettes et commentaire du match',
  noComment: 'Aucun commentaire.',
  noTags: 'Aucune étiquette.',
  loading: 'Chargement …',
  engine: 'GNU Backgammon local',
  navigation: 'Espaces de travail',
  positionDetails: 'Détails de la position',
  retry: 'Réessayer',
  lastMatch: 'Restauration de votre dernière position …',
  analysisProfile: 'Profil d’analyse par défaut',
  configurationError: 'Impossible de charger les paramètres.',
  chunkFailed:
    'Impossible de charger cette partie de NeonDojo. Si NeonDojo vient d’être mis à jour, rechargez la page.',
  reloadPage: 'Recharger la page',
};
const pl: Catalog = {
  analysis: 'Analiza',
  database: 'Baza meczów',
  settings: 'Ustawienia',
  openMatch: 'Otwórz mecz',
  evaluation: 'Ocena',
  notes: 'Notatki',
  cards: 'Fiszki',
  nextError: 'Następny błąd',
  databaseTitle: 'Baza meczów',
  databaseDescription:
    'Znajdź mecze i pozycje, czytaj notatki i otwórz mecz do analizy.',
  welcomeTitle: 'Twój następny ruch zaczyna się tutaj.',
  welcomeDescription:
    'Otwórz mecz z bazy lub zaimportuj plik, aby rozpocząć analizę.',
  details: 'Szczegóły meczu',
  closeDetails: 'Zamknij szczegóły',
  importedOn: 'Zaimportowano',
  format: 'Format',
  positions: 'Pozycje',
  matchNotes: 'Tagi i komentarz meczu',
  noComment: 'Brak komentarza.',
  noTags: 'Brak tagów.',
  loading: 'Ładowanie …',
  engine: 'Lokalny GNU Backgammon',
  navigation: 'Obszary robocze',
  positionDetails: 'Szczegóły pozycji',
  retry: 'Spróbuj ponownie',
  lastMatch: 'Przywracanie ostatniej pozycji …',
  analysisProfile: 'Domyślny profil analizy',
  configurationError: 'Nie można wczytać ustawień.',
  chunkFailed:
    'Nie można wczytać tej części NeonDojo. Jeśli NeonDojo zostało właśnie zaktualizowane, odśwież stronę.',
  reloadPage: 'Odśwież stronę',
};
const it: Catalog = {
  analysis: 'Analisi',
  database: 'Archivio partite',
  settings: 'Impostazioni',
  openMatch: 'Apri partita',
  evaluation: 'Valutazione',
  notes: 'Note',
  cards: 'Schede',
  nextError: 'Errore successivo',
  databaseTitle: 'Archivio partite',
  databaseDescription:
    'Trova partite e posizioni, leggi note e apri una partita da analizzare.',
  welcomeTitle: 'La tua prossima mossa inizia qui.',
  welcomeDescription:
    'Apri una partita dall’archivio o importa un file per iniziare.',
  details: 'Dettagli della partita',
  closeDetails: 'Chiudi dettagli',
  importedOn: 'Importato',
  format: 'Formato',
  positions: 'Posizioni',
  matchNotes: 'Tag e commento della partita',
  noComment: 'Nessun commento.',
  noTags: 'Nessun tag.',
  loading: 'Caricamento …',
  engine: 'GNU Backgammon locale',
  navigation: 'Aree di lavoro',
  positionDetails: 'Dettagli della posizione',
  retry: 'Riprova',
  lastMatch: 'Ripristino dell’ultima posizione …',
  analysisProfile: 'Profilo di analisi predefinito',
  configurationError: 'Impossibile caricare le impostazioni.',
  chunkFailed:
    'Impossibile caricare questa parte di NeonDojo. Se NeonDojo è stato appena aggiornato, ricarica la pagina.',
  reloadPage: 'Ricarica la pagina',
};
export const workspaceMessages = { en, de, es, fr, pl, it };
export function workspaceText(
  locale: Locale,
  key: Key,
  params: TranslationParams = {},
) {
  return interpolate(workspaceMessages[locale][key], params);
}
export function useWorkspaceText() {
  const { locale } = useI18n();
  return (key: Key, params?: TranslationParams) =>
    workspaceText(locale, key, params);
}
