import AnkiCardFace from './AnkiCardFace';
import { useEffect, useState, type RefObject } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { request } from '../../../shared/api/api';
import Board from '../../../shared/board/Board';
import { matchContext } from '../../../shared/board/boardMath';
import { useCardPracticeText } from './card-practice-messages';
import CubePracticeChoices from '../practice/CubePracticeChoices';
import { useI18n } from '../../../shared/i18n/i18n';
import { useLearningText } from './learning-messages';
import type {
  CardAnswer,
  CardQuestion,
  PipResult,
  PracticeAnswer,
  PracticeEvaluation,
  ScoreAnswer,
  ScoreQuestion,
  ScorePractice as ScorePracticeSession,
} from './learning-api';
import PlayBoardDialog from '../../../shared/board/PlayBoardDialog';
import PracticeResult from '../practice/PracticeResult';
import { SearchFilterInput } from '../../../shared/ui/SearchFilterForm';
import type { Theme } from '../../../shared/board/themes';

type Mode = 'pips' | 'score' | 'preview';

export default function CardSpecialPractice({
  mode,
  cards,
  theme,
  onClose,
  returnFocus,
}: {
  mode: Mode;
  cards: CardQuestion[];
  theme: Theme;
  onClose: () => void;
  returnFocus: RefObject<HTMLElement | null>;
}) {
  const t = useCardPracticeText();
  return (
    <PlayBoardDialog
      heading={t(mode)}
      busy={false}
      onClose={onClose}
      returnFocus={returnFocus}
    >
      {mode === 'pips' && cards[0]?.position && (
        <PipPractice
          id={cards[0].id}
          position={cards[0].position}
          players={cards[0].players}
          theme={theme}
        />
      )}
      {mode === 'score' && <ScorePractice card={cards[0]} theme={theme} />}
      {mode === 'preview' && <TimedPreview cards={cards} theme={theme} />}
    </PlayBoardDialog>
  );
}

function PipPractice({
  id,
  position,
  players,
  theme,
}: {
  id: string;
  position: NonNullable<CardQuestion['position']>;
  players: CardQuestion['players'];
  theme: Theme;
}) {
  const t = useCardPracticeText();
  const { t: uiT, formatError, formatNumber } = useI18n();
  const [white, setWhite] = useState('');
  const [black, setBlack] = useState('');
  const [result, setResult] = useState<PipResult>();
  const [error, setError] = useState<unknown>();
  const [busy, setBusy] = useState(false);
  const valid = [white, black].every(
    (value) => /^\d{1,3}$/.test(value) && Number(value) <= 375,
  );
  async function check() {
    setBusy(true);
    setError(undefined);
    try {
      setResult(
        await request<PipResult>('/study/card-pips', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            card_id: id,
            counts: [Number(white), Number(black)],
          }),
        }),
      );
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <Board
        position={position}
        white={players[0]}
        black={players[1]}
        theme={theme}
        showPipCounts={!!result}
      />
      <form
        className="learning-controls"
        onSubmit={(event) => {
          event.preventDefault();
          if (valid && !busy) void check();
        }}
      >
        <SearchFilterInput
          label={uiT('common.white')}
          value={white}
          inputMode="numeric"
          maxLength={3}
          onChange={(value) => {
            setWhite(value);
            setResult(undefined);
          }}
          disabled={busy}
        />
        <SearchFilterInput
          label={uiT('common.black')}
          value={black}
          inputMode="numeric"
          maxLength={3}
          onChange={(value) => {
            setBlack(value);
            setResult(undefined);
          }}
          disabled={busy}
        />
        <ActionButton
          type="submit"
          icon="confirm"
          label={t('checkPips')}
          disabled={!valid || busy}
        >
          {t('checkPips')}
        </ActionButton>
      </form>
      {!!error && <p role="alert">{formatError(error)}</p>}
      {result && (
        <ul aria-live="polite">
          {result.counts.map((count, index) => (
            <li key={index}>
              {uiT(index === 0 ? 'common.white' : 'common.black')} ·{' '}
              {t(result.correct[index] ? 'correct' : 'incorrect')} ·{' '}
              {t('barPips', {
                total: count,
                bar: formatNumber(result.bar_pips[index]),
              })}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function ScorePractice({ card, theme }: { card: CardQuestion; theme: Theme }) {
  const t = useCardPracticeText();
  const learning = useLearningText();
  const { t: uiT, formatError, formatNumber } = useI18n();
  const [session, setSession] = useState<ScorePracticeSession>();
  const questions = session?.questions;
  const [results, setResults] = useState<PracticeEvaluation[]>([]);
  const [index, setIndex] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>();
  const path = `/cards/${card.id}/score-variants`;
  useEffect(() => {
    const abort = new AbortController();
    void request<ScorePracticeSession>(path, {
      method: 'POST',
      signal: abort.signal,
    })
      .then(setSession)
      .catch((failure) => {
        if (!abort.signal.aborted) setError(failure);
      });
    return () => abort.abort();
  }, [path]);
  const question = questions?.[index];
  const response = card.kind === 'cube_response';
  const result = results[index];
  function scoreLabel(q: ScoreQuestion) {
    if (!q.card.position) return '';
    const context = matchContext(q.card.position.gnubg_match_id);
    return context.length
      ? `${context.scores.map((n) => formatNumber(n)).join('–')} · ${uiT('app.matchLength', { count: context.length })}`
      : uiT('common.moneyGame');
  }
  async function check(answer: PracticeAnswer) {
    if (!session || !question || busy || result) return;
    setBusy(true);
    setError(undefined);
    try {
      const evaluation = await request<PracticeEvaluation>(path, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          index: question.index,
          practice_id: session.id,
          answer,
        } satisfies ScoreAnswer),
      });
      setResults((previous) => [...previous, evaluation]);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      {!!error && <p role="alert">{formatError(error)}</p>}
      {!questions && !error && <p role="status">{learning('loading')}</p>}
      {question?.card.position && (
        <>
          <p>{scoreLabel(question)}</p>
          <Board
            position={question.card.position}
            white={card.players[0]}
            black={card.players[1]}
            theme={theme}
            cubeOfferPending={response}
          />
          {result ? (
            <>
              <PracticeResult evaluation={result} />
              <ActionButton
                icon="play"
                label={t('next')}
                shortcut="Enter"
                onClick={() => setIndex(index + 1)}
              >
                {t('next')}
              </ActionButton>
            </>
          ) : (
            <CubePracticeChoices
              key={question.index}
              kind={response ? 'cube_response' : 'cube_offer'}
              label={learning(response ? 'questionResponse' : 'questionOffer')}
              busy={busy}
              onCheck={(answer) => void check(answer)}
            />
          )}
        </>
      )}
      {questions && !question && (
        <ul>
          {questions.map((variant, i) => (
            <li key={variant.index}>
              <strong>{scoreLabel(variant)}</strong>
              <PracticeResult evaluation={results[i]} />
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

function TimedPreview({
  cards,
  theme,
}: {
  cards: CardQuestion[];
  theme: Theme;
}) {
  const t = useCardPracticeText();
  const learning = useLearningText();
  const { formatError, formatDecision } = useI18n();
  const [questionTime, setQuestionTime] = useState('12');
  const [answerTime, setAnswerTime] = useState('8');
  const [playing, setPlaying] = useState(false);
  const [index, setIndex] = useState(0);
  const [answer, setAnswer] = useState<CardAnswer>();
  const [error, setError] = useState<unknown>();
  const card = cards[index];
  const valid = [questionTime, answerTime].every(
    (time) => /^\d{1,2}$/.test(time) && Number(time) > 0 && Number(time) <= 60,
  );
  useEffect(() => {
    if (!playing || !card || !valid) return;
    const controller = new AbortController();
    const timer = window.setTimeout(
      () => {
        if (answer) {
          setAnswer(undefined);
          setIndex((value) => value + 1);
          return;
        }
        void request<CardAnswer>(`/cards/${card.id}/preview`, {
          signal: controller.signal,
        })
          .then((data) => {
            if (!controller.signal.aborted) setAnswer(data);
          })
          .catch((failure) => {
            if (!controller.signal.aborted) {
              setError(failure);
              setPlaying(false);
            }
          });
      },
      Number(answer ? answerTime : questionTime) * 1000,
    );
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [playing, card, answer, questionTime, answerTime, valid]);
  const control = playing && card ? 'pause' : 'play';
  const best =
    answer?.solution?.decision?.best ??
    answer?.solution?.candidates.find(({ rank }) => rank === 1)?.notation;
  return (
    <>
      <div className="learning-controls">
        <SearchFilterInput
          label={t('questionSeconds')}
          value={questionTime}
          onChange={setQuestionTime}
          inputMode="numeric"
          maxLength={2}
        />
        <SearchFilterInput
          label={t('answerSeconds')}
          value={answerTime}
          onChange={setAnswerTime}
          inputMode="numeric"
          maxLength={2}
        />
        <ActionButton
          icon={control === 'pause' ? 'pause' : 'play'}
          label={t(control)}
          disabled={!valid}
          onClick={() => {
            setError(undefined);
            if (!card) {
              setIndex(0);
              setPlaying(true);
            } else setPlaying(!playing);
          }}
        >
          {t(control)}
        </ActionButton>
      </div>
      {!!error && <p role="alert">{formatError(error)}</p>}
      <p role="status">
        {t('previewSeen', { count: index + Number(!!answer) })}
      </p>
      {card && !card.position && (
        <AnkiCardFace
          id={card.id}
          title={card.prompt}
          side={answer ? 'back' : 'front'}
        />
      )}
      {card?.position && (
        <>
          <h3>
            {learning(
              card.kind === 'checker'
                ? 'questionChecker'
                : card.kind === 'cube_response'
                  ? 'questionResponse'
                  : 'questionOffer',
            )}
          </h3>
          <Board
            position={card.position}
            white={card.players[0]}
            black={card.players[1]}
            theme={theme}
            cubeOfferPending={card.kind === 'cube_response'}
          />
          {best && <p role="status">{formatDecision(best)}</p>}
        </>
      )}
    </>
  );
}
