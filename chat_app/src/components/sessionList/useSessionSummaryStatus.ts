import { useCallback, useEffect, useRef, useState } from 'react';

import type { Session } from '../../types';
import { getSessionStatus } from './helpers';

interface UseSessionSummaryStatusOptions {
  sessions: Session[];
  apiClient: any;
}

export const useSessionSummaryStatus = ({ sessions, apiClient }: UseSessionSummaryStatusOptions) => {
  const [sessionHasSummaryMap, setSessionHasSummaryMap] = useState<Record<string, boolean>>({});
  const checkingSummaryIdsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    const validIds = new Set(sessions.map((session: Session) => session.id));
    checkingSummaryIdsRef.current.forEach((sessionId) => {
      if (!validIds.has(sessionId)) {
        checkingSummaryIdsRef.current.delete(sessionId);
      }
    });

    setSessionHasSummaryMap((prev) => {
      const next: Record<string, boolean> = {};
      let changed = false;
      Object.entries(prev).forEach(([sessionId, hasSummary]) => {
        if (validIds.has(sessionId)) {
          next[sessionId] = hasSummary;
        } else {
          changed = true;
        }
      });
      return changed ? next : prev;
    });
  }, [sessions]);

  const checkSessionSummaryStatus = useCallback(async (sessionIds: string[]) => {
    const uniqueSessionIds = Array.from(new Set(
      sessionIds
        .map((sessionId) => String(sessionId || '').trim())
        .filter((sessionId) => sessionId.length > 0)
    ));
    const pendingSessionIds = uniqueSessionIds.filter(
      (sessionId) => !checkingSummaryIdsRef.current.has(sessionId)
    );
    if (pendingSessionIds.length === 0) {
      return;
    }

    pendingSessionIds.forEach((sessionId) => checkingSummaryIdsRef.current.add(sessionId));
    try {
      const pairs = await Promise.all(
        pendingSessionIds.map(async (sessionId) => {
          try {
            const payload = await apiClient.getSessionSummaries(sessionId, { limit: 1, offset: 0 });
            const hasSummary = payload?.has_summary === true
              || (Array.isArray(payload?.items) && payload.items.length > 0);
            return { sessionId, hasSummary };
          } catch (error) {
            console.warn('Failed to detect session summary status:', sessionId, error);
            return { sessionId, hasSummary: false };
          }
        })
      );

      setSessionHasSummaryMap((prev) => {
        const next = { ...prev };
        let changed = false;
        pairs.forEach(({ sessionId, hasSummary }) => {
          if (next[sessionId] !== hasSummary) {
            next[sessionId] = hasSummary;
            changed = true;
          }
        });
        return changed ? next : prev;
      });
    } finally {
      pendingSessionIds.forEach((sessionId) => checkingSummaryIdsRef.current.delete(sessionId));
    }
  }, [apiClient]);

  useEffect(() => {
    if (sessions.length === 0) {
      return;
    }

    const unknownSessionIds = sessions
      .filter((session: Session) => getSessionStatus(session) === 'active')
      .map((session: Session) => session.id)
      .filter((sessionId) => (
        typeof sessionHasSummaryMap[sessionId] !== 'boolean'
      ));
    if (unknownSessionIds.length === 0) {
      return;
    }

    void checkSessionSummaryStatus(unknownSessionIds);
  }, [checkSessionSummaryStatus, sessionHasSummaryMap, sessions]);

  useEffect(() => {
    if (sessions.length === 0) {
      return;
    }

    const sessionIds = sessions
      .filter((session: Session) => getSessionStatus(session) === 'active')
      .map((session: Session) => session.id);
    if (sessionIds.length === 0) {
      return;
    }
    void checkSessionSummaryStatus(sessionIds);

    const timer = window.setInterval(() => {
      void checkSessionSummaryStatus(sessionIds);
    }, 30000);
    return () => window.clearInterval(timer);
  }, [checkSessionSummaryStatus, sessions]);

  return {
    sessionHasSummaryMap,
  };
};
