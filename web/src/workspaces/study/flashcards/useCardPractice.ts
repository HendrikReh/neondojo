import { useEffect, useRef, useState } from 'react';
import { request } from '../../../shared/http/api';
import { AppError } from '../../../shared/i18n/i18n-core';
import { roundPath } from './CardPracticeRound';
import type { PracticeRound } from './learning-api';

const practiceStorageKey = 'neondojo.card-practice-round';

function storedPracticeRound() {
  try {
    return localStorage.getItem(practiceStorageKey);
  } catch {
    return null;
  }
}

/** Starts practice rounds and resumes the last one, also after a reload. */
export function useCardPractice({
  scope,
  visible,
  startPath,
}: {
  /** The listed cards; another list ends a round that is still starting. */
  scope: string;
  visible: boolean;
  startPath: () => string;
}) {
  const [practiceRound, setPracticeRound] = useState<PracticeRound>();
  const [resumeRound, setResumeRound] = useState(storedPracticeRound);
  const [practiceStarting, setPracticeStarting] = useState(false);
  const [practiceError, setPracticeError] = useState<unknown>();
  const practiceRequest = useRef<AbortController | null>(null);
  useEffect(() => () => practiceRequest.current?.abort(), []);
  useEffect(() => {
    if (practiceRequest.current) {
      practiceRequest.current.abort();
      practiceRequest.current = null;
      setPracticeStarting(false);
    }
    setPracticeError(undefined);
  }, [scope, visible]);
  function rememberRound(id: string | null) {
    setResumeRound(id);
    try {
      if (id) localStorage.setItem(practiceStorageKey, id);
      else localStorage.removeItem(practiceStorageKey);
    } catch {
      // The current round remains usable in this tab.
    }
  }
  async function openPractice(existing?: string) {
    if (practiceRequest.current) return;
    const controller = new AbortController();
    practiceRequest.current = controller;
    setPracticeStarting(true);
    setPracticeError(undefined);
    try {
      const round = await request<PracticeRound>(
        existing ? roundPath(existing) : startPath(),
        { method: existing ? 'GET' : 'POST', signal: controller.signal },
      );
      if (controller.signal.aborted) return;
      rememberRound(round.id);
      setPracticeRound(round);
    } catch (failure) {
      if (controller.signal.aborted) return;
      if (
        existing &&
        failure instanceof AppError &&
        failure.params.status === '404'
      )
        rememberRound(null);
      setPracticeError(failure);
    } finally {
      if (practiceRequest.current === controller) {
        practiceRequest.current = null;
        setPracticeStarting(false);
      }
    }
  }
  return {
    round: practiceRound,
    setRound: setPracticeRound,
    resumeRound,
    rememberRound,
    starting: practiceStarting,
    error: practiceError,
    open: openPractice,
  };
}
