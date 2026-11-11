import {
  forwardRef,
  useCallback,
  type ButtonHTMLAttributes,
  type AnchorHTMLAttributes,
  type ReactNode,
  type Ref,
} from 'react';
import ActionIcon, { type ActionIconName } from './action-icons';
import { useActionTooltip } from './useActionTooltip';
import './action-controls.css';
interface ActionProps {
  icon: ActionIconName;
  label: string;
  compact?: boolean;
  tooltip?: string;
}
function actionTooltipText(
  label: string,
  compact: boolean,
  children: ReactNode,
  tooltip?: string,
) {
  const text = tooltip?.trim();
  if (compact) return text ?? label;
  const visibleText = typeof children === 'string' ? children.trim() : label;
  if (text && text !== label.trim() && text !== visibleText) return text;
}
function assignRef<T>(ref: Ref<T> | undefined, value: T | null) {
  if (typeof ref === 'function') return ref(value);
  else if (ref) ref.current = value;
}
export type ActionButtonProps = ButtonHTMLAttributes<HTMLButtonElement> &
  ActionProps;
const ActionButton = forwardRef<HTMLButtonElement, ActionButtonProps>(
  function ActionButton(
    {
      icon,
      label,
      compact = false,
      tooltip,
      title,
      className = '',
      children,
      disabled,
      onFocus,
      onBlur,
      onPointerEnter,
      onPointerLeave,
      onPointerDown,
      onClick,
      'aria-describedby': description,
      'aria-label': accessName,
      ...props
    },
    ref,
  ) {
    const tip = useActionTooltip(
      actionTooltipText(label, compact, children, tooltip ?? title),
      disabled,
    );
    const mergedRef = useCallback(
      (node: HTMLButtonElement | null) => {
        tip.anchor.current = node;
        const cleanup = assignRef(ref, node);
        if (typeof cleanup === 'function')
          return () => {
            tip.anchor.current = null;
            cleanup();
          };
      },
      [ref, tip.anchor],
    );
    return (
      <button
        {...props}
        disabled={disabled}
        ref={mergedRef}
        className={`action-button ${compact ? 'action-button--compact' : ''} ${className}`}
        aria-label={accessName ?? label}
        aria-describedby={
          [description, tip.describedBy].filter(Boolean).join(' ') || undefined
        }
        data-action-icon={icon}
        data-action-compact={compact}
        onFocus={(e) => {
          tip.onFocus();
          onFocus?.(e);
        }}
        onBlur={(e) => {
          tip.onBlur();
          onBlur?.(e);
        }}
        onPointerEnter={(e) => {
          tip.onPointerEnter();
          onPointerEnter?.(e);
        }}
        onPointerLeave={(e) => {
          tip.onPointerLeave();
          onPointerLeave?.(e);
        }}
        onPointerDown={(e) => {
          tip.dismiss();
          onPointerDown?.(e);
        }}
        onClick={(e) => {
          tip.dismiss();
          onClick?.(e);
        }}
      >
        <ActionIcon name={icon} />
        {!compact && <span className="action-label">{children ?? label}</span>}
        {tip.content}
      </button>
    );
  },
);
export default ActionButton;
export type ActionLinkProps = AnchorHTMLAttributes<HTMLAnchorElement> &
  ActionProps;
export const ActionLink = forwardRef<HTMLAnchorElement, ActionLinkProps>(
  function ActionLink(
    {
      icon,
      label,
      compact = false,
      tooltip,
      title,
      className = '',
      children,
      onFocus,
      onBlur,
      onPointerEnter,
      onPointerLeave,
      onPointerDown,
      onClick,
      'aria-describedby': description,
      'aria-label': accessName,
      ...props
    },
    ref,
  ) {
    const tip = useActionTooltip(
      actionTooltipText(label, compact, children, tooltip ?? title),
    );
    const mergedRef = useCallback(
      (node: HTMLAnchorElement | null) => {
        tip.anchor.current = node;
        const cleanup = assignRef(ref, node);
        if (typeof cleanup === 'function')
          return () => {
            tip.anchor.current = null;
            cleanup();
          };
      },
      [ref, tip.anchor],
    );
    return (
      <a
        {...props}
        ref={mergedRef}
        className={`action-button ${compact ? 'action-button--compact' : ''} ${className}`}
        aria-label={accessName ?? label}
        aria-describedby={
          [description, tip.describedBy].filter(Boolean).join(' ') || undefined
        }
        data-action-icon={icon}
        data-action-compact={compact}
        onFocus={(e) => {
          tip.onFocus();
          onFocus?.(e);
        }}
        onBlur={(e) => {
          tip.onBlur();
          onBlur?.(e);
        }}
        onPointerEnter={(e) => {
          tip.onPointerEnter();
          onPointerEnter?.(e);
        }}
        onPointerLeave={(e) => {
          tip.onPointerLeave();
          onPointerLeave?.(e);
        }}
        onPointerDown={(e) => {
          tip.dismiss();
          onPointerDown?.(e);
        }}
        onClick={(e) => {
          tip.dismiss();
          onClick?.(e);
        }}
      >
        <ActionIcon name={icon} />
        {!compact && <span className="action-label">{children ?? label}</span>}
        {tip.content}
      </a>
    );
  },
);
