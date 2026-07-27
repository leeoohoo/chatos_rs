// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';

interface PluginHookSummary {
  event: string | null;
  blockingFailure: boolean;
  executions: number;
  matched: number;
  failed: number;
  timedOut: number;
  workspaceWriteRequested: number;
  workspaceWriteApproved: number;
  workspaceWriteDenied: number;
}

export interface PluginRuntimeEventSummary {
  key: string;
  createdAt: string | null;
  eventType: 'plugin_runtime' | 'plugin_hook_blocked';
  pluginId: string | null;
  releaseId: string | null;
  componentKey: string | null;
  sessionId: string | null;
  phase: string;
  status: string;
  operation: string | null;
  toolName: string | null;
  healthStatus: string | null;
  durationMs: number | null;
  error: string | null;
  hook: PluginHookSummary | null;
}

const record = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

const text = (value: unknown, limit = 256): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const normalized = value.trim().replace(/\s+/gu, ' ');
  if (!normalized) {
    return null;
  }
  return normalized.length > limit ? `${normalized.slice(0, limit)}…` : normalized;
};

const nonNegativeNumber = (value: unknown): number | null => (
  typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null
);

const hookSummary = (value: unknown): PluginHookSummary | null => {
  const hook = record(value);
  if (!hook) {
    return null;
  }
  const executions = Array.isArray(hook.executions)
    ? hook.executions.map(record).filter((item): item is Record<string, unknown> => Boolean(item))
    : [];
  return {
    event: text(hook.event, 128),
    blockingFailure: hook.blocking_failure === true,
    executions: executions.length,
    matched: executions.filter((item) => item.matched === true).length,
    failed: executions.filter((item) => item.matched === true && item.succeeded === false).length,
    timedOut: executions.filter((item) => item.timed_out === true).length,
    workspaceWriteRequested: executions.filter((item) => item.workspace_write === true).length,
    workspaceWriteApproved: executions.filter((item) => (
      item.workspace_write === true && item.workspace_write_approved === true
    )).length,
    workspaceWriteDenied: executions.filter((item) => (
      item.workspace_write === true && item.workspace_write_approved === false
    )).length,
  };
};

export const pluginRuntimeSummariesFromEvents = (
  events: MessageTaskRunnerRunEvent[],
): PluginRuntimeEventSummary[] => events.flatMap((event): PluginRuntimeEventSummary[] => {
  const eventType = text(event.event_type, 64)?.toLowerCase();
  if (eventType !== 'plugin_runtime' && eventType !== 'plugin_hook_blocked') {
    return [];
  }
  const payload = record(event.payload);
  if (eventType === 'plugin_hook_blocked') {
    return [{
      key: event.id,
      createdAt: text(event.created_at, 64),
      eventType,
      pluginId: text(payload?.plugin_id),
      releaseId: text(payload?.release_id),
      componentKey: text(payload?.component_key),
      sessionId: text(payload?.adapter_session_id),
      phase: 'hook',
      status: 'blocked',
      operation: text(payload?.event, 128),
      toolName: text(payload?.tool_name),
      healthStatus: null,
      durationMs: null,
      error: text(event.message ?? payload?.error, 1024),
      hook: {
        event: text(payload?.event, 128),
        blockingFailure: true,
        executions: 0,
        matched: 0,
        failed: 0,
        timedOut: 0,
        workspaceWriteRequested: 0,
        workspaceWriteApproved: 0,
        workspaceWriteDenied: 0,
      },
    }];
  }
  return [{
    key: event.id,
    createdAt: text(event.created_at, 64),
    eventType,
    pluginId: text(payload?.plugin_id),
    releaseId: text(payload?.release_id),
    componentKey: text(payload?.component_key),
    sessionId: text(payload?.adapter_session_id),
    phase: text(payload?.phase, 64) || 'runtime',
    status: text(payload?.status, 64) || 'unknown',
    operation: text(payload?.operation),
    toolName: text(payload?.tool_name),
    healthStatus: text(payload?.health_status, 64),
    durationMs: nonNegativeNumber(payload?.duration_ms),
    error: text(payload?.error, 1024),
    hook: hookSummary(payload?.hook_dispatch),
  }];
});
