type BusyChatState = {
  isLoading?: boolean;
  isStreaming?: boolean;
  streamingPhase?: 'thinking' | 'reviewing' | null;
} | null | undefined;

type PanelMap<T> = Record<string, T[] | undefined> | null | undefined;

export const countPendingSessionPanels = <T, U>({
  sessionId,
  taskReviewPanelsBySession,
  uiPromptPanelsBySession,
}: {
  sessionId: string | null | undefined;
  taskReviewPanelsBySession?: PanelMap<T>;
  uiPromptPanelsBySession?: PanelMap<U>;
}): {
  taskReviewCount: number;
  uiPromptCount: number;
  pendingCount: number;
} => {
  const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
  if (!normalizedSessionId) {
    return {
      taskReviewCount: 0,
      uiPromptCount: 0,
      pendingCount: 0,
    };
  }

  const taskReviewCount = Array.isArray(taskReviewPanelsBySession?.[normalizedSessionId])
    ? taskReviewPanelsBySession?.[normalizedSessionId]?.length || 0
    : 0;
  const uiPromptCount = Array.isArray(uiPromptPanelsBySession?.[normalizedSessionId])
    ? uiPromptPanelsBySession?.[normalizedSessionId]?.length || 0
    : 0;

  return {
    taskReviewCount,
    uiPromptCount,
    pendingCount: taskReviewCount + uiPromptCount,
  };
};

export const resolveSessionBusyPhase = ({
  chatState,
  pendingTaskReviewCount = 0,
  pendingUiPromptCount = 0,
}: {
  chatState?: BusyChatState;
  pendingTaskReviewCount?: number;
  pendingUiPromptCount?: number;
}): 'thinking' | 'reviewing' | null => {
  if (chatState?.streamingPhase === 'reviewing') {
    return 'reviewing';
  }
  if (chatState?.streamingPhase === 'thinking') {
    return 'thinking';
  }
  if (chatState?.isLoading || chatState?.isStreaming) {
    return 'thinking';
  }
  if (pendingTaskReviewCount > 0 || pendingUiPromptCount > 0) {
    return 'thinking';
  }
  return null;
};
