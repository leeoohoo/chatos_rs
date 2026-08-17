// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import type { MessageToolCallLike } from '../messageItem/messageReaders';
import type { TimelineItem, TimelineStatus } from '../userMessages/ConversationProcessTimelineModel';

type UnknownRecord = Record<string, unknown>;

type ToolResultState = {
  eventId: string;
  payload: UnknownRecord;
};

type ToolResultIndex = {
  byCallId: Map<string, ToolResultState>;
  legacyByName: Map<string, ToolResultState[]>;
};

const readRecord = (value: unknown): UnknownRecord | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : null
);

const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

const hasOwn = (record: UnknownRecord, key: string): boolean => (
  Object.prototype.hasOwnProperty.call(record, key)
);

const hasDisplayValue = (value: unknown): boolean => {
  if (value === undefined || value === null) {
    return false;
  }
  if (typeof value === 'string') {
    return value.trim().length > 0;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  const record = readRecord(value);
  return record ? Object.keys(record).length > 0 : true;
};

const eventType = (event: MessageTaskRunnerRunEvent): string => (
  readString(event.event_type).toLowerCase()
);

const eventDate = (event: MessageTaskRunnerRunEvent): Date => {
  const parsed = new Date(readString(event.created_at));
  return Number.isNaN(parsed.getTime()) ? new Date(Number.NaN) : parsed;
};

const nestedFunction = (record: UnknownRecord): UnknownRecord | null => (
  readRecord(record.function)
);

const toolCallId = (value: unknown): string => {
  const record = readRecord(value);
  if (!record) {
    return '';
  }
  return readString(
    record.id
    || record.call_id
    || record.tool_call_id
    || record.toolCallId
    || record.toolCallID,
  );
};

const toolCallName = (value: unknown): string => {
  const record = readRecord(value);
  if (!record) {
    return '';
  }
  return readString(nestedFunction(record)?.name || record.name);
};

const normalizeArguments = (value: unknown): MessageToolCallLike['arguments'] => {
  if (typeof value === 'string') {
    return value;
  }
  const record = readRecord(value);
  if (record) {
    return record;
  }
  if (value === undefined || value === null) {
    return {};
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

const toolCallArguments = (value: unknown): MessageToolCallLike['arguments'] => {
  const record = readRecord(value);
  if (!record) {
    return {};
  }
  const fn = nestedFunction(record);
  return normalizeArguments(fn && hasOwn(fn, 'arguments') ? fn.arguments : record.arguments);
};

const readToolCalls = (payload: unknown): unknown[] => {
  if (Array.isArray(payload)) {
    return payload;
  }
  const record = readRecord(payload);
  if (!record) {
    return [];
  }
  for (const key of ['tool_calls', 'toolCalls', 'calls', 'tools']) {
    if (Array.isArray(record[key])) {
      return record[key] as unknown[];
    }
  }
  return record.function || record.name ? [record] : [];
};

const readToolResultCallId = (payload: UnknownRecord): string => readString(
  payload.tool_call_id
  || payload.toolCallId
  || payload.call_id
  || payload.callId
  || payload.id,
);

const isFinalToolResult = (payload: UnknownRecord): boolean => payload.is_stream !== true;

const buildToolResults = (
  events: MessageTaskRunnerRunEvent[],
): ToolResultIndex => {
  const byCallId = new Map<string, ToolResultState>();
  const legacyByName = new Map<string, ToolResultState[]>();
  events.forEach((event) => {
    if (eventType(event) !== 'tool_stream') {
      return;
    }
    const payload = readRecord(event.payload);
    if (!payload || !isFinalToolResult(payload)) {
      return;
    }
    const callId = readToolResultCallId(payload);
    const state = { eventId: event.id, payload };
    if (callId) {
      byCallId.set(callId, state);
      return;
    }
    const name = readString(payload.name);
    if (name) {
      const results = legacyByName.get(name) || [];
      results.push(state);
      legacyByName.set(name, results);
    }
  });
  return { byCallId, legacyByName };
};

const buildKnownToolCallIds = (events: MessageTaskRunnerRunEvent[]): Set<string> => {
  const ids = new Set<string>();
  events.forEach((event) => {
    if (eventType(event) !== 'tools_start') {
      return;
    }
    readToolCalls(event.payload).forEach((call) => {
      const callId = toolCallId(call);
      if (callId) {
        ids.add(callId);
      }
    });
  });
  return ids;
};

const toolResultValue = (payload: UnknownRecord): unknown => {
  if (hasOwn(payload, 'result') && hasDisplayValue(payload.result)) {
    return payload.result;
  }
  if (hasDisplayValue(payload.content)) {
    return payload.content;
  }
  return payload.preview;
};

const stringifyError = (value: unknown): string => {
  const text = readString(value);
  if (text) {
    return text;
  }
  if (value === undefined || value === null) {
    return '工具执行失败';
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

const toolResultError = (payload: UnknownRecord): string => (
  payload.is_error === true || payload.success === false
    ? stringifyError(payload.content ?? payload.result)
    : ''
);

const buildToolCallItem = (
  event: MessageTaskRunnerRunEvent,
  call: unknown,
  index: number,
  result: ToolResultState | undefined,
): Extract<TimelineItem, { type: 'tool_call' }> | null => {
  const name = toolCallName(call);
  const callId = toolCallId(call);
  if (!name && !callId) {
    return null;
  }
  const error = result ? toolResultError(result.payload) : '';
  const resultValue = result ? toolResultValue(result.payload) : undefined;
  const hasResult = hasDisplayValue(resultValue);
  const status: TimelineStatus = error
    ? 'error'
    : result
      ? 'completed'
      : 'pending';
  const createdAt = eventDate(event);
  const toolCall: MessageToolCallLike = {
    id: callId,
    messageId: event.id,
    name,
    arguments: toolCallArguments(call),
    createdAt,
  };

  return {
    createdAt,
    error,
    hasResult,
    id: `run-tool-${event.id}-${callId || index}`,
    result: resultValue,
    status,
    toolCall,
    type: 'tool_call',
  };
};

const extractEventText = (event: MessageTaskRunnerRunEvent): string => {
  const payload = readRecord(event.payload);
  for (const value of [payload?.text, payload?.content, payload?.delta, event.message]) {
    const text = readString(value);
    if (text) {
      return text;
    }
  }
  return '';
};

const stableValueKey = (value: unknown): string => {
  if (Array.isArray(value)) {
    return `[${value.map(stableValueKey).join(',')}]`;
  }
  const record = readRecord(value);
  if (record) {
    return `{${Object.keys(record)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${stableValueKey(record[key])}`)
      .join(',')}}`;
  }
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
};

const repeatedLifecycleKey = (event: MessageTaskRunnerRunEvent): string => (
  `${eventType(event)}\u0000${extractEventText(event)}\u0000${stableValueKey(event.payload)}`
);

const buildModelItem = (
  event: MessageTaskRunnerRunEvent,
  label: string,
  content: string,
  suffix = '',
): Extract<TimelineItem, { type: 'model' }> => ({
  content,
  createdAt: eventDate(event),
  id: `run-model-${event.id}${suffix}`,
  label,
  type: 'model',
});

const buildLifecycleModelItem = (
  event: MessageTaskRunnerRunEvent,
  modelRequestIndex: number,
): Extract<TimelineItem, { type: 'model' }> | null => {
  const type = eventType(event);
  if (type === 'model_request') {
    return buildModelItem(
      event,
      '模型请求',
      extractEventText(event) || `即将发起第 ${modelRequestIndex} 次模型请求`,
    );
  }
  const labelByType: Record<string, string> = {
    queued: '任务入队',
    running: '任务开始执行',
    mcp_runtime: 'MCP 会话已准备',
    tools_end: '工具批次已完成',
    execution_review_checkpoint: '执行检查点',
    completed: '任务已完成',
    succeeded: '任务已完成',
    success: '任务已完成',
    cancelled: '任务已取消',
    canceled: '任务已取消',
  };
  const label = labelByType[type];
  if (label) {
    return buildModelItem(event, label, extractEventText(event) || label);
  }
  return null;
};

const buildLifecycleErrorItem = (
  event: MessageTaskRunnerRunEvent,
): Extract<TimelineItem, { type: 'tool_result' }> | null => {
  const type = eventType(event);
  if (!type.includes('failed') && type !== 'error') {
    return null;
  }
  const error = extractEventText(event) || type.replace(/_/g, ' ');
  return {
    callId: '',
    createdAt: eventDate(event),
    error,
    hasResult: true,
    id: `run-lifecycle-error-${event.id}`,
    result: event.payload,
    status: 'error',
    type: 'tool_result',
  };
};

const buildUnmatchedToolResultItem = (
  event: MessageTaskRunnerRunEvent,
  payload: UnknownRecord,
): Extract<TimelineItem, { type: 'tool_result' }> | null => {
  const callId = readToolResultCallId(payload);
  const error = toolResultError(payload);
  const result = toolResultValue(payload);
  if (!error && !hasDisplayValue(result)) {
    return null;
  }
  return {
    callId,
    createdAt: eventDate(event),
    error,
    hasResult: true,
    id: `run-tool-result-${event.id}`,
    result,
    status: error ? 'error' : 'completed',
    type: 'tool_result',
  };
};

export const buildRunProcessTimelineItems = (
  events: MessageTaskRunnerRunEvent[],
): TimelineItem[] => {
  const toolResults = buildToolResults(events);
  const knownToolCallIds = buildKnownToolCallIds(events);
  const consumedLegacyResultEventIds = new Set<string>();
  const items: TimelineItem[] = [];
  const repeatedLifecycleItems = new Map<string, TimelineItem>();
  let modelRequestIndex = 0;

  for (let index = 0; index < events.length;) {
    const event = events[index];
    const type = eventType(event);

    if (type === 'thinking' || type === 'chunk') {
      const group: MessageTaskRunnerRunEvent[] = [event];
      index += 1;
      while (index < events.length && eventType(events[index]) === type) {
        group.push(events[index]);
        index += 1;
      }
      const content = group.map(extractEventText).filter(Boolean).join('\n\n');
      if (content) {
        items.push(buildModelItem(
          event,
          type === 'thinking' ? '模型思考' : '模型输出',
          content,
          `-${group.length}`,
        ));
      }
      continue;
    }

    if (type === 'tools_start') {
      readToolCalls(event.payload).forEach((call, callIndex) => {
        const callId = toolCallId(call);
        const name = toolCallName(call);
        const legacyResults = name ? toolResults.legacyByName.get(name) : undefined;
        const legacyResult = legacyResults?.find(
          (result) => !consumedLegacyResultEventIds.has(result.eventId),
        );
        if (legacyResult) {
          consumedLegacyResultEventIds.add(legacyResult.eventId);
        }
        const item = buildToolCallItem(
          event,
          call,
          callIndex,
          (callId ? toolResults.byCallId.get(callId) : undefined) || legacyResult,
        );
        if (item) {
          items.push(item);
        }
      });
      index += 1;
      continue;
    }

    if (type === 'tool_stream') {
      const payload = readRecord(event.payload);
      const callId = payload ? readToolResultCallId(payload) : '';
      if (
        payload
        && isFinalToolResult(payload)
        && (!callId || !knownToolCallIds.has(callId))
        && !consumedLegacyResultEventIds.has(event.id)
      ) {
        const item = buildUnmatchedToolResultItem(event, payload);
        if (item) {
          items.push(item);
        }
      }
      index += 1;
      continue;
    }

    if (type === 'model_request') {
      modelRequestIndex += 1;
    }
    const lifecycleErrorItem = buildLifecycleErrorItem(event);
    if (lifecycleErrorItem) {
      const key = repeatedLifecycleKey(event);
      const existing = repeatedLifecycleItems.get(key);
      if (existing && existing.type === 'tool_result') {
        existing.repeatCount = (existing.repeatCount || 1) + 1;
      } else {
        items.push(lifecycleErrorItem);
        repeatedLifecycleItems.set(key, lifecycleErrorItem);
      }
      index += 1;
      continue;
    }
    const lifecycleItem = buildLifecycleModelItem(event, modelRequestIndex);
    if (lifecycleItem) {
      const key = repeatedLifecycleKey(event);
      const existing = repeatedLifecycleItems.get(key);
      if (existing && existing.type === 'model') {
        existing.repeatCount = (existing.repeatCount || 1) + 1;
      } else {
        items.push(lifecycleItem);
        repeatedLifecycleItems.set(key, lifecycleItem);
      }
    }
    index += 1;
  }

  return items;
};
