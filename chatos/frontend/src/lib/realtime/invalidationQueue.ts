// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useRef } from 'react';

interface RealtimeInvalidationQueueOptions<T> {
  delayMs?: number;
  initialDelayMs?: number;
  onExecute: (payload: T) => void | Promise<void>;
}

interface RealtimeInvalidationQueueHandle<T> {
  run: (payload: T) => void;
}

export const useRealtimeInvalidationQueue = <T,>({
  delayMs = 250,
  initialDelayMs = 0,
  onExecute,
}: RealtimeInvalidationQueueOptions<T>): RealtimeInvalidationQueueHandle<T> => {
  const onExecuteRef = useRef(onExecute);
  const inflightRef = useRef(false);
  const pendingPayloadRef = useRef<T | null>(null);
  const timerRef = useRef<number | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    onExecuteRef.current = onExecute;
  }, [onExecute]);

  useEffect(() => () => {
    mountedRef.current = false;
    pendingPayloadRef.current = null;
    inflightRef.current = false;
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  const runQueued = useCallback((payload: T) => {
    if (!mountedRef.current) {
      return;
    }
    inflightRef.current = true;
    Promise.resolve(onExecuteRef.current(payload))
      .catch((error) => {
        console.error('Failed to handle realtime invalidation event:', error);
      })
      .finally(() => {
        if (!mountedRef.current) {
          return;
        }
        timerRef.current = window.setTimeout(() => {
          timerRef.current = null;
          if (!mountedRef.current) {
            return;
          }
          const pendingPayload = pendingPayloadRef.current;
          if (pendingPayload !== null) {
            pendingPayloadRef.current = null;
            runQueued(pendingPayload);
            return;
          }
          inflightRef.current = false;
        }, delayMs);
      });
  }, [delayMs]);

  const scheduleRun = useCallback(() => {
    if (timerRef.current !== null) {
      return;
    }
    timerRef.current = window.setTimeout(() => {
      timerRef.current = null;
      if (!mountedRef.current || inflightRef.current) {
        return;
      }
      const pendingPayload = pendingPayloadRef.current;
      if (pendingPayload === null) {
        return;
      }
      pendingPayloadRef.current = null;
      runQueued(pendingPayload);
    }, initialDelayMs);
  }, [initialDelayMs, runQueued]);

  const run = useCallback((payload: T) => {
    pendingPayloadRef.current = payload;
    if (inflightRef.current) {
      return;
    }
    scheduleRun();
  }, [scheduleRun]);

  return { run };
};
