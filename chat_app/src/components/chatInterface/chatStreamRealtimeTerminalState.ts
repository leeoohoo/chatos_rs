import {
  reconcilePersistedTurnMessages,
  shouldReloadMessagesAfterTerminalState,
} from '../../lib/store/actions/sendMessage/persistedTurnMessages';
import {
  failSendMessageState,
  finalizeStreamingSessionState,
} from '../../lib/store/actions/sendMessage/sessionState';
import { recoverStreamingTurnBySnapshot } from '../../lib/store/actions/sendMessage/turnRecovery';
import type { Message } from '../../types';
import type { ChatStoreDraft, ChatStoreSet } from '../../lib/store/types';
import { shouldRecoverMessagesForActiveSession } from './chatStreamRealtimeBridgeState';

type RealtimeTerminalRecoveryApiClient =
  Parameters<typeof recoverStreamingTurnBySnapshot>[0]['apiClient'];

export interface TerminalEventContext {
  sessionId: string;
  turnId: string;
  tempAssistantMessageId: string;
  tempUserId: string | null;
}

export interface TerminalEventPersistedMessages {
  persistedUserMessage: Message | null;
  persistedAssistantMessage: Message | null;
}

interface RealtimeTerminalSuccessOutcome {
  kind: 'success';
}

interface RealtimeTerminalFailureOutcome {
  kind: 'failure';
  tempAssistantMessage: Message;
  failureContent: string;
  readableError: string;
}

type RealtimeTerminalOutcome =
  | RealtimeTerminalSuccessOutcome
  | RealtimeTerminalFailureOutcome;

export const applyRealtimeTerminalMessages = (
  set: ChatStoreSet,
  context: TerminalEventContext,
  persisted: TerminalEventPersistedMessages,
) => {
  set((state) => {
    reconcilePersistedTurnMessages(
      state,
      context.tempAssistantMessageId,
      context.tempUserId,
      persisted.persistedUserMessage,
      persisted.persistedAssistantMessage,
    );
    finalizeStreamingSessionState(state, {
      sessionId: context.sessionId,
      assistantMessageId: context.tempAssistantMessageId,
      sawDone: true,
    });
  });
};

export const applyRealtimeTerminalFailure = (
  set: ChatStoreSet,
  context: TerminalEventContext,
  persisted: TerminalEventPersistedMessages,
  tempAssistantMessage: Message,
  failureContent: string,
  readableError: string,
) => {
  set((state) => {
    reconcilePersistedTurnMessages(
      state,
      context.tempAssistantMessageId,
      context.tempUserId,
      persisted.persistedUserMessage,
      persisted.persistedAssistantMessage,
    );
    failSendMessageState(state, {
      sessionId: context.sessionId,
      tempAssistantId: context.tempAssistantMessageId,
      tempAssistantMessage,
      failureContent,
      readableError,
    });
  });
};

export const shouldReloadAfterRealtimeTerminalEvent = (
  state: ChatStoreDraft,
  context: Pick<TerminalEventContext, 'sessionId' | 'tempAssistantMessageId' | 'tempUserId'>,
): boolean => (
  shouldRecoverMessagesForActiveSession(state, context.sessionId)
  && shouldReloadMessagesAfterTerminalState(
    state,
    context.tempAssistantMessageId,
    context.tempUserId,
    {
      allowLocalTerminalAssistant: true,
    },
  )
);

export const recoverMessagesAfterRealtimeTerminalEvent = async (
  apiClient: RealtimeTerminalRecoveryApiClient,
  set: ChatStoreSet,
  state: ChatStoreDraft,
  context: TerminalEventContext,
): Promise<boolean> => {
  const result = await recoverStreamingTurnBySnapshot({
    apiClient,
    set,
    sessionId: context.sessionId,
    turnId: context.turnId,
    tempAssistantMessageId: context.tempAssistantMessageId,
    tempUserId: context.tempUserId,
    preferredUserMessageId: context.tempUserId,
  });
  if (result.recovered) {
    return true;
  }
  if (typeof state.loadMessages === 'function') {
    await state.loadMessages(context.sessionId);
  }
  return false;
};

export const settleRealtimeTerminalEvent = async (
  apiClient: RealtimeTerminalRecoveryApiClient,
  set: ChatStoreSet,
  getState: () => ChatStoreDraft,
  context: TerminalEventContext,
  persisted: TerminalEventPersistedMessages,
  outcome: RealtimeTerminalOutcome,
): Promise<void> => {
  if (outcome.kind === 'success') {
    applyRealtimeTerminalMessages(set, context, persisted);
  } else {
    applyRealtimeTerminalFailure(
      set,
      context,
      persisted,
      outcome.tempAssistantMessage,
      outcome.failureContent,
      outcome.readableError,
    );
  }

  const latestState = getState();
  if (!shouldReloadAfterRealtimeTerminalEvent(latestState, context)) {
    return;
  }

  await recoverMessagesAfterRealtimeTerminalEvent(
    apiClient,
    set,
    latestState,
    context,
  );
};
