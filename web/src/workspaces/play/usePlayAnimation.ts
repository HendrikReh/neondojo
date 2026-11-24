import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import type {
  PlaybackFrame,
  PlayFrame,
} from '../../shared/board/play-animation';
import { playSounds } from '../../shared/board/play-audio';

/** What the workspace around the board shows of a frame. */
export type PlayScene =
  | { kind: 'checker'; player: number }
  | {
      kind: 'dice';
      player: number;
      opening: boolean;
      rolling: boolean;
      tied: boolean;
    };

const sceneOf = (frame: PlaybackFrame): PlayScene =>
  frame.kind === 'checker'
    ? { kind: 'checker', player: frame.player }
    : {
        kind: 'dice',
        player: frame.player,
        opening: !!frame.opening,
        rolling: !!frame.rolling,
        tied: !!frame.tied,
      };

const sameScene = (a: PlayScene | null, b: PlayScene) =>
  a?.kind === b.kind &&
  a.player === b.player &&
  (a.kind === 'checker' ||
    b.kind === 'checker' ||
    (a.opening === b.opening && a.rolling === b.rolling && a.tied === b.tied));

type FrameListener = (frame: PlaybackFrame | null) => void;
/** The displayed frame. Dice change it every 65 ms, so only the board reads it. */
class PlayFrames {
  current: PlaybackFrame | null = null;
  private readonly listeners = new Set<FrameListener>();
  show(frame: PlaybackFrame | null) {
    this.current = frame;
    for (const listener of this.listeners) listener(frame);
  }
  subscribe(listener: FrameListener) {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  }
}
export type { PlayFrames };

/**
 * The displayed frame as state of the board. This is deliberately not
 * useSyncExternalStore: that renders the end of an animation before the state
 * set with it, and the board would show the previous checkpoint in between.
 */
export function usePlayFrame(frames: PlayFrames) {
  const [frame, setFrame] = useState(frames.current);
  useLayoutEffect(() => {
    setFrame(frames.current);
    return frames.subscribe(setFrame);
  }, [frames]);
  return frame;
}

/** Queue presentation only. Cancellation never changes the saved engine state. */
export function usePlayAnimation(visible: boolean) {
  const [frames] = useState(() => new PlayFrames());
  const [scene, setScene] = useState<PlayScene | null>(null);
  const [animating, setAnimating] = useState(false);
  const epoch = useRef(0),
    sequence = useRef(0),
    pending = useRef(0);
  const tail = useRef<Promise<void>>(Promise.resolve());
  const waits = useRef(new Map<ReturnType<typeof setTimeout>, () => void>());
  const show = useCallback(
    (frame: PlaybackFrame | null) => {
      frames.show(frame);
      setScene((current) => {
        if (!frame) return null;
        const next = sceneOf(frame);
        return sameScene(current, next) ? current : next;
      });
    },
    [frames],
  );
  const cancel = useCallback(() => {
    ++epoch.current;
    pending.current = 0;
    tail.current = Promise.resolve();
    for (const [timer, resolve] of waits.current) {
      clearTimeout(timer);
      resolve();
    }
    waits.current.clear();
    show(null);
    setAnimating(false);
  }, [show]);
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
                show({
                  ...next,
                  id,
                  rolling: true,
                  dice: [((tick * 3 + 1) % 6) + 1, ((tick * 5 + 3) % 6) + 1],
                });
                await delay(reduced ? 80 : 65);
              }
            }
            if (token !== epoch.current) return;
            show({ ...next, id, rolling: false });
            const settle = next.roll ? (next.opening ? 440 : 240) : 600;
            const reducedSettle = next.roll && !next.opening ? 100 : 180;
            await delay(reduced ? reducedSettle : settle);
          } else {
            show({ ...next, id });
            await delay(reduced ? 80 : 280);
            if (token !== epoch.current) return;
            show({ ...next, id, before: next.after, motions: [] });
            await delay(reduced ? 10 : 45);
          }
        }
      })
      .finally(() => {
        if (token !== epoch.current) return;
        if (--pending.current === 0) {
          show(null);
          setAnimating(false);
        }
      });
    tail.current = job.catch(() => {});
    return job;
  }
  return {
    frames,
    scene,
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
