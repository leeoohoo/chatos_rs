import { applyTurnProcessCache } from '../helpers/messages';
import { normalizeContactProjectScopeSessions } from './sessionsUtils';
import {
  cloneStreamingMessageDraft,
  ensureSessionTurnMaps,
  isSessionActive,
} from './sessionsUtils';

interface ApplySelectSessionStateArgs {
  state: any;
  sessionId: string;
  session: any;
  messages: any[];
  previousSessionId: string | null;
  sessionAiSelectionFromMetadata: {
    selectedModelId?: string | null;
    selectedAgentId?: string | null;
  } | null;
  keepActivePanel?: boolean;
}

export const applySelectSessionState = ({
  state,
  sessionId,
  session,
  messages,
  previousSessionId,
  sessionAiSelectionFromMetadata,
  keepActivePanel,
}: ApplySelectSessionStateArgs) => {
  const chatState = state.sessionChatState[sessionId];
  const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
  let nextMessages = messages;

  if (chatState?.isStreaming && chatState.streamingMessageId) {
    const hasStreamingMessage = nextMessages.some((m: any) => m.id === chatState.streamingMessageId);
    if (!hasStreamingMessage && draftMessage && typeof draftMessage === 'object') {
      nextMessages = [...nextMessages, cloneStreamingMessageDraft(draftMessage)];
    }
  }

  if (draftMessage && typeof draftMessage === 'object') {
    const draftClone = cloneStreamingMessageDraft(draftMessage);
    const draftId = typeof (draftClone as any)?.id === 'string' ? (draftClone as any).id : '';
    const draftIndex = draftId
      ? nextMessages.findIndex((m: any) => m?.id === draftId)
      : -1;

    if (draftIndex === -1) {
      nextMessages = [...nextMessages, draftClone];
    } else {
      const existing = nextMessages[draftIndex] || {};
      const existingTime = new Date((existing as any)?.updatedAt || (existing as any)?.createdAt || 0).getTime();
      const draftTime = new Date((draftClone as any)?.updatedAt || (draftClone as any)?.createdAt || 0).getTime();
      const existingContentLength = typeof (existing as any)?.content === 'string'
        ? (existing as any).content.length
        : 0;
      const draftContentLength = typeof (draftClone as any)?.content === 'string'
        ? (draftClone as any).content.length
        : 0;
      const shouldReplaceWithDraft = Boolean(
        chatState?.isStreaming
        || draftTime > existingTime
        || draftContentLength > existingContentLength
        || (existing as any)?.status !== (draftClone as any)?.status
      );
      if (shouldReplaceWithDraft) {
        nextMessages[draftIndex] = {
          ...existing,
          ...draftClone,
        };
      }
    }

    if (!chatState?.isStreaming && state.sessionStreamingMessageDrafts) {
      state.sessionStreamingMessageDrafts[sessionId] = null;
    }
  }

  ensureSessionTurnMaps(state, sessionId);

  nextMessages = applyTurnProcessCache(
    nextMessages,
    state.sessionTurnProcessCache?.[sessionId],
    state.sessionTurnProcessState?.[sessionId],
  );

  state.currentSessionId = sessionId;
  state.currentSession = session;
  const index = state.sessions.findIndex((s: any) => s.id === sessionId);
  if (index !== -1 && session) {
    state.sessions[index] = session;
  } else if (session) {
    const isActive = isSessionActive(session as any);
    if (isActive) {
      const merged = [session, ...(state.sessions || []).filter((s: any) => s?.id !== session.id)];
      state.sessions = normalizeContactProjectScopeSessions(merged);
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
  state.hasMoreMessages = messages.length >= 50;
  state.isStreaming = chatState?.isStreaming ?? false;
  state.streamingMessageId = chatState?.streamingMessageId ?? null;
  if (chatState) {
    state.isLoading = chatState.isLoading;
  }
  if (!session) {
    state.error = 'Session not found';
  }
};
