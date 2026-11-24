import { useEffect, useRef, useState, type RefObject } from 'react';
import type { AcceptedSession } from './usePlaySession';

export type PlayDialogState = ReturnType<typeof usePlayDialogs>;

/** The dialogs the player opens over the board, closed while Play is hidden. */
export function usePlayDialogs(visible: boolean) {
  const [hintOpen, setHintOpen] = useState(false);
  const [hintReady, setHintReady] = useState(false);
  const [resignOpen, setResignOpen] = useState(false);
  const [forfeitOpen, setForfeitOpen] = useState(false);
  const [pauseWarning, setPauseWarning] = useState(false);
  const [learningReviewOpen, setLearningReviewOpen] = useState(false);
  const restoreResignFocus = useRef(false);
  const restoreHintFocus = useRef(false);
  useEffect(() => {
    if (visible) return;
    setHintOpen(false);
    setResignOpen(false);
    setForfeitOpen(false);
    setPauseWarning(false);
    setLearningReviewOpen(false);
  }, [visible]);
  return {
    hintOpen,
    hintReady,
    setHintReady,
    resignOpen,
    forfeitOpen,
    setForfeitOpen,
    pauseWarning,
    setPauseWarning,
    learningReviewOpen,
    setLearningReviewOpen,
    openHint: () => {
      setHintReady(false);
      setHintOpen(true);
    },
    closeHint: () => {
      restoreHintFocus.current = true;
      setHintOpen(false);
    },
    openResign: () => setResignOpen(true),
    closeResign: () => {
      restoreResignFocus.current = true;
      setResignOpen(false);
    },
    /** Closes what belongs to the checkpoint that another one replaces. */
    accept: ({
      sameSession,
      sameGame,
      preserveSteps,
    }: Pick<AcceptedSession, 'sameSession' | 'sameGame' | 'preserveSteps'>) => {
      if (!sameGame) setLearningReviewOpen(false);
      if (preserveSteps && sameSession) return;
      setHintOpen(false);
      setResignOpen(false);
      setForfeitOpen(false);
      setPauseWarning(false);
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    },
    restoreResignFocus,
    restoreHintFocus,
  };
}

/** Returns the focus to the button that opened Resign or Hint. */
export function usePlayDialogFocus({
  dialogs,
  resignTrigger,
  hintTrigger,
  hidden,
  busy,
  interrupted,
}: {
  dialogs: PlayDialogState;
  resignTrigger: RefObject<HTMLButtonElement | null>;
  hintTrigger: RefObject<HTMLButtonElement | null>;
  hidden: boolean;
  busy: boolean;
  interrupted: boolean;
}) {
  const { hintOpen, resignOpen, restoreResignFocus, restoreHintFocus } =
    dialogs;
  useEffect(() => {
    if (hidden) {
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    } else if (!hintOpen && !resignOpen && !busy) {
      const trigger = restoreResignFocus.current
        ? resignTrigger
        : restoreHintFocus.current
          ? hintTrigger
          : undefined;
      if (!interrupted) trigger?.current?.focus({ preventScroll: true });
      restoreResignFocus.current = false;
      restoreHintFocus.current = false;
    }
  }, [hidden, hintOpen, resignOpen, busy, interrupted]);
}
