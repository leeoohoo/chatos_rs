import { useMemo, type ComponentProps } from 'react';

import { SessionList } from '../SessionList';

export const useSessionListProps = (): ComponentProps<typeof SessionList> => useMemo(() => ({
  onSelectSession: () => undefined,
  onOpenSessionSummary: (_sessionId: string) => undefined,
  onOpenSessionRuntimeContext: (_sessionId: string) => undefined,
  activeSummarySessionId: null,
  activeRuntimeContextSessionId: null,
}), []);
