// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo, type ComponentProps } from 'react';

import { SessionList } from '../SessionList';

export const useSessionListProps = (): ComponentProps<typeof SessionList> => useMemo(() => ({
  onSelectSession: () => undefined,
  onOpenSessionSummary: (_sessionId: string) => undefined,
  onOpenSessionRuntimeContext: (_sessionId: string) => undefined,
  activeSummarySessionId: null,
  activeRuntimeContextSessionId: null,
}), []);
