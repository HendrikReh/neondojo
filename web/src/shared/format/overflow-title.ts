const overflowSelector = '[data-tooltip-overflow]';

/**
 * Read the complete text of labels that CSS has clipped inside the trigger,
 * including the trigger itself. A label may name its complete text in
 * `data-tooltip-text`; otherwise its text content is used.
 */
export function clippedTooltipText(anchor: Element) {
  const labels = [
    ...(anchor.matches(overflowSelector) ? [anchor] : []),
    ...anchor.querySelectorAll(overflowSelector),
  ];
  const texts = labels
    .filter(
      (label) =>
        label.scrollWidth > label.clientWidth ||
        // A line-clamped label clips vertically instead.
        (label.scrollHeight > label.clientHeight &&
          getComputedStyle(label).getPropertyValue('-webkit-line-clamp') !==
            'none'),
    )
    .map(
      (label) =>
        label.getAttribute('data-tooltip-text')?.trim() ||
        label.textContent?.trim(),
    )
    .filter((text): text is string => Boolean(text));
  return [...new Set(texts)].join(' · ') || undefined;
}
