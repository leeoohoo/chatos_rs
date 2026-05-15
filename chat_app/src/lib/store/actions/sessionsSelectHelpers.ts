import type { Message, Session } from '../../../types';
import { applyTurnProcessCache } from '../helpers/messages';
import {
  asRecord,
  normalizeDate,
  readValue,
} from '../helpers/normalizerUtils';
import { normalizeContactSessions } from './sessionsUtils';
import {
  buildDraftUserMessageForStreaming,
  cloneStreamingMessageDraft,
  ensureSessionTurnMaps,
  isSessionActive,
  normalizeTurnId,
  resolveUserByTurnId,
} from './sessionsUtils';
import type { ChatState, SessionAiSelection } from '../types';

interface ApplySelectSessionStateArgs {
  state: ChatState;
  sessionId: string;
  session: Session | null;
  messages: Message[];
  previousSessionId: string | null;
  localStreamingMessage: Message | null;
  sessionAiSelectionFromMetadata: Partial<SessionAiSelection> | null;
  keepActivePanel?: boolean;
}

const readMessageMetadataString = (message: Message | null | undefined, key: string): string => {
  const metadata = asRecord(message?.metadata);
  const value = readValue(metadata, key);
  return typeof value === 'string' ? value : '';
};

const readHistoryDraftUserId = (message: Message | null | undefined): string => {
  const metadata = asRecord(message?.metadata);
  const draftUser = asRecord(readValue(metadata, 'historyDraftUserMessage'));
  const value = readValue(draftUser, 'id');
  return typeof value === 'string' ? value : '';
};

const readHistoryDraftUserRecord = (message: Message | null | undefined): Record<string, unknown> | null => {
  const metadata = asRecord(message?.metadata);
  return asRecord(readValue(metadata, 'historyDraftUserMessage'));
};

const isDraftUserMessageSnapshot = (
  value: unknown,
): value is NonNullable<NonNullable<Message['metadata']>['historyDraftUserMessage']> => (
  (() => {
    const record = asRecord(value);
    return Boolean(
      record
      && typeof record.id === 'string'
      && typeof record.content === 'string'
      && typeof record.createdAt === 'string'
    );
  })()
);

export const applySelectSessionState = ({
  state,
  sessionId,
  session,
  messages,
  previousSessionId,
  localStreamingMessage,
  sessionAiSelectionFromMetadata,
  keepActivePanel,
}: ApplySelectSessionStateArgs) => {
  const chatState = state.sessionChatState[sessionId];
  const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
  let nextMessages = messages;

  if (chatState?.isStreaming && chatState.streamingMessageId) {
    const hasStreamingMessage = nextMessages.some((message) => message.id === chatState.streamingMessageId);
    if (!hasStreamingMessage) {
      let restoredStreamingMessage: Message | null = null;
      if (draftMessage && typeof draftMessage === 'object') {
        restoredStreamingMessage = cloneStreamingMessageDraft(draftMessage);
      } else if (localStreamingMessage && typeof localStreamingMessage === 'object') {
        restoredStreamingMessage = cloneStreamingMessageDraft(localStreamingMessage);
      }

      const streamingDraftSource = restoredStreamingMessage || localStreamingMessage;
      if (streamingDraftSource) {
        const linkedUserMessageId = normalizeTurnId(
          readMessageMetadataString(streamingDraftSource, 'historyFinalForUserMessageId')
            || readHistoryDraftUserId(streamingDraftSource),
        );
        const linkedTurnId = normalizeTurnId(
          readMessageMetadataString(streamingDraftSource, 'historyFinalForTurnId')
          || readMessageMetadataString(streamingDraftSource, 'conversation_turn_id'),
        );
        const linkedUserById = linkedUserMessageId
          ? nextMessages.find((message) => message?.role === 'user' && message?.id === linkedUserMessageId)
          : null;
        const linkedUserByTurn = linkedUserById || !linkedTurnId
          ? null
          : resolveUserByTurnId(nextMessages, linkedTurnId);
        const linkedUserMessage = linkedUserById || linkedUserByTurn;

        if (linkedUserMessage && restoredStreamingMessage?.metadata) {
          restoredStreamingMessage.metadata.historyFinalForUserMessageId = linkedUserMessage.id;
          const resolvedTurnId = linkedTurnId || normalizeTurnId(
            linkedUserMessage?.metadata?.conversation_turn_id || linkedUserMessage?.metadata?.historyProcess?.turnId,
          );
          if (resolvedTurnId) {
            restoredStreamingMessage.metadata.historyFinalForTurnId = resolvedTurnId;
          }
          const historyDraftUser = readHistoryDraftUserRecord(restoredStreamingMessage);
          if (isDraftUserMessageSnapshot(historyDraftUser)) {
            historyDraftUser.id = linkedUserMessage.id;
            restoredStreamingMessage.metadata.historyDraftUserMessage = historyDraftUser;
          }
        }

        if ((linkedUserMessageId || linkedTurnId) && !linkedUserMessage) {
          const draftUserMessage = buildDraftUserMessageForStreaming(
            sessionId,
            streamingDraftSource,
            chatState.streamingMessageId,
          );
          if (draftUserMessage) {
            nextMessages = [...nextMessages, draftUserMessage];
          }
        }
      }

      nextMessages = [
        ...nextMessages,
        restoredStreamingMessage || localStreamingMessage || {
          id: chatState.streamingMessageId,
          sessionId,
          role: 'assistant',
          content: '',
          status: 'streaming',
          createdAt: new Date(),
          metadata: {
            toolCalls: [],
            contentSegments: [{ content: '', type: 'text' }],
            currentSegmentIndex: 0,
          },
        },
      ];
    }
  }

  if (chatState?.isStreaming && draftMessage && typeof draftMessage === 'object') {
    const draftClone = cloneStreamingMessageDraft(draftMessage);
    const draftId = typeof draftClone.id === 'string' ? draftClone.id : '';
    const draftIndex = draftId
      ? nextMessages.findIndex((message) => message?.id === draftId)
      : -1;

    if (draftIndex === -1) {
      nextMessages = [...nextMessages, draftClone];
    } else {
      const existing = nextMessages[draftIndex];
      const existingTime = normalizeDate(existing?.updatedAt || existing?.createdAt || 0).getTime();
      const draftTime = normalizeDate(draftClone?.updatedAt || draftClone?.createdAt || 0).getTime();
      const existingContentLength = typeof existing?.content === 'string'
        ? existing.content.length
        : 0;
      const draftContentLength = typeof draftClone?.content === 'string'
        ? draftClone.content.length
        : 0;
      const shouldReplaceWithDraft = Boolean(
        chatState?.isStreaming
        || draftTime > existingTime
        || draftContentLength > existingContentLength
        || existing?.status !== draftClone?.status
      );
      if (shouldReplaceWithDraft) {
        nextMessages[draftIndex] = {
          ...existing,
          ...draftClone,
        };
      }
    }
  } else if (state.sessionStreamingMessageDrafts) {
    state.sessionStreamingMessageDrafts[sessionId] = null;
  }

  ensureSessionTurnMaps(state, sessionId);

  nextMessages = applyTurnProcessCache(
    nextMessages,
    state.sessionTurnProcessCache?.[sessionId],
    state.sessionTurnProcessState?.[sessionId],
  );

  state.currentSessionId = sessionId;
  state.currentSession = session;
  const index = state.sessions.findIndex((item) => item.id === sessionId);
  if (index !== -1 && session) {
    state.sessions[index] = session;
  } else if (session) {
    const isActive = isSessionActive(session);
    if (isActive) {
      const merged = [session, ...(state.sessions || []).filter((item) => item?.id !== session.id)];
      state.sessions = normalizeContactSessions(merged);
    }
  }

  const savedAiSelection = state.sessionAiSelectionBySession?.[sessionId];
  if (savedAiSelection) {
    state.selectedModelId = savedAiSelection.selectedModelId ?? null;
    state.selectedAgentId = savedAiSelection.selectedAgentId ?? null;
  } else if (sessionAiSelectionFromMetadata) {
    if (!state.sessionAiSelectionBySession) {
      state.sessionAiSelectionBySession = {};
    }
    state.sessionAiSelectionBySession[sessionId] = {
      selectedModelId: sessionAiSelectionFromMetadata.selectedModelId ?? null,
      selectedAgentId: sessionAiSelectionFromMetadata.selectedAgentId ?? null,
    };
    state.selectedModelId = sessionAiSelectionFromMetadata.selectedModelId ?? null;
    state.selectedAgentId = sessionAiSelectionFromMetadata.selectedAgentId ?? null;
  } else if (
    (previousSessionId === null || previousSessionId === sessionId)
    && (state.selectedModelId || state.selectedAgentId)
  ) {
    if (!state.sessionAiSelectionBySession) {
      state.sessionAiSelectionBySession = {};
    }
    state.sessionAiSelectionBySession[sessionId] = {
      selectedModelId: state.selectedModelId ?? null,
      selectedAgentId: state.selectedAgentId ?? null,
    };
  } else {
    state.selectedModelId = null;
    state.selectedAgentId = null;
  }

  state.messages = nextMessages;
  if (!keepActivePanel) {
    state.activePanel = 'chat';
  }
  state.isLoading = false;
  state.isStreaming = chatState?.isStreaming ?? false;
  state.streamingMessageId = chatState?.streamingMessageId ?? null;
  if (chatState) {
    state.isLoading = chatState.isLoading;
  }
  if (!session) {
    state.error = 'Session not found';
  }
};
