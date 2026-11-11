import { useCallback, useEffect, useRef, useState } from 'react';
import type { PlaybackFrame, PlayFrame } from './play-animation';
import { playSounds } from './play-audio';

/** Queue presentation only. Cancellation never changes the saved engine state. */
export function usePlayAnimation(visible: boolean) {
  const [frame, setFrame] = useState<PlaybackFrame | null>(null);
  const [animating, setAnimating] = useState(false);
  const epoch = useRef(0),
    sequence = useRef(0),
    pending = useRef(0);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const waits = useRef(new Map<ReturnType<typeof setTimeout>, () => void>());
  const cancel = useCallback(() => {
    ++epoch.current;
    pending.current = 0;
    tail.current = Promise.resolve();
    for (const [timer, resolve] of waits.current) {
      clearTimeout(timer);
      resolve();
    }
    waits.current.clear();
    setFrame(null);
    setAnimating(false);
  }, []);
  useEffect(() => {
    if (!visible) cancel();
  }, [visible, cancel]);
  useEffect(() => () => cancel(), [cancel]);
  function delay(ms: number) {
    return new Promise<void>((resolve) => {
      const timer = setTimeout(() => {
        waits.current.delete(timer);
        resolve();
      }, ms);
      waits.current.set(timer, resolve);
    });
  }
  function play(frames: PlayFrame[]): Promise<void> {
    if (!frames.length) return Promise.resolve();
    const token = epoch.current;
    ++pending.current;
    setAnimating(true);
    const job = tail.current
      .then(async () => {
        const reduced = window.matchMedia(
          '(prefers-reduced-motion: reduce)',
        ).matches;
        for (const next of frames) {
          if (token !== epoch.current) return;
          const id = ++sequence.current;
          if (next.sound) playSounds([next.sound]);
          if (next.kind === 'dice') {
            if (next.roll) {
              const ticks = reduced ? 1 : 6;
              for (let tick = 0; tick < ticks; tick++) {
                if (token !== epoch.current) return;
                setFrame({
                  ...next,
                  id,
                  rolling: true,
                  dice: [((tick * 3 + 1) % 6) + 1, ((tick * 5 + 3) % 6) + 1],
                });
                await delay(reduced ? 80 : 65);
              }
            }
            if (token !== epoch.current) return;
            setFrame({ ...next, id, rolling: false });
            await delay(
              next.roll ? (reduced ? 100 : 240) : reduced ? 180 : 600,
            );
          } else {
            setFrame({ ...next, id });
            await delay(reduced ? 80 : 280);
            if (token !== epoch.current) return;
            setFrame({ ...next, id, before: next.after, motions: [] });
            await delay(reduced ? 10 : 45);
          }
        }
      })
      .finally(() => {
        if (token !== epoch.current) return;
        if (--pending.current === 0) {
          setFrame(null);
          setAnimating(false);
        }
      });
    tail.current = job.catch(() => {});
    return job;
  }
  return {
    frame,
    animating,
    play,
    cancel,
    isPlaying: () => pending.current > 0,
  };
}

export function useRollingFaces(active: boolean): number[] {
  const [faces, setFaces] = useState([1, 6]);
  useEffect(() => {
    if (!active) return;
    let tick = 0;
    const timer = setInterval(() => {
      ++tick;
      setFaces([((tick * 3 + 1) % 6) + 1, ((tick * 5 + 3) % 6) + 1]);
    }, 65);
    return () => clearInterval(timer);
  }, [active]);
  return faces;
}
