import { useI18n } from './i18n';
export const playModes = ['competition', 'training', 'coach', 'tutor'] as const;
export type PlayMode = (typeof playModes)[number];
export const isPlayMode = (value: unknown): value is PlayMode =>
  typeof value === 'string' && playModes.includes(value as PlayMode);
const en = {
  activeMode: 'Active mode',
  changeMode: 'Change play mode',
  thresholdSaving: 'Saving…',
  thresholdSaved: 'Saved',
  liveThresholdHelp:
    'Equity loss that triggers Tutor warnings in this match. Applies immediately, including to an unconfirmed decision.',
  unconfirmed: 'Unconfirmed decision',
  learningUsed: 'Learning aids have been used in this match.',
  firstPr: 'First-attempt PR',
  firstPrHelp:
    'Evaluates your first choices at the positions reached in this match. Corrected choices do not replace the original attempts.',
  mode: 'Play mode',
  defaultMode: 'Default play mode',
  competition: 'Competition',
  training: 'Training',
  coach: 'Coach',
  tutor: 'Tutor',
  competitionHelp:
    'Play independently. Analysis and pip counts stay hidden until the match ends.',
  trainingHelp:
    'Compare full evaluations, alternatives and winning chances after your decisions.',
  coachHelp:
    'Receive compact feedback and equity differences, without absolute evaluations or probabilities.',
  tutorHelp:
    'Reconsider mistakes before playing. The first attempt is kept for review.',
  threshold: 'Tutor warning threshold',
  thresholdHelp:
    'Equity loss that triggers a Tutor warning. Separate from error categories. Applies to new matches.',
  automatic: 'Open training analysis automatically',
  automaticHelp:
    'When disabled, use Hint to request analysis. Applies to new matches.',
  hint: 'Hint',
  solution: 'Show solution',
  retry: 'Try again',
  anyway: 'Play anyway',
  continue: 'Continue',
  playMove: 'Play move',
  details: 'Details',
  warning:
    'This decision exceeds your warning threshold. Would you like to reconsider?',
  feedback: 'Decision feedback',
  doubled: 'Doubled',
  notDoubled: 'Not doubled',
  first: 'First attempt',
  selected: 'Selected decision',
  played: 'Played decision',
  loss: 'Equity loss',
  equity: 'Equity',
  probabilities: 'Win / gammon / backgammon',
  opponent: 'Opponent',
  alternatives: 'Alternatives',
  none: 'Good',
  doubtful: 'Inaccuracy',
  bad: 'Error',
  very_bad: 'Blunder',
  assisted: 'With help',
  independent: 'Independent',
  originalHelp:
    'The first attempt is retained, even when you correct your decision.',
  analysisDepth: 'Analysis depth',
  review: 'Learning review',
  reviewTooltip: 'Learning review',
  noReview: 'No evaluated decisions yet.',
  totalLoss: 'First-attempt equity loss',
  attempts: 'Attempts',
  defaultsError: 'Could not load play preferences.',
};
type Key = keyof typeof en;
const de: Record<Key, string> = {
  activeMode: 'Aktiver Modus',
  changeMode: 'Spielmodus wechseln',
  thresholdSaving: 'Wird gespeichert …',
  thresholdSaved: 'Gespeichert',
  liveThresholdHelp:
    'Equity-Verlust, ab dem der Tutor in diesem Match warnt. Gilt sofort, auch für eine unbestätigte Entscheidung.',
  unconfirmed: 'Unbestätigte Entscheidung',
  learningUsed: 'In diesem Match wurden bereits Lernhilfen verwendet.',
  firstPr: 'PR der ersten Versuche',
  firstPrHelp:
    'Bewertet deine ersten Entscheidungen an den im Match erreichten Stellungen. Korrigierte Entscheidungen ersetzen die ursprünglichen Versuche nicht.',
  mode: 'Spielmodus',
  defaultMode: 'Voreingestellter Spielmodus',
  competition: 'Competition',
  training: 'Training',
  coach: 'Coach',
  tutor: 'Tutor',
  competitionHelp:
    'Spiele selbstständig. Analyse und Pipcounts bleiben bis zum Matchende verborgen.',
  trainingHelp:
    'Vergleiche vollständige Bewertungen, Alternativen und Gewinnchancen nach deinen Entscheidungen.',
  coachHelp:
    'Erhalte kompaktes Feedback und Equity-Abstände, ohne absolute Bewertungen oder Wahrscheinlichkeiten.',
  tutorHelp:
    'Überprüfe Fehler vor dem Spielen. Der erste Versuch bleibt für die Auswertung erhalten.',
  threshold: 'Tutor-Warnschwelle',
  thresholdHelp:
    'Equity-Verlust, ab dem der Tutor warnt. Unabhängig von den Fehlerklassen. Gilt für neue Matches.',
  automatic: 'Trainingsanalyse automatisch öffnen',
  automaticHelp:
    'Ausgeschaltet rufst du die Analyse über Hinweis ab. Gilt für neue Matches.',
  hint: 'Hinweis',
  solution: 'Lösung zeigen',
  retry: 'Noch einmal versuchen',
  anyway: 'Trotzdem spielen',
  continue: 'Weiter',
  playMove: 'Zug spielen',
  details: 'Details',
  warning:
    'Diese Entscheidung überschreitet deine Warnschwelle. Möchtest du sie noch einmal prüfen?',
  feedback: 'Rückmeldung zur Entscheidung',
  doubled: 'Verdoppelt',
  notDoubled: 'Nicht verdoppelt',
  first: 'Erster Versuch',
  selected: 'Gewählte Entscheidung',
  played: 'Gespielte Entscheidung',
  loss: 'Equity-Verlust',
  equity: 'Equity',
  probabilities: 'Gewinn / Gammon / Backgammon',
  opponent: 'Gegner',
  alternatives: 'Alternativen',
  none: 'Gut',
  doubtful: 'Ungenauigkeit',
  bad: 'Fehler',
  very_bad: 'Grober Fehler',
  assisted: 'Mit Hilfe',
  independent: 'Selbstständig',
  originalHelp:
    'Der erste Versuch bleibt erhalten, auch wenn du deine Entscheidung korrigierst.',
  analysisDepth: 'Analysetiefe',
  review: 'Lernauswertung',
  reviewTooltip: 'Spielauswertung',
  noReview: 'Noch keine bewerteten Entscheidungen.',
  totalLoss: 'Equity-Verlust der ersten Versuche',
  attempts: 'Versuche',
  defaultsError: 'Die Spieleinstellungen konnten nicht geladen werden.',
};
const extra: Record<string, Partial<Record<Key, string>>> = {
  es: {
    thresholdSaving: 'Guardando…',
    thresholdSaved: 'Guardado',
    liveThresholdHelp:
      'Pérdida de equity que activa los avisos del tutor en esta partida. Se aplica inmediatamente, incluso a una decisión sin confirmar.',
    mode: 'Modo de juego',
    defaultMode: 'Modo de juego predeterminado',
    threshold: 'Umbral de aviso del tutor',
    automatic: 'Abrir el análisis de entrenamiento automáticamente',
    hint: 'Pista',
    solution: 'Mostrar solución',
    retry: 'Intentar de nuevo',
    anyway: 'Jugar de todos modos',
    continue: 'Continuar',
    playMove: 'Jugar movimiento',
    details: 'Detalles',
    warning: 'Esta decisión supera tu umbral. ¿Quieres reconsiderarla?',
    feedback: 'Evaluación de la decisión',
    doubled: 'Doblado',
    notDoubled: 'No doblado',
    first: 'Primer intento',
    selected: 'Decisión elegida',
    played: 'Decisión jugada',
    loss: 'Pérdida de equity',
    alternatives: 'Alternativas',
    none: 'Bien',
    doubtful: 'Imprecisión',
    bad: 'Error',
    very_bad: 'Error grave',
    assisted: 'Con ayuda',
    independent: 'Independiente',
    analysisDepth: 'Profundidad del análisis',
    review: 'Revisión del aprendizaje',
    reviewTooltip: 'Revisión del aprendizaje',
    noReview: 'Aún no hay decisiones evaluadas.',
    totalLoss: 'Pérdida de equity del primer intento',
    attempts: 'Intentos',
    opponent: 'Rival',
    probabilities: 'Victoria / gammon / backgammon',
  },
  fr: {
    thresholdSaving: 'Enregistrement…',
    thresholdSaved: 'Enregistré',
    liveThresholdHelp:
      'Perte d’équité qui déclenche les avertissements du tuteur dans ce match. S’applique immédiatement, y compris à une décision non confirmée.',
    mode: 'Mode de jeu',
    defaultMode: 'Mode de jeu par défaut',
    threshold: 'Seuil d’avertissement du tuteur',
    automatic: 'Ouvrir automatiquement l’analyse d’entraînement',
    hint: 'Conseil',
    solution: 'Afficher la solution',
    retry: 'Réessayer',
    anyway: 'Jouer quand même',
    continue: 'Continuer',
    playMove: 'Jouer le coup',
    details: 'Détails',
    warning:
      'Cette décision dépasse votre seuil. Voulez-vous la reconsidérer ?',
    feedback: 'Évaluation de la décision',
    doubled: 'Doublé',
    notDoubled: 'Non doublé',
    first: 'Premier essai',
    selected: 'Décision choisie',
    played: 'Décision jouée',
    loss: 'Perte d’équité',
    alternatives: 'Alternatives',
    none: 'Bien',
    doubtful: 'Imprécision',
    bad: 'Erreur',
    very_bad: 'Erreur grave',
    assisted: 'Avec aide',
    independent: 'Sans aide',
    analysisDepth: 'Profondeur d’analyse',
    review: 'Bilan d’apprentissage',
    reviewTooltip: 'Bilan d’apprentissage',
    noReview: 'Aucune décision évaluée.',
    totalLoss: 'Perte d’équité des premiers essais',
    attempts: 'Essais',
    opponent: 'Adversaire',
    probabilities: 'Victoire / gammon / backgammon',
  },
  it: {
    thresholdSaving: 'Salvataggio…',
    thresholdSaved: 'Salvato',
    liveThresholdHelp:
      'Perdita di equity che attiva gli avvisi del tutor in questa partita. Si applica subito, anche a una decisione non confermata.',
    mode: 'Modalità di gioco',
    defaultMode: 'Modalità predefinita',
    threshold: 'Soglia di avviso del tutor',
    automatic: 'Aprire automaticamente l’analisi di allenamento',
    hint: 'Suggerimento',
    solution: 'Mostra soluzione',
    retry: 'Riprova',
    anyway: 'Gioca comunque',
    continue: 'Continua',
    playMove: 'Gioca la mossa',
    details: 'Dettagli',
    warning: 'Questa decisione supera la soglia. Vuoi ripensarci?',
    feedback: 'Valutazione della decisione',
    doubled: 'Raddoppiato',
    notDoubled: 'Non raddoppiato',
    first: 'Primo tentativo',
    selected: 'Decisione scelta',
    played: 'Decisione giocata',
    loss: 'Perdita di equity',
    alternatives: 'Alternative',
    none: 'Bene',
    doubtful: 'Imprecisione',
    bad: 'Errore',
    very_bad: 'Errore grave',
    assisted: 'Con aiuto',
    independent: 'Senza aiuto',
    analysisDepth: 'Profondità di analisi',
    review: 'Riepilogo di apprendimento',
    reviewTooltip: 'Riepilogo di apprendimento',
    noReview: 'Nessuna decisione valutata.',
    totalLoss: 'Perdita di equity dei primi tentativi',
    attempts: 'Tentativi',
    opponent: 'Avversario',
    probabilities: 'Vittoria / gammon / backgammon',
  },
  pl: {
    thresholdSaving: 'Zapisywanie…',
    thresholdSaved: 'Zapisano',
    liveThresholdHelp:
      'Strata equity wyzwalająca ostrzeżenia tutora w tym meczu. Obowiązuje od razu, także dla niepotwierdzonej decyzji.',
    mode: 'Tryb gry',
    defaultMode: 'Domyślny tryb gry',
    threshold: 'Próg ostrzeżenia tutora',
    automatic: 'Automatycznie otwieraj analizę treningową',
    hint: 'Podpowiedź',
    solution: 'Pokaż rozwiązanie',
    retry: 'Spróbuj ponownie',
    anyway: 'Zagraj mimo to',
    continue: 'Dalej',
    playMove: 'Zagraj ruch',
    details: 'Szczegóły',
    warning: 'Ta decyzja przekracza próg. Czy chcesz ją przemyśleć?',
    feedback: 'Ocena decyzji',
    doubled: 'Podwojono',
    notDoubled: 'Nie podwojono',
    first: 'Pierwsza próba',
    selected: 'Wybrana decyzja',
    played: 'Zagrana decyzja',
    loss: 'Strata equity',
    alternatives: 'Alternatywy',
    none: 'Dobrze',
    doubtful: 'Niedokładność',
    bad: 'Błąd',
    very_bad: 'Poważny błąd',
    assisted: 'Z pomocą',
    independent: 'Samodzielnie',
    analysisDepth: 'Głębokość analizy',
    review: 'Podsumowanie nauki',
    reviewTooltip: 'Podsumowanie nauki',
    noReview: 'Brak ocenionych decyzji.',
    totalLoss: 'Strata equity pierwszych prób',
    attempts: 'Próby',
    opponent: 'Przeciwnik',
    probabilities: 'Wygrana / gammon / backgammon',
  },
};
Object.assign(extra.es, {
  activeMode: 'Modo activo',
  changeMode: 'Cambiar modo de juego',
  unconfirmed: 'Decisión sin confirmar',
  learningUsed: 'Se han utilizado ayudas en esta partida.',
  competitionHelp:
    'Juega sin ayuda. El análisis y los pips se ocultan hasta terminar la partida.',
  trainingHelp:
    'Compara evaluaciones completas, alternativas y probabilidades tras tus decisiones.',
  coachHelp:
    'Recibe comentarios y diferencias de equity, sin evaluaciones absolutas ni probabilidades.',
  tutorHelp:
    'Revisa los errores antes de jugar. Se conserva el primer intento.',
  thresholdHelp:
    'Pérdida de equity que activa el aviso. Independiente de las categorías de error. Se aplica a nuevas partidas.',
  automaticHelp:
    'Si se desactiva, solicita el análisis con Pista. Se aplica a nuevas partidas.',
  originalHelp: 'Se conserva el primer intento aunque corrijas la decisión.',
  defaultsError: 'No se pudieron cargar las preferencias de juego.',
  firstPr: 'PR del primer intento',
  firstPrHelp:
    'Evalúa tus primeras decisiones en las posiciones alcanzadas. Las correcciones no sustituyen los intentos originales.',
});
Object.assign(extra.fr, {
  activeMode: 'Mode actif',
  changeMode: 'Changer le mode de jeu',
  unconfirmed: 'Décision non confirmée',
  learningUsed: 'Des aides ont été utilisées pendant ce match.',
  competitionHelp:
    'Jouez sans aide. Analyse et comptes de pips restent cachés jusqu’à la fin du match.',
  trainingHelp:
    'Comparez évaluations complètes, alternatives et probabilités après vos décisions.',
  coachHelp:
    'Recevez des retours et écarts d’équité, sans évaluations absolues ni probabilités.',
  tutorHelp:
    'Réexaminez vos erreurs avant de jouer. Le premier essai est conservé.',
  thresholdHelp:
    'Perte d’équité déclenchant un avertissement. Indépendante des catégories d’erreur. Pour les nouveaux matchs.',
  automaticHelp:
    'Désactivé, demandez l’analyse avec Conseil. Pour les nouveaux matchs.',
  originalHelp: 'Le premier essai reste conservé même après correction.',
  defaultsError: 'Impossible de charger les préférences de jeu.',
  firstPr: 'PR des premiers essais',
  firstPrHelp:
    'Évalue les premiers choix dans les positions atteintes. Les corrections ne remplacent pas les essais initiaux.',
  equity: 'Équité',
});
Object.assign(extra.it, {
  activeMode: 'Modalità attiva',
  changeMode: 'Cambia modalità di gioco',
  unconfirmed: 'Decisione non confermata',
  learningUsed: 'Sono stati utilizzati aiuti in questo match.',
  competitionHelp:
    'Gioca senza aiuti. Analisi e conteggio pip restano nascosti fino alla fine del match.',
  trainingHelp:
    'Confronta valutazioni complete, alternative e probabilità dopo le tue decisioni.',
  coachHelp:
    'Ricevi riscontri e differenze di equity, senza valutazioni assolute né probabilità.',
  tutorHelp:
    'Riconsidera gli errori prima di giocare. Il primo tentativo viene conservato.',
  thresholdHelp:
    'Perdita di equity che attiva l’avviso. Indipendente dalle categorie di errore. Per i nuovi match.',
  automaticHelp:
    'Se disattivato, richiedi l’analisi con Suggerimento. Per i nuovi match.',
  originalHelp:
    'Il primo tentativo viene conservato anche dopo una correzione.',
  defaultsError: 'Impossibile caricare le preferenze di gioco.',
  firstPr: 'PR dei primi tentativi',
  firstPrHelp:
    'Valuta le prime scelte nelle posizioni raggiunte. Le correzioni non sostituiscono i tentativi originali.',
});
Object.assign(extra.pl, {
  activeMode: 'Aktywny tryb',
  changeMode: 'Zmień tryb gry',
  unconfirmed: 'Niepotwierdzona decyzja',
  learningUsed: 'W tym meczu korzystano z pomocy.',
  competitionHelp:
    'Graj samodzielnie. Analiza i liczba pipów są ukryte do końca meczu.',
  trainingHelp:
    'Porównuj pełne oceny, alternatywy i prawdopodobieństwa po swoich decyzjach.',
  coachHelp:
    'Otrzymuj oceny i różnice equity, bez wartości bezwzględnych ani prawdopodobieństw.',
  tutorHelp:
    'Przemyśl błędy przed zagraniem. Pierwsza próba zostaje zachowana.',
  thresholdHelp:
    'Strata equity wyzwalająca ostrzeżenie. Niezależna od kategorii błędów. Dotyczy nowych meczów.',
  automaticHelp:
    'Po wyłączeniu poproś o analizę przyciskiem Podpowiedź. Dotyczy nowych meczów.',
  originalHelp:
    'Pierwsza próba zostaje zachowana także po poprawieniu decyzji.',
  defaultsError: 'Nie udało się wczytać ustawień gry.',
  firstPr: 'PR pierwszych prób',
  firstPrHelp:
    'Ocenia pierwsze wybory w osiągniętych pozycjach. Poprawki nie zastępują pierwotnych prób.',
});
export function playModeText(locale: string, key: Key) {
  return (locale === 'de' ? de : extra[locale])?.[key] ?? en[key];
}
export function usePlayModeText() {
  const { locale } = useI18n();
  return (key: Key) => playModeText(locale, key);
}
