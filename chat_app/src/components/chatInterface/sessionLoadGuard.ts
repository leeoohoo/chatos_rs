import type { MutableRefObject } from 'react';

interface SessionLoadScope {
  currentSessionRef: MutableRefObject<string | null>;
  sessionId: string;
}

interface SessionLoadRequestScope extends SessionLoadScope {
  requestSeq: number;
  requestSeqRef: MutableRefObject<number>;
}

interface RunGuardedSessionLoadOptions<T> {
  applyResult: (value: T) => void;
  errorMessage: string;
  load: () => Promise<T>;
  setError: (value: string | null) => void;
  setLoading: (value: boolean) => void;
  shouldApply: () => boolean;
  showLoading?: boolean;
}

export const beginSessionLoadRequest = (
  requestSeqRef: MutableRefObject<number>,
): number => {
  const requestSeq = requestSeqRef.current + 1;
  requestSeqRef.current = requestSeq;
  return requestSeq;
};

export const isLoadRequestCurrent = (
  requestSeqRef: MutableRefObject<number>,
  requestSeq: number,
): boolean => (
  requestSeqRef.current === requestSeq
);

export const isSessionLoadCurrent = ({
  currentSessionRef,
  sessionId,
}: SessionLoadScope): boolean => (
  currentSessionRef.current === sessionId
);

export const isSessionLoadRequestCurrent = ({
  currentSessionRef,
  requestSeq,
  requestSeqRef,
  sessionId,
}: SessionLoadRequestScope): boolean => (
  isLoadRequestCurrent(requestSeqRef, requestSeq)
  && isSessionLoadCurrent({ currentSessionRef, sessionId })
);

export const runGuardedSessionLoad = async <T>({
  applyResult,
  errorMessage,
  load,
  setError,
  setLoading,
  shouldApply,
  showLoading = true,
}: RunGuardedSessionLoadOptions<T>): Promise<void> => {
  if (showLoading) {
    setLoading(true);
  }
  setError(null);
  try {
    const value = await load();
    if (!shouldApply()) {
      return;
    }
    applyResult(value);
  } catch (error) {
    if (!shouldApply()) {
      return;
    }
    setError(error instanceof Error ? error.message : errorMessage);
  } finally {
    if (shouldApply()) {
      setLoading(false);
    }
  }
};
