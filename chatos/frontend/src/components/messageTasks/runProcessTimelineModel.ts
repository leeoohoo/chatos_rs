// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { MessageTaskRunnerRunEvent } from '../../lib/api/client/types';
import type { MessageToolCallLike } from '../messageItem/messageReaders';
import type { TimelineItem, TimelineStatus } from '../userMessages/ConversationProcessTimelineModel';

type UnknownRecord = Record<string, unknown>;

type ToolResultState = {
  payload: UnknownRecord;
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
): Map<string, ToolResultState> => {
  const results = new Map<string, ToolResultState>();
  events.forEach((event) => {
    if (eventType(event) !== 'tool_stream') {
      return;
    }
    const payload = readRecord(event.payload);
    if (!payload || !isFinalToolResult(payload)) {
      return;
    }
    const callId = readToolResultCallId(payload);
    if (callId) {
      results.set(callId, { payload });
    }
  });
  return results;
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
  if (hasOwn(payload, 'result') && payload.result !== null) {
    return payload.result;
  }
  return payload.content;
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
  const hasResult = Boolean(result);
  const status: TimelineStatus = error
    ? 'error'
    : hasResult
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
    result: result ? toolResultValue(result.payload) : undefined,
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
  return null;
};

const buildUnmatchedToolResultItem = (
  event: MessageTaskRunnerRunEvent,
  payload: UnknownRecord,
): Extract<TimelineItem, { type: 'tool_result' }> => {
  const callId = readToolResultCallId(payload);
  const error = toolResultError(payload);
  return {
    callId,
    createdAt: eventDate(event),
    error,
    hasResult: true,
    id: `run-tool-result-${event.id}`,
    result: toolResultValue(payload),
    status: error ? 'error' : 'completed',
    type: 'tool_result',
  };
};

export const buildRunProcessTimelineItems = (
  events: MessageTaskRunnerRunEvent[],
): TimelineItem[] => {
  const toolResults = buildToolResults(events);
  const knownToolCallIds = buildKnownToolCallIds(events);
  const items: TimelineItem[] = [];
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
        const item = buildToolCallItem(
          event,
          call,
          callIndex,
          callId ? toolResults.get(callId) : undefined,
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
      if (payload && isFinalToolResult(payload) && (!callId || !knownToolCallIds.has(callId))) {
        items.push(buildUnmatchedToolResultItem(event, payload));
      }
      index += 1;
      continue;
    }

    if (type === 'model_request') {
      modelRequestIndex += 1;
    }
    const lifecycleItem = buildLifecycleModelItem(event, modelRequestIndex);
    if (lifecycleItem) {
      items.push(lifecycleItem);
    }
    index += 1;
  }

  return items;
};
