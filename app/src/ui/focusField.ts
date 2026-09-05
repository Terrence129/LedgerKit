/** Reveal progressive-disclosure ancestors before moving keyboard focus. */
export function focusField(field: string): void {
  requestAnimationFrame(() => {
    const target = document.getElementById(field);
    let parent = target?.parentElement;
    while (parent) {
      if (parent instanceof HTMLDetailsElement) parent.open = true;
      parent = parent.parentElement;
    }
    target?.focus();
  });
}
