const FOCUSABLE_SETTINGS_SELECTOR = [
  "button:not([disabled])",
  "[href]",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex=\"-1\"]):not([disabled])",
].join(",");

/**
 * Return focusable descendants in document order while excluding controls
 * hidden by an ancestor or by their own `hidden` attribute. Keeping this
 * helper outside AppShell makes the modal focus contract easy to regression
 * test without mounting the full Tauri shell.
 */
export function getSettingsFocusableElements(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SETTINGS_SELECTOR))
    .filter((element) => {
      for (let current: HTMLElement | null = element; current; current = current.parentElement) {
        if (current.hidden || current.getAttribute("aria-hidden") === "true") return false;
        const style = typeof window === "undefined" ? undefined : window.getComputedStyle(current);
        if (style?.display === "none" || style?.visibility === "hidden") return false;
      }
      return true;
    });
}

