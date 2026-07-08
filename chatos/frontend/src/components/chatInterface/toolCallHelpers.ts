// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../types';

interface ToolCallLike {
  id?: string;
  name?: string;
  tool_call_id?: string;
  toolCallId?: string;
  result?: unknown;
  finalResult?: unknown;
  error?: unknown;
  completed?: boolean;
}

interface MessageWithToolCalls {
  sessionId?: string;
  toolCalls?: ToolCallLike[];
  metadata?: (NonNullable<Message['metadata']> & {
    conversation_turn_id?: string;
    toolCalls?: ToolCallLike[];
  }) | undefined;
}

export const isTaskMutationToolName = (name: unknown): boolean => {
  const normalized = String(name || '').toLowerCase();
  if (!normalized) {
    return false;
  }

  const taskScope = normalized.includes('task_manager') || normalized.includes('task');
  if (!taskScope) {
    return false;
  }

  return normalized.includes('add_task')
    || normalized.includes('update_task')
    || normalized.includes('complete_task')
    || normalized.includes('delete_task');
};

export const collectMessageToolCalls = (message: MessageWithToolCalls): ToolCallLike[] => {
  const topLevel = Array.isArray(message?.toolCalls) ? message.toolCalls : [];
  const metadataLevel = Array.isArray(message?.metadata?.toolCalls)
    ? message.metadata.toolCalls
    : [];

  const merged = [...metadataLevel, ...topLevel];
  if (merged.length <= 1) {
    return merged;
  }

  const seen = new Set<string>();
  return merged.filter((toolCall, index) => {
    const key = String(
      toolCall?.id || toolCall?.tool_call_id || toolCall?.toolCallId || `${index}:${toolCall?.name || ''}`
    );
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
};

export const shouldRefreshForTaskMutationToolCall = (toolCall: ToolCallLike): boolean => (
  isTaskMutationToolName(toolCall?.name)
);

export const hasToolCallError = (toolCall: ToolCallLike): boolean => {
  if (toolCall?.error === null || toolCall?.error === undefined) {
    return false;
  }
  if (typeof toolCall.error === 'string') {
    return toolCall.error.trim().length > 0;
  }
  return true;
};

export const parseMaybeJsonValue = (value: unknown): unknown => {
  if (typeof value !== 'string') {
    return value;
  }

  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }

  try {
    return JSON.parse(trimmed);
  } catch (_) {
    return null;
  }
};

export const collectTaskIdsFromToolResult = (
  value: unknown,
  collector: Set<string>,
  depth = 0,
): void => {
  if (!value || depth > 5) {
    return;
  }

  if (Array.isArray(value)) {
    value.forEach((item) => collectTaskIdsFromToolResult(item, collector, depth + 1));
    return;
  }

  if (typeof value !== 'object') {
    return;
  }

  const record = value as Record<string, unknown>;

  const taskId = typeof record.task_id === 'string' ? record.task_id.trim() : '';
  if (taskId) {
    collector.add(taskId);
  }

  if (record.task && typeof record.task === 'object') {
    const nestedTask = record.task as Record<string, unknown>;
    const nestedId = typeof nestedTask.id === 'string' ? nestedTask.id.trim() : '';
    if (nestedId) {
      collector.add(nestedId);
    }
    collectTaskIdsFromToolResult(record.task, collector, depth + 1);
  }

  if (Array.isArray(record.tasks)) {
    record.tasks.forEach((task) => {
      if (task && typeof task === 'object') {
        const taskIdValue = typeof (task as Record<string, unknown>).id === 'string'
          ? (task as Record<string, unknown>).id as string
          : '';
        if (taskIdValue.trim()) {
          collector.add(taskIdValue.trim());
        }
      }
    });
    collectTaskIdsFromToolResult(record.tasks, collector, depth + 1);
  }

  const looksLikeTask = typeof record.id === 'string'
    && (typeof record.title === 'string' || typeof record.status === 'string');
  if (looksLikeTask) {
    collector.add((record.id as string).trim());
  }

  Object.values(record).forEach((child) => collectTaskIdsFromToolResult(child, collector, depth + 1));
};

export const extractTaskIdsFromToolCall = (toolCall: ToolCallLike): string[] => {
  const output = new Set<string>();
  const candidates = [
    toolCall?.result,
    toolCall?.finalResult,
    parseMaybeJsonValue(toolCall?.result),
    parseMaybeJsonValue(toolCall?.finalResult),
  ];

  candidates.forEach((item) => collectTaskIdsFromToolResult(item, output));
  return Array.from(output);
};
