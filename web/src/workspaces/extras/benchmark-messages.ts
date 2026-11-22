import { useI18n, type Locale } from '../../shared/i18n/i18n';
import {
  formatMessage,
  type TranslationParams,
} from '../../shared/i18n/i18n-core';
const en = {
  globalBusy:
    'Benchmark running or waiting. Other engine actions wait until measurement finishes.',
  extras: 'Extras',
  benchmark: 'Benchmark',
  navigation: 'Extra tools',
  title: 'Engine benchmark',
  reference: 'Fixed reference',
  referenceFacts:
    '{points, plural, one {# point} other {# points}} · {games, plural, one {# game} other {# games}} · {moves, plural, one {# checker move} other {# checker moves}}',
  referenceVersion: 'Reference version {version}',
  protocol: 'Protocol',
  timeout:
    'Timeout: {seconds, plural, one {# second} other {# seconds}} per game',
  noTimeout: 'No time limit',
  engine: 'Benchmark engine',
  plies: 'Analysis depth',
  start: 'Start benchmark',
  starting: 'Starting …',
  noEngine:
    'No benchmark engine is available. Check the installation in Settings.',
  timing:
    'Total time includes engine startup, loading, checker and cube analysis, and result preparation. Queueing and database writes are outside the measured pipeline. The average divides that total by the number of analyzed checker moves.',
  serial:
    'Selected depths run from lowest to highest. Runs continue when you leave this view.',
  engineBusy: 'Other engine work waits until the benchmark queue finishes.',
  active: 'Current runs',
  history: 'History',
  empty: 'No benchmark runs yet.',
  loading: 'Loading …',
  retry: 'Try again',
  queued: 'Queued',
  running: 'Running',
  completed: 'Completed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  waiting:
    'Waiting for engine availability; other engine work must finish first.',
  progress:
    '{completed} / {total} games · {moves} / {totalMoves} checker moves',
  cancel: 'Cancel run',
  cancelling: 'Cancelling …',
  date: 'Date',
  engineColumn: 'Engine',
  depth: 'Ply',
  total: 'Total analysis time',
  average: 'Ø analysis time / move',
  status: 'Status',
  cpus: '{count, plural, one {# logical CPU} other {# logical CPUs}}',
  open: 'Open benchmark',
};
type Key = keyof typeof en;
const de: Record<Key, string> = {
  globalBusy:
    'Benchmark läuft oder wartet. Andere Engine-Aktionen warten, bis die Messung abgeschlossen ist.',
  extras: 'Extras',
  benchmark: 'Benchmark',
  navigation: 'Zusatzwerkzeuge',
  title: 'Engine-Benchmark',
  reference: 'Feste Referenz',
  referenceFacts:
    '{points, plural, one {# Punkt} other {# Punkte}} · {games, plural, one {# Spiel} other {# Spiele}} · {moves, plural, one {# Checkerzug} other {# Checkerzüge}}',
  referenceVersion: 'Referenzversion {version}',
  protocol: 'Protokoll',
  timeout:
    'Zeitlimit: {seconds, plural, one {# Sekunde} other {# Sekunden}} pro Spiel',
  noTimeout: 'Ohne Zeitlimit',
  engine: 'Benchmark-Engine',
  plies: 'Analysetiefe',
  start: 'Benchmark starten',
  starting: 'Wird gestartet …',
  noEngine:
    'Keine Benchmark-Engine verfügbar. Prüfe die Installation in den Einstellungen.',
  timing:
    'Die Gesamtzeit umfasst Engine-Starts, Laden, Checker- und Cube-Analyse sowie Ergebnisaufbereitung. Wartezeit und Datenbankzugriffe liegen außerhalb der Messung. Für den Durchschnitt wird die Gesamtzeit durch die Zahl analysierter Checkerzüge geteilt.',
  serial:
    'Die Messung läuft von der niedrigsten zur höchsten gewählten Tiefe. Läufe werden beim Verlassen der Ansicht fortgesetzt.',
  engineBusy:
    'Andere Engine-Arbeit wartet, bis die Benchmark-Warteschlange abgeschlossen ist.',
  active: 'Aktuelle Läufe',
  history: 'Historie',
  empty: 'Noch keine Benchmarkläufe.',
  loading: 'Wird geladen …',
  retry: 'Erneut versuchen',
  queued: 'Wartend',
  running: 'Läuft',
  completed: 'Abgeschlossen',
  failed: 'Fehlgeschlagen',
  cancelled: 'Abgebrochen',
  waiting:
    'Wartet auf die Engine; andere Engine-Arbeit muss zunächst abgeschlossen werden.',
  progress: '{completed} / {total} Spiele · {moves} / {totalMoves} Checkerzüge',
  cancel: 'Lauf abbrechen',
  cancelling: 'Wird abgebrochen …',
  date: 'Datum',
  engineColumn: 'Engine',
  depth: 'Ply',
  total: 'Gesamtanalysezeit',
  average: 'Ø Analysezeit / Zug',
  status: 'Status',
  cpus: '{count, plural, one {# logische CPU} other {# logische CPUs}}',
  open: 'Benchmark öffnen',
};
const base: Record<Locale, Record<Key, string>> = {
  en,
  de,
  es: {
    globalBusy:
      'Hay una medición en curso o en espera. Las demás tareas del motor esperan a que termine.',
    extras: 'Extras',
    benchmark: 'Benchmark',
    navigation: 'Herramientas adicionales',
    title: 'Benchmark de motores',
    reference: 'Referencia fija',
    referenceFacts:
      '{points, plural, one {# punto} other {# puntos}} · {games, plural, one {# partida} other {# partidas}} · {moves, plural, one {# jugada de fichas} other {# jugadas de fichas}}',
    referenceVersion: 'Versión de referencia {version}',
    protocol: 'Protocolo',
    timeout:
      'Límite de {seconds, plural, one {# segundo} other {# segundos}} por partida',
    noTimeout: 'Sin límite de tiempo',
    engine: 'Motor del benchmark',
    plies: 'Profundidad de análisis',
    start: 'Iniciar benchmark',
    starting: 'Iniciando…',
    noEngine:
      'No hay un motor disponible para el benchmark. Comprueba la instalación en Ajustes.',
    timing:
      'El tiempo total incluye el arranque, la carga, el análisis de fichas y cubo y la preparación del resultado. La espera y la escritura en la base de datos quedan fuera de la medición. La media divide el total por el número de jugadas de fichas analizadas.',
    serial:
      'Las profundidades se miden de menor a mayor. Las mediciones continúan al salir de esta vista.',
    engineBusy:
      'Las demás tareas del motor esperan a que terminen las mediciones pendientes.',
    active: 'Mediciones activas',
    history: 'Historial',
    empty: 'Todavía no hay mediciones.',
    loading: 'Cargando…',
    retry: 'Reintentar',
    queued: 'En espera',
    running: 'En curso',
    completed: 'Completada',
    failed: 'Fallida',
    cancelled: 'Cancelada',
    waiting: 'Esperando a que el motor termine otras tareas.',
    progress:
      '{completed} / {total} partidas · {moves} / {totalMoves} jugadas de fichas',
    cancel: 'Cancelar medición',
    cancelling: 'Cancelando…',
    date: 'Fecha',
    engineColumn: 'Motor',
    depth: 'Ply',
    total: 'Tiempo total de análisis',
    average: 'Tiempo medio de análisis por jugada',
    status: 'Estado',
    cpus: '{count, plural, one {# CPU lógica} other {# CPU lógicas}}',
    open: 'Abrir benchmark',
  },
  fr: {
    globalBusy:
      'Une mesure est en cours ou en attente. Les autres tâches du moteur attendent sa fin.',
    extras: 'Extras',
    benchmark: 'Benchmark',
    navigation: 'Outils supplémentaires',
    title: 'Benchmark des moteurs',
    reference: 'Référence fixe',
    referenceFacts:
      '{points, plural, one {# point} other {# points}} · {games, plural, one {# partie} other {# parties}} · {moves, plural, one {# coup de pions} other {# coups de pions}}',
    referenceVersion: 'Version de référence {version}',
    protocol: 'Protocole',
    timeout:
      'Limite de {seconds, plural, one {# seconde} other {# secondes}} par partie',
    noTimeout: 'Sans limite de temps',
    engine: 'Moteur du benchmark',
    plies: 'Profondeur d’analyse',
    start: 'Lancer le benchmark',
    starting: 'Démarrage…',
    noEngine:
      'Aucun moteur disponible pour le benchmark. Vérifiez l’installation dans les paramètres.',
    timing:
      'Le temps total comprend le démarrage, le chargement, l’analyse des coups et du cube et la préparation du résultat. L’attente et l’écriture en base sont hors mesure. La moyenne divise le temps total par le nombre de coups de pions analysés.',
    serial:
      'Les profondeurs sont mesurées par ordre croissant. Les mesures continuent si vous quittez cette vue.',
    engineBusy:
      'Les autres tâches du moteur attendent la fin des mesures en file d’attente.',
    active: 'Mesures actives',
    history: 'Historique',
    empty: 'Aucune mesure pour le moment.',
    loading: 'Chargement…',
    retry: 'Réessayer',
    queued: 'En attente',
    running: 'En cours',
    completed: 'Terminée',
    failed: 'Échec',
    cancelled: 'Annulée',
    waiting: 'Attend la fin des autres tâches du moteur.',
    progress:
      '{completed} / {total} parties · {moves} / {totalMoves} coups de pions',
    cancel: 'Annuler la mesure',
    cancelling: 'Annulation…',
    date: 'Date',
    engineColumn: 'Moteur',
    depth: 'Ply',
    total: 'Temps total d’analyse',
    average: 'Temps moyen d’analyse par coup',
    status: 'État',
    cpus: '{count, plural, one {# processeur logique} other {# processeurs logiques}}',
    open: 'Ouvrir le benchmark',
  },
  it: {
    globalBusy:
      'Una misurazione è in corso o in attesa. Le altre attività del motore attendono che finisca.',
    extras: 'Extra',
    benchmark: 'Benchmark',
    navigation: 'Strumenti aggiuntivi',
    title: 'Benchmark dei motori',
    reference: 'Riferimento fisso',
    referenceFacts:
      '{points, plural, one {# punto} other {# punti}} · {games, plural, one {# partita} other {# partite}} · {moves, plural, one {# mossa di pedine} other {# mosse di pedine}}',
    referenceVersion: 'Versione di riferimento {version}',
    protocol: 'Protocollo',
    timeout:
      'Limite di {seconds, plural, one {# secondo} other {# secondi}} per partita',
    noTimeout: 'Senza limite di tempo',
    engine: 'Motore del benchmark',
    plies: 'Profondità di analisi',
    start: 'Avvia benchmark',
    starting: 'Avvio…',
    noEngine:
      'Nessun motore disponibile per il benchmark. Verifica l’installazione nelle impostazioni.',
    timing:
      'Il tempo totale comprende avvio, caricamento, analisi di mosse e cubo e preparazione dei risultati. L’attesa e la scrittura nel database sono escluse dalla misurazione. La media divide il totale per il numero di mosse di pedine analizzate.',
    serial:
      'Le profondità sono misurate in ordine crescente. Le misurazioni proseguono quando lasci questa vista.',
    engineBusy:
      'Le altre attività del motore attendono la fine delle misurazioni in coda.',
    active: 'Misurazioni in corso',
    history: 'Cronologia',
    empty: 'Nessuna misurazione disponibile.',
    loading: 'Caricamento…',
    retry: 'Riprova',
    queued: 'In attesa',
    running: 'In corso',
    completed: 'Completata',
    failed: 'Non riuscita',
    cancelled: 'Annullata',
    waiting: 'Attende il completamento delle altre attività del motore.',
    progress:
      '{completed} / {total} partite · {moves} / {totalMoves} mosse di pedine',
    cancel: 'Annulla misurazione',
    cancelling: 'Annullamento…',
    date: 'Data',
    engineColumn: 'Motore',
    depth: 'Ply',
    total: 'Tempo totale di analisi',
    average: 'Tempo medio di analisi per mossa',
    status: 'Stato',
    cpus: '{count, plural, one {# CPU logica} other {# CPU logiche}}',
    open: 'Apri benchmark',
  },
  pl: {
    globalBusy:
      'Pomiar trwa lub oczekuje. Pozostałe zadania silnika czekają na jego zakończenie.',
    extras: 'Dodatki',
    benchmark: 'Benchmark',
    navigation: 'Narzędzia dodatkowe',
    title: 'Benchmark silników',
    reference: 'Stały materiał odniesienia',
    referenceFacts:
      '{points} pkt · {games, plural, one {# partia} few {# partie} many {# partii}} · {moves, plural, one {# ruch pionkami} few {# ruchy pionkami} many {# ruchów pionkami}}',
    referenceVersion: 'Wersja materiału odniesienia {version}',
    protocol: 'Protokół',
    timeout:
      'Limit {seconds, plural, one {# sekunda} few {# sekundy} many {# sekund}} na partię',
    noTimeout: 'Bez limitu czasu',
    engine: 'Silnik benchmarku',
    plies: 'Głębokość analizy',
    start: 'Uruchom benchmark',
    starting: 'Uruchamianie…',
    noEngine:
      'Brak silnika dostępnego do benchmarku. Sprawdź instalację w ustawieniach.',
    timing:
      'Czas całkowity obejmuje uruchomienie, wczytanie, analizę ruchów i kostki oraz przygotowanie wyników. Oczekiwanie i zapis do bazy są poza pomiarem. Średnia to czas całkowity podzielony przez liczbę przeanalizowanych ruchów pionkami.',
    serial:
      'Pomiary biegną od najniższej do najwyższej głębokości. Trwają po opuszczeniu widoku.',
    engineBusy: 'Pozostałe zadania silnika czekają na koniec kolejki pomiarów.',
    active: 'Aktywne pomiary',
    history: 'Historia',
    empty: 'Brak pomiarów.',
    loading: 'Wczytywanie…',
    retry: 'Spróbuj ponownie',
    queued: 'Oczekuje',
    running: 'W toku',
    completed: 'Zakończony',
    failed: 'Nieudany',
    cancelled: 'Anulowany',
    waiting: 'Oczekuje na zakończenie innych zadań silnika.',
    progress:
      '{completed} / {total} partii · {moves} / {totalMoves} ruchów pionkami',
    cancel: 'Anuluj pomiar',
    cancelling: 'Anulowanie…',
    date: 'Data',
    engineColumn: 'Silnik',
    depth: 'Ply',
    total: 'Całkowity czas analizy',
    average: 'Średni czas analizy ruchu',
    status: 'Stan',
    cpus: '{count, plural, one {# procesor logiczny} few {# procesory logiczne} many {# procesorów logicznych}}',
    open: 'Otwórz benchmark',
  },
};
const playEn = {
  matchAnalysis: 'Match analysis',
  depthFallback:
    '{ply}-ply selected because the requested depth is unavailable.',
  mode: 'Measurement type',
  playPly: 'Playing depth',
  details: 'Details',
  methodology: 'Reference and methodology',
  threads: '{count, plural, one {# thread} other {# threads}}',
  recordedEngineVersion: 'Engine version',
  rulesVersion: 'GNUbg rules version',
  description:
    'Measure full-match analysis time and play-turn latency on this backend machine.',
  playLatency: 'Play-turn latency',
  playAverage: 'Ø play-turn latency',
  startPlay: 'Measure play-turn latency',
  playDepth: 'Default playing depth from Settings · {ply}-ply',
  playProgress: '{completed} / {total} checker turns',
  playTiming:
    'Measures reference checker turns with the selected engine, from loading the game state to producing the next state. Includes process startup.',
  actions: 'Actions',
  deleteRun: 'Delete run',
  deleteQuestion: 'Delete this run permanently?',
  confirmDelete: 'Delete',
  keepRun: 'Keep',
  deleting: 'Deleting …',
};
type PlayKey = keyof typeof playEn;
const play: Record<Locale, Record<PlayKey, string>> = {
  en: playEn,
  de: {
    matchAnalysis: 'Matchanalyse',
    depthFallback:
      '{ply} Ply gewählt, da die gewünschte Tiefe nicht verfügbar ist.',
    mode: 'Messart',
    playPly: 'Spieltiefe',
    details: 'Details',
    methodology: 'Referenz und Methodik',
    threads: '{count, plural, one {# Thread} other {# Threads}}',
    recordedEngineVersion: 'Engine-Version',
    rulesVersion: 'Version der GNUbg-Regel-Engine',
    description:
      'Matchanalysezeit und Spielzuglatenz auf diesem Backend-Rechner messen.',
    playLatency: 'Spielzuglatenz',
    playAverage: 'Ø Spielzuglatenz',
    startPlay: 'Spielzuglatenz messen',
    playDepth: 'Standard-Spieltiefe aus den Einstellungen · {ply}-ply',
    playProgress: '{completed} / {total} Checkerzüge',
    playTiming:
      'Misst Referenzzüge mit der gewählten Engine. Die Zeit umfasst den Prozessstart und reicht bis zum neuen Spielstand.',
    actions: 'Aktionen',
    deleteRun: 'Lauf löschen',
    deleteQuestion: 'Diesen Lauf endgültig löschen?',
    confirmDelete: 'Löschen',
    keepRun: 'Behalten',
    deleting: 'Wird gelöscht …',
  },
  es: {
    matchAnalysis: 'Análisis del encuentro',
    depthFallback:
      'Se seleccionó {ply} ply porque la profundidad solicitada no está disponible.',
    mode: 'Tipo de medición',
    playPly: 'Profundidad de juego',
    details: 'Detalles',
    methodology: 'Referencia y metodología',
    threads: '{count, plural, one {# hilo} other {# hilos}}',
    recordedEngineVersion: 'Versión del motor',
    rulesVersion: 'Versión de las reglas de GNUbg',
    description:
      'Mide el tiempo de análisis de una partida completa y la latencia por jugada en este equipo servidor.',
    playLatency: 'Latencia por jugada',
    playAverage: 'Latencia media por jugada',
    startPlay: 'Medir latencia por jugada',
    playDepth: 'Profundidad de juego configurada · {ply} ply',
    playProgress: '{completed} / {total} jugadas de fichas',
    playTiming:
      'Mide jugadas de referencia con el motor seleccionado, desde la carga hasta el nuevo estado de la partida. Incluye el arranque del proceso.',
    actions: 'Acciones',
    deleteRun: 'Eliminar medición',
    deleteQuestion: '¿Eliminar esta medición definitivamente?',
    confirmDelete: 'Eliminar',
    keepRun: 'Conservar',
    deleting: 'Eliminando…',
  },
  fr: {
    matchAnalysis: 'Analyse de match',
    depthFallback:
      '{ply} ply sélectionné, car la profondeur demandée est indisponible.',
    mode: 'Type de mesure',
    playPly: 'Profondeur de jeu',
    details: 'Détails',
    methodology: 'Référence et méthode',
    threads: '{count, plural, one {# thread} other {# threads}}',
    recordedEngineVersion: 'Version du moteur',
    rulesVersion: 'Version des règles GNUbg',
    description:
      'Mesurez le temps d’analyse d’un match complet et la latence par coup sur cet ordinateur serveur.',
    playLatency: 'Latence par coup',
    playAverage: 'Latence moyenne par coup',
    startPlay: 'Mesurer la latence par coup',
    playDepth: 'Profondeur de jeu configurée · {ply} ply',
    playProgress: '{completed} / {total} coups de pions',
    playTiming:
      'Mesure des coups de référence avec le moteur sélectionné, du chargement au nouvel état de la partie. Le démarrage du processus est inclus.',
    actions: 'Actions',
    deleteRun: 'Supprimer la mesure',
    deleteQuestion: 'Supprimer définitivement cette mesure ?',
    confirmDelete: 'Supprimer',
    keepRun: 'Conserver',
    deleting: 'Suppression…',
  },
  it: {
    matchAnalysis: 'Analisi del match',
    depthFallback:
      'Selezionati {ply} ply perché la profondità richiesta non è disponibile.',
    mode: 'Tipo di misurazione',
    playPly: 'Profondità di gioco',
    details: 'Dettagli',
    methodology: 'Riferimento e metodologia',
    threads: '{count, plural, one {# thread} other {# thread}}',
    recordedEngineVersion: 'Versione del motore',
    rulesVersion: 'Versione delle regole GNUbg',
    description:
      'Misura il tempo di analisi di un match completo e la latenza per mossa su questo computer server.',
    playLatency: 'Latenza per mossa',
    playAverage: 'Latenza media per mossa',
    startPlay: 'Misura la latenza per mossa',
    playDepth: 'Profondità di gioco impostata · {ply} ply',
    playProgress: '{completed} / {total} mosse di pedine',
    playTiming:
      'Misura mosse di riferimento con il motore selezionato, dal caricamento al nuovo stato della partita. Include l’avvio del processo.',
    actions: 'Azioni',
    deleteRun: 'Elimina misurazione',
    deleteQuestion: 'Eliminare definitivamente questa misurazione?',
    confirmDelete: 'Elimina',
    keepRun: 'Mantieni',
    deleting: 'Eliminazione…',
  },
  pl: {
    matchAnalysis: 'Analiza meczu',
    depthFallback:
      'Wybrano {ply} ply, ponieważ żądana głębokość jest niedostępna.',
    mode: 'Rodzaj pomiaru',
    playPly: 'Głębokość gry',
    details: 'Szczegóły',
    methodology: 'Odniesienie i metodologia',
    threads: '{count, plural, one {# wątek} few {# wątki} many {# wątków}}',
    recordedEngineVersion: 'Wersja silnika',
    rulesVersion: 'Wersja reguł GNUbg',
    description:
      'Zmierz czas analizy całego meczu i czas odpowiedzi na ruch na tym komputerze serwera.',
    playLatency: 'Czas odpowiedzi na ruch',
    playAverage: 'Średni czas ruchu',
    startPlay: 'Zmierz czas ruchu',
    playDepth: 'Ustawiona głębokość gry · {ply} ply',
    playProgress: '{completed} / {total} ruchów pionkami',
    playTiming:
      'Mierzy ruchy referencyjne wybranym silnikiem, od wczytania stanu gry do uzyskania nowego stanu. Obejmuje uruchomienie procesu.',
    actions: 'Akcje',
    deleteRun: 'Usuń pomiar',
    deleteQuestion: 'Trwale usunąć ten pomiar?',
    confirmDelete: 'Usuń',
    keepRun: 'Zachowaj',
    deleting: 'Usuwanie…',
  },
};
function isPlayKey(key: Key | PlayKey): key is PlayKey {
  return Object.hasOwn(playEn, key);
}
export const benchmarkText = (
  locale: Locale,
  key: Key | PlayKey,
  values?: TranslationParams,
) =>
  formatMessage(
    locale,
    isPlayKey(key) ? play[locale][key] : base[locale][key],
    values,
  );
export function useBenchmarkText() {
  const { locale } = useI18n();
  return (key: Key | PlayKey, values?: TranslationParams) =>
    benchmarkText(locale, key, values);
}
