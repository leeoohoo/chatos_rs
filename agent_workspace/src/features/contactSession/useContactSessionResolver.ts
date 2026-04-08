import { useCallback, useRef } from 'react';

import type { Session } from '../../types';
import {
  findLatestRecordForContactProjectScope,
  isRecordMatchedContactProjectScope,
  normalizeProjectScopeId,
  resolveContactAgentIdFromScopeRecord,
  resolveContactIdFromScopeRecord,
  resolveProjectScopeIdFromRecord,
  resolveSessionTimestamp,
} from './sessionResolver';

export interface ContactScopeEntity {
  id: string;
  agentId: string;
  name?: string;
}

type CreateSessionFn = (
  payload: {
    title: string;
    contactAgentId: string;
    contactId: string;
    selectedModelId: string | null;
    projectId: string;
    projectRoot: string | null;
    mcpEnabled: boolean;
    enabledMcpIds: string[];
  },
  options?: { keepActivePanel?: boolean },
) => Promise<string | undefined | null>;

interface ScopeResolverApiClient {
  getSessions: (
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getSessionMessages: (
    sessionId: string,
    params?: { limit?: number; offset?: number; compact?: boolean },
  ) => Promise<unknown[]>;
}

interface UseContactScopeResolverOptions {
  sessions: Session[];
  currentSession: Session | null | undefined;
  createSession: CreateSessionFn;
  apiClient?: ScopeResolverApiClient;
  defaultProjectId?: string | null;
  includeApiLookup?: boolean;
}

interface EnsureBackingSessionOptions {
  projectId?: string | null;
  title?: string;
  selectedModelId?: string | null;
  projectRoot?: string | null;
  mcpEnabled?: boolean;
  enabledMcpIds?: string[];
  createSessionOptions?: { keepActivePanel?: boolean };
}

interface DisplayScopeSessionOptions {
  projectId?: string | null;
}

interface BuildDisplayScopeMapOptions extends DisplayScopeSessionOptions {
  keyPrefix?: string;
}

const sanitizeMcpIds = (ids: unknown): string[] => {
  if (!Array.isArray(ids)) {
    return [];
  }
  const out: string[] = [];
  for (const item of ids) {
    if (typeof item !== 'string') {
      continue;
    }
    const trimmed = item.trim();
    if (!trimmed || out.includes(trimmed)) {
      continue;
    }
    out.push(trimmed);
  }
  return out;
};

const readStringField = (value: unknown, key: string): string => {
  if (!value || typeof value !== 'object') {
    return '';
  }
  const raw = (value as Record<string, unknown>)[key];
  return typeof raw === 'string' ? raw.trim() : '';
};

export const useContactScopeResolver = ({
  sessions,
  currentSession,
  createSession,
  apiClient,
  defaultProjectId = '0',
  includeApiLookup = true,
}: UseContactScopeResolverOptions) => {
  // Hook 名字暂时保留，但它解析的是“联系人 + 项目作用域 -> backing session 容器”。
  const sessionCacheRef = useRef<Record<string, string>>({});

  const resolveProjectId = useCallback((projectId?: string | null): string => {
    return normalizeProjectScopeId(projectId ?? defaultProjectId);
  }, [defaultProjectId]);

  const resolveCacheKey = useCallback((contactId: string, projectId?: string | null): string => {
    return `${contactId}::${resolveProjectId(projectId)}`;
  }, [resolveProjectId]);

  const findSessionInStoreById = useCallback((sessionId: string): Session | null => {
    const targetId = typeof sessionId === 'string' ? sessionId.trim() : '';
    if (!targetId) {
      return null;
    }
    const matched = (sessions || []).find((session) => {
      const id = typeof session?.id === 'string' ? session.id.trim() : '';
      return id === targetId;
    });
    return matched || null;
  }, [sessions]);

  const currentBackingSession = useCallback((): Session | null => {
    const currentSessionId = typeof currentSession?.id === 'string'
      ? currentSession.id.trim()
      : '';
    if (!currentSessionId) {
      return null;
    }
    return findSessionInStoreById(currentSessionId);
  }, [currentSession?.id, findSessionInStoreById]);

  const isSessionIdStillMatched = useCallback((
    sessionId: string,
    contact: ContactScopeEntity,
    projectId?: string | null,
  ): boolean => {
    const matchedSession = findSessionInStoreById(sessionId);
    if (!matchedSession) {
      return false;
    }
    return isRecordMatchedContactProjectScope(matchedSession, contact, resolveProjectId(projectId));
  }, [findSessionInStoreById, resolveProjectId]);

  const findExistingSessionIdInStore = useCallback((
    contact: ContactScopeEntity,
    projectId?: string | null,
  ): string | null => {
    const matched = findLatestRecordForContactProjectScope(
      sessions || [],
      contact,
      resolveProjectId(projectId),
    );
    const sessionId = typeof matched?.id === 'string' ? matched.id.trim() : '';
    return sessionId || null;
  }, [resolveProjectId, sessions]);

  const findExistingSessionIdFromApi = useCallback(async (
    contact: ContactScopeEntity,
    projectId?: string | null,
  ): Promise<string | null> => {
    if (!apiClient || !includeApiLookup) {
      return null;
    }

    const normalizedProjectId = resolveProjectId(projectId);
    const pageSize = 200;
    const maxPages = 8;
    const candidates: unknown[] = [];

    for (let page = 0; page < maxPages; page += 1) {
      const rows = await apiClient.getSessions(undefined, normalizedProjectId, {
        limit: pageSize,
        offset: page * pageSize,
      });
      if (!Array.isArray(rows) || rows.length === 0) {
        break;
      }
      for (const row of rows) {
        if (isRecordMatchedContactProjectScope(
          row as Record<string, unknown>,
          contact,
          normalizedProjectId,
        )) {
          candidates.push(row);
        }
      }
      if (rows.length < pageSize) {
        break;
      }
    }

    if (candidates.length === 0) {
      return null;
    }

    candidates.sort((left, right) => (
      resolveSessionTimestamp(right as Record<string, unknown>)
      - resolveSessionTimestamp(left as Record<string, unknown>)
    ));
    const shortlist = candidates.slice(0, 20);
    for (const item of shortlist) {
      const sessionId = readStringField(item, 'id');
      if (!sessionId) {
        continue;
      }
      try {
        const previewMessages = await apiClient.getSessionMessages(sessionId, {
          limit: 1,
          offset: 0,
          compact: false,
        });
        if (Array.isArray(previewMessages) && previewMessages.length > 0) {
          return sessionId;
        }
      } catch {
        // ignore preview error, fallback to the first valid id.
      }
    }

    const fallback = shortlist.find((item) => readStringField(item, 'id').length > 0);
    return fallback ? readStringField(fallback, 'id') : null;
  }, [apiClient, includeApiLookup, resolveProjectId]);

  const resolveDisplayBackingSessionId = useCallback((
    contact: ContactScopeEntity,
    options?: DisplayScopeSessionOptions,
  ): string | null => {
    const normalizedProjectId = resolveProjectId(options?.projectId);
    const cacheKey = resolveCacheKey(contact.id, normalizedProjectId);

    const currentContactId = resolveContactIdFromScopeRecord(currentSession);
    const currentContactAgentId = resolveContactAgentIdFromScopeRecord(currentSession);
    const currentSessionProjectId = resolveProjectScopeIdFromRecord(currentSession);
    const resolvedCurrentBackingSession = currentBackingSession();
    if (
      resolvedCurrentBackingSession?.id
      && (
        (currentContactId && currentContactId === contact.id)
        || (currentContactAgentId && currentContactAgentId === contact.agentId)
      )
      && currentSessionProjectId === normalizedProjectId
    ) {
      sessionCacheRef.current[cacheKey] = resolvedCurrentBackingSession.id;
      return resolvedCurrentBackingSession.id;
    }

    const cachedSessionId = sessionCacheRef.current[cacheKey];
    if (cachedSessionId && cachedSessionId.trim()) {
      const normalizedCached = cachedSessionId.trim();
      if (isSessionIdStillMatched(normalizedCached, contact, normalizedProjectId)) {
        return normalizedCached;
      }
      delete sessionCacheRef.current[cacheKey];
    }

    const localSessionId = findExistingSessionIdInStore(contact, normalizedProjectId);
    if (localSessionId) {
      sessionCacheRef.current[cacheKey] = localSessionId;
      return localSessionId;
    }
    return null;
  }, [
    currentBackingSession,
    currentSession,
    findExistingSessionIdInStore,
    isSessionIdStillMatched,
    resolveCacheKey,
    resolveProjectId,
  ]);

  const buildDisplayBackingSessionIdMap = useCallback((
    contacts: ContactScopeEntity[],
    options?: BuildDisplayScopeMapOptions,
  ): Record<string, string> => {
    const prefix = options?.keyPrefix ?? '';
    const map: Record<string, string> = {};
    for (const contact of contacts || []) {
      const sessionId = resolveDisplayBackingSessionId(contact, {
        projectId: options?.projectId,
      });
      if (!sessionId) {
        continue;
      }
      map[`${prefix}${contact.id}`] = sessionId;
    }
    return map;
  }, [resolveDisplayBackingSessionId]);

  const ensureBackingSessionForContactScope = useCallback(async (
    contact: ContactScopeEntity,
    options?: EnsureBackingSessionOptions,
  ): Promise<string | null> => {
    const normalizedProjectId = resolveProjectId(options?.projectId);
    const cacheKey = resolveCacheKey(contact.id, normalizedProjectId);

    const cachedSessionId = sessionCacheRef.current[cacheKey];
    if (cachedSessionId && cachedSessionId.trim()) {
      const normalizedCached = cachedSessionId.trim();
      if (isSessionIdStillMatched(normalizedCached, contact, normalizedProjectId)) {
        return normalizedCached;
      }
      delete sessionCacheRef.current[cacheKey];
    }

    const runtimeSessionId = resolveDisplayBackingSessionId(contact, {
      projectId: normalizedProjectId,
    });
    if (runtimeSessionId) {
      sessionCacheRef.current[cacheKey] = runtimeSessionId;
      return runtimeSessionId;
    }

    try {
      const existingSessionId = await findExistingSessionIdFromApi(contact, normalizedProjectId);
      if (existingSessionId) {
        sessionCacheRef.current[cacheKey] = existingSessionId;
        return existingSessionId;
      }
    } catch (error) {
      console.error('Failed to resolve existing contact session:', error);
    }

    const createdSessionId = await createSession({
      title: options?.title || contact.name || '联系人',
      contactAgentId: contact.agentId,
      contactId: contact.id,
      selectedModelId: options?.selectedModelId ?? null,
      projectId: normalizedProjectId,
      projectRoot: options?.projectRoot ?? null,
      mcpEnabled: options?.mcpEnabled ?? true,
      enabledMcpIds: sanitizeMcpIds(options?.enabledMcpIds ?? []),
    }, options?.createSessionOptions);

    const resolvedCreatedSessionId = typeof createdSessionId === 'string' ? createdSessionId.trim() : '';
    if (resolvedCreatedSessionId) {
      sessionCacheRef.current[cacheKey] = resolvedCreatedSessionId;
      return resolvedCreatedSessionId;
    }
    return null;
  }, [
    createSession,
    findExistingSessionIdFromApi,
    isSessionIdStillMatched,
    resolveCacheKey,
    resolveDisplayBackingSessionId,
    resolveProjectId,
  ]);

  const clearCachedBackingSessionIdsForContact = useCallback((
    contactId: string,
    projectId?: string | null,
  ): string[] => {
    const normalizedProjectId = projectId === undefined ? null : resolveProjectId(projectId);
    const removed: string[] = [];
    for (const [key, sessionId] of Object.entries(sessionCacheRef.current)) {
      const [cachedContactId, cachedProjectId] = key.split('::');
      if (cachedContactId !== contactId) {
        continue;
      }
      if (normalizedProjectId && cachedProjectId !== normalizedProjectId) {
        continue;
      }
      delete sessionCacheRef.current[key];
      const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
      if (normalizedSessionId && !removed.includes(normalizedSessionId)) {
        removed.push(normalizedSessionId);
      }
    }
    return removed;
  }, [resolveProjectId]);

  return {
    ensureBackingSessionForContactScope,
    resolveDisplayBackingSessionId,
    buildDisplayBackingSessionIdMap,
    clearCachedBackingSessionIdsForContact,
    ensureContactSession: ensureBackingSessionForContactScope,
    resolveDisplayRuntimeSessionId: resolveDisplayBackingSessionId,
    buildDisplayRuntimeSessionIdMap: buildDisplayBackingSessionIdMap,
    clearCachedSessionIdsForContact: clearCachedBackingSessionIdsForContact,
  };
};

export type ContactSessionEntity = ContactScopeEntity;
export const useContactSessionResolver = useContactScopeResolver;
