import type { MistakeGroup } from './recurring-mistakes-api';
import type { StudyDeck } from './learning-api';

type Pending = { status: 'loading' } | { status: 'error' };
type Cards = Pending | { status: 'ready'; deck?: StudyDeck | null };
type Mistakes =
  | Pending
  | { status: 'choose-player' }
  | { status: 'ready'; group?: MistakeGroup };
type StudyAction =
  | { kind: 'deck'; deck: StudyDeck }
  | {
      kind: 'mistake';
      group: MistakeGroup;
      rates: NonNullable<MistakeGroup['rates']>;
    }
  | { kind: 'choose-player' | 'cards' | 'loading' | 'unavailable' };

export function defaultStudyPlayer(configured: string, players: string[]) {
  const name = configured.trim().toLowerCase();
  if (!name) return '';
  const matches = players.filter(
    (player) => player.trim().toLowerCase() === name,
  );
  return matches.length === 1 ? matches[0] : '';
}

export function rankMistakeGroups(groups: MistakeGroup[]) {
  return groups
    .filter(({ match_count }) => match_count >= 2)
    .sort(
      (a, b) =>
        rankedLoss(b) - rankedLoss(a) ||
        b.total_loss - a.total_loss ||
        b.match_count - a.match_count ||
        b.count - a.count ||
        `${a.game_plan}:${a.mistake_kind}`.localeCompare(
          `${b.game_plan}:${b.mistake_kind}`,
          'en',
        ),
    );
}

function rankedLoss(group: MistakeGroup) {
  return group.rates && !group.rates.small_sample
    ? group.rates.loss_per_100
    : -1;
}

export function recommendStudyAction(
  cards: Cards,
  mistakes: Mistakes,
): StudyAction {
  if (cards.status === 'ready' && cards.deck)
    return { kind: 'deck', deck: cards.deck };
  if (cards.status === 'loading' || mistakes.status === 'loading')
    return { kind: 'loading' };
  if (mistakes.status === 'ready' && mistakes.group) {
    const { group } = mistakes;
    if (group.match_count >= 2 && group.rates && !group.rates.small_sample)
      return { kind: 'mistake', group, rates: group.rates };
  }
  if (mistakes.status === 'choose-player') return { kind: 'choose-player' };
  if (cards.status === 'error' || mistakes.status === 'error')
    return { kind: 'unavailable' };
  return { kind: 'cards' };
}
