import {
  recoverStreamingTurnBySnapshot,
} from '../../lib/store/actions/sendMessage/turnRecovery';
import {
  createDefaultSessionChatState,
} from '../../lib/store/actions/sendMessage/sessionState';
import {
  isTaskRunnerAsyncPlanMessage,
  isTaskRunnerCallbackMessage,
} from '../../lib/store/helpers/messageNormalization';
import type { ChatStoreDraft, ChatStoreSet } from '../../lib/store/types';

type RuntimeRecoveryApiClient = Parameters<typeof recoverStreamingTurnBySnapshot>[0]['apiClient'];
type RuntimeRecoveryApiClientLike = Partial<RuntimeRecoveryApiClient>;

const normalizeId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const shouldAutoRecoverPendingPanelSession = ({
  targetSessionId,
  currentSessionId,
  activeTurnId,
  conversationTurnId,
  isLoading,
  isStreaming,
  isStopping,
}: {
  targetSessionId: string;
  currentSessionId: string | null | undefined;
  activeTurnId: string | null | undefined;
  conversationTurnId: string | null | undefined;
  isLoading: boolean;
  isStreaming: boolean;
  isStopping: boolean;
}): boolean => {
  const normalizedTargetSessionId = normalizeId(targetSessionId);
  const normalizedCurrentSessionId = normalizeId(currentSessionId);
  const normalizedActiveTurnId = normalizeId(activeTurnId);
  const normalizedConversationTurnId = normalizeId(conversationTurnId);

  if (
    !normalizedTargetSessionId
    || !normalizedCurrentSessionId
    || normalizedTargetSessionId !== normalizedCurrentSessionId
    || !normalizedConversationTurnId
  ) {
    return false;
  }
  if (isLoading || isStreaming || isStopping) {
    return false;
  }
  if (!normalizedActiveTurnId) {
    return true;
  }
  return normalizedActiveTurnId === normalizedConversationTurnId;
};

const findAssistantCandidateForTurn = (
  state: ChatStoreDraft,
  sessionId: string,
  turnId: string,
) => {
  for (let index = state.messages.length - 1; index >= 0; index -= 1) {
    const message = state.messages[index];
    if (!message || message.role !== 'assistant') {
      continue;
    }
    if (isTaskRunnerCallbackMessage(message)) {
      continue;
    }
    if (isTaskRunnerAsyncPlanMessage(message)) {
      continue;
    }
    if (normalizeId(message.sessionId) !== sessionId) {
      continue;
    }
    const messageTurnId = normalizeId(
      message.metadata?.conversation_turn_id
      || message.metadata?.historyFinalForTurnId
      || message.metadata?.historyProcess?.turnId,
    );
    if (messageTurnId === turnId) {
      return message;
    }
  }
  return null;
};

const readLinkedUserMessageId = (assistantMessage: ChatStoreDraft['messages'][number] | null): string | null => {
  const linkedUserMessageId = normalizeId(
    assistantMessage?.metadata?.historyFinalForUserMessageId
    || assistantMessage?.metadata?.historyDraftUserMessage?.id,
  );
  return linkedUserMessageId || null;
};

const applyOptimisticBusyState = (
  set: ChatStoreSet,
  {
    sessionId,
    turnId,
    assistantMessageId,
    previewText,
  }: {
    sessionId: string;
    turnId: string;
    assistantMessageId: string;
    previewText: string;
  },
) => {
  set((state) => {
    const prev = state.sessionChatState?.[sessionId] || createDefaultSessionChatState();
    state.sessionChatState[sessionId] = {
      ...prev,
      isLoading: true,
      isStreaming: true,
      isStopping: false,
      streamingPhase: 'thinking',
      streamingMessageId: assistantMessageId,
      activeTurnId: turnId,
      streamingPreviewText: previewText,
      streamingTransport: 'realtime',
    };
    if (state.currentSessionId === sessionId) {
      state.isLoading = true;
      state.isStreaming = true;
      state.streamingMessageId = assistantMessageId;
    }
  });
};

export const recoverPendingPanelConversation = async ({
  apiClient,
  getState,
  set,
  sessionId,
  conversationTurnId,
}: {
  apiClient: RuntimeRecoveryApiClientLike;
  getState: () => ChatStoreDraft;
  set: ChatStoreSet;
  sessionId: string;
  conversationTurnId: string;
}): Promise<void> => {
  const normalizedSessionId = normalizeId(sessionId);
  const normalizedTurnId = normalizeId(conversationTurnId);
  if (!normalizedSessionId || !normalizedTurnId) {
    return;
  }

  const latestState = getState();
  const latestChatState = latestState.sessionChatState?.[normalizedSessionId];
  if (!shouldAutoRecoverPendingPanelSession({
    targetSessionId: normalizedSessionId,
    currentSessionId: latestState.currentSessionId,
    activeTurnId: latestChatState?.activeTurnId || null,
    conversationTurnId: normalizedTurnId,
    isLoading: latestChatState?.isLoading ?? false,
    isStreaming: latestChatState?.isStreaming ?? false,
    isStopping: latestChatState?.isStopping ?? false,
  })) {
    return;
  }

  const assistantCandidate = findAssistantCandidateForTurn(
    latestState,
    normalizedSessionId,
    normalizedTurnId,
  );
  const tempAssistantMessageId = normalizeId(assistantCandidate?.id)
    || `recovered_streaming_${normalizedTurnId}`;
  const tempUserId = readLinkedUserMessageId(assistantCandidate);

  if (assistantCandidate) {
    applyOptimisticBusyState(set, {
      sessionId: normalizedSessionId,
      turnId: normalizedTurnId,
      assistantMessageId: tempAssistantMessageId,
      previewText: typeof assistantCandidate.content === 'string'
        ? assistantCandidate.content
        : '',
    });
  }

  if (
    typeof apiClient.getConversationLatestTurnRuntimeContext !== 'function'
    || typeof apiClient.getConversationTurnRuntimeContextByTurn !== 'function'
    || typeof apiClient.getConversationTurnMessagesByTurn !== 'function'
    || typeof apiClient.getConversationTurnMessages !== 'function'
  ) {
    const postRecoveryState = getState();
    if (typeof postRecoveryState.syncSessionMessagesInBackground === 'function') {
      await postRecoveryState.syncSessionMessagesInBackground(normalizedSessionId);
    }
    return;
  }

  try {
    const result = await recoverStreamingTurnBySnapshot({
      apiClient: apiClient as RuntimeRecoveryApiClient,
      set,
      sessionId: normalizedSessionId,
      turnId: normalizedTurnId,
      tempAssistantMessageId,
      tempUserId,
      preferredUserMessageId: tempUserId,
    });
    if (result.recovered) {
      return;
    }
  } catch (error) {
    console.error('Failed to recover pending panel conversation state:', error);
  }

  const postRecoveryState = getState();
  if (typeof postRecoveryState.syncSessionMessagesInBackground === 'function') {
    await postRecoveryState.syncSessionMessagesInBackground(normalizedSessionId);
  }
};
