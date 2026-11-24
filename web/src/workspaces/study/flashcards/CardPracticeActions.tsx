import type { ReactNode } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { SearchFilterSelect } from '../../../shared/ui/SearchFilterForm';
import { useI18n } from '../../../shared/i18n/i18n';
import { useCardPracticeText } from './card-practice-messages';
import type { CardPage, CardQuestion } from './learning-api';
import { useLearningText } from './learning-messages';

export type SpecialPractice = {
  mode: 'pips' | 'score' | 'preview';
  cards: CardQuestion[];
};

/** Starts a round with the listed cards or an exercise with the selected one. */
export default function CardPracticeActions({
  cards,
  loading,
  limit,
  onLimit,
  starting,
  error,
  resumable,
  selectedItem,
  children,
  onStart,
  onResume,
  onSpecial,
}: {
  cards: CardPage['items'];
  loading: boolean;
  limit: number;
  onLimit: (limit: number) => void;
  starting: boolean;
  error: unknown;
  resumable: boolean;
  selectedItem?: CardPage['items'][number];
  /** The actions for the selected cards. */
  children: ReactNode;
  onStart: () => void;
  onResume: () => void;
  /** The trigger gets the focus back when the exercise closes. */
  onSpecial: (practice: SpecialPractice, trigger: HTMLButtonElement) => void;
}) {
  const t = useLearningText();
  const practiceT = useCardPracticeText();
  const { formatError, formatNumber } = useI18n();
  return (
    <>
      <div className="learning-result-toolbar">
        <div className="learning-controls learning-practice-row">
          <SearchFilterSelect
            label={practiceT('size')}
            value={String(limit)}
            onChange={(value) => onLimit(Number(value))}
          >
            <option value="10">{formatNumber(10)}</option>
            <option value="20">{formatNumber(20)}</option>
          </SearchFilterSelect>
          <ActionButton
            icon="play"
            label={practiceT('start')}
            disabled={starting || loading || !cards.length}
            onClick={onStart}
          >
            {practiceT('start')}
          </ActionButton>
          {resumable && (
            <ActionButton
              icon="open"
              label={practiceT('resume')}
              disabled={starting}
              onClick={onResume}
            >
              {practiceT('resume')}
            </ActionButton>
          )}
          <ActionButton
            icon="view"
            label={practiceT('preview')}
            disabled={loading || !cards.length}
            onClick={(event) => {
              onSpecial(
                {
                  mode: 'preview',
                  cards: cards.slice(0, limit),
                },
                event.currentTarget,
              );
            }}
          >
            {practiceT('preview')}
          </ActionButton>
        </div>
        {children}
      </div>
      {starting && <p role="status">{t('loading')}</p>}
      {!!error && <p role="alert">{formatError(error)}</p>}

      {selectedItem?.position && (
        <div className="learning-controls">
          <ActionButton
            icon="play"
            label={practiceT('pips')}
            onClick={(event) => {
              onSpecial(
                {
                  mode: 'pips',
                  cards: [selectedItem],
                },
                event.currentTarget,
              );
            }}
          >
            {practiceT('pips')}
          </ActionButton>
          {selectedItem.kind !== 'checker' && (
            <ActionButton
              icon="play"
              label={practiceT('score')}
              onClick={(event) => {
                onSpecial(
                  {
                    mode: 'score',
                    cards: [selectedItem],
                  },
                  event.currentTarget,
                );
              }}
            >
              {practiceT('score')}
            </ActionButton>
          )}
        </div>
      )}
    </>
  );
}
