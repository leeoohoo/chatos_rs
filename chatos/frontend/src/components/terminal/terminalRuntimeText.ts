// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MutableRefObject } from 'react';

export interface TerminalRuntimeText {
  genericError: string;
  realtimeConnectionFailed: string;
  authFailed: string;
  historyLoadFailed: string;
}

export type TerminalRuntimeTextRef = MutableRefObject<TerminalRuntimeText>;
