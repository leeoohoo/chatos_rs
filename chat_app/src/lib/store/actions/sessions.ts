import type { Session } from '../../../types';
import type ApiClient from '../../api/client';
import { fetchSession } from '../helpers/sessions';
import { applyTurnProcessCache, fetchSessionMessages } from '../helpers/messages';
import { debugLog } from '@/lib/utils';

const cloneStreamingMessageDraft = <T,>(value: T): T => {
  try {
    if (typeof structuredClone === 'function') {
      return structuredClone(value);
    }
  } catch {
    // ignore and fallback to JSON clone
  }

  try {
    return JSON.parse(JSON.stringify(value));
  } catch {
    return value;
  }
};

const ensureSessionTurnMaps = (state: any, sessionId: string) => {
  if (!state.sessionTurnProcessState) {
    state.sessionTurnProcessState = {};
  }
  if (!state.sessionTurnProcessState[sessionId]) {
    state.sessionTurnProcessState[sessionId] = {};
  }

  if (!state.sessionTurnProcessCache) {
    state.sessionTurnProcessCache = {};
  }
  if (!state.sessionTurnProcessCache[sessionId]) {
    state.sessionTurnProcessCache[sessionId] = {};
  }
};

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getSessionParams: () => { userId: string; projectId: string };
  customUserId?: string;
  customProjectId?: string;
}

export function createSessionActions({
  set,
  get,
  client,
  getSessionParams,
  customUserId,
  customProjectId,
}: Deps) {
  return {
    loadSessions: async (options: any = {}) => {
      try {
        debugLog('🔍 loadSessions 被调用');
        if (!options.silent) {
          set((state: any) => {
            state.isLoading = true;
            state.error = null;
          });
          debugLog('🔍 loadSessions isLoading 设置为 true');
        }

        const { userId, projectId } = getSessionParams();

        debugLog('🔍 loadSessions 调用 client.getSessions', { userId, projectId, customUserId, customProjectId, options });
        const sessions = await client.getSessions(userId, projectId, { limit: options.limit, offset: options.offset });
        debugLog('🔍 loadSessions 返回结果:', sessions);

        const existing = options.append ? (get().sessions || []) : [];
        const merged = options.append ? [...existing, ...sessions] : sessions;
        const deduped: Session[] = [];
        const seen = new Set<string>();
        for (const s of merged) {
          if (s && !seen.has(s.id)) {
            seen.add(s.id);
            deduped.push(s);
          }
        }

        set((state: any) => {
          state.sessions = deduped;
          if (!options.silent) {
            state.isLoading = false;
          }
          if (state.currentSessionId) {
            const matched = deduped.find(s => s.id === state.currentSessionId);
            if (matched) {
              state.currentSession = matched;
            }
          }
        });

        const currentState = get();
        if (deduped.length > 0 && !currentState.currentSessionId) {
          const lastSessionId = localStorage.getItem(`lastSessionId_${userId}_${projectId}`);
          let sessionToSelect = null;

          if (lastSessionId) {
            sessionToSelect = deduped.find(s => s.id === lastSessionId);
          }

          if (!sessionToSelect) {
            sessionToSelect = [...deduped].sort((a, b) =>
              new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
            )[0];
          }

          if (sessionToSelect) {
            debugLog('🔍 自动选择会话:', sessionToSelect.id);
            setTimeout(() => {
              get().selectSession(sessionToSelect.id);
            }, 0);
          }
        }

        debugLog('🔍 loadSessions 完成');
        return sessions;
      } catch (error) {
        console.error('🔍 loadSessions 错误:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load sessions';
          if (!options.silent) {
            state.isLoading = false;
          }
        });
        return [];
      }
    },

    createSession: async (title = 'New Chat') => {
      try {
        const { userId, projectId } = getSessionParams();

        debugLog('🔍 createSession 使用参数:', { userId, projectId, title });
        debugLog('🔍 createSession 自定义参数:', { customUserId, customProjectId });
        debugLog('🔍 createSession 最终使用的参数:', {
          userId: userId,
          projectId: projectId,
          isCustomUserId: !!customUserId,
          isCustomProjectId: !!customProjectId,
        });

        const sessionData: { id: string; title: string; user_id: string; project_id?: string } = {
          id: crypto.randomUUID(),
          title,
          user_id: userId,
        };
        if (projectId) {
          sessionData.project_id = projectId;
        }

        const session = await client.createSession(sessionData);
        debugLog('✅ createSession API调用成功:', session);

        const formattedSession = {
          id: session.id,
          title: session.title,
          createdAt: new Date(session.created_at),
          updatedAt: new Date(session.updated_at),
          messageCount: 0,
          tokenUsage: 0,
          pinned: false,
          archived: false,
          tags: null,
          metadata: null,
        };

        set((state: any) => {
          state.sessions.unshift(formattedSession);
          state.currentSessionId = formattedSession.id;
          state.currentSession = formattedSession;
          state.messages = [];
          if (!state.sessionStreamingMessageDrafts) {
            state.sessionStreamingMessageDrafts = {};
          }
          state.sessionStreamingMessageDrafts[formattedSession.id] = null;
          state.activePanel = 'chat';
          state.error = null;
        });

        localStorage.setItem(`lastSessionId_${userId}_${projectId}`, formattedSession.id);
        debugLog('🔍 保存新创建的会话ID到 localStorage:', formattedSession.id);

        return formattedSession.id;
      } catch (error) {
        console.error('❌ createSession 失败:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to create session';
        });
        throw error;
      }
    },

    selectSession: async (sessionId: string) => {
      const beforeSelect = get();
      const sameSessionState = beforeSelect.sessionChatState?.[sessionId];
      if (beforeSelect.currentSessionId === sessionId && sameSessionState?.isStreaming) {
        debugLog('🔍 当前会话正在流式中，忽略重复切换请求:', sessionId);
        return;
      }

      try {
        set((state: any) => {
          state.isLoading = true;
          state.error = null;
        });

        const session = await fetchSession(client, sessionId);
        const messages = await fetchSessionMessages(client, sessionId, { limit: 50, offset: 0 });
        const stateSnapshot = get();
        const snapshotChatState = stateSnapshot.sessionChatState?.[sessionId];
        const localStreamingMessage = snapshotChatState?.streamingMessageId
          ? stateSnapshot.messages.find((m: any) => m.id === snapshotChatState.streamingMessageId && m.sessionId === sessionId)
          : null;

        set((state: any) => {
          const chatState = state.sessionChatState[sessionId];
          const draftMessage = state.sessionStreamingMessageDrafts?.[sessionId];
          let nextMessages = messages;

          if (chatState?.isStreaming && chatState.streamingMessageId) {
            const hasStreamingMessage = nextMessages.some((m: any) => m.id === chatState.streamingMessageId);
            if (!hasStreamingMessage) {
              let restoredStreamingMessage: any = null;
              if (draftMessage && typeof draftMessage === 'object') {
                restoredStreamingMessage = cloneStreamingMessageDraft(draftMessage);
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

          ensureSessionTurnMaps(state, sessionId);

          nextMessages = applyTurnProcessCache(
            nextMessages,
            state.sessionTurnProcessCache?.[sessionId],
            state.sessionTurnProcessState?.[sessionId],
          );

          state.currentSessionId = sessionId;
          (state as any).currentSession = session;
          state.messages = nextMessages;
          state.activePanel = 'chat';
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
        });

        if (session) {
          const { userId, projectId } = getSessionParams();
          localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
          debugLog('🔍 保存会话ID到 localStorage:', sessionId);
        }
      } catch (error) {
        console.error('Failed to select session:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to select session';
          state.isLoading = false;
        });
      }
    },

    updateSession: async (sessionId: string, _updates: Partial<Session>) => {
      try {
        console.warn('updateSession not implemented yet');
        const updatedSession = null;

        set((state: any) => {
          const index = state.sessions.findIndex((s: any) => s.id === sessionId);
          if (index !== -1 && updatedSession) {
            state.sessions[index] = updatedSession;
          }
          if (state.currentSessionId === sessionId) {
            state.currentSession = updatedSession;
          }
        });
      } catch (error) {
        console.error('Failed to update session:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update session';
        });
      }
    },

    deleteSession: async (sessionId: string) => {
      try {
        await client.deleteSession(sessionId);

        set((state: any) => {
          state.sessions = state.sessions.filter((s: any) => s.id !== sessionId);
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
          if (state.currentSessionId === sessionId) {
            state.currentSessionId = null;
            state.currentSession = null;
            state.messages = [];
          }
          if (state.activePanel === 'chat' && state.currentSessionId === null) {
            state.activePanel = state.currentProjectId ? 'project' : 'chat';
          }
        });
      } catch (error) {
        console.error('Failed to delete session:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete session';
        });
      }
    },
  };
}
