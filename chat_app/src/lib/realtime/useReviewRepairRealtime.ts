import { useCallback, useEffect, useRef, useState } from 'react';

import type ApiClient from '../api/client';
import { useRealtimeConnectionState, useRealtimeEvent, useRealtimeTopic } from './RealtimeProvider';
import type { RealtimeEventEnvelope, ReviewRepairRealtimePayload } from './types';

const REVIEW_REPAIR_STATUS_CACHE_TTL_MS = 1000;

interface UseReviewRepairRealtimeOptions {
  apiClient: ApiClient;
  sessionId: string | null;
  enabled?: boolean;
  onCompleted?: () => void | Promise<void>;
  onFailed?: (errorMessage: string) => void;
}

interface ReviewRepairState {
  reviewRepairRunning: boolean;
  reviewRepairPendingCount: number | null;
  refreshReviewRepairStatus: (sessionId: string) => Promise<{ running: boolean; pendingCount: number | null }>;
  markReviewRepairStarting: () => void;
}

interface ReviewRepairStatusSnapshot {
  running: boolean;
  pendingCount: number | null;
  fetchedAt: number;
}

const reviewRepairStatusCache = new Map<string, ReviewRepairStatusSnapshot>();
const reviewRepairStatusInflight = new Map<string, Promise<{ running: boolean; pendingCount: number | null }>>();

const isReviewRepairPayload = (
  envelope: RealtimeEventEnvelope,
): envelope is RealtimeEventEnvelope & { payload: ReviewRepairRealtimePayload & { kind: 'review_repair' } } => {
  return envelope?.payload?.kind === 'review_repair';
};

const readCachedReviewRepairStatus = (
  sessionId: string,
): { running: boolean; pendingCount: number | null } | null => {
  const cached = reviewRepairStatusCache.get(sessionId);
  if (!cached) {
    return null;
  }
  if (Date.now() - cached.fetchedAt > REVIEW_REPAIR_STATUS_CACHE_TTL_MS) {
    reviewRepairStatusCache.delete(sessionId);
    return null;
  }
  return {
    running: cached.running,
    pendingCount: cached.pendingCount,
  };
};

const writeCachedReviewRepairStatus = (
  sessionId: string,
  status: { running: boolean; pendingCount: number | null },
) => {
  reviewRepairStatusCache.set(sessionId, {
    ...status,
    fetchedAt: Date.now(),
  });
};

const loadReviewRepairStatus = async (
  apiClient: ApiClient,
  sessionId: string,
  options: { force?: boolean } = {},
): Promise<{ running: boolean; pendingCount: number | null }> => {
  const normalizedSessionId = String(sessionId || '').trim();
  if (!normalizedSessionId) {
    return { running: false, pendingCount: null };
  }

  if (!options.force) {
    const cached = readCachedReviewRepairStatus(normalizedSessionId);
    if (cached) {
      return cached;
    }
    const inflight = reviewRepairStatusInflight.get(normalizedSessionId);
    if (inflight) {
      return inflight;
    }
  }

  const request = apiClient.getConversationReviewRepairStatus(normalizedSessionId)
    .then((result) => {
      if (result?.success === false) {
        throw new Error(result.detail || result.error || '获取复盘状态失败');
      }
      const nextStatus = {
        running: result?.result?.running === true,
        pendingCount: typeof result?.result?.pending_message_count === 'number'
          ? result.result.pending_message_count
          : null,
      };
      writeCachedReviewRepairStatus(normalizedSessionId, nextStatus);
      return nextStatus;
    })
    .finally(() => {
      const current = reviewRepairStatusInflight.get(normalizedSessionId);
      if (current === request) {
        reviewRepairStatusInflight.delete(normalizedSessionId);
      }
    });

  reviewRepairStatusInflight.set(normalizedSessionId, request);
  return request;
};

export const useReviewRepairRealtime = ({
  apiClient,
  sessionId,
  enabled = true,
  onCompleted,
  onFailed,
}: UseReviewRepairRealtimeOptions): ReviewRepairState => {
  const connectionState = useRealtimeConnectionState();
  const [reviewRepairRunning, setReviewRepairRunning] = useState(false);
  const [reviewRepairPendingCount, setReviewRepairPendingCount] = useState<number | null>(null);
  const sessionIdRef = useRef<string | null>(sessionId);
  const pendingCountRef = useRef<number | null>(null);
  const completionCallbackRef = useRef(onCompleted);
  const failedCallbackRef = useRef(onFailed);
  const lastCompletionAtRef = useRef(0);
  const lastFailureAtRef = useRef(0);
  const statusHydratedRef = useRef(false);

  useEffect(() => {
    pendingCountRef.current = reviewRepairPendingCount;
  }, [reviewRepairPendingCount]);

  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);

  useEffect(() => {
    statusHydratedRef.current = false;
  }, [sessionId]);

  useEffect(() => {
    completionCallbackRef.current = onCompleted;
  }, [onCompleted]);

  useEffect(() => {
    failedCallbackRef.current = onFailed;
  }, [onFailed]);

  useRealtimeTopic(
    sessionId ? { scope: 'conversation', id: sessionId } : null,
    enabled && Boolean(sessionId),
  );

  const triggerCompleted = useCallback(() => {
    const now = Date.now();
    if (now - lastCompletionAtRef.current < 1000) {
      return;
    }
    lastCompletionAtRef.current = now;
    void completionCallbackRef.current?.();
  }, []);

  const triggerFailed = useCallback((message: string) => {
    const now = Date.now();
    if (now - lastFailureAtRef.current < 1000) {
      return;
    }
    lastFailureAtRef.current = now;
    failedCallbackRef.current?.(message);
  }, []);

  const applyStatusToState = useCallback((
    currentSessionId: string,
    status: { running: boolean; pendingCount: number | null },
  ) => {
    if (!currentSessionId || sessionIdRef.current !== currentSessionId) {
      return status;
    }
    setReviewRepairRunning(status.running);
    setReviewRepairPendingCount(status.pendingCount);
    return status;
  }, []);

  const refreshReviewRepairStatus = useCallback(async (
    currentSessionId: string,
  ): Promise<{ running: boolean; pendingCount: number | null }> => {
    if (!currentSessionId) {
      setReviewRepairRunning(false);
      setReviewRepairPendingCount(null);
      return { running: false, pendingCount: null };
    }
    const status = await loadReviewRepairStatus(apiClient, currentSessionId, { force: true });
    return applyStatusToState(currentSessionId, status);
  }, [apiClient, applyStatusToState]);

  const markReviewRepairStarting = useCallback(() => {
    setReviewRepairRunning(true);
  }, []);

  useRealtimeEvent((event) => {
    if (!enabled || !sessionId || !isReviewRepairPayload(event)) {
      return;
    }
    const conversationId = String(
      event.conversation_id
      || event.payload.conversation_id
      || '',
    ).trim();
    if (!conversationId || conversationId !== sessionId) {
      return;
    }

    const nextRunning = Boolean(event.payload.running);
    const pendingCountFromPayload = typeof event.payload.pending_message_count === 'number'
      ? event.payload.pending_message_count
      : null;
    const cachedPendingCount = readCachedReviewRepairStatus(conversationId)?.pendingCount;
    const nextPendingCount = pendingCountFromPayload ?? (
      event.event === 'conversation.review_repair.failed'
        ? (cachedPendingCount ?? pendingCountRef.current ?? null)
        : null
    );

    setReviewRepairRunning(nextRunning);
    setReviewRepairPendingCount(nextPendingCount);
    statusHydratedRef.current = true;
    writeCachedReviewRepairStatus(conversationId, {
      running: nextRunning,
      pendingCount: nextPendingCount,
    });

    if (event.event === 'conversation.review_repair.completed') {
      triggerCompleted();
    } else if (event.event === 'conversation.review_repair.failed') {
      triggerFailed(event.payload.error || '执行复盘失败');
      void refreshReviewRepairStatus(conversationId).catch((error) => {
        console.error('Failed to refresh review repair status after realtime failure:', error);
      });
    }
  });

  useEffect(() => {
    if (!enabled || !sessionId) {
      setReviewRepairRunning(false);
      setReviewRepairPendingCount(null);
      return undefined;
    }

    const cachedStatus = readCachedReviewRepairStatus(sessionId);
    if (cachedStatus) {
      statusHydratedRef.current = true;
      applyStatusToState(sessionId, cachedStatus);
      return undefined;
    }

    if (connectionState === 'connected' && statusHydratedRef.current) {
      return undefined;
    }

    void loadReviewRepairStatus(apiClient, sessionId)
      .then((status) => {
        statusHydratedRef.current = true;
        applyStatusToState(sessionId, status);
      })
      .catch((error) => {
        console.error('Failed to load review repair status:', error);
      });
    return undefined;
  }, [
    connectionState,
    enabled,
    apiClient,
    applyStatusToState,
    sessionId,
  ]);

  useEffect(() => {
    if (!enabled || !sessionId || connectionState === 'connected' || !reviewRepairRunning) {
      return;
    }
    if (typeof document === 'undefined') {
      return undefined;
    }
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        void refreshReviewRepairStatus(sessionId).catch((error) => {
          console.error('Failed to refresh review repair status on visibility restore:', error);
          triggerFailed(error instanceof Error ? error.message : '获取复盘状态失败');
        });
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [
    connectionState,
    enabled,
    refreshReviewRepairStatus,
    reviewRepairRunning,
    sessionId,
    triggerFailed,
  ]);

  return {
    reviewRepairRunning,
    reviewRepairPendingCount,
    refreshReviewRepairStatus,
    markReviewRepairStarting,
  };
};
