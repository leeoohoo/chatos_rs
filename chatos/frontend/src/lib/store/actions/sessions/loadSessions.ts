// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../../../types';
import { debugLog } from '@/lib/utils';
import { ApiRequestError } from '../../../api/client/shared';
import { normalizeSession } from '../../helpers/sessions';
import type { ContactRecord } from '../../types';
import { readSessionAiSelectionFromMetadata } from '../../helpers/sessionAiSelection';
import type { ChatStoreDraft } from '../../types';
import {
  type MemoryContact,
  isSessionActive,
  resetCurrentSessionViewState,
  splitSessionsByMappedContacts,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';
import type {
  LoadSessionsOptions,
  SessionActionDeps,
} from './types';
import {
  buildSessionsListCacheKey,
  getOrCreateSessionsClientCacheState,
  loadSessionDetail,
  markSessionCachesStale,
  normalizeTrackedSessions,
  removeSessionCaches,
  syncLoadedSessions,
} from './cache';

export function createLoadSessionActions({
  set,
  get,
  client,
  getSessionParams,
  customUserId,
  customProjectId,
}: SessionActionDeps) {
  const toMemoryContacts = (contacts: ContactRecord[], userId: string): MemoryContact[] => {
    return (contacts || []).map((contact) => ({
      id: contact.id,
      user_id: userId,
      agent_id: contact.agentId,
      agent_name_snapshot: contact.name,
      status: contact.status,
      created_at: contact.createdAt?.toISOString?.(),
      updated_at: contact.updatedAt?.toISOString?.(),
    }));
  };

  const applyLoadedSessionsToState = (
    deduped: Session[],
    userId: string,
    projectId: string,
    options: LoadSessionsOptions,
  ) => {
    set((state: ChatStoreDraft) => {
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
        const matched = deduped.find((session) => session.id === state.currentSessionId);
        if (matched) {
          state.currentSession = matched;
          syncCurrentProjectFromSession(state, matched);
        } else {
          resetCurrentSessionViewState(state);
        }
      }
    });

    const currentState = get();
    if (
      deduped.length > 0
      && !currentState.currentSessionId
      && currentState.activePanel !== 'project'
    ) {
      const activeSessions = deduped.filter((session: Session) => isSessionActive(session));
      if (activeSessions.length > 0) {
        const lastSessionId = localStorage.getItem(`lastSessionId_${userId}_${projectId}`);
        let sessionToSelect: Session | undefined;

        if (lastSessionId) {
          sessionToSelect = activeSessions.find((session) => session.id === lastSessionId);
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
  };

  return {
    loadSessions: async (options: LoadSessionsOptions = {}) => {
      try {
        debugLog('🔍 loadSessions 被调用');
        if (!options.silent) {
          set((state: ChatStoreDraft) => {
            state.isLoading = true;
            state.error = null;
          });
          debugLog('🔍 loadSessions isLoading 设置为 true');
        }

        const { userId, projectId } = getSessionParams();
        const cacheKey = buildSessionsListCacheKey(userId, projectId);
        const cacheState = getOrCreateSessionsClientCacheState(client);
        const allowPrimaryListCache = !options.force && !options.append && !options.limit && !options.offset;
        const cached = allowPrimaryListCache ? cacheState.listCache.get(cacheKey) : null;

        debugLog('🔍 loadSessions 调用 client.getSessions', { userId, projectId, customUserId, customProjectId, options });
        if (cached && !cached.stale) {
          applyLoadedSessionsToState(cached.sessions, userId, projectId, options);
          debugLog('🔍 loadSessions 命中本地缓存');
          return cached.sessions;
        }

        const executeLoad = async (): Promise<{ contacts: ContactRecord[]; sessions: Session[] }> => {
          const contacts = await get().loadContacts();
          const memoryContacts = toMemoryContacts(contacts, userId);
          const requestProjectId = client.sessionScopeUsesLocalRuntime(projectId)
            ? projectId
            : undefined;
          const rawSessions = await client.getSessions(
            userId,
            requestProjectId,
            { limit: options.limit, offset: options.offset },
          );
          const sessions = Array.isArray(rawSessions)
            ? rawSessions.map(normalizeSession)
            : [];

          const { matchedSessions: filteredByContacts } = splitSessionsByMappedContacts(
            sessions,
            memoryContacts,
          );
          const mergedByContact = filteredByContacts;
          debugLog('🔍 loadSessions 返回结果:', mergedByContact);

          const existing = options.append ? (get().sessions || []) : [];
          const merged = options.append ? [...existing, ...mergedByContact] : mergedByContact;
          const dedupedById: Session[] = [];
          const seen = new Set<string>();
          for (const session of merged) {
            if (session && !seen.has(session.id)) {
              seen.add(session.id);
              dedupedById.push(session);
            }
          }

          return {
            contacts,
            sessions: normalizeTrackedSessions(dedupedById, contacts),
          };
        };

        let deduped: Session[];
        if (allowPrimaryListCache) {
          let inflight = cacheState.listInflight.get(cacheKey);
          if (!inflight) {
            inflight = executeLoad()
              .then(({ contacts, sessions }) => {
                syncLoadedSessions(client, userId, projectId, sessions, contacts);
                return sessions;
              })
              .finally(() => {
                cacheState.listInflight.delete(cacheKey);
              });
            cacheState.listInflight.set(cacheKey, inflight);
          }
          deduped = await inflight;
        } else {
          const loaded = await executeLoad();
          deduped = loaded.sessions;
        }

        applyLoadedSessionsToState(deduped, userId, projectId, options);
        debugLog('🔍 loadSessions 完成');
        return deduped;
      } catch (error) {
        console.error('🔍 loadSessions 错误:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to load sessions';
          if (!options.silent) {
            state.isLoading = false;
          }
        });
        return [];
      }
    },

    markSessionsStale: (options?: { userId?: string | null; sessionId?: string | null }) => {
      markSessionCachesStale(client, options);
    },

    refreshSessionById: async (sessionId: string) => {
      const trimmed = sessionId.trim();
      if (!trimmed) {
        return null;
      }
      try {
        const refreshed = await loadSessionDetail(client, trimmed, { force: true });
        const contacts = (get().contacts || []) as ContactRecord[];
        const tracked = normalizeTrackedSessions([refreshed], contacts).length > 0;
        set((state: ChatStoreDraft) => {
          const remaining = (state.sessions || []).filter((session) => session.id !== trimmed);
          state.sessions = tracked
            ? normalizeTrackedSessions([refreshed, ...remaining], state.contacts || [])
            : normalizeTrackedSessions(remaining, state.contacts || []);
          if (state.currentSessionId === trimmed) {
            state.currentSession = refreshed;
          }
          const selection = readSessionAiSelectionFromMetadata(refreshed?.metadata);
          if (selection) {
            if (!state.sessionAiSelectionBySession) {
              state.sessionAiSelectionBySession = {};
            }
            state.sessionAiSelectionBySession[trimmed] = selection;
            if (state.currentSessionId === trimmed) {
              state.selectedModelId = selection.selectedModelId ?? null;
              state.selectedAgentId = selection.selectedAgentId ?? null;
            }
          }
        });
        return refreshed;
      } catch (error) {
        if (error instanceof ApiRequestError && error.status === 404) {
          removeSessionCaches(client, trimmed);
          set((state: ChatStoreDraft) => {
            const remaining = (state.sessions || []).filter((session) => session.id !== trimmed);
            state.sessions = normalizeTrackedSessions(remaining, state.contacts || []);
            if (state.currentSessionId === trimmed) {
              resetCurrentSessionViewState(state);
            }
            if (state.activePanel === 'chat' && state.currentSessionId === null) {
              state.activePanel = state.currentProjectId ? 'project' : 'chat';
            }
          });
          return null;
        }
        console.error('Failed to refresh session by id:', error);
        markSessionCachesStale(client, {
          sessionId: trimmed,
          userId: getSessionParams().userId,
        });
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to refresh session';
        });
        return null;
      }
    },
  };
}
