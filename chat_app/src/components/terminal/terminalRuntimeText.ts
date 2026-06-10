import type { MutableRefObject } from 'react';

export interface TerminalRuntimeText {
  genericError: string;
  realtimeConnectionFailed: string;
  authFailed: string;
  historyLoadFailed: string;
}

export type TerminalRuntimeTextRef = MutableRefObject<TerminalRuntimeText>;
