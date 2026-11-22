import ActionButton from '../../shared/ui/ActionButton';
import type { MatchGame } from '../../shared/api/api';
import { useI18n } from '../../shared/i18n/i18n';

export default function GameSelector({
  games,
  selected,
  onSelect,
}: {
  games: MatchGame[];
  selected?: number;
  onSelect: (game: number) => void;
}) {
  const { t } = useI18n();
  const index = games.findIndex((game) => game.game_no === selected);
  return (
    <section className="game-selector" aria-label={t('games.heading')}>
      <span className="eyebrow">{t('games.heading')}</span>
      <div className="game-selector-controls">
        <ActionButton
          icon="back"
          label={t('games.previous')}
          compact
          aria-label={t('games.previous')}
          disabled={index <= 0}
          onClick={() => onSelect(games[index - 1].game_no)}
        />
        <label>
          <span className="sr-only">{t('games.choose')}</span>
          <select
            aria-label={t('games.choose')}
            value={selected ?? ''}
            disabled={!games.length}
            onChange={(event) => onSelect(Number(event.target.value))}
          >
            {!games.length && <option value="">{t('games.loading')}</option>}
            {games.map((game) => (
              <option key={game.game_no} value={game.game_no}>
                {t('games.option', {
                  game: game.game_no,
                  white: game.white_score,
                  black: game.black_score,
                  moves: game.position_count,
                })}
                {game.crawford ? ' · Crawford' : ''}
              </option>
            ))}
          </select>
        </label>
        <ActionButton
          icon="next"
          label={t('games.next')}
          compact
          aria-label={t('games.next')}
          disabled={index < 0 || index >= games.length - 1}
          onClick={() => onSelect(games[index + 1].game_no)}
        />
      </div>
    </section>
  );
}
