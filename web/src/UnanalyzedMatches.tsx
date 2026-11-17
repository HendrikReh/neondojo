import ActionButton from './ActionButton';
import type { components } from './generated-api';
import { useStudyOverviewText } from './study-messages';

/**
 * Says how many of the chosen player's matches have no analysis and are
 * therefore missing from the mistake statistics, and leads to the place
 * where they can be analyzed.
 */
export default function UnanalyzedMatches({
  page,
  onOpenPlayers,
}: {
  page: Pick<
    components['schemas']['Mistakes'],
    'player_matches' | 'unanalyzed_matches'
  >;
  onOpenPlayers: () => void;
}) {
  const t = useStudyOverviewText();
  if (!page.unanalyzed_matches) return null;
  return (
    <p className="unanalyzed-matches" data-unanalyzed-matches>
      {t('unanalyzed', {
        missing: page.unanalyzed_matches,
        total: page.player_matches,
      })}{' '}
      <ActionButton
        type="button"
        icon="analysis"
        label={t('openPlayers')}
        onClick={onOpenPlayers}
      >
        {t('openPlayers')}
      </ActionButton>
    </p>
  );
}
