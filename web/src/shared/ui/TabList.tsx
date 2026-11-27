import {
  useCallback,
  useRef,
  type KeyboardEvent,
  type ReactNode,
} from 'react';

export type TabOrientation = 'horizontal' | 'vertical';

export interface RovingTabsOptions<T extends string> {
  items: readonly T[];
  active: T | null | undefined;
  onSelect: (id: T) => void;
  orientation?: TabOrientation;
  wrap?: boolean;
  disabled?: (id: T) => boolean;
  shouldFocus?: (id: T) => boolean;
}

export interface RovingTabsController<T extends string> {
  getTabRef: (id: T) => (element: HTMLButtonElement | null) => void;
  onKeyDown: (event: KeyboardEvent<HTMLButtonElement>, current: T) => void;
  tabIndex: (id: T) => 0 | -1;
  focusTab: (id: T) => void;
  getTabElement: (id: T) => HTMLButtonElement | null;
}

export function nextRovingTab<T extends string>(
  enabledItems: readonly T[],
  current: T,
  key: string,
  options: { orientation?: TabOrientation; wrap?: boolean } = {},
): T | undefined {
  if (enabledItems.length === 0) return undefined;
  const orientation = options.orientation ?? 'horizontal';
  const wrap = options.wrap ?? true;
  const prevKey = orientation === 'vertical' ? 'ArrowUp' : 'ArrowLeft';
  const nextKey = orientation === 'vertical' ? 'ArrowDown' : 'ArrowRight';
  const index = enabledItems.indexOf(current);
  const last = enabledItems.length - 1;
  if (key === 'Home') return enabledItems[0];
  if (key === 'End') return enabledItems[last];
  if (key === prevKey) {
    if (index < 0) return enabledItems[0];
    if (index === 0) return wrap ? enabledItems[last] : enabledItems[0];
    return enabledItems[index - 1];
  }
  if (key === nextKey) {
    if (index < 0) return enabledItems[0];
    if (index === last) return wrap ? enabledItems[0] : enabledItems[last];
    return enabledItems[index + 1];
  }
  return undefined;
}

export function useRovingTabs<T extends string>({
  items,
  active,
  onSelect,
  orientation = 'horizontal',
  wrap = true,
  disabled,
  shouldFocus,
}: RovingTabsOptions<T>): RovingTabsController<T> {
  const refs = useRef<Partial<Record<T, HTMLButtonElement | null>>>({});
  const enabledItems = disabled
    ? items.filter((id) => !disabled(id))
    : items;
  const activeTabStop =
    active != null && enabledItems.includes(active) ? active : enabledItems[0];

  const getTabRef = useCallback(
    (id: T) => (element: HTMLButtonElement | null) => {
      refs.current[id] = element;
    },
    [],
  );

  const focusTab = useCallback((id: T) => {
    refs.current[id]?.focus();
  }, []);

  const getTabElement = useCallback(
    (id: T) => refs.current[id] ?? null,
    [],
  );

  const tabIndex = (id: T): 0 | -1 => (id === activeTabStop ? 0 : -1);

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, current: T) => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey)
      return;
    const next = nextRovingTab(enabledItems, current, event.key, {
      orientation,
      wrap,
    });
    if (next === undefined) return;
    event.preventDefault();
    if (!wrap && next === current && next === active) return;
    if (!shouldFocus || shouldFocus(next)) refs.current[next]?.focus();
    onSelect(next);
  };

  return { getTabRef, onKeyDown, tabIndex, focusTab, getTabElement };
}

export interface TabItem<T extends string> {
  value: T;
  label: ReactNode;
  id?: string;
  controls?: string;
  disabled?: boolean;
  className?: string;
  title?: string;
}

export function TabList<T extends string>({
  as = 'div',
  className,
  ariaLabel,
  tabs,
  active,
  onSelect,
  orientation = 'horizontal',
  wrap = true,
  shouldFocus,
  roving,
}: {
  as?: 'div' | 'nav';
  className?: string;
  ariaLabel: string;
  tabs: readonly TabItem<T>[];
  active: T | null | undefined;
  onSelect: (id: T) => void;
  orientation?: TabOrientation;
  wrap?: boolean;
  shouldFocus?: (id: T) => boolean;
  roving?: RovingTabsController<T>;
}) {
  const internal = useRovingTabs<T>({
    items: tabs.map((tab) => tab.value),
    active,
    onSelect,
    orientation,
    wrap,
    disabled: (id) => Boolean(tabs.find((tab) => tab.value === id)?.disabled),
    shouldFocus,
  });
  const controller = roving ?? internal;
  const Container = as;

  return (
    <Container
      className={className}
      role="tablist"
      aria-label={ariaLabel}
      aria-orientation={orientation === 'vertical' ? 'vertical' : undefined}
    >
      {tabs.map((tab) => (
        <button
          key={tab.value}
          type="button"
          role="tab"
          id={tab.id}
          aria-controls={tab.controls}
          aria-selected={active === tab.value}
          tabIndex={controller.tabIndex(tab.value)}
          disabled={tab.disabled}
          className={tab.className}
          title={tab.title}
          ref={controller.getTabRef(tab.value)}
          onClick={() => onSelect(tab.value)}
          onKeyDown={(event) => controller.onKeyDown(event, tab.value)}
        >
          {tab.label}
        </button>
      ))}
    </Container>
  );
}
