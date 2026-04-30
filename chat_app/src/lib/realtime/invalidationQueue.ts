import { useCallback, useEffect, useRef } from 'react';

interface RealtimeInvalidationQueueOptions<T> {
  delayMs?: number;
  onExecute: (payload: T) => void | Promise<void>;
}

interface RealtimeInvalidationQueueHandle<T> {
  run: (payload: T) => void;
}

export const useRealtimeInvalidationQueue = <T,>({
  delayMs = 250,
  onExecute,
}: RealtimeInvalidationQueueOptions<T>): RealtimeInvalidationQueueHandle<T> => {
  const onExecuteRef = useRef(onExecute);
  const inflightRef = useRef(false);
  const pendingPayloadRef = useRef<T | null>(null);

  useEffect(() => {
    onExecuteRef.current = onExecute;
  }, [onExecute]);

  const runQueued = useCallback((payload: T) => {
    inflightRef.current = true;
    Promise.resolve(onExecuteRef.current(payload))
      .catch((error) => {
        console.error('Failed to handle realtime invalidation event:', error);
      })
      .finally(() => {
        window.setTimeout(() => {
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

  const run = useCallback((payload: T) => {
    if (inflightRef.current) {
      pendingPayloadRef.current = payload;
      return;
    }
    runQueued(payload);
  }, [runQueued]);

  return { run };
};
