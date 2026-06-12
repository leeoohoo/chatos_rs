import type { Session } from '../../../../types';
import { debugLog, generateId } from '@/lib/utils';
import { normalizeSession } from '../../helpers/sessions';
import {
  mergeSessionAiSelectionIntoMetadata,
  readSessionAiSelectionFromMetadata,
} from '../../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../../helpers/sessionRuntime';
import type {
  ChatStoreDraft,
  SessionAiSelection,
  SessionCreateOptions,
  SessionCreatePayload,
} from '../../types';
import {
  deleteSessionMessagesCacheEntry,
  matchSessionContactProjectScope,
  normalizeProjectScopeId,
  resolveSessionTimestamp,
  syncCurrentProjectFromSession,
} from '../sessionsUtils';
import type { SessionActionDeps } from './types';
import { normalizeTrackedSessions, upsertSessionCaches } from './cache';

export function createSessionCreateActions({
  set,
  get,
  client,
  getSessionParams,
  customUserId,
  customProjectId,
}: SessionActionDeps) {
  return {
    createSession: async (
      payload: string | SessionCreatePayload = 'New Chat',
      options: SessionCreateOptions = {},
    ) => {
      try {
        const shouldActivateSession = options.activateSession !== false;
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
            matchSessionContactProjectScope(session, {
              contactId,
              contactAgentId,
              projectId: effectiveProjectId,
            })
          ));
          if (existingSession) {
            if (shouldActivateSession) {
              await get().selectSession(existingSession.id, {
                keepActivePanel: options.keepActivePanel,
              });
            }
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
                matchSessionContactProjectScope(session, {
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
              if (shouldActivateSession) {
                await get().selectSession(remoteExisting.id, {
                  keepActivePanel: options.keepActivePanel,
                });
              }
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
          metadata?: Record<string, unknown>;
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
        upsertSessionCaches(client, formattedSession);
        const selectionFromMetadata = readSessionAiSelectionFromMetadata(formattedSession.metadata);
        const effectiveSelection = selectionFromMetadata || inheritedAiSelection;

        set((state: ChatStoreDraft) => {
          state.sessions = normalizeTrackedSessions(
            [formattedSession, ...(state.sessions || [])],
            state.contacts || [],
          );
          if (!state.sessionAiSelectionBySession) {
            state.sessionAiSelectionBySession = {};
          }
          state.sessionAiSelectionBySession[formattedSession.id] = {
            selectedModelId: effectiveSelection.selectedModelId,
            selectedAgentId: effectiveSelection.selectedAgentId,
          };
          if (shouldActivateSession) {
            state.currentSessionId = formattedSession.id;
            state.currentSession = formattedSession;
            syncCurrentProjectFromSession(state, formattedSession);
            state.selectedModelId = effectiveSelection.selectedModelId;
            state.selectedAgentId = effectiveSelection.selectedAgentId;
            state.messages = [];
            if (!options.keepActivePanel) {
              state.activePanel = 'chat';
            }
          }
          state.error = null;
        });

        set((state: ChatStoreDraft) => {
          deleteSessionMessagesCacheEntry(state, formattedSession.id);
        });
        if (shouldActivateSession) {
          localStorage.setItem(`lastSessionId_${userId}_${effectiveProjectId}`, formattedSession.id);
          debugLog('🔍 保存新创建的会话ID到 localStorage:', formattedSession.id);
        }

        return formattedSession.id;
      } catch (error) {
        console.error('❌ createSession 失败:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to create session';
        });
        throw error;
      }
    },
  };
}
