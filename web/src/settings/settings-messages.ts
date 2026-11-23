import { defineMessages, type MessageText } from '../shared/i18n/messages';
import { depthLabel } from '../shared/engine/analysis-depth';

const en = {
  title: 'Settings',
  description: 'Personal preferences and engine configuration',
  categories: 'Settings categories',
  general: 'General',
  analysis: 'Analysis',
  play: 'Playing',
  board: 'Board',
  close: 'Close settings',
  save: 'Save',
  saving: 'Saving…',
  loading: 'Loading settings…',
  retry: 'Try again',
  done: 'Close',
  keepEditing: 'Keep editing',
  unsaved: 'Unapplied changes',
  applyConfiguration: 'Saves changes to the current configuration.',
  applyInstallation:
    'Engine paths and thread limits are saved on this computer.',
  verifySageTarget: 'Verifies and saves the Python path on this computer.',
  localEngines: 'Engines on this computer',
  available: 'Available',
  engineStatusError: 'Engine status could not be refreshed.',
  manageBooks: 'Manage opening books',
  localEnginesHelp: 'Paths and thread limits apply to this computer.',
  language: 'Language',
  languageHelp:
    'Choose the app language. It changes when you select “{action}”.',
  playerName: 'Player name',
  playerHelp: 'For future games. Names in imported matches stay as recorded.',
  analysisEngine: 'Analysis engine',
  analysisProfile: 'Default analysis profile',
  fast: 'Quick — {depth}',
  deep: 'Deeper — {depth}',
  thorough: 'Thorough — {depth}',
  errorThresholds: 'Error thresholds',
  errorThresholdsHelp:
    'Minimum normalized equity loss for each grade. Error is the default warning threshold for Tutor and for missed doubles in Coach.',
  invalidErrorThresholds:
    'Enter four strictly increasing thresholds greater than zero and at most one.',
  analysisHelp:
    'Applies to new analysis jobs. Existing results and running jobs retain their engine and profile.',
  path: 'GNUbg executable path',
  pathHelp:
    'This path is on the backend computer, which may differ from the computer running your browser.',
  installation: 'GNUbg',
  check: 'Check installation',
  checking: 'Checking version and position evaluation…',
  checkHelp:
    'Checks the version and a real position evaluation, including the required neural weights.',
  checkSuccess: 'Installation checked: {version}',
  gnubgAnalysisThreads: 'GNUbg threads for analysis',
  gnubgPlayThreads: 'GNUbg threads for playing',
  sageAnalysisThreads: 'NeonGammon threads for analysis',
  sagePlayThreads: 'NeonGammon threads for playing',
  defaultThreads: 'Default',
  threadRangeUnavailable: 'Thread range unavailable.',
  retainedThreadCount: 'Saved value {count}.',
  configurableThreadRange: 'Configurable range {min}–{max}.',
  threadsHelp:
    'Analysis threads apply to new analyses and engine duels, playing threads to new games. By default GNUbg uses 1 thread for both. NeonGammon uses 4 for analysis and duels and 2 for playing.',
  parallelAnalysisJobs: 'Simultaneous analysis jobs',
  defaultParallelJobs: 'Default (1)',
  parallelAnalysisJobsHelp:
    'Position, dice equity and match analyses that may run at the same time on this computer. NeonGammon runs one analysis at a time. Each job uses its own analysis threads, so jobs × threads should stay within the logical CPUs. Engine duels and games do not count.',
  checkNeeded: 'The edited GNUbg settings have not been checked yet.',
  engineVersion: 'Detected version: {version}',
  unavailable: 'Not available',
  planned: 'NeonGammon',
  playEngine: 'Engine for future games',
  playStrength: 'Strength for future games',
  playFeedbackGroup: 'Play mode and feedback',
  playPreferencesGroup: 'Sounds and board direction',
  playHelp:
    'Settings for the future playing mode. Saving does not start a game. Analysis settings are independent.',
  strengthQuick: 'Quick — {depth}',
  strengthStronger: 'Stronger — {depth}',
  strengthStrongest: 'Strongest — {depth}',
  boardHelp:
    'Choose a board. The preview uses the same rendering and original theme artwork as the analysis board.',
  gallery: 'Available board themes',
  preview: 'Preview: {theme}',
  selected: 'Selected',
  light: 'Light',
  dark: 'Dark',
  loadError: 'Settings could not be loaded.',
  saveError: 'Settings could not be saved. Your draft has been retained.',
  checkError: 'The installation check failed.',
  conflict:
    'Settings changed elsewhere. Your draft is retained. Load the latest saved settings before editing again.',
  loadLatest: 'Load latest and discard draft',
  invalidResponse: 'The backend returned incomplete settings.',
};

export type SettingsMessageKey = keyof typeof en;

const messages = defineMessages('settings', {
  en,
  de: {
    title: 'Einstellungen',
    description: 'Persönliche Einstellungen und Enginekonfiguration',
    categories: 'Einstellungskategorien',
    general: 'Allgemein',
    analysis: 'Analyse',
    play: 'Spielen',
    board: 'Brett',
    close: 'Einstellungen schließen',
    save: 'Speichern',
    saving: 'Wird gespeichert…',
    loading: 'Einstellungen werden geladen…',
    retry: 'Erneut versuchen',
    done: 'Schließen',
    keepEditing: 'Weiter bearbeiten',
    unsaved: 'Nicht angewendete Änderungen',
    applyConfiguration: 'Speichert Änderungen in der aktuellen Konfiguration.',
    applyInstallation:
      'Engine-Pfade und Thread-Limits werden auf diesem Rechner gespeichert.',
    verifySageTarget: 'Prüft und speichert den Python-Pfad auf diesem Rechner.',
    localEngines: 'Engines auf diesem Rechner',
    available: 'Verfügbar',
    engineStatusError: 'Der Engine-Status konnte nicht aktualisiert werden.',
    manageBooks: 'Eröffnungsbücher verwalten',
    localEnginesHelp: 'Pfade und Thread-Limits gelten für diesen Rechner.',
    language: 'Sprache',
    languageHelp:
      'Wähle die Sprache der App. Die Änderung wird mit „{action}“ wirksam.',
    playerName: 'Spielername',
    playerHelp:
      'Für künftige Spiele. Namen in importierten Matches bleiben erhalten.',
    analysisEngine: 'Analyse-Engine',
    analysisProfile: 'Standardprofil für Analysen',
    fast: 'Schnell — {depth}',
    deep: 'Vertieft — {depth}',
    thorough: 'Gründlich — {depth}',
    errorThresholds: 'Fehlergrenzen',
    errorThresholdsHelp:
      'Mindestverlust an normalisierter Equity je Stufe. „Fehler“ ist die Standardwarnschwelle im Tutor und für ausgelassene Doppler im Coach.',
    invalidErrorThresholds:
      'Gib vier aufsteigende Grenzwerte ein. Jeder Wert muss größer als null und höchstens eins sein.',
    analysisHelp:
      'Gilt für neue Analyseaufträge. Vorhandene Ergebnisse und laufende Aufträge behalten ihre Engine und ihr Profil.',
    path: 'GNUbg-Programmpfad',
    pathHelp:
      'Dieser Pfad bezeichnet den Rechner des Backends. Das kann ein anderer Rechner als der deines Browsers sein.',
    installation: 'GNUbg',
    check: 'Installation prüfen',
    checking: 'Version und Positionsbewertung werden geprüft…',
    checkHelp:
      'Prüft Version und eine echte Positionsbewertung einschließlich benötigter Netzgewichte.',
    checkSuccess: 'Installation geprüft: {version}',
    gnubgAnalysisThreads: 'GNUbg-Threads für Analysen',
    gnubgPlayThreads: 'GNUbg-Threads beim Spielen',
    sageAnalysisThreads: 'NeonGammon-Threads für Analysen',
    sagePlayThreads: 'NeonGammon-Threads beim Spielen',
    defaultThreads: 'Standard',
    threadRangeUnavailable: 'Der Threadbereich ist derzeit nicht verfügbar.',
    retainedThreadCount: 'Gespeicherter Wert {count}.',
    configurableThreadRange: 'Einstellbarer Bereich {min}–{max}.',
    threadsHelp:
      'Analyse-Threads gelten für neue Analysen und Engine-Duelle, Spiel-Threads für neue Partien. Standardmäßig nutzt GNUbg jeweils 1 Thread. NeonGammon nutzt 4 für Analysen und Duelle und 2 beim Spielen.',
    parallelAnalysisJobs: 'Gleichzeitige Analyse-Jobs',
    defaultParallelJobs: 'Standard (1)',
    parallelAnalysisJobsHelp:
      'Positions-, Würfel-Equity- und Matchanalysen, die auf diesem Rechner gleichzeitig laufen dürfen. NeonGammon rechnet jeweils nur eine Analyse. Jeder Job nutzt eigene Analyse-Threads; Jobs × Threads sollte die logischen CPUs nicht übersteigen. Engine-Duelle und Partien zählen nicht mit.',
    checkNeeded:
      'Die geänderten GNUbg-Einstellungen wurden noch nicht geprüft.',
    engineVersion: 'Erkannte Version: {version}',
    unavailable: 'Nicht verfügbar',
    planned: 'NeonGammon',
    playEngine: 'Engine für künftige Spiele',
    playStrength: 'Spielstärke für künftige Spiele',
    playFeedbackGroup: 'Spielmodus und Trainingsfeedback',
    playPreferencesGroup: 'Spielgeräusche und Auswürfelrichtung',
    playHelp:
      'Einstellungen für den künftigen Spielmodus. Speichern startet kein Spiel. Analyseprofile sind unabhängig.',
    strengthQuick: 'Schnell — {depth}',
    strengthStronger: 'Stärker — {depth}',
    strengthStrongest: 'Am stärksten — {depth}',
    boardHelp:
      'Wähle ein Brett. Die Vorschau nutzt dasselbe Rendering und die originalen Themegrafiken wie das Analysebrett.',
    gallery: 'Verfügbare Brett-Themes',
    preview: 'Vorschau: {theme}',
    selected: 'Ausgewählt',
    light: 'Hell',
    dark: 'Dunkel',
    loadError: 'Die Einstellungen konnten nicht geladen werden.',
    saveError:
      'Die Einstellungen konnten nicht gespeichert werden. Dein Entwurf bleibt erhalten.',
    checkError: 'Die Installationsprüfung ist fehlgeschlagen.',
    conflict:
      'Die Einstellungen wurden an anderer Stelle geändert. Dein Entwurf bleibt erhalten. Lade vor weiteren Änderungen die neueste gespeicherte Version.',
    loadLatest: 'Neueste Version laden und Entwurf verwerfen',
    invalidResponse:
      'Das Backend hat unvollständige Einstellungen zurückgegeben.',
  },
  es: {
    title: 'Configuración',
    description: 'Preferencias personales y configuración del motor',
    categories: 'Categorías de configuración',
    general: 'General',
    analysis: 'Análisis',
    play: 'Jugar',
    board: 'Tablero',
    close: 'Cerrar configuración',
    save: 'Guardar',
    saving: 'Guardando…',
    loading: 'Cargando configuración…',
    retry: 'Reintentar',
    done: 'Cerrar',
    keepEditing: 'Seguir editando',
    unsaved: 'Cambios sin aplicar',
    applyConfiguration: 'Guarda los cambios en la configuración actual.',
    applyInstallation:
      'Las rutas de los motores y los límites de hilos se guardan en este equipo.',
    verifySageTarget: 'Verifica y guarda la ruta de Python en este equipo.',
    localEngines: 'Motores en este equipo',
    available: 'Disponible',
    engineStatusError: 'No se pudo actualizar el estado de los motores.',
    manageBooks: 'Gestionar libros de aperturas',
    localEnginesHelp:
      'Las rutas y los límites de hilos se aplican a este equipo.',
    language: 'Idioma',
    languageHelp:
      'Elige el idioma de la aplicación. El cambio se aplica al seleccionar «{action}».',
    playerName: 'Nombre del jugador',
    playerHelp:
      'Para partidas futuras. Los nombres de las partidas importadas se conservan.',
    analysisEngine: 'Motor de análisis',
    analysisProfile: 'Perfil de análisis predeterminado',
    fast: 'Rápido — {depth}',
    deep: 'Profundo — {depth}',
    thorough: 'Exhaustivo — {depth}',
    errorThresholds: 'Umbrales de error',
    errorThresholdsHelp:
      'Pérdida mínima de equidad normalizada para cada nivel. «Error» es el umbral de aviso inicial del tutor y de los dobles omitidos en el entrenador.',
    invalidErrorThresholds:
      'Introduce cuatro umbrales estrictamente crecientes, mayores que cero y como máximo uno.',
    analysisHelp:
      'Se aplica a nuevos análisis. Los resultados existentes y los trabajos en curso conservan su motor y perfil.',
    path: 'Ruta del ejecutable GNUbg',
    pathHelp:
      'Esta ruta corresponde al equipo del backend, que puede ser distinto del equipo de tu navegador.',
    installation: 'GNUbg',
    check: 'Comprobar instalación',
    checking: 'Comprobando versión y evaluación de posición…',
    checkHelp:
      'Comprueba la versión y una evaluación real, incluidos los pesos de la red neuronal necesarios.',
    checkSuccess: 'Instalación comprobada: {version}',
    gnubgAnalysisThreads: 'Hilos de GNUbg para análisis',
    gnubgPlayThreads: 'Hilos de GNUbg para jugar',
    sageAnalysisThreads: 'Hilos de NeonGammon para análisis',
    sagePlayThreads: 'Hilos de NeonGammon para jugar',
    defaultThreads: 'Predeterminado',
    threadRangeUnavailable: 'El intervalo de hilos no está disponible.',
    retainedThreadCount: 'Valor guardado {count}.',
    configurableThreadRange: 'Intervalo configurable {min}–{max}.',
    threadsHelp:
      'Los hilos de análisis se aplican a nuevos análisis y duelos de motores; los de juego, a nuevas partidas. GNUbg usa 1 hilo por defecto en ambos casos. NeonGammon usa 4 para análisis y duelos y 2 para jugar.',
    parallelAnalysisJobs: 'Trabajos de análisis simultáneos',
    defaultParallelJobs: 'Predeterminado (1)',
    parallelAnalysisJobsHelp:
      'Análisis de posiciones, de equity de dados y del encuentro que pueden ejecutarse a la vez en este equipo. NeonGammon ejecuta un análisis cada vez. Cada trabajo usa sus propios hilos de análisis, así que trabajos × hilos no debería superar las CPU lógicas. Los duelos de motores y las partidas no cuentan.',
    checkNeeded: 'Los ajustes modificados de GNUbg aún no se han comprobado.',
    engineVersion: 'Versión detectada: {version}',
    unavailable: 'No disponible',
    planned: 'NeonGammon',
    playEngine: 'Motor para partidas futuras',
    playStrength: 'Fuerza para partidas futuras',
    playFeedbackGroup: 'Modo de juego y comentarios',
    playPreferencesGroup: 'Sonidos y dirección de salida',
    playHelp:
      'Configuración del futuro modo de juego. Guardar no inicia una partida. El perfil de análisis es independiente.',
    strengthQuick: 'Rápido — {depth}',
    strengthStronger: 'Más fuerte — {depth}',
    strengthStrongest: 'Máxima fuerza — {depth}',
    boardHelp:
      'Elige un tablero. La vista previa utiliza el mismo renderizado y las imágenes originales que el tablero de análisis.',
    gallery: 'Temas de tablero disponibles',
    preview: 'Vista previa: {theme}',
    selected: 'Seleccionado',
    light: 'Claras',
    dark: 'Oscuras',
    loadError: 'No se pudo cargar la configuración.',
    saveError: 'No se pudo guardar la configuración. Tu borrador se conserva.',
    checkError: 'La comprobación de la instalación ha fallado.',
    conflict:
      'La configuración cambió en otro lugar. Tu borrador se conserva. Carga la última versión guardada antes de volver a editar.',
    loadLatest: 'Cargar la última versión y descartar el borrador',
    invalidResponse: 'El backend devolvió una configuración incompleta.',
  },
  fr: {
    title: 'Paramètres',
    description: 'Préférences personnelles et configuration du moteur',
    categories: 'Catégories de paramètres',
    general: 'Général',
    analysis: 'Analyse',
    play: 'Jouer',
    board: 'Plateau',
    close: 'Fermer les paramètres',
    save: 'Enregistrer',
    saving: 'Enregistrement…',
    loading: 'Chargement des paramètres…',
    retry: 'Réessayer',
    done: 'Fermer',
    keepEditing: 'Continuer la modification',
    unsaved: 'Modifications non appliquées',
    applyConfiguration:
      'Enregistre les modifications dans la configuration actuelle.',
    applyInstallation:
      'Les chemins des moteurs et les limites de threads sont enregistrés sur cet ordinateur.',
    verifySageTarget:
      'Vérifie et enregistre le chemin Python sur cet ordinateur.',
    localEngines: 'Moteurs sur cet ordinateur',
    available: 'Disponible',
    engineStatusError: 'L’état des moteurs n’a pas pu être actualisé.',
    manageBooks: 'Gérer les bibliothèques d’ouvertures',
    localEnginesHelp:
      'Les chemins et les limites de threads s’appliquent à cet ordinateur.',
    language: 'Langue',
    languageHelp:
      'Choisissez la langue de l’application. Le changement prend effet lorsque vous sélectionnez « {action} ».',
    playerName: 'Nom du joueur',
    playerHelp:
      'Pour les futures parties. Les noms des matchs importés sont conservés.',
    analysisEngine: 'Moteur d’analyse',
    analysisProfile: 'Profil d’analyse par défaut',
    fast: 'Rapide — {depth}',
    deep: 'Approfondi — {depth}',
    thorough: 'Détaillé — {depth}',
    errorThresholds: 'Seuils d’erreur',
    errorThresholdsHelp:
      'Perte minimale d’équité normalisée par niveau. « Erreur » est le seuil d’alerte par défaut pour le tuteur et les doubles manqués en mode Coach.',
    invalidErrorThresholds:
      'Saisissez quatre seuils strictement croissants, supérieurs à zéro et inférieurs ou égaux à un.',
    analysisHelp:
      'S’applique aux nouvelles analyses. Les résultats existants et les tâches en cours conservent leur moteur et leur profil.',
    path: 'Chemin de l’exécutable GNUbg',
    pathHelp:
      'Ce chemin désigne l’ordinateur du backend, qui peut être différent de celui de votre navigateur.',
    installation: 'GNUbg',
    check: 'Vérifier l’installation',
    checking: 'Vérification de la version et de l’évaluation d’une position…',
    checkHelp:
      'Vérifie la version et une véritable évaluation, y compris les poids du réseau neuronal requis.',
    checkSuccess: 'Installation vérifiée : {version}',
    gnubgAnalysisThreads: 'Threads de GNUbg pour l’analyse',
    gnubgPlayThreads: 'Threads de GNUbg pour jouer',
    sageAnalysisThreads: 'Threads de NeonGammon pour l’analyse',
    sagePlayThreads: 'Threads de NeonGammon pour jouer',
    defaultThreads: 'Par défaut',
    threadRangeUnavailable: 'La plage de threads est indisponible.',
    retainedThreadCount: 'Valeur enregistrée {count}.',
    configurableThreadRange: 'Plage configurable de {min} à {max}.',
    threadsHelp:
      'Les threads d’analyse s’appliquent aux nouvelles analyses et confrontations entre moteurs, ceux de jeu aux nouvelles parties. Par défaut, GNUbg utilise 1 thread dans les deux cas. NeonGammon en utilise 4 pour les analyses et les confrontations, et 2 pour jouer.',
    parallelAnalysisJobs: 'Analyses simultanées',
    defaultParallelJobs: 'Par défaut (1)',
    parallelAnalysisJobsHelp:
      'Analyses de position, d’équité des dés et de match pouvant s’exécuter en même temps sur cet ordinateur. NeonGammon exécute une analyse à la fois. Chaque analyse utilise ses propres threads d’analyse : analyses × threads ne devrait pas dépasser les CPU logiques. Les confrontations entre moteurs et les parties ne comptent pas.',
    checkNeeded:
      'Les paramètres modifiés de GNUbg n’ont pas encore été vérifiés.',
    engineVersion: 'Version détectée : {version}',
    unavailable: 'Indisponible',
    planned: 'NeonGammon',
    playEngine: 'Moteur pour les futures parties',
    playStrength: 'Force pour les futures parties',
    playFeedbackGroup: 'Mode de jeu et retours',
    playPreferencesGroup: 'Sons et direction de sortie',
    playHelp:
      'Paramètres du futur mode de jeu. Enregistrer ne lance aucune partie. Le profil d’analyse est indépendant.',
    strengthQuick: 'Rapide — {depth}',
    strengthStronger: 'Plus fort — {depth}',
    strengthStrongest: 'Force maximale — {depth}',
    boardHelp:
      'Choisissez un plateau. L’aperçu utilise le même rendu et les images originales que le plateau d’analyse.',
    gallery: 'Thèmes de plateau disponibles',
    preview: 'Aperçu : {theme}',
    selected: 'Sélectionné',
    light: 'Clairs',
    dark: 'Foncés',
    loadError: 'Impossible de charger les paramètres.',
    saveError:
      'Impossible d’enregistrer les paramètres. Votre brouillon est conservé.',
    checkError: 'La vérification de l’installation a échoué.',
    conflict:
      'Les paramètres ont été modifiés ailleurs. Votre brouillon est conservé. Chargez la dernière version enregistrée avant de poursuivre.',
    loadLatest: 'Charger la dernière version et abandonner le brouillon',
    invalidResponse: 'Le backend a renvoyé des paramètres incomplets.',
  },
  it: {
    title: 'Impostazioni',
    description: 'Preferenze personali e configurazione del motore',
    categories: 'Categorie delle impostazioni',
    general: 'Generali',
    analysis: 'Analisi',
    play: 'Giocare',
    board: 'Tavola',
    close: 'Chiudi impostazioni',
    save: 'Salva',
    saving: 'Salvataggio…',
    loading: 'Caricamento delle impostazioni…',
    retry: 'Riprova',
    done: 'Chiudi',
    keepEditing: 'Continua a modificare',
    unsaved: 'Modifiche non applicate',
    applyConfiguration: 'Salva le modifiche nella configurazione corrente.',
    applyInstallation:
      'I percorsi dei motori e i limiti dei thread vengono salvati su questo computer.',
    verifySageTarget:
      'Verifica e salva il percorso di Python su questo computer.',
    localEngines: 'Motori su questo computer',
    available: 'Disponibile',
    engineStatusError: 'Impossibile aggiornare lo stato dei motori.',
    manageBooks: 'Gestisci i libri delle aperture',
    localEnginesHelp:
      'I percorsi e i limiti dei thread si applicano a questo computer.',
    language: 'Lingua',
    languageHelp:
      'Scegli la lingua dell’app. La modifica ha effetto quando selezioni «{action}».',
    playerName: 'Nome del giocatore',
    playerHelp:
      'Per le partite future. I nomi delle partite importate restano invariati.',
    analysisEngine: 'Motore di analisi',
    analysisProfile: 'Profilo di analisi predefinito',
    fast: 'Rapido — {depth}',
    deep: 'Approfondito — {depth}',
    thorough: 'Accurato — {depth}',
    errorThresholds: 'Soglie di errore',
    errorThresholdsHelp:
      'Perdita minima di equità normalizzata per ogni livello. «Errore» è la soglia di avviso predefinita per Tutor e per i raddoppi mancati in Coach.',
    invalidErrorThresholds:
      'Inserisci quattro soglie strettamente crescenti, maggiori di zero e al massimo pari a uno.',
    analysisHelp:
      'Si applica alle nuove analisi. I risultati esistenti e i lavori in corso conservano il proprio motore e profilo.',
    path: 'Percorso dell’eseguibile GNUbg',
    pathHelp:
      'Questo percorso si riferisce al computer del backend, che può essere diverso da quello del browser.',
    installation: 'GNUbg',
    check: 'Verifica installazione',
    checking: 'Verifica della versione e della valutazione di una posizione…',
    checkHelp:
      'Verifica la versione e una valutazione reale, inclusi i pesi della rete neurale necessari.',
    checkSuccess: 'Installazione verificata: {version}',
    gnubgAnalysisThreads: 'Thread di GNUbg per l’analisi',
    gnubgPlayThreads: 'Thread di GNUbg per giocare',
    sageAnalysisThreads: 'Thread di NeonGammon per l’analisi',
    sagePlayThreads: 'Thread di NeonGammon per giocare',
    defaultThreads: 'Predefinito',
    threadRangeUnavailable: 'L’intervallo di thread non è disponibile.',
    retainedThreadCount: 'Valore salvato {count}.',
    configurableThreadRange: 'Intervallo configurabile {min}–{max}.',
    threadsHelp:
      'I thread di analisi si applicano a nuove analisi e sfide tra motori, quelli di gioco a nuove partite. Per impostazione predefinita GNUbg usa 1 thread in entrambi i casi. NeonGammon ne usa 4 per analisi e sfide e 2 per giocare.',
    parallelAnalysisJobs: 'Analisi simultanee',
    defaultParallelJobs: 'Predefinito (1)',
    parallelAnalysisJobsHelp:
      'Analisi di posizioni, equity dei dadi e del match che possono essere eseguite contemporaneamente su questo computer. NeonGammon esegue un’analisi alla volta. Ogni analisi usa i propri thread di analisi, quindi analisi × thread non dovrebbe superare le CPU logiche. Sfide tra motori e partite non vengono conteggiate.',
    checkNeeded:
      'Le impostazioni modificate di GNUbg non sono ancora state verificate.',
    engineVersion: 'Versione rilevata: {version}',
    unavailable: 'Non disponibile',
    planned: 'NeonGammon',
    playEngine: 'Motore per le partite future',
    playStrength: 'Forza per le partite future',
    playFeedbackGroup: 'Modalità di gioco e feedback',
    playPreferencesGroup: 'Suoni e direzione di uscita',
    playHelp:
      'Impostazioni per la futura modalità di gioco. Salvare non avvia una partita. Il profilo di analisi è indipendente.',
    strengthQuick: 'Rapido — {depth}',
    strengthStronger: 'Più forte — {depth}',
    strengthStrongest: 'Forza massima — {depth}',
    boardHelp:
      'Scegli una tavola. L’anteprima usa lo stesso rendering e le immagini originali della tavola di analisi.',
    gallery: 'Temi della tavola disponibili',
    preview: 'Anteprima: {theme}',
    selected: 'Selezionato',
    light: 'Chiari',
    dark: 'Scuri',
    loadError: 'Impossibile caricare le impostazioni.',
    saveError: 'Impossibile salvare le impostazioni. La bozza è conservata.',
    checkError: 'La verifica dell’installazione non è riuscita.',
    conflict:
      'Le impostazioni sono state modificate altrove. La bozza è conservata. Carica l’ultima versione salvata prima di continuare.',
    loadLatest: 'Carica l’ultima versione e scarta la bozza',
    invalidResponse: 'Il backend ha restituito impostazioni incomplete.',
  },
  pl: {
    title: 'Ustawienia',
    description: 'Preferencje osobiste i konfiguracja silnika',
    categories: 'Kategorie ustawień',
    general: 'Ogólne',
    analysis: 'Analiza',
    play: 'Gra',
    board: 'Plansza',
    close: 'Zamknij ustawienia',
    save: 'Zapisz',
    saving: 'Zapisywanie…',
    loading: 'Wczytywanie ustawień…',
    retry: 'Spróbuj ponownie',
    done: 'Zamknij',
    keepEditing: 'Kontynuuj edycję',
    unsaved: 'Niezastosowane zmiany',
    applyConfiguration: 'Zapisuje zmiany w bieżącej konfiguracji.',
    applyInstallation:
      'Ścieżki silników i limity wątków są zapisywane na tym komputerze.',
    verifySageTarget: 'Sprawdza i zapisuje ścieżkę Pythona na tym komputerze.',
    localEngines: 'Silniki na tym komputerze',
    available: 'Dostępny',
    engineStatusError: 'Nie udało się odświeżyć stanu silników.',
    manageBooks: 'Zarządzaj książkami otwarć',
    localEnginesHelp: 'Ścieżki i limity wątków dotyczą tego komputera.',
    language: 'Język',
    languageHelp:
      'Wybierz język aplikacji. Zmiana zacznie obowiązywać po wybraniu „{action}”.',
    playerName: 'Nazwa gracza',
    playerHelp:
      'Dla przyszłych gier. Nazwy graczy w importowanych meczach pozostają bez zmian.',
    analysisEngine: 'Silnik analizy',
    analysisProfile: 'Domyślny profil analizy',
    fast: 'Szybki — {depth}',
    deep: 'Pogłębiony — {depth}',
    thorough: 'Dokładny — {depth}',
    errorThresholds: 'Progi błędów',
    errorThresholdsHelp:
      'Minimalna strata znormalizowanej equity dla każdego poziomu. „Błąd” jest domyślnym progiem ostrzeżeń tutora i pominiętych podwojeń w trybie trenera.',
    invalidErrorThresholds:
      'Wpisz cztery ściśle rosnące progi większe od zera i nieprzekraczające jedności.',
    analysisHelp:
      'Dotyczy nowych analiz. Istniejące wyniki i trwające zadania zachowują swój silnik i profil.',
    path: 'Ścieżka do programu GNUbg',
    pathHelp:
      'Ta ścieżka dotyczy komputera backendu, który może być inny niż komputer przeglądarki.',
    installation: 'GNUbg',
    check: 'Sprawdź instalację',
    checking: 'Sprawdzanie wersji i oceny pozycji…',
    checkHelp:
      'Sprawdza wersję i rzeczywistą ocenę pozycji, w tym wymagane wagi sieci neuronowej.',
    checkSuccess: 'Instalacja sprawdzona: {version}',
    gnubgAnalysisThreads: 'Wątki GNUbg do analizy',
    gnubgPlayThreads: 'Wątki GNUbg do gry',
    sageAnalysisThreads: 'Wątki NeonGammon do analizy',
    sagePlayThreads: 'Wątki NeonGammon do gry',
    defaultThreads: 'Domyślnie',
    threadRangeUnavailable: 'Zakres wątków jest niedostępny.',
    retainedThreadCount: 'Zapisana wartość {count}.',
    configurableThreadRange: 'Zakres ustawień {min}–{max}.',
    threadsHelp:
      'Wątki analizy dotyczą nowych analiz i pojedynków silników, a wątki gry — nowych partii. Domyślnie GNUbg używa 1 wątku w obu przypadkach. NeonGammon używa 4 do analiz i pojedynków oraz 2 do gry.',
    parallelAnalysisJobs: 'Równoczesne analizy',
    defaultParallelJobs: 'Domyślnie (1)',
    parallelAnalysisJobsHelp:
      'Analizy pozycji, equity kości i meczu, które mogą działać jednocześnie na tym komputerze. NeonGammon wykonuje jedną analizę naraz. Każda analiza używa własnych wątków analizy, więc analizy × wątki nie powinny przekraczać logicznych procesorów. Pojedynki silników i partie nie są liczone.',
    checkNeeded: 'Zmienione ustawienia GNUbg nie zostały jeszcze sprawdzone.',
    engineVersion: 'Wykryta wersja: {version}',
    unavailable: 'Niedostępny',
    planned: 'NeonGammon',
    playEngine: 'Silnik dla przyszłych gier',
    playStrength: 'Siła gry dla przyszłych gier',
    playFeedbackGroup: 'Tryb gry i informacje zwrotne',
    playPreferencesGroup: 'Dźwięki i kierunek wyprowadzania',
    playHelp:
      'Ustawienia przyszłego trybu gry. Zapisanie nie rozpoczyna gry. Profil analizy jest niezależny.',
    strengthQuick: 'Szybka — {depth}',
    strengthStronger: 'Silniejsza — {depth}',
    strengthStrongest: 'Najsilniejsza — {depth}',
    boardHelp:
      'Wybierz planszę. Podgląd używa tego samego renderowania i oryginalnych grafik co plansza analizy.',
    gallery: 'Dostępne motywy planszy',
    preview: 'Podgląd: {theme}',
    selected: 'Wybrany',
    light: 'Jasne',
    dark: 'Ciemne',
    loadError: 'Nie udało się wczytać ustawień.',
    saveError: 'Nie udało się zapisać ustawień. Twój szkic został zachowany.',
    checkError: 'Sprawdzenie instalacji nie powiodło się.',
    conflict:
      'Ustawienia zostały zmienione w innym miejscu. Twój szkic został zachowany. Wczytaj najnowszą zapisaną wersję przed dalszą edycją.',
    loadLatest: 'Wczytaj najnowszą wersję i odrzuć szkic',
    invalidResponse: 'Backend zwrócił niekompletne ustawienia.',
  },
});

export const settingsMessages = messages.catalogs;
export const settingsText = messages.text;
export const useSettingsText = messages.useText;

export type SettingsText = MessageText<SettingsMessageKey>;

/** GNUbg's three named profiles are its 1- to 3-ply searches. */
export function depthOptionLabel(
  t: SettingsText,
  ply: number,
  engine: string,
  names: readonly [SettingsMessageKey, SettingsMessageKey, SettingsMessageKey],
) {
  const depth = depthLabel(ply, engine);
  const name = engine === 'gnubg' ? names[ply - 1] : undefined;
  return name ? t(name, { depth }) : depth;
}

/** Option text for a bot's playing strength. */
export const playStrengthLabel = (
  t: SettingsText,
  ply: number,
  engine: string,
  topTwo = false,
) =>
  topTwo
    ? '2-ply + Top-2 (10 mEq)'
    : depthOptionLabel(t, ply, engine, [
        'strengthQuick',
        'strengthStronger',
        'strengthStrongest',
      ]);
