import type { Session } from '../../../types';
import type ApiClient from '../../api/client';
import { fetchSession, normalizeSession } from '../helpers/sessions';
import { applyTurnProcessCache, fetchSessionMessages } from '../helpers/messages';
import {
  mergeSessionAiSelectionIntoMetadata,
  readSessionAiSelectionFromMetadata,
} from '../helpers/sessionAiSelection';
import {
  mergeSessionRuntimeIntoMetadata,
  readSessionRuntimeFromMetadata,
} from '../helpers/sessionRuntime';
import type {
  SessionAiSelection,
  SessionCreateOptions,
  SessionCreatePayload,
  SessionSelectOptions,
} from '../types';
import { debugLog, generateId } from '@/lib/utils';

const SESSION_MESSAGES_CACHE_MAX_ENTRIES = 16;
type SessionMessagesCacheEntry = {
  fetchedAt: number;
  messages: any[];
};
const sessionMessagesPageCache = new Map<string, SessionMessagesCacheEntry>();

const createPerfMeasureStopper = (measureName: string): (() => number | null) => {
  if (typeof performance === 'undefined' || typeof performance.mark !== 'function' || typeof performance.measure !== 'function') {
    return () => null;
  }

  const startMark = `${measureName}:start`;
  const endMark = `${measureName}:end`;
  performance.mark(startMark);

  return () => {
    performance.mark(endMark);
    performance.measure(measureName, startMark, endMark);
    const entries = performance.getEntriesByName(measureName);
    const duration = entries.length > 0 ? entries[entries.length - 1].duration : null;
    performance.clearMarks(startMark);
    performance.clearMarks(endMark);
    performance.clearMeasures(measureName);
    return duration;
  };
};

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

const writeSessionMessagesCache = (sessionId: string, messages: any[]) => {
  sessionMessagesPageCache.set(sessionId, {
    fetchedAt: Date.now(),
    messages: cloneStreamingMessageDraft(messages),
  });

  while (sessionMessagesPageCache.size > SESSION_MESSAGES_CACHE_MAX_ENTRIES) {
    const oldestKey = sessionMessagesPageCache.keys().next().value;
    if (!oldestKey) {
      break;
    }
    sessionMessagesPageCache.delete(oldestKey);
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

const normalizeDate = (value: any): Date => {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
};

const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const resolveSessionTimestamp = (session: Session): number => {
  const updated = new Date((session as any).updatedAt ?? (session as any).createdAt ?? Date.now());
  const ts = updated.getTime();
  return Number.isFinite(ts) ? ts : 0;
};

const normalizeProjectScopeId = (projectId: string | null | undefined): string => {
  const trimmed = typeof projectId === 'string' ? projectId.trim() : '';
  return trimmed.length > 0 ? trimmed : '0';
};

const resolveSessionProjectScopeId = (session: Session | null | undefined): string => {
  if (!session) {
    return '0';
  }
  const rawProjectId = typeof (session as any).projectId === 'string'
    ? (session as any).projectId.trim()
    : (typeof (session as any).project_id === 'string'
      ? (session as any).project_id.trim()
      : '');
  if (rawProjectId.length > 0) {
    return normalizeProjectScopeId(rawProjectId);
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const runtimeProjectId = typeof runtime?.projectId === 'string'
    ? runtime.projectId.trim()
    : '';
  if (runtimeProjectId.length > 0) {
    return runtimeProjectId;
  }
  return '0';
};

const normalizeContactSessions = (sessions: Session[]): Session[] => {
  const byContactProject = new Map<string, Session>();
  for (const session of sessions) {
    const identity = resolveSessionContactIdentity(session);
    const contactKey = identity.contactId || identity.contactAgentId;
    if (!contactKey) {
      continue;
    }
    const key = `${contactKey}::${resolveSessionProjectScopeId(session)}`;
    const existing = byContactProject.get(key);
    if (!existing || resolveSessionTimestamp(session) >= resolveSessionTimestamp(existing)) {
      byContactProject.set(key, session);
    }
  }
  return Array.from(byContactProject.values()).sort(
    (a, b) => resolveSessionTimestamp(b) - resolveSessionTimestamp(a),
  );
};

const resolveUserByTurnId = (messages: any[], turnId: string) => {
  if (!turnId) {
    return null;
  }

  return messages.find((message: any) => {
    if (message?.role !== 'user') {
      return false;
    }
    const messageTurnId = normalizeTurnId(
      message?.metadata?.conversation_turn_id || message?.metadata?.historyProcess?.turnId,
    );
    return messageTurnId === turnId;
  }) || null;
};

const buildDraftUserMessageForStreaming = (
  sessionId: string,
  draftMessage: any,
  finalAssistantMessageId: string,
) => {
  const linkedUserMessageId = normalizeTurnId(
    typeof draftMessage?.metadata?.historyFinalForUserMessageId === 'string'
      ? draftMessage.metadata.historyFinalForUserMessageId
      : (
        typeof draftMessage?.metadata?.historyDraftUserMessage?.id === 'string'
          ? draftMessage.metadata.historyDraftUserMessage.id
          : ''
      )
  );
  const turnId = typeof draftMessage?.metadata?.conversation_turn_id === 'string'
    ? draftMessage.metadata.conversation_turn_id
    : '';
  const effectiveUserMessageId = linkedUserMessageId || (turnId ? `temp_user_turn_${turnId}` : '');
  if (!effectiveUserMessageId) {
    return null;
  }

  const draftUser = draftMessage?.metadata?.historyDraftUserMessage || {};

  return {
    id: effectiveUserMessageId,
    sessionId,
    role: 'user' as const,
    content: typeof draftUser.content === 'string' ? draftUser.content : '',
    status: 'completed' as const,
    createdAt: normalizeDate(draftUser.createdAt || draftMessage?.createdAt || Date.now()),
    metadata: {
      ...(turnId ? { conversation_turn_id: turnId } : {}),
      historyProcess: {
        hasProcess: false,
        toolCallCount: 0,
        thinkingCount: 0,
        processMessageCount: 0,
        userMessageId: effectiveUserMessageId,
        ...(turnId ? { turnId } : {}),
        finalAssistantMessageId: finalAssistantMessageId || null,
        expanded: false,
        loaded: false,
        loading: false,
      },
    },
  };
};

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getSessionParams: () => { userId: string; projectId: string };
  customUserId?: string;
  customProjectId?: string;
}

type MemoryContact = {
  id: string;
  user_id: string;
  agent_id: string;
  agent_name_snapshot?: string | null;
  status?: string | null;
  created_at?: string;
  updated_at?: string;
};

const normalizeContact = (value: any): MemoryContact | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const id = typeof value.id === 'string' ? value.id.trim() : '';
  const agentId = typeof value.agent_id === 'string' ? value.agent_id.trim() : '';
  const userId = typeof value.user_id === 'string' ? value.user_id.trim() : '';
  if (!id || !agentId || !userId) {
    return null;
  }
  return {
    id,
    user_id: userId,
    agent_id: agentId,
    agent_name_snapshot: typeof value.agent_name_snapshot === 'string'
      ? value.agent_name_snapshot.trim()
      : null,
    status: typeof value.status === 'string' ? value.status.trim() : null,
    created_at: typeof value.created_at === 'string' ? value.created_at : undefined,
    updated_at: typeof value.updated_at === 'string' ? value.updated_at : undefined,
  };
};

const resolveSessionContactIdentity = (session: Session | null | undefined): {
  contactId: string | null;
  contactAgentId: string | null;
} => {
  if (!session) {
    return { contactId: null, contactAgentId: null };
  }
  const runtime = readSessionRuntimeFromMetadata((session as any).metadata);
  const contactId = typeof runtime?.contactId === 'string' ? runtime.contactId.trim() : '';
  const contactAgentId = typeof runtime?.contactAgentId === 'string' ? runtime.contactAgentId.trim() : '';
  return {
    contactId: contactId.length > 0 ? contactId : null,
    contactAgentId: contactAgentId.length > 0 ? contactAgentId : null,
  };
};

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
        const [rawContacts, rawSessions] = await Promise.all([
          client.getContacts(userId, { limit: 2000, offset: 0 }).catch(() => []),
          client.getSessions(
            userId,
            undefined,
            { limit: options.limit, offset: options.offset },
          ),
        ]);
        const contacts = (Array.isArray(rawContacts) ? rawContacts : [])
          .map(normalizeContact)
          .filter((item): item is MemoryContact => !!item)
          .filter((item) => {
            const status = typeof item.status === 'string' ? item.status.toLowerCase() : '';
            return status === '' || status === 'active';
          });
        const contactsById = new Map(contacts.map((item) => [item.id, item]));
        const contactsByAgentId = new Map(contacts.map((item) => [item.agent_id, item]));

        const sessions = Array.isArray(rawSessions)
          ? rawSessions.map(normalizeSession)
          : [];

        const mappedContactIds = new Set<string>();
        const mappedContactAgentIds = new Set<string>();
        const filteredByContacts = sessions.filter((session) => {
          const status = typeof session.status === 'string'
            ? session.status.toLowerCase()
            : '';
          if (session.archived || status === 'archived' || status === 'archiving') {
            return false;
          }
          const identity = resolveSessionContactIdentity(session);
          if (identity.contactId && contactsById.has(identity.contactId)) {
            mappedContactIds.add(identity.contactId);
            const mappedContact = contactsById.get(identity.contactId);
            if (mappedContact) {
              mappedContactAgentIds.add(mappedContact.agent_id);
            }
            return true;
          }
          if (identity.contactAgentId && contactsByAgentId.has(identity.contactAgentId)) {
            mappedContactAgentIds.add(identity.contactAgentId);
            const mappedContact = contactsByAgentId.get(identity.contactAgentId);
            if (mappedContact) {
              mappedContactIds.add(mappedContact.id);
            }
            return true;
          }
          return false;
        });

        const missingContacts = contacts.filter((contact) => {
          if (mappedContactIds.has(contact.id)) {
            return false;
          }
          return !mappedContactAgentIds.has(contact.agent_id);
        });

        const backfilledSessions: Session[] = [];
        for (const contact of missingContacts) {
          const metadata = mergeSessionRuntimeIntoMetadata(null, {
            contactAgentId: contact.agent_id,
            contactId: contact.id,
            selectedModelId: null,
            projectId: '0',
            projectRoot: null,
            mcpEnabled: true,
            enabledMcpIds: [],
          });
          try {
            const created = await client.createSession({
              id: generateId(),
              title: contact.agent_name_snapshot || '联系人',
              user_id: userId,
              project_id: '0',
              metadata,
            });
            backfilledSessions.push(normalizeSession(created));
          } catch (error) {
            debugLog('🔍 联系人补建会话失败，忽略', {
              contactId: contact.id,
              agentId: contact.agent_id,
              error: error instanceof Error ? error.message : String(error),
            });
          }
        }

        const mergedByContact = [
          ...filteredByContacts,
          ...backfilledSessions,
        ];
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
        const deduped = normalizeContactSessions(dedupedById);

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
          const activeSessions = deduped.filter((session: Session) => {
            const status = typeof session.status === 'string'
              ? session.status.toLowerCase()
              : '';
            return !(session.archived || status === 'archived' || status === 'archiving');
          });
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
          const existingSession = (stateBeforeCreate.sessions || []).find((session: Session) => {
            const status = typeof session.status === 'string'
              ? session.status.toLowerCase()
              : '';
            if (session.archived || status === 'archived' || status === 'archiving') {
              return false;
            }

            const identity = resolveSessionContactIdentity(session);
            let sameContact = false;
            if (contactId) {
              sameContact = identity.contactId === contactId;
              if (!sameContact && contactAgentId) {
                sameContact = identity.contactAgentId === contactAgentId;
              }
            } else if (contactAgentId) {
              sameContact = identity.contactAgentId === contactAgentId;
            }

            if (!sameContact) {
              return false;
            }
            return resolveSessionProjectScopeId(session) === effectiveProjectId;
          });
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
              .filter((session: Session) => {
                const status = typeof session.status === 'string'
                  ? session.status.toLowerCase()
                  : '';
                if (session.archived || status === 'archived' || status === 'archiving') {
                  return false;
                }

                const identity = resolveSessionContactIdentity(session);
                let sameContact = false;
                if (contactId) {
                  sameContact = identity.contactId === contactId;
                  if (!sameContact && contactAgentId) {
                    sameContact = identity.contactAgentId === contactAgentId;
                  }
                } else if (contactAgentId) {
                  sameContact = identity.contactAgentId === contactAgentId;
                }
                if (!sameContact) {
                  return false;
                }

                return resolveSessionProjectScopeId(session) === effectiveProjectId;
              })
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

        sessionMessagesPageCache.delete(formattedSession.id);
        localStorage.setItem(`lastSessionId_${userId}_${effectiveProjectId}`, formattedSession.id);
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
        // 同一会话流式过程中仍允许切回聊天面板，避免在项目/终端面板点击会话无响应
        if (!options.keepActivePanel && beforeSelect.activePanel !== 'chat') {
          set((state: any) => {
            state.activePanel = 'chat';
          });
        }
        debugLog('🔍 当前会话正在流式中，忽略重复切换请求:', sessionId);
        return;
      }

      try {
        set((state: any) => {
          state.isLoading = true;
          state.error = null;
        });

        const existingSession = (beforeSelect.sessions || []).find((item: Session) => item.id === sessionId) || null;
        const [session, messages] = await Promise.all([
          existingSession ? Promise.resolve(existingSession) : fetchSession(client, sessionId),
          fetchSessionMessages(client, sessionId, { limit: 50, offset: 0 }),
        ]);
        writeSessionMessagesCache(sessionId, messages);
        const sessionAiSelectionFromMetadata = readSessionAiSelectionFromMetadata(session?.metadata);
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
              } else if (localStreamingMessage && typeof localStreamingMessage === 'object') {
                restoredStreamingMessage = cloneStreamingMessageDraft(localStreamingMessage);
              }

              const streamingDraftSource = restoredStreamingMessage || localStreamingMessage;
              if (streamingDraftSource) {
                const linkedUserMessageId = normalizeTurnId(
                  typeof streamingDraftSource.metadata?.historyFinalForUserMessageId === 'string'
                    ? streamingDraftSource.metadata.historyFinalForUserMessageId
                    : (
                      typeof streamingDraftSource.metadata?.historyDraftUserMessage?.id === 'string'
                        ? streamingDraftSource.metadata.historyDraftUserMessage.id
                        : ''
                    ),
                );
                const linkedTurnId = normalizeTurnId(
                  streamingDraftSource.metadata?.historyFinalForTurnId
                  || streamingDraftSource.metadata?.conversation_turn_id,
                );
                const linkedUserById = linkedUserMessageId
                  ? nextMessages.find((message: any) => message?.role === 'user' && message?.id === linkedUserMessageId)
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
                  if (restoredStreamingMessage.metadata.historyDraftUserMessage) {
                    restoredStreamingMessage.metadata.historyDraftUserMessage.id = linkedUserMessage.id;
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
          (state as any).currentSession = session;
          const index = state.sessions.findIndex((s: any) => s.id === sessionId);
          if (index !== -1 && session) {
            state.sessions[index] = session;
          } else if (session) {
            const status = typeof (session as any).status === 'string'
              ? (session as any).status.toLowerCase()
              : '';
            const isActive = !(session.archived || status === 'archived' || status === 'archiving');
            if (isActive) {
              const merged = [session, ...(state.sessions || []).filter((s: any) => s?.id !== session.id)];
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
          if (!options.keepActivePanel) {
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
        sessionMessagesPageCache.delete(sessionId);
      } catch (error) {
        console.error('Failed to delete session:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete session';
        });
      }
    },
  };
}
