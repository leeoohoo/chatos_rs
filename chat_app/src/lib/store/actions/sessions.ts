import type { Session } from '../../../types';
import type ApiClient from '../../api/client';
import { fetchSession, normalizeSession } from '../helpers/sessions';
import { fetchSessionMessages } from '../helpers/messages';
import {
  mergeSessionAiSelectionIntoMetadata,
  readSessionAiSelectionFromMetadata,
} from '../helpers/sessionAiSelection';
import {
  mergeSessionRuntimeIntoMetadata,
} from '../helpers/sessionRuntime';
import type {
  SessionAiSelection,
  SessionCreateOptions,
  SessionCreatePayload,
  SessionSelectOptions,
} from '../types';
import { debugLog, generateId } from '@/lib/utils';
import {
  createPerfMeasureStopper,
  deleteSessionMessagesCacheEntry,
  isSessionActive,
  matchContactProjectScopeSessionRecord,
  normalizeContactProjectScopeSessions,
  normalizeProjectScopeId,
  resolveSessionTimestamp,
  writeSessionMessagesCache,
} from './sessionsUtils';
import { applySelectSessionState } from './sessionsSelectHelpers';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getSessionParams: () => { userId: string; projectId: string };
  customUserId?: string;
  customProjectId?: string;
  onSessionActivated?: (sessionId: string | null) => void;
}

export function createSessionActions({
  set,
  get,
  client,
  getSessionParams,
  customUserId,
  customProjectId,
  onSessionActivated,
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
        const rawSessions = await client.getSessions(
          userId,
          undefined,
          { limit: options.limit, offset: options.offset },
        );
        const sessions = Array.isArray(rawSessions)
          ? rawSessions.map(normalizeSession)
          : [];
        const mergedByContact = normalizeContactProjectScopeSessions(sessions);
        debugLog('🔍 loadSessions 返回结果:', mergedByContact);

        const existing = options.append ? (get().sessions || []) : [];
        const merged = options.append ? [...existing, ...mergedByContact] : mergedByContact;
        const dedupedById: Session[] = [];
        const seen = new Set<string>();
        for (const s of merged) {
          if (s && !seen.has(s.id)) {
            seen.add(s.id);
            dedupedById.push(s);
          }
        }
        const deduped = normalizeContactProjectScopeSessions(dedupedById);

        set((state: any) => {
          state.sessions = deduped;
          if (!state.sessionAiSelectionBySession) {
            state.sessionAiSelectionBySession = {};
          }
          for (const session of deduped) {
            const selection = readSessionAiSelectionFromMetadata(session?.metadata);
            if (selection) {
              state.sessionAiSelectionBySession[session.id] = selection;
            }
          }
          if (!options.silent) {
            state.isLoading = false;
          }
          if (state.currentSessionId) {
            const matched = deduped.find(s => s.id === state.currentSessionId);
            if (matched) {
              state.currentSession = matched;
            } else {
              state.currentSessionId = null;
              state.currentSession = null;
              state.messages = [];
            }
          }
        });

        const currentState = get();
        if (deduped.length > 0 && !currentState.currentSessionId) {
          const activeSessions = deduped.filter((session: Session) => isSessionActive(session));
          if (activeSessions.length > 0) {
            const lastSessionId = localStorage.getItem(`lastSessionId_${userId}_${projectId}`);
            let sessionToSelect: Session | undefined;

            if (lastSessionId) {
              sessionToSelect = activeSessions.find(s => s.id === lastSessionId);
            }

            if (!sessionToSelect) {
              sessionToSelect = [...activeSessions].sort((a, b) =>
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
        }

        debugLog('🔍 loadSessions 完成');
        return deduped;
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

    createSession: async (
      payload: string | SessionCreatePayload = 'New Chat',
      options: SessionCreateOptions = {},
    ) => {
      try {
        const payloadObject: SessionCreatePayload = typeof payload === 'string'
          ? { title: payload }
          : (payload || {});
        const title = (payloadObject.title || 'New Chat').trim() || 'New Chat';
        const { userId, projectId: fallbackProjectId } = getSessionParams();
        const requestedProjectId = typeof payloadObject.projectId === 'string'
          ? payloadObject.projectId.trim()
          : '';
        const fallbackScopedProjectId = typeof fallbackProjectId === 'string'
          ? fallbackProjectId.trim()
          : '';
        const effectiveProjectId = normalizeProjectScopeId(
          requestedProjectId || fallbackScopedProjectId || null,
        );
        const stateBeforeCreate = get();
        const selectedModelId = payloadObject.selectedModelId ?? stateBeforeCreate.selectedModelId ?? null;
        const contactAgentId = typeof payloadObject.contactAgentId === 'string'
          ? (payloadObject.contactAgentId.trim() || null)
          : null;
        const contactId = typeof payloadObject.contactId === 'string'
          ? (payloadObject.contactId.trim() || null)
          : null;
        const effectiveProjectRoot = effectiveProjectId === '0'
          ? null
          : (typeof payloadObject.projectRoot === 'string'
            ? (payloadObject.projectRoot.trim() || null)
            : null);

        if (contactId || contactAgentId) {
          const existingSession = (stateBeforeCreate.sessions || []).find((session: Session) => (
            matchContactProjectScopeSessionRecord(session, {
              contactId,
              contactAgentId,
              projectId: effectiveProjectId,
            })
          ));
          if (existingSession) {
            await get().selectSession(existingSession.id, {
              keepActivePanel: options.keepActivePanel,
            });
            return existingSession.id;
          }

          try {
            const remoteRows = await client.getSessions(
              userId,
              effectiveProjectId,
              { limit: 200, offset: 0 },
            );
            const remoteMatched = (Array.isArray(remoteRows) ? remoteRows : [])
              .map(normalizeSession)
              .filter((session: Session) => (
                matchContactProjectScopeSessionRecord(session, {
                  contactId,
                  contactAgentId,
                  projectId: effectiveProjectId,
                })
              ))
              .sort((left: Session, right: Session) =>
                resolveSessionTimestamp(right) - resolveSessionTimestamp(left),
              );

            const remoteExisting = remoteMatched[0];
            if (remoteExisting?.id) {
              await get().selectSession(remoteExisting.id, {
                keepActivePanel: options.keepActivePanel,
              });
              return remoteExisting.id;
            }
          } catch (error) {
            debugLog('🔍 createSession 远端查重失败，继续创建', {
              contactId,
              contactAgentId,
              projectId: effectiveProjectId,
              error: error instanceof Error ? error.message : String(error),
            });
          }
        }

        const inheritedAiSelection: SessionAiSelection = {
          selectedModelId,
          selectedAgentId: contactAgentId,
        };
        const selectionMetadata = mergeSessionAiSelectionIntoMetadata(
          null,
          inheritedAiSelection,
        );
        const initialMetadata = mergeSessionRuntimeIntoMetadata(selectionMetadata, {
          contactAgentId,
          contactId,
          selectedModelId,
          projectId: effectiveProjectId,
          projectRoot: effectiveProjectRoot,
          mcpEnabled: payloadObject.mcpEnabled ?? true,
          enabledMcpIds: payloadObject.enabledMcpIds ?? [],
        });

        debugLog('🔍 createSession 使用参数:', { userId, projectId: effectiveProjectId, title });
        debugLog('🔍 createSession 自定义参数:', { customUserId, customProjectId });
        debugLog('🔍 createSession 最终使用的参数:', {
          userId: userId,
          projectId: effectiveProjectId,
          isCustomUserId: !!customUserId,
          isCustomProjectId: !!customProjectId,
        });

        const sessionData: {
          id: string;
          title: string;
          user_id: string;
          project_id?: string;
          metadata?: Record<string, any>;
        } = {
          id: generateId(),
          title,
          user_id: userId,
          project_id: effectiveProjectId,
        };
        if (Object.keys(initialMetadata).length > 0) {
          sessionData.metadata = initialMetadata;
        }

        const session = await client.createSession(sessionData);
        debugLog('✅ createSession API调用成功:', session);

        const formattedSession = normalizeSession({
          ...session,
          metadata: session?.metadata ?? (Object.keys(initialMetadata).length > 0 ? initialMetadata : null),
        });
        const selectionFromMetadata = readSessionAiSelectionFromMetadata(formattedSession.metadata);
        const effectiveSelection = selectionFromMetadata || inheritedAiSelection;

        set((state: any) => {
          state.sessions.unshift(formattedSession);
          state.currentSessionId = formattedSession.id;
          state.currentSession = formattedSession;
          if (!state.sessionAiSelectionBySession) {
            state.sessionAiSelectionBySession = {};
          }
          state.sessionAiSelectionBySession[formattedSession.id] = {
            selectedModelId: effectiveSelection.selectedModelId,
            selectedAgentId: effectiveSelection.selectedAgentId,
          };
          state.selectedModelId = effectiveSelection.selectedModelId;
          state.selectedAgentId = effectiveSelection.selectedAgentId;
          state.messages = [];
          if (!state.sessionStreamingMessageDrafts) {
            state.sessionStreamingMessageDrafts = {};
          }
          state.sessionStreamingMessageDrafts[formattedSession.id] = null;
          if (!options.keepActivePanel) {
            state.activePanel = 'chat';
          }
          state.error = null;
        });

        deleteSessionMessagesCacheEntry(formattedSession.id);
        localStorage.setItem(`lastSessionId_${userId}_${effectiveProjectId}`, formattedSession.id);
        debugLog('🔍 保存新创建的会话ID到 localStorage:', formattedSession.id);
        onSessionActivated?.(formattedSession.id);

        return formattedSession.id;
      } catch (error) {
        console.error('❌ createSession 失败:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to create session';
        });
        throw error;
      }
    },

    selectSession: async (
      sessionId: string,
      options: SessionSelectOptions = {},
    ) => {
      const selectStartedAt = Date.now();
      const stopPerfMeasure = createPerfMeasureStopper(`store.selectSession.${sessionId}.${selectStartedAt}`);
      const beforeSelect = get();
      const previousSessionId = beforeSelect.currentSessionId;
      const sameSessionState = beforeSelect.sessionChatState?.[sessionId];
      if (beforeSelect.currentSessionId === sessionId && sameSessionState?.isStreaming) {
        // 同一会话运行过程中仍允许切回聊天面板，避免在项目/终端面板点击会话无响应
        if (!options.keepActivePanel && beforeSelect.activePanel !== 'chat') {
          set((state: any) => {
            state.activePanel = 'chat';
          });
        }
        debugLog('🔍 当前会话仍在运行中，忽略重复切换请求:', sessionId);
        return;
      }

      try {
        set((state: any) => {
          state.isLoading = true;
          state.error = null;
        });

        const existingSession = (beforeSelect.sessions || []).find((item: Session) => item.id === sessionId) || null;
        const session = existingSession ? existingSession : await fetchSession(client, sessionId);
        const messages = await fetchSessionMessages(client, sessionId, {
          limit: 50,
          offset: 0,
          session,
        });
        writeSessionMessagesCache(sessionId, messages);
        const sessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(session?.metadata);

        set((state: any) => {
          applySelectSessionState({
            state,
            sessionId,
            session,
            messages,
            previousSessionId,
            sessionAiSelectionFromMetadata,
            keepActivePanel: options.keepActivePanel,
          });
        });

        if (session) {
          const { userId, projectId } = getSessionParams();
          localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
          debugLog('🔍 保存会话ID到 localStorage:', sessionId);
        }
        const latestMessagesForSession = (get().messages || []).filter((message: any) => message?.sessionId === sessionId);
        if (latestMessagesForSession.length > 0) {
          writeSessionMessagesCache(sessionId, latestMessagesForSession);
        } else {
          writeSessionMessagesCache(sessionId, messages);
        }
        debugLog('[Store] selectSession completed', {
          sessionId,
          previousSessionId,
          messageCount: messages.length,
          cacheHit: false,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
        });
        onSessionActivated?.(sessionId);
      } catch (error) {
        console.error('Failed to select session:', error);
        debugLog('[Store] selectSession failed', {
          sessionId,
          previousSessionId,
          perfMs: stopPerfMeasure() ?? null,
          elapsedMs: Date.now() - selectStartedAt,
          error: error instanceof Error ? error.message : String(error),
        });
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to select session';
          state.isLoading = false;
        });
      }
    },

    updateSession: async (sessionId: string, updates: Partial<Session>) => {
      try {
        const payload: { title?: string; description?: string; metadata?: any } = {};
        if (typeof updates?.title === 'string') {
          payload.title = updates.title;
        }
        if (Object.prototype.hasOwnProperty.call(updates || {}, 'metadata')) {
          payload.metadata = (updates as any).metadata ?? null;
        }
        if (Object.prototype.hasOwnProperty.call(updates || {}, 'description')) {
          payload.description = (updates as any).description ?? null;
        }

        if (Object.keys(payload).length === 0) {
          return;
        }

        const response = await client.updateSession(sessionId, payload);
        const updatedSession = response ? normalizeSession(response) : null;
        const selectionFromMetadata = readSessionAiSelectionFromMetadata(
          updatedSession?.metadata ?? payload.metadata,
        );

        set((state: any) => {
          const index = state.sessions.findIndex((s: any) => s.id === sessionId);
          if (index !== -1 && updatedSession) {
            state.sessions[index] = updatedSession;
          }
          if (state.currentSessionId === sessionId) {
            state.currentSession = updatedSession;
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
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update session';
        });
      }
    },

    deleteSession: async (sessionId: string) => {
      try {
        await client.deleteSession(sessionId);
        const now = new Date();

        set((state: any) => {
          const index = state.sessions.findIndex((s: any) => s.id === sessionId);
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
        });
        deleteSessionMessagesCacheEntry(sessionId);
        if (get().currentSessionId === null) {
          onSessionActivated?.(null);
        }
      } catch (error) {
        console.error('Failed to delete session:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete session';
        });
      }
    },
  };
}
