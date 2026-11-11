import {
  createElement,
  useCallback,
  type AriaAttributes,
  type ComponentPropsWithRef,
  type DOMAttributes,
  type FocusEvent,
  type MouseEvent,
  type PointerEvent,
  type Ref,
} from 'react';
import { createPortal } from 'react-dom';
import { useActionTooltip, type TooltipText } from './useActionTooltip';

type Trigger = 'button' | 'input' | 'span' | 'tr' | 'g';

export default function Tooltip<Tag extends Trigger>({
  as,
  text,
  ...attributes
}: ComponentPropsWithRef<Tag> & { as: Tag; text?: TooltipText }) {
  const {
    ref,
    onFocus,
    onBlur,
    onPointerEnter,
    onPointerLeave,
    onPointerDown,
    onClick,
    'aria-describedby': description,
    ...props
  } = attributes as DOMAttributes<Element> &
    AriaAttributes & { ref?: Ref<Element>; disabled?: boolean };
  const disabled =
    Boolean(props.disabled) ||
    props['aria-disabled'] === true ||
    props['aria-disabled'] === 'true';
  const tip = useActionTooltip(text, disabled);
  const connect = useCallback(
    (node: Element | null) => {
      tip.anchor.current = node;
      if (typeof ref === 'function') {
        const cleanup = ref(node);
        if (typeof cleanup === 'function')
          return () => {
            tip.anchor.current = null;
            cleanup();
          };
      } else if (ref) ref.current = node;
    },
    [tip.anchor, ref],
  );
  return (
    <>
      {createElement(as, {
        ...props,
        ref: connect,
        'aria-describedby':
          [description, tip.describedBy].filter(Boolean).join(' ') || undefined,
        'aria-description':
          props['aria-description'] ??
          (typeof text === 'string' && text !== props['aria-label']
            ? text
            : undefined),
        onFocus: (event: FocusEvent<Element>) => {
          if (event.target === event.currentTarget) tip.onFocus();
          onFocus?.(event);
        },
        onBlur: (event: FocusEvent<Element>) => {
          if (event.target === event.currentTarget) tip.onBlur();
          onBlur?.(event);
        },
        onPointerEnter: (event: PointerEvent<Element>) => {
          tip.onPointerEnter();
          onPointerEnter?.(event);
        },
        onPointerLeave: (event: PointerEvent<Element>) => {
          tip.onPointerLeave();
          onPointerLeave?.(event);
        },
        onPointerDown: (event: PointerEvent<Element>) => {
          tip.dismiss();
          onPointerDown?.(event);
        },
        onClick: (event: MouseEvent<Element>) => {
          tip.dismiss();
          onClick?.(event);
        },
      })}
      {tip.content &&
        tip.anchor.current &&
        createPortal(
          tip.content,
          tip.anchor.current.closest('dialog, .app-shell') ?? document.body,
        )}
    </>
  );
}
