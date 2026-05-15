import type { Session } from '../../../../types';
import { normalizeSession } from '../../helpers/sessions';
import { readSessionAiSelectionFromMetadata } from '../../helpers/sessionAiSelection';
import type { ChatStoreDraft } from '../../types';
import {
  deleteSessionMessagesCacheEntry,
  resetCurrentSessionViewState,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';
import type { SessionActionDeps } from './types';
import {
  markSessionCachesStale,
  normalizeTrackedSessions,
  removeSessionCaches,
  upsertSessionCaches,
} from './cache';

export function createSessionMutationActions({
  set,
  client,
  getSessionParams,
}: SessionActionDeps) {
  const removeSessionStateLocally = (state: ChatStoreDraft, sessionId: string) => {
    state.sessions = (state.sessions || []).filter((session) => session.id !== sessionId);
    if (state.sessionStreamingMessageDrafts && sessionId in state.sessionStreamingMessageDrafts) {
      delete state.sessionStreamingMessageDrafts[sessionId];
    }
    if (state.sessionChatState && sessionId in state.sessionChatState) {
      delete state.sessionChatState[sessionId];
    }
    if (state.sessionTurnProcessState && sessionId in state.sessionTurnProcessState) {
      delete state.sessionTurnProcessState[sessionId];
    }
    if (state.sessionTurnProcessCache && sessionId in state.sessionTurnProcessCache) {
      delete state.sessionTurnProcessCache[sessionId];
    }
    if (state.sessionAiSelectionBySession && sessionId in state.sessionAiSelectionBySession) {
      delete state.sessionAiSelectionBySession[sessionId];
    }
    if (state.sessionMessagePaginationState && sessionId in state.sessionMessagePaginationState) {
      delete state.sessionMessagePaginationState[sessionId];
    }
    if (state.sessionRuntimeGuidanceState && sessionId in state.sessionRuntimeGuidanceState) {
      delete state.sessionRuntimeGuidanceState[sessionId];
    }
    if (state.currentSessionId === sessionId) {
      resetCurrentSessionViewState(state);
    }
    if (state.activePanel === 'chat' && state.currentSessionId === null) {
      state.activePanel = state.currentProjectId ? 'project' : 'chat';
    }
  };

  return {
    applyRealtimeSessionSnapshot: (sessionPayload: Session | unknown) => {
      const updatedSession = normalizeSession(sessionPayload);
      const normalizedSessionId = String(updatedSession?.id || '').trim();
      if (!normalizedSessionId) {
        return null;
      }
      upsertSessionCaches(client, updatedSession);
      const selectionFromMetadata = readSessionAiSelectionFromMetadata(updatedSession.metadata);
      set((state: ChatStoreDraft) => {
        state.sessions = normalizeTrackedSessions(
          [
            updatedSession,
            ...(state.sessions || []).filter((session: Session) => session.id !== normalizedSessionId),
          ],
          state.contacts || [],
        );
        if (state.currentSessionId === normalizedSessionId) {
          state.currentSession = updatedSession;
          syncCurrentProjectFromSession(state, updatedSession);
          if (selectionFromMetadata) {
            state.selectedModelId = selectionFromMetadata.selectedModelId ?? null;
            state.selectedAgentId = selectionFromMetadata.selectedAgentId ?? null;
          }
        }
        if (selectionFromMetadata) {
          if (!state.sessionAiSelectionBySession) {
            state.sessionAiSelectionBySession = {};
          }
          state.sessionAiSelectionBySession[normalizedSessionId] = {
            selectedModelId: selectionFromMetadata.selectedModelId ?? null,
            selectedAgentId: selectionFromMetadata.selectedAgentId ?? null,
          };
        }
      });
      return updatedSession;
    },

    updateSession: async (sessionId: string, updates: Partial<Session>) => {
      try {
        const updatesRecord = updates as Partial<Session> & { description?: string | null };
        const payload: { title?: string; description?: string; metadata?: Session['metadata'] } = {};
        if (typeof updates?.title === 'string') {
          payload.title = updates.title;
        }
        if (Object.prototype.hasOwnProperty.call(updatesRecord, 'metadata')) {
          payload.metadata = updatesRecord.metadata ?? null;
        }
        if (Object.prototype.hasOwnProperty.call(updatesRecord, 'description')) {
          payload.description = typeof updatesRecord.description === 'string'
            ? updatesRecord.description
            : undefined;
        }

        if (Object.keys(payload).length === 0) {
          return;
        }

        const response = await client.updateSession(sessionId, payload);
        const updatedSession = response ? normalizeSession(response) : null;
        const selectionFromMetadata = readSessionAiSelectionFromMetadata(
          updatedSession?.metadata ?? payload.metadata,
        );
        if (updatedSession) {
          upsertSessionCaches(client, updatedSession);
        } else {
          markSessionCachesStale(client, {
            sessionId,
            userId: getSessionParams().userId,
          });
        }

        set((state: ChatStoreDraft) => {
          if (updatedSession) {
            state.sessions = normalizeTrackedSessions(
              [updatedSession, ...(state.sessions || []).filter((session: Session) => session.id !== sessionId)],
              state.contacts || [],
            );
          }
          if (state.currentSessionId === sessionId) {
            state.currentSession = updatedSession;
            syncCurrentProjectFromSession(state, updatedSession);
            if (selectionFromMetadata) {
              state.selectedModelId = selectionFromMetadata.selectedModelId ?? null;
              state.selectedAgentId = selectionFromMetadata.selectedAgentId ?? null;
            }
          }
          if (selectionFromMetadata) {
            if (!state.sessionAiSelectionBySession) {
              state.sessionAiSelectionBySession = {};
            }
            state.sessionAiSelectionBySession[sessionId] = {
              selectedModelId: selectionFromMetadata.selectedModelId ?? null,
              selectedAgentId: selectionFromMetadata.selectedAgentId ?? null,
            };
          }
        });
      } catch (error) {
        console.error('Failed to update session:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update session';
        });
      }
    },

    deleteSession: async (sessionId: string) => {
      try {
        await client.deleteSession(sessionId);
        const now = new Date();
        markSessionCachesStale(client, {
          sessionId,
          userId: getSessionParams().userId,
        });

        set((state: ChatStoreDraft) => {
          const index = state.sessions.findIndex((session) => session.id === sessionId);
          if (index !== -1) {
            state.sessions[index] = {
              ...state.sessions[index],
              archived: true,
              status: 'archiving',
              updatedAt: now,
            };
          }
          if (state.sessionStreamingMessageDrafts && sessionId in state.sessionStreamingMessageDrafts) {
            delete state.sessionStreamingMessageDrafts[sessionId];
          }
          if (state.sessionChatState && sessionId in state.sessionChatState) {
            delete state.sessionChatState[sessionId];
          }
          if (state.sessionTurnProcessState && sessionId in state.sessionTurnProcessState) {
            delete state.sessionTurnProcessState[sessionId];
          }
          if (state.sessionTurnProcessCache && sessionId in state.sessionTurnProcessCache) {
            delete state.sessionTurnProcessCache[sessionId];
          }
          if (state.sessionAiSelectionBySession && sessionId in state.sessionAiSelectionBySession) {
            delete state.sessionAiSelectionBySession[sessionId];
          }
          if (state.currentSessionId === sessionId) {
            state.currentSessionId = null;
            state.currentSession = null;
            state.selectedModelId = null;
            state.selectedAgentId = null;
            state.messages = [];
          }
          if (state.activePanel === 'chat' && state.currentSessionId === null) {
            state.activePanel = state.currentProjectId ? 'project' : 'chat';
          }
          deleteSessionMessagesCacheEntry(state, sessionId);
        });
      } catch (error) {
        console.error('Failed to delete session:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete session';
        });
      }
    },

    removeSessionLocally: (sessionId: string) => {
      const trimmed = sessionId.trim();
      if (!trimmed) {
        return;
      }
      removeSessionCaches(client, trimmed);
      set((state: ChatStoreDraft) => {
        removeSessionStateLocally(state, trimmed);
        deleteSessionMessagesCacheEntry(state, trimmed);
      });
    },
  };
}
