// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect } from 'react';

import type { Message } from '../../types';

export const TASK_RUNNER_CALLBACK_RECONCILE_INTERVAL_MS = 5_000;

const readStringArray = (value: unknown): string[] => (
  Array.isArray(value)
    ? value
      .filter((item): item is string => typeof item === 'string')
      .map((item) => item.trim())
      .filter(Boolean)
    : []
);

export const hasOutstandingTaskRunnerCallbacks = (messages: Message[]): boolean => (
  messages.some((message) => {
    if (message.role !== 'user') {
      return false;
    }
    const taskRunnerAsync = message.metadata?.task_runner_async;
    const mode = String(taskRunnerAsync?.mode || '').trim();
    if (!taskRunnerAsync || !['contact_async', 'project_requirement_execution'].includes(mode)) {
      return false;
    }

    const createdTaskIds = readStringArray(taskRunnerAsync.created_task_ids);
    const terminalTaskIds = new Set(readStringArray(taskRunnerAsync.terminal_task_ids));
    if (createdTaskIds.some((taskId) => !terminalTaskIds.has(taskId))) {
      return true;
    }

    if (
      readStringArray(taskRunnerAsync.running_task_ids).length > 0
      || readStringArray(taskRunnerAsync.queued_task_ids).length > 0
      || readStringArray(taskRunnerAsync.pending_task_ids).length > 0
    ) {
      return true;
    }

    if (createdTaskIds.length > 0) {
      return false;
    }
    const status = String(taskRunnerAsync.overall_status || taskRunnerAsync.status || '')
      .trim()
      .toLowerCase();
    return status === 'pending' || status === 'processing' || status === 'running';
  })
);

interface UseTaskRunnerCallbackReconciliationOptions {
  enabled: boolean;
  sessionId: string | null;
  syncSessionMessages: (sessionId: string) => Promise<void>;
  intervalMs?: number;
}

export const useTaskRunnerCallbackReconciliation = ({
  enabled,
  sessionId,
  syncSessionMessages,
  intervalMs = TASK_RUNNER_CALLBACK_RECONCILE_INTERVAL_MS,
}: UseTaskRunnerCallbackReconciliationOptions): void => {
  useEffect(() => {
    const normalizedSessionId = String(sessionId || '').trim();
    if (!enabled || !normalizedSessionId) {
      return undefined;
    }

    const reconcile = () => {
      void syncSessionMessages(normalizedSessionId);
    };
    reconcile();
    const timer = window.setInterval(reconcile, intervalMs);
    return () => {
      window.clearInterval(timer);
    };
  }, [enabled, intervalMs, sessionId, syncSessionMessages]);
};
