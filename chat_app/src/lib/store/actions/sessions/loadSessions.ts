import type { Session } from '../../../../types';
import { debugLog, generateId } from '@/lib/utils';
import { normalizeSession } from '../../helpers/sessions';
import { readSessionAiSelectionFromMetadata } from '../../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../../helpers/sessionRuntime';
import type { ChatStoreDraft } from '../../types';
import {
  isSessionActive,
  normalizeContact,
  normalizeContactSessions,
  splitSessionsByMappedContacts,
} from '../sessionsUtils';
import type { MemoryContact } from '../sessionsUtils';
import type {
  LoadSessionsOptions,
  SessionActionDeps,
} from './types';

export function createLoadSessionActions({
  set,
  get,
  client,
  getSessionParams,
  customUserId,
  customProjectId,
}: SessionActionDeps) {
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

        const sessions = Array.isArray(rawSessions)
          ? rawSessions.map(normalizeSession)
          : [];

        const { matchedSessions: filteredByContacts, missingContacts } = splitSessionsByMappedContacts(
          sessions,
          contacts,
        );

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
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to load sessions';
          if (!options.silent) {
            state.isLoading = false;
          }
        });
        return [];
      }
    },
  };
}
