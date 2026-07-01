// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useRef } from 'react';

import type { Session } from '../../types';
import {
  findBestMatchedSession,
  hasSessionMessages,
  isSessionMatchedContactAndProject,
  normalizeProjectScopeId,
  PUBLIC_PROJECT_ID,
  resolveContactAgentIdFromSession,
  resolveContactIdFromSession,
  resolveSessionProjectScopeId,
  resolveSessionTimestamp,
} from './sessionResolver';

export interface ContactSessionEntity {
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
  },
  options?: { keepActivePanel?: boolean },
) => Promise<string | undefined | null>;

interface SessionResolverApiClient {
  getSessions: (
    userId?: string,
    projectId?: string,
    paging?: { limit?: number; offset?: number },
  ) => Promise<unknown[]>;
  getConversationMessages: (
    sessionId: string,
    params?: { limit?: number; offset?: number; compact?: boolean },
  ) => Promise<unknown[]>;
}

interface UseContactSessionResolverOptions {
  sessions: Session[];
  currentSession: Session | null | undefined;
  createSession: CreateSessionFn;
  apiClient?: SessionResolverApiClient;
  defaultProjectId?: string | null;
  includeApiLookup?: boolean;
}

interface EnsureContactSessionOptions {
  projectId?: string | null;
  title?: string;
  selectedModelId?: string | null;
  projectRoot?: string | null;
  preferredSessionId?: string | null;
  preferredSessionHasMessages?: boolean;
  createIfMissing?: boolean;
  createSessionOptions?: { keepActivePanel?: boolean; activateSession?: boolean };
}

interface DisplayRuntimeSessionOptions {
  projectId?: string | null;
  preferredSessionId?: string | null;
}

interface BuildDisplayRuntimeMapOptions extends DisplayRuntimeSessionOptions {
  keyPrefix?: string;
}

const readStringField = (value: unknown, key: string): string => {
  if (!value || typeof value !== 'object') {
    return '';
  }
  const raw = (value as Record<string, unknown>)[key];
  return typeof raw === 'string' ? raw.trim() : '';
};

const normalizeSessionId = (value: string | null | undefined): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const useContactSessionResolver = ({
  sessions,
  currentSession,
  createSession,
  apiClient,
  defaultProjectId = PUBLIC_PROJECT_ID,
  includeApiLookup = true,
}: UseContactSessionResolverOptions) => {
  const sessionCacheRef = useRef<Record<string, string>>({});
  const apiLookupResultRef = useRef<Record<string, string | null>>({});
  const apiLookupInflightRef = useRef<Record<string, Promise<string | null>>>({});

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

  const isSessionIdStillMatched = useCallback((
    sessionId: string,
    contact: ContactSessionEntity,
    projectId?: string | null,
  ): boolean => {
    const matchedSession = findSessionInStoreById(sessionId);
    if (!matchedSession) {
      return false;
    }
    return isSessionMatchedContactAndProject(matchedSession, contact, resolveProjectId(projectId));
  }, [findSessionInStoreById, resolveProjectId]);

  const findExistingSessionIdInStore = useCallback((
    contact: ContactSessionEntity,
    projectId?: string | null,
    preferredSessionId?: string | null,
  ): string | null => {
    const matched = findBestMatchedSession(
      sessions || [],
      contact,
      resolveProjectId(projectId),
      preferredSessionId,
    );
    const sessionId = typeof matched?.id === 'string' ? matched.id.trim() : '';
    return sessionId || null;
  }, [resolveProjectId, sessions]);

  const findExistingSessionIdFromApi = useCallback(async (
    contact: ContactSessionEntity,
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
        if (isSessionMatchedContactAndProject(row as Record<string, unknown>, contact, normalizedProjectId)) {
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
        const previewMessages = await apiClient.getConversationMessages(sessionId, {
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

  const readCachedSessionId = useCallback((
    cacheKey: string,
    contact: ContactSessionEntity,
    projectId: string,
    preferredSessionId?: string | null,
  ): string | null => {
    const cachedSessionId = sessionCacheRef.current[cacheKey];
    const normalizedCached = typeof cachedSessionId === 'string' ? cachedSessionId.trim() : '';
    if (!normalizedCached) {
      delete sessionCacheRef.current[cacheKey];
      return null;
    }
    if (isSessionIdStillMatched(normalizedCached, contact, projectId)) {
      const cachedSession = findSessionInStoreById(normalizedCached);
      const bestSession = findBestMatchedSession(sessions || [], contact, projectId, preferredSessionId);
      if (
        cachedSession
        && bestSession?.id
        && bestSession.id !== normalizedCached
        && hasSessionMessages(bestSession)
        && !hasSessionMessages(cachedSession)
      ) {
        delete sessionCacheRef.current[cacheKey];
        return null;
      }
      return normalizedCached;
    }
    if (apiLookupResultRef.current[cacheKey] === normalizedCached) {
      const bestSession = findBestMatchedSession(sessions || [], contact, projectId, preferredSessionId);
      if (
        bestSession?.id
        && bestSession.id !== normalizedCached
        && hasSessionMessages(bestSession)
      ) {
        delete sessionCacheRef.current[cacheKey];
        return null;
      }
      return normalizedCached;
    }
    delete sessionCacheRef.current[cacheKey];
    return null;
  }, [findSessionInStoreById, isSessionIdStillMatched, sessions]);

  const resolveExistingSessionIdFromApi = useCallback(async (
    cacheKey: string,
    contact: ContactSessionEntity,
    projectId: string,
  ): Promise<string | null> => {
    if (Object.prototype.hasOwnProperty.call(apiLookupResultRef.current, cacheKey)) {
      return apiLookupResultRef.current[cacheKey];
    }

    const existingInflight = apiLookupInflightRef.current[cacheKey];
    if (existingInflight) {
      return existingInflight;
    }

    const inflight = findExistingSessionIdFromApi(contact, projectId)
      .then((sessionId) => {
        apiLookupResultRef.current[cacheKey] = sessionId;
        return sessionId;
      })
      .finally(() => {
        delete apiLookupInflightRef.current[cacheKey];
      });
    apiLookupInflightRef.current[cacheKey] = inflight;
    return inflight;
  }, [findExistingSessionIdFromApi]);

  const resolveDisplayRuntimeSessionId = useCallback((
    contact: ContactSessionEntity,
    options?: DisplayRuntimeSessionOptions,
  ): string | null => {
    const normalizedProjectId = resolveProjectId(options?.projectId);
    const cacheKey = resolveCacheKey(contact.id, normalizedProjectId);
    const preferredSessionId = normalizeSessionId(options?.preferredSessionId);

    const currentContactId = resolveContactIdFromSession(currentSession);
    const currentContactAgentId = resolveContactAgentIdFromSession(currentSession);
    const currentSessionProjectId = resolveSessionProjectScopeId(currentSession);
    const normalizedContactId = typeof contact.id === 'string' ? contact.id.trim() : '';
    const normalizedContactAgentId = typeof contact.agentId === 'string' ? contact.agentId.trim() : '';
    if (
      currentSession?.id
      && (normalizedContactId
        ? currentContactId === normalizedContactId
        : Boolean(normalizedContactAgentId && currentContactAgentId === normalizedContactAgentId))
      && currentSessionProjectId === normalizedProjectId
      && (
        !preferredSessionId
        || currentSession.id === preferredSessionId
        || hasSessionMessages(currentSession)
      )
    ) {
      sessionCacheRef.current[cacheKey] = currentSession.id;
      return currentSession.id;
    }

    const cachedSessionId = readCachedSessionId(
      cacheKey,
      contact,
      normalizedProjectId,
      preferredSessionId,
    );
    if (cachedSessionId) {
      return cachedSessionId;
    }

    const localSessionId = findExistingSessionIdInStore(
      contact,
      normalizedProjectId,
      preferredSessionId,
    );
    if (localSessionId) {
      sessionCacheRef.current[cacheKey] = localSessionId;
      return localSessionId;
    }
    return null;
  }, [
    currentSession,
    findExistingSessionIdInStore,
    readCachedSessionId,
    resolveCacheKey,
    resolveProjectId,
  ]);

  const buildDisplayRuntimeSessionIdMap = useCallback((
    contacts: ContactSessionEntity[],
    options?: BuildDisplayRuntimeMapOptions,
  ): Record<string, string> => {
    const prefix = options?.keyPrefix ?? '';
    const map: Record<string, string> = {};
    for (const contact of contacts || []) {
      const sessionId = resolveDisplayRuntimeSessionId(contact, {
        projectId: options?.projectId,
      });
      if (!sessionId) {
        continue;
      }
      map[`${prefix}${contact.id}`] = sessionId;
    }
    return map;
  }, [resolveDisplayRuntimeSessionId]);

  const ensureContactSession = useCallback(async (
    contact: ContactSessionEntity,
    options?: EnsureContactSessionOptions,
  ): Promise<string | null> => {
    const normalizedProjectId = resolveProjectId(options?.projectId);
    const cacheKey = resolveCacheKey(contact.id, normalizedProjectId);
    const preferredSessionId = normalizeSessionId(options?.preferredSessionId);

    if (preferredSessionId && options?.preferredSessionHasMessages === true) {
      const preferredLocalSession = findSessionInStoreById(preferredSessionId);
      if (
        !preferredLocalSession
        || isSessionMatchedContactAndProject(preferredLocalSession, contact, normalizedProjectId)
      ) {
        sessionCacheRef.current[cacheKey] = preferredSessionId;
        apiLookupResultRef.current[cacheKey] = preferredSessionId;
        return preferredSessionId;
      }
    }

    const cachedSessionId = readCachedSessionId(
      cacheKey,
      contact,
      normalizedProjectId,
      preferredSessionId,
    );
    if (cachedSessionId) {
      return cachedSessionId;
    }

    try {
      const existingSessionId = await resolveExistingSessionIdFromApi(
        cacheKey,
        contact,
        normalizedProjectId,
      );
      if (existingSessionId) {
        sessionCacheRef.current[cacheKey] = existingSessionId;
        return existingSessionId;
      }
    } catch (error) {
      console.error('Failed to resolve existing contact session:', error);
    }

    const runtimeSessionId = resolveDisplayRuntimeSessionId(contact, {
      projectId: normalizedProjectId,
      preferredSessionId,
    });
    if (runtimeSessionId) {
      sessionCacheRef.current[cacheKey] = runtimeSessionId;
      return runtimeSessionId;
    }

    if (options?.createIfMissing === false) {
      apiLookupResultRef.current[cacheKey] = null;
      return null;
    }

    const createdSessionId = await createSession({
      title: options?.title || contact.name || '联系人',
      contactAgentId: contact.agentId,
      contactId: contact.id,
      selectedModelId: options?.selectedModelId ?? null,
      projectId: normalizedProjectId,
      projectRoot: options?.projectRoot ?? null,
    }, options?.createSessionOptions);

    const resolvedCreatedSessionId = typeof createdSessionId === 'string' ? createdSessionId.trim() : '';
    if (resolvedCreatedSessionId) {
      sessionCacheRef.current[cacheKey] = resolvedCreatedSessionId;
      apiLookupResultRef.current[cacheKey] = resolvedCreatedSessionId;
      return resolvedCreatedSessionId;
    }
    return null;
  }, [
    createSession,
    findSessionInStoreById,
    readCachedSessionId,
    resolveCacheKey,
    resolveDisplayRuntimeSessionId,
    resolveExistingSessionIdFromApi,
    resolveProjectId,
  ]);

  const clearCachedSessionIdsForContact = useCallback((
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
      delete apiLookupResultRef.current[key];
      delete apiLookupInflightRef.current[key];
      const normalizedSessionId = typeof sessionId === 'string' ? sessionId.trim() : '';
      if (normalizedSessionId && !removed.includes(normalizedSessionId)) {
        removed.push(normalizedSessionId);
      }
    }
    return removed;
  }, [resolveProjectId]);

  return {
    ensureContactSession,
    resolveDisplayRuntimeSessionId,
    buildDisplayRuntimeSessionIdMap,
    clearCachedSessionIdsForContact,
  };
};
