import { useEffect, useId, useRef, useState } from 'react';
import {
  useAnalysisErrorThresholds,
  useWorkspaceDefaults,
} from '../../../settings/configuration/ConfigurationProvider';
import { defaultConfiguration } from '../../../settings/configuration/configuration-model';
import { request } from '../../../shared/http/api';
import type { Theme } from '../../../shared/board/themes';
import { useDecimalInput } from '../../../shared/format/decimal-input';
import {
  sessionString,
  useWorkspaceSession,
} from '../../../shared/hooks/useWorkspaceSession';
import { useI18n } from '../../../shared/i18n/i18n';
import ActionButton from '../../../shared/ui/ActionButton';
import type { components } from '../../../generated-api';
import { useAnalysisCatalogText } from '../../analysis/analysis-catalog-messages';
import CardSuggestionsPanel from './CardSuggestionsPanel';
import { isCollectionScope } from './card-search-api';
import { useCardPracticeText } from './card-practice-messages';
import CreateDeckForm from './CreateDeckForm';
import DeckSelectField from './DeckSelectField';
import { useLearningText } from './learning-messages';
import { savedDeck, useDecks } from './useDecks';

/** Adds the analysed position or a game's mistakes to a deck from Analysis. */
export function AddCardsPanel({
  positionId,
  matchId,
  analysisId,
  gameNo,
  revision,
  onChanged,
  allowBatch = true,
  white,
  black,
  theme,
}: {
  positionId: string | undefined;
  matchId: string;
  analysisId?: string;
  gameNo?: number;
  revision: number;
  onChanged: () => void;
  allowBatch?: boolean;
  white: string;
  black: string;
  theme: Theme;
}) {
  const catalogText = useAnalysisCatalogText();
  const legacy = useWorkspaceDefaults();
  const t = useLearningText();
  const practiceT = useCardPracticeText();
  const [newTags, setNewTags] = useState('');
  const [creatingDeck, setCreatingDeck] = useState(false);
  const createDeckId = useId();
  const newDeckButton = useRef<HTMLButtonElement>(null);
  const { formatError, t: baseT } = useI18n();
  const { decks, error: deckError, addDeck, settled } = useDecks(revision);
  const [deckId, setDeckId] = useWorkspaceSession(
    'analysis.card-deck',
    legacy?.learning.deck_id,
    savedDeck(),
    sessionString,
  );
  const selectedDeckId = decks.some((deck) => deck.id === deckId) ? deckId : '';
  const profileLoss = useAnalysisErrorThresholds().error;
  const legacyLoss = legacy?.learning.generation.min_loss;
  const [minLoss, setMinLoss] = useWorkspaceSession(
    'analysis.card-min-loss',
    legacyLoss !== undefined &&
      legacyLoss !== defaultConfiguration.learning.generation.min_loss
      ? String(legacyLoss)
      : undefined,
    String(profileLoss),
    sessionString,
  );
  const minLossInput = useDecimalInput({
    value: minLoss,
    onChange: setMinLoss,
    step: 0.01,
    min: 0,
    max: 10,
  });
  const [player, setPlayer] = useWorkspaceSession(
    'analysis.card-player',
    legacy?.learning.generation.player,
    '',
    (value) =>
      value === '' || value === '0' || value === '1' ? value : undefined,
  );
  const [busy, setBusy] = useState(false);
  const [needsAnalysis, setNeedsAnalysis] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [processed, setProcessed] = useState<number | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    generation.current++;
    setProcessed(null);
    setNeedsAnalysis(false);
    setError(null);
    setBusy(false);
  }, [positionId, matchId, gameNo, allowBatch, analysisId]);
  useEffect(() => {
    if (settled && deckId && !isCollectionScope(deckId) && !selectedDeckId)
      setDeckId('');
  }, [deckId, selectedDeckId, settled]);
  const hasDeck = decks.some((deck) => deck.id === deckId);
  const validLoss =
    minLoss.trim() !== '' &&
    Number.isFinite(Number(minLoss)) &&
    Number(minLoss) >= 0;
  async function add(kind: 'position' | 'match') {
    if (!hasDeck || (kind === 'match' && !allowBatch)) return;
    if (!analysisId) {
      setNeedsAnalysis(true);
      return;
    }
    const version = ++generation.current;
    setBusy(true);
    setError(null);
    setProcessed(null);
    try {
      let body: components['schemas']['AddCards'];
      if (kind === 'position') {
        body = { position_id: positionId };
      } else {
        body = {
          game_no: gameNo,
          min_loss: Number(minLoss),
          player: player === '' ? null : Number(player),
        };
      }
      const result = await request<components['schemas']['AddCardsResult']>(
        `/decks/${deckId}/cards`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            ...body,
            analysis_id: analysisId,
            match_id: matchId,
            tags: newTags
              .split(',')
              .map((tag) => tag.trim())
              .filter(Boolean),
          } satisfies components['schemas']['AddCards']),
        },
      );
      if (version === generation.current) {
        setProcessed(result.added + result.updated);
        onChanged();
      }
    } catch (error) {
      if (version === generation.current) setError(error);
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }
  return (
    <section className="add-cards" aria-label={t('addTitle')}>
      <h3>{t('addTitle')}</h3>
      <p>{t('addHint')}</p>
      <div className="learning-controls">
        <DeckSelectField
          decks={decks}
          value={selectedDeckId}
          onChange={setDeckId}
          label={t('chooseDeck')}
          disabled={busy}
          placeholder={t('chooseDeck')}
        />
        <ActionButton
          icon="add"
          label={t('addPosition')}
          disabled={!positionId || !hasDeck || busy}
          onClick={() => void add('position')}
        >
          {t('addPosition')}
        </ActionButton>
        <ActionButton
          ref={newDeckButton}
          type="button"
          icon="add"
          label={t('newDeck')}
          aria-expanded={creatingDeck}
          aria-controls={createDeckId}
          onClick={() => setCreatingDeck(!creatingDeck)}
        />
      </div>
      <CreateDeckForm
        id={createDeckId}
        hidden={!creatingDeck}
        onCreated={(deck) => {
          addDeck(deck);
          setDeckId(deck.id);
          setCreatingDeck(false);
          newDeckButton.current?.focus();
          onChanged();
        }}
      />
      {allowBatch && (
        <div className="learning-controls">
          <label>
            {t('minLoss')}{' '}
            <input
              aria-label={t('minLoss')}
              {...minLossInput.inputProps}
              onChange={(e) => minLossInput.change(e.target.value)}
            />
          </label>
          <label>
            {t('player')}{' '}
            <select
              aria-label={t('player')}
              value={player}
              onChange={(e) => setPlayer(e.target.value)}
            >
              <option value="">{t('both')}</option>
              <option value="0">{baseT('common.white')}</option>
              <option value="1">{baseT('common.black')}</option>
            </select>
          </label>
          <ActionButton
            icon="add"
            label={
              gameNo
                ? baseT('games.addErrors', { game: gameNo })
                : t('addErrors')
            }
            disabled={!hasDeck || gameNo === undefined || busy || !validLoss}
            onClick={() => void add('match')}
          >
            {gameNo
              ? baseT('games.addErrors', { game: gameNo })
              : t('addErrors')}
          </ActionButton>
        </div>
      )}
      <label>
        {practiceT('topicFilter')}{' '}
        <input
          value={newTags}
          maxLength={650}
          onChange={(event) => setNewTags(event.target.value)}
        />
      </label>
      {allowBatch && (
        <CardSuggestionsPanel
          key={JSON.stringify([matchId, deckId, player, minLoss])}
          deckId={deckId}
          matchId={matchId}
          player={player}
          minLoss={Number(minLoss)}
          white={white}
          black={black}
          theme={theme}
          disabled={!hasDeck || !validLoss || busy}
          onChanged={onChanged}
        />
      )}
      {needsAnalysis && <p role="alert">{catalogText('chooseAnother')}</p>}
      {busy && <p role="status">{t('adding')}</p>}
      {processed !== null && (
        <p role="status">{t('added', { count: processed })}</p>
      )}
      {!!(error || deckError) && (
        <p role="alert">{formatError(error || deckError)}</p>
      )}
    </section>
  );
}
