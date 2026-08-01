// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';

interface ReviewRepairPollingOptions {
  enabled: boolean;
  running: boolean;
  sessionId: string | null;
  refreshStatus: (
    sessionId: string,
  ) => Promise<{ running: boolean; pendingCount: number | null }>;
  onCompleted: () => void;
  onFailed: (message: string) => void;
  fallbackErrorMessage: string;
}

export const useReviewRepairPolling = ({
  enabled,
  running,
  sessionId,
  refreshStatus,
  onCompleted,
  onFailed,
  fallbackErrorMessage,
}: ReviewRepairPollingOptions): void => {
  useEffect(() => {
    if (!enabled || !sessionId || !running) {
      return undefined;
    }
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const poll = async () => {
      try {
        const status = await refreshStatus(sessionId);
        if (cancelled) {
          return;
        }
        if (!status.running) {
          onCompleted();
          return;
        }
      } catch (error) {
        if (!cancelled) {
          onFailed(error instanceof Error ? error.message : fallbackErrorMessage);
        }
        return;
      }
      timer = setTimeout(() => {
        void poll();
      }, 800);
    };
    timer = setTimeout(() => {
      void poll();
    }, 500);
    return () => {
      cancelled = true;
      if (timer) {
        clearTimeout(timer);
      }
    };
  }, [
    enabled,
    fallbackErrorMessage,
    onCompleted,
    onFailed,
    refreshStatus,
    running,
    sessionId,
  ]);
};
