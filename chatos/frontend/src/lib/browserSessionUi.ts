// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface BrowserSessionUiTarget {
  id: string;
  workspaceId: string;
  deviceId?: string | null;
  projectId?: string | null;
  mode: 'managed';
  status?: string | null;
  url?: string | null;
  title?: string | null;
}

const OPEN_BROWSER_SESSION_EVENT = 'chatos:open-browser-session';

export const openBrowserSessionPanel = (target: BrowserSessionUiTarget): void => {
  if (typeof window === 'undefined') {
    return;
  }
  window.dispatchEvent(new CustomEvent<BrowserSessionUiTarget>(OPEN_BROWSER_SESSION_EVENT, {
    detail: target,
  }));
};

export const subscribeBrowserSessionPanel = (
  listener: (target: BrowserSessionUiTarget) => void,
): (() => void) => {
  if (typeof window === 'undefined') {
    return () => undefined;
  }
  const handler = (event: Event) => {
    const target = (event as CustomEvent<BrowserSessionUiTarget>).detail;
    if (target?.id && target.workspaceId && target.mode === 'managed') {
      listener(target);
    }
  };
  window.addEventListener(OPEN_BROWSER_SESSION_EVENT, handler);
  return () => window.removeEventListener(OPEN_BROWSER_SESSION_EVENT, handler);
};
