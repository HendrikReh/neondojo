import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';
import Tooltip from '../ui/Tooltip';

type Drag = {
  pointer: number;
  target: HTMLElement;
  startX: number;
  startWidth: number;
  width: number;
  frame?: number;
};

// A drag writes the width straight to the container's CSS variable and the
// separator's aria-valuenow once per frame. React state changes only when the
// drag starts and ends, so pointer moves do not re-render the workspace.
function showWidth(
  container: HTMLElement | null,
  cssVar: string,
  drag: Drag,
  width: number,
) {
  container?.style.setProperty(cssVar, `${width}px`);
  drag.target.setAttribute('aria-valuenow', String(width));
}

function stopDrag(
  container: HTMLElement | null,
  cssVar: string,
  drag: Drag,
  width: number,
) {
  if (drag.frame !== undefined) cancelAnimationFrame(drag.frame);
  showWidth(container, cssVar, drag, width);
  if (drag.target.hasPointerCapture(drag.pointer))
    drag.target.releasePointerCapture(drag.pointer);
}

export function usePaneSplitter({
  containerRef,
  cssVar,
  width,
  min,
  max,
  direction = 'right',
  active = true,
  resetKey,
  onCommit,
  onReset,
  ariaLabel,
  ariaControls,
  helpText,
  className,
  hidden,
  preventFocusScroll = false,
}: {
  containerRef: RefObject<HTMLElement | null>;
  cssVar: string;
  width: number;
  min: number;
  max: number;
  direction?: 'right' | 'left';
  active?: boolean;
  resetKey?: string | number | null;
  onCommit: (width: number) => void;
  onReset: () => void;
  ariaLabel: string;
  ariaControls: string;
  helpText: string;
  className: string;
  hidden?: boolean;
  preventFocusScroll?: boolean;
}) {
  const [dragging, setDragging] = useState(false);
  const drag = useRef<Drag | undefined>(undefined);
  const clamp = (value: number) =>
    Math.round(Math.max(min, Math.min(max, value)));
  const sign = direction === 'right' ? 1 : -1;

  const cancel = useCallback(() => {
    const previous = drag.current;
    drag.current = undefined;
    setDragging(false);
    if (previous)
      stopDrag(containerRef.current, cssVar, previous, previous.startWidth);
  }, [containerRef, cssVar]);

  useLayoutEffect(() => {
    cancel();
  }, [active, resetKey, cancel]);

  useLayoutEffect(
    () => () => {
      const previous = drag.current;
      drag.current = undefined;
      if (previous?.frame !== undefined) cancelAnimationFrame(previous.frame);
      if (previous?.target.hasPointerCapture(previous.pointer))
        previous.target.releasePointerCapture(previous.pointer);
    },
    [],
  );

  const handle = (
    <Tooltip
      as="span"
      role="separator"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-orientation="vertical"
      aria-controls={ariaControls}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={width}
      text={helpText}
      className={className}
      hidden={hidden}
      data-dragging={dragging}
      onPointerDown={(event) => {
        if (event.button !== 0 || !event.isPrimary || drag.current) return;
        event.preventDefault();
        event.stopPropagation();
        event.currentTarget.focus(
          preventFocusScroll ? { preventScroll: true } : undefined,
        );
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = {
          pointer: event.pointerId,
          target: event.currentTarget,
          startX: event.clientX,
          startWidth: width,
          width,
        };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        current.width = clamp(
          current.startWidth + sign * (event.clientX - current.startX),
        );
        current.frame ??= requestAnimationFrame(() => {
          current.frame = undefined;
          if (drag.current === current)
            showWidth(containerRef.current, cssVar, current, current.width);
        });
      }}
      onPointerUp={(event) => {
        const current = drag.current;
        if (!current || current.pointer !== event.pointerId) return;
        drag.current = undefined;
        stopDrag(containerRef.current, cssVar, current, current.width);
        setDragging(false);
        if (current.width !== current.startWidth) onCommit(current.width);
      }}
      onPointerCancel={cancel}
      onLostPointerCapture={cancel}
      onDoubleClick={() => {
        cancel();
        onReset();
      }}
      onKeyDown={(event) => {
        if (
          ![
            'ArrowLeft',
            'ArrowRight',
            'Home',
            'End',
            'Enter',
            'Escape',
          ].includes(event.key)
        )
          return;
        event.preventDefault();
        event.stopPropagation();
        if (event.key === 'Escape') {
          cancel();
          return;
        }
        if (drag.current) return;
        if (event.key === 'Enter') {
          onReset();
          return;
        }
        const step = event.shiftKey ? 50 : 10;
        const delta = event.key === 'ArrowLeft' ? -step * sign : step * sign;
        const next =
          event.key === 'Home'
            ? min
            : event.key === 'End'
              ? max
              : width + delta;
        const nextWidth = clamp(next);
        if (nextWidth !== width) onCommit(nextWidth);
      }}
    />
  );

  return { dragging, handle };
}
