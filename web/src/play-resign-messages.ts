import { useI18n, type Locale } from './i18n';
import { formatMessage, type TranslationParams } from './i18n-core';
const en = {
  forfeitMatch: 'Forfeit match',
  forfeitHelp:
    'The match ends immediately and is recorded as a loss. Unconfirmed moves are discarded.',
  matchForfeited: 'Match forfeited. You lost.',
  resign: 'Resign',
  heading: 'Resign this game',
  help: 'Offer to resign the current game. The bot may decline the offer. Your unconfirmed moves are kept if you cancel.',
  level: 'Resignation level',
  confirmGame: 'Resign game',
  confirmMatch: 'Resign match',
  cancel: 'Continue playing',
  award: '{player} receives {points} if accepted.',
  score: 'Match score if accepted',
  target: 'Match to {points, plural, one {# point} other {# points}}',
  matchEnds: 'If accepted, the match ends. {player} wins.',
  matchContinues: 'If accepted, the match continues.',
  declined:
    'The bot declined this resignation. Offer a higher level or continue playing. Your game is unchanged.',
};
type Key = keyof typeof en;
const messages: Record<Locale, Record<Key, string>> = {
  en,
  de: {
    forfeitMatch: 'Match aufgeben',
    forfeitHelp:
      'Das Match endet sofort und wird als Niederlage gewertet. Unbestätigte Züge werden verworfen.',
    matchForfeited: 'Match durch Aufgabe verloren.',
    resign: 'Aufgeben',
    heading: 'Dieses Spiel aufgeben',
    help: 'Biete die Aufgabe des aktuellen Spiels an. Der Bot kann ablehnen. Beim Abbrechen bleiben deine unbestätigten Züge erhalten.',
    level: 'Aufgabewertung',
    confirmGame: 'Spiel aufgeben',
    confirmMatch: 'Match aufgeben',
    cancel: 'Weiterspielen',
    award: '{player} erhält bei Annahme {points}.',
    score: 'Matchstand bei Annahme',
    target: 'Match bis {points, plural, one {# Punkt} other {# Punkte}}',
    matchEnds: 'Bei Annahme endet das Match. {player} gewinnt.',
    matchContinues: 'Bei Annahme wird das Match fortgesetzt.',
    declined:
      'Der Bot hat die Aufgabe abgelehnt. Biete eine höhere Wertung an oder spiele weiter. Dein Spielstand bleibt unverändert.',
  },
  es: {
    forfeitMatch: 'Abandonar el match',
    forfeitHelp:
      'El match termina de inmediato y se registra como derrota. Se descartan los movimientos sin confirmar.',
    matchForfeited: 'Match perdido por abandono.',
    resign: 'Abandonar',
    heading: 'Abandonar esta partida',
    help: 'Ofrece abandonar la partida actual. El bot puede rechazarlo. Al cancelar se conservan los movimientos sin confirmar.',
    level: 'Valor del abandono',
    confirmGame: 'Abandonar partida',
    confirmMatch: 'Abandonar match',
    cancel: 'Seguir jugando',
    award: '{player} recibe {points} si acepta.',
    score: 'Marcador si se acepta',
    target: 'Match a {points, plural, one {# punto} other {# puntos}}',
    matchEnds: 'Si se acepta, termina el match. Gana {player}.',
    matchContinues: 'Si se acepta, el match continúa.',
    declined:
      'El bot ha rechazado el abandono. Ofrece un valor superior o sigue jugando. La partida no ha cambiado.',
  },
  fr: {
    forfeitMatch: 'Abandonner le match',
    forfeitHelp:
      'Le match se termine immédiatement et compte comme une défaite. Les déplacements non confirmés sont annulés.',
    matchForfeited: 'Match perdu par abandon.',
    resign: 'Abandonner',
    heading: 'Abandonner cette partie',
    help: 'Proposez d’abandonner la partie en cours. Le bot peut refuser. En annulant, vos déplacements non confirmés sont conservés.',
    level: 'Valeur de l’abandon',
    confirmGame: 'Abandonner la partie',
    confirmMatch: 'Abandonner le match',
    cancel: 'Continuer à jouer',
    award: '{player} reçoit {points} en cas d’acceptation.',
    score: 'Score en cas d’acceptation',
    target: 'Match en {points, plural, one {# point} other {# points}}',
    matchEnds: 'En cas d’acceptation, le match se termine. {player} gagne.',
    matchContinues: 'En cas d’acceptation, le match continue.',
    declined:
      'Le bot a refusé l’abandon. Proposez une valeur supérieure ou continuez à jouer. La partie reste inchangée.',
  },
  it: {
    forfeitMatch: 'Abbandona il match',
    forfeitHelp:
      'Il match termina subito e viene registrato come sconfitta. Le mosse non confermate vengono scartate.',
    matchForfeited: 'Match perso per abbandono.',
    resign: 'Abbandona',
    heading: 'Abbandona questa partita',
    help: 'Proponi di abbandonare la partita attuale. Il bot può rifiutare. Annullando si conservano le mosse non confermate.',
    level: 'Valore dell’abbandono',
    confirmGame: 'Abbandona partita',
    confirmMatch: 'Abbandona match',
    cancel: 'Continua a giocare',
    award: '{player} riceve {points} se accetta.',
    score: 'Punteggio in caso di accettazione',
    target: 'Match a {points, plural, one {# punto} other {# punti}}',
    matchEnds: 'Se accetta, il match termina. Vince {player}.',
    matchContinues: 'Se accetta, il match continua.',
    declined:
      'Il bot ha rifiutato l’abbandono. Offri un valore superiore o continua a giocare. La partita è invariata.',
  },
  pl: {
    forfeitMatch: 'Poddaj mecz',
    forfeitHelp:
      'Mecz kończy się od razu i zostaje zapisany jako przegrana. Niezatwierdzone ruchy zostaną odrzucone.',
    matchForfeited: 'Mecz przegrany przez poddanie.',
    resign: 'Poddaj',
    heading: 'Poddaj tę partię',
    help: 'Zaproponuj poddanie bieżącej partii. Bot może odmówić. Anulowanie zachowuje niezatwierdzone ruchy.',
    level: 'Wartość poddania',
    confirmGame: 'Poddaj partię',
    confirmMatch: 'Poddaj mecz',
    cancel: 'Graj dalej',
    award: '{player} otrzyma {points} po przyjęciu.',
    score: 'Wynik po przyjęciu',
    target: 'Mecz do {points} pkt',
    matchEnds: 'Po przyjęciu mecz się kończy. Wygrywa {player}.',
    matchContinues: 'Po przyjęciu mecz będzie kontynuowany.',
    declined:
      'Bot odrzucił poddanie. Zaproponuj wyższą wartość lub graj dalej. Stan gry się nie zmienił.',
  },
};
export function usePlayResignText() {
  const { locale } = useI18n();
  return (key: Key, values: TranslationParams = {}) =>
    formatMessage(locale, messages[locale][key], values);
}
