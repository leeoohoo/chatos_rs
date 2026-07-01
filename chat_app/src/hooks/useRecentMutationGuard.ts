// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useRef } from 'react';

interface UseRecentMutationGuardOptions<TPayload> {
  buildKey: (payload: TPayload) => string;
  ttlMs?: number;
}

export const useRecentMutationGuard = <TPayload,>({
  buildKey,
  ttlMs = 4000,
}: UseRecentMutationGuardOptions<TPayload>) => {
  const recentMutationGuardRef = useRef<Map<string, number>>(new Map());

  const markRecentMutation = useCallback((payload: TPayload) => {
    const key = buildKey(payload);
    if (!key) {
      return;
    }
    recentMutationGuardRef.current.set(key, Date.now());
  }, [buildKey]);

  const consumeRecentMutation = useCallback((payload: TPayload): boolean => {
    const key = buildKey(payload);
    if (!key) {
      return false;
    }
    const seenAt = recentMutationGuardRef.current.get(key);
    if (!seenAt) {
      return false;
    }
    if (Date.now() - seenAt > ttlMs) {
      recentMutationGuardRef.current.delete(key);
      return false;
    }
    recentMutationGuardRef.current.delete(key);
    return true;
  }, [buildKey, ttlMs]);

  return {
    markRecentMutation,
    consumeRecentMutation,
  };
};
