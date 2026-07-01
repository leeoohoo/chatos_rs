// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Session } from '../../../../types';
import type ApiClient from '../../../api/client';
import { normalizeSession } from '../../helpers/sessions';
import type { ContactRecord } from '../../types';
import {
  normalizeContactSessions,
  resolveSessionContactIdentity,
  isSessionActive,
} from '../sessionsUtils';

interface SessionContactScope {
  contactAgentIds: Set<string>;
  contactIds: Set<string>;
}

interface SessionsListCacheEntry {
  sessions: Session[];
  stale: boolean;
  scope: SessionContactScope;
}

interface SessionsDetailCacheEntry {
  session: Session;
  stale: boolean;
}

interface SessionsClientCacheState {
  detailCache: Map<string, SessionsDetailCacheEntry>;
  detailInflight: Map<string, Promise<Session>>;
  listCache: Map<string, SessionsListCacheEntry>;
  listInflight: Map<string, Promise<Session[]>>;
}

const sessionsClientCaches = new WeakMap<ApiClient, SessionsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const buildContactScope = (contacts: ContactRecord[]): SessionContactScope => {
  const contactIds = new Set<string>();
  const contactAgentIds = new Set<string>();
  for (const contact of contacts || []) {
    const contactId = String(contact?.id || '').trim();
    const agentId = String(contact?.agentId || '').trim();
    if (contactId) {
      contactIds.add(contactId);
    }
    if (agentId) {
      contactAgentIds.add(agentId);
    }
  }
  return {
    contactAgentIds,
    contactIds,
  };
};

const shouldIncludeSessionForScope = (
  session: Session,
  scope?: SessionContactScope | null,
): boolean => {
  if (!session || !isSessionActive(session)) {
    return false;
  }
  const identity = resolveSessionContactIdentity(session);
  if (!identity.contactId && !identity.contactAgentId) {
    return false;
  }
  if (!scope || (scope.contactIds.size === 0 && scope.contactAgentIds.size === 0)) {
    return true;
  }
  if (identity.contactId && scope.contactIds.has(identity.contactId)) {
    return true;
  }
  if (identity.contactAgentId && scope.contactAgentIds.has(identity.contactAgentId)) {
    return true;
  }
  return false;
};

const normalizeSessionsForScope = (
  sessions: Session[],
  scope?: SessionContactScope | null,
): Session[] => {
  return normalizeContactSessions(
    (sessions || []).filter((session) => shouldIncludeSessionForScope(session, scope)),
  );
};

export const normalizeTrackedSessions = (
  sessions: Session[],
  contacts: ContactRecord[],
): Session[] => {
  return normalizeSessionsForScope(sessions, buildContactScope(contacts));
};

export const buildSessionsListCacheKey = (userId: string): string => normalizeUserId(userId);

export const getOrCreateSessionsClientCacheState = (
  apiClient: ApiClient,
): SessionsClientCacheState => {
  const existing = sessionsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: SessionsClientCacheState = {
    detailCache: new Map(),
    detailInflight: new Map(),
    listCache: new Map(),
    listInflight: new Map(),
  };
  sessionsClientCaches.set(apiClient, next);
  return next;
};

const syncSessionDetailCache = (apiClient: ApiClient, session: Session) => {
  const normalizedSessionId = normalizeSessionId(session.id);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  cacheState.detailCache.set(normalizedSessionId, {
    session,
    stale: false,
  });
};

const syncSessionListCaches = (
  apiClient: ApiClient,
  updater: (sessions: Session[], scope: SessionContactScope) => Session[],
) => {
  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  cacheState.listCache.forEach((entry, key) => {
    cacheState.listCache.set(key, {
      ...entry,
      sessions: updater(entry.sessions, entry.scope),
      stale: false,
    });
  });
};

export const syncLoadedSessions = (
  apiClient: ApiClient,
  userId: string,
  sessions: Session[],
  contacts: ContactRecord[],
) => {
  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  const scope = buildContactScope(contacts);
  cacheState.listCache.set(buildSessionsListCacheKey(userId), {
    sessions,
    stale: false,
    scope,
  });
  sessions.forEach((session) => {
    syncSessionDetailCache(apiClient, session);
  });
};

export const markSessionCachesStale = (
  apiClient: ApiClient,
  options?: { sessionId?: string | null; userId?: string | null },
) => {
  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  const normalizedUserId = normalizeUserId(String(options?.userId || ''));
  const normalizedSessionId = normalizeSessionId(String(options?.sessionId || ''));

  if (normalizedUserId) {
    const cached = cacheState.listCache.get(buildSessionsListCacheKey(normalizedUserId));
    if (cached) {
      cacheState.listCache.set(buildSessionsListCacheKey(normalizedUserId), {
        ...cached,
        stale: true,
      });
    }
  } else {
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        ...entry,
        stale: true,
      });
    });
  }

  if (normalizedSessionId) {
    const cached = cacheState.detailCache.get(normalizedSessionId);
    if (cached) {
      cacheState.detailCache.set(normalizedSessionId, {
        ...cached,
        stale: true,
      });
    }
  }
};

export const removeSessionCaches = (apiClient: ApiClient, sessionId: string) => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  cacheState.detailCache.delete(normalizedSessionId);
  cacheState.detailInflight.delete(normalizedSessionId);
  syncSessionListCaches(
    apiClient,
    (sessions, scope) => normalizeSessionsForScope(
      sessions.filter((session) => session.id !== normalizedSessionId),
      scope,
    ),
  );
};

export const upsertSessionCaches = (apiClient: ApiClient, session: Session) => {
  const normalizedSessionId = normalizeSessionId(session.id);
  if (!normalizedSessionId) {
    return;
  }
  syncSessionDetailCache(apiClient, session);
  syncSessionListCaches(apiClient, (sessions, scope) => {
    const remaining = sessions.filter((item) => item.id !== normalizedSessionId);
    if (!shouldIncludeSessionForScope(session, scope)) {
      return normalizeSessionsForScope(remaining, scope);
    }
    return normalizeSessionsForScope([session, ...remaining], scope);
  });
};

export const loadSessionDetail = async (
  apiClient: ApiClient,
  sessionId: string,
  options?: { force?: boolean },
): Promise<Session> => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    throw new Error('session id is required');
  }

  const cacheState = getOrCreateSessionsClientCacheState(apiClient);
  const cached = cacheState.detailCache.get(normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return cached.session;
  }

  let inflight = cacheState.detailInflight.get(normalizedSessionId);
  if (!inflight) {
    inflight = apiClient.getSession(normalizedSessionId)
      .then((payload) => normalizeSession(payload))
      .then((session) => {
        syncSessionDetailCache(apiClient, session);
        syncSessionListCaches(apiClient, (sessions, scope) => {
          const remaining = sessions.filter((item) => item.id !== normalizedSessionId);
          if (!shouldIncludeSessionForScope(session, scope)) {
            return normalizeSessionsForScope(remaining, scope);
          }
          return normalizeSessionsForScope([session, ...remaining], scope);
        });
        return session;
      })
      .finally(() => {
        cacheState.detailInflight.delete(normalizedSessionId);
      });
    cacheState.detailInflight.set(normalizedSessionId, inflight);
  }

  return inflight;
};
