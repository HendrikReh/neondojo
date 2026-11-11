/** Read the complete text of labels that CSS has clipped inside the trigger. */
export function clippedTooltipText(anchor: Element) {
  return (
    Array.from(anchor.querySelectorAll('[data-tooltip-overflow]'))
      .filter((label) => label.scrollWidth > label.clientWidth)
      .map((label) => label.textContent?.trim())
      .filter(Boolean)
      .join(' · ') || undefined
  );
}
