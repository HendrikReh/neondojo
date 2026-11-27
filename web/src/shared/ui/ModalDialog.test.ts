import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import ModalDialog, {
  FOCUSABLE_SELECTOR,
  lockBodyScroll,
  trapTabKey,
} from './ModalDialog';

function fakeElement(name: string, options: { disabled?: boolean; visible?: boolean } = {}) {
  const focus = vi.fn();
  const element = {
    name,
    focus,
    matches: (selector: string) =>
      selector === ':disabled' ? Boolean(options.disabled) : false,
    getClientRects: () => (options.visible === false ? [] : [{}]),
  };
  return element as unknown as HTMLElement & { focus: ReturnType<typeof vi.fn> };
}

function fakeContainer(children: HTMLElement[]) {
  const focus = vi.fn();
  return {
    focus,
    contains: (target: Element | null) =>
      children.includes(target as unknown as HTMLElement),
    querySelectorAll: () => children,
  } as unknown as HTMLElement & { focus: ReturnType<typeof vi.fn> };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

it('includes summary and tabindex=0 in the shared focusable selector', () => {
  expect(FOCUSABLE_SELECTOR).toContain('summary');
  expect(FOCUSABLE_SELECTOR).toContain('[tabindex="0"]');
  expect(FOCUSABLE_SELECTOR).toContain('button:not(:disabled)');
  expect(FOCUSABLE_SELECTOR).toContain('a[href]');
});

it('traps Tab and Shift+Tab within visible enabled controls', () => {
  const first = fakeElement('first');
  const hidden = fakeElement('hidden', { visible: false });
  const disabled = fakeElement('disabled', { disabled: true });
  const last = fakeElement('last');
  const container = fakeContainer([first, hidden, disabled, last]);

  const forwardAtLast = { key: 'Tab', shiftKey: false, preventDefault: vi.fn() };
  expect(trapTabKey(forwardAtLast, container, last)).toBe(true);
  expect(forwardAtLast.preventDefault).toHaveBeenCalledTimes(1);
  expect(first.focus).toHaveBeenCalledTimes(1);

  const backwardAtFirst = { key: 'Tab', shiftKey: true, preventDefault: vi.fn() };
  expect(trapTabKey(backwardAtFirst, container, first)).toBe(true);
  expect(backwardAtFirst.preventDefault).toHaveBeenCalledTimes(1);
  expect(last.focus).toHaveBeenCalledTimes(1);

  const outside = fakeElement('outside');
  const recapture = { key: 'Tab', shiftKey: false, preventDefault: vi.fn() };
  expect(trapTabKey(recapture, container, outside)).toBe(true);
  expect(first.focus).toHaveBeenCalledTimes(2);

  const empty = fakeContainer([]);
  const fallback = { key: 'Tab', shiftKey: false, preventDefault: vi.fn() };
  expect(trapTabKey(fallback, empty, null, empty)).toBe(true);
  expect(empty.focus).toHaveBeenCalledTimes(1);
});

it('reference-counts nested body scroll locks', () => {
  const body = { style: { overflow: 'auto' } };
  vi.stubGlobal('document', { body });
  const unlockOuter = lockBodyScroll();
  expect(body.style.overflow).toBe('hidden');
  const unlockInner = lockBodyScroll();
  expect(body.style.overflow).toBe('hidden');
  unlockInner();
  expect(body.style.overflow).toBe('hidden');
  unlockOuter();
  expect(body.style.overflow).toBe('auto');
});

it('renders native dialog semantics cleanly', () => {
  const html = renderToStaticMarkup(
    createElement(ModalDialog, {
      className: 'test-dialog',
      ariaLabelledBy: 'dialog-heading',
      ariaDescribedBy: 'dialog-desc',
      onCancel: () => {},
      children: createElement('h2', { id: 'dialog-heading' }, 'Dialog title'),
    }),
  );
  expect(html).toContain(
    '<dialog class="test-dialog" aria-labelledby="dialog-heading" aria-describedby="dialog-desc">',
  );
  expect(html).toContain('Dialog title');
});
