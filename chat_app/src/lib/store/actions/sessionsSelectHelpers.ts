// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message, Session } from '../../../types';
import { normalizeContactSessions } from './sessionsUtils';
import {
  isSessionActive,
} from './sessionsUtils';
import type { ChatState, SessionAiSelection } from '../types';

interface ApplySelectSessionStateArgs {
  state: ChatState;
  sessionId: string;
  session: Session | null;
  messages: Message[];
  previousSessionId: string | null;
  sessionAiSelectionFromMetadata: Partial<SessionAiSelection> | null;
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

  state.messages = messages;
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
