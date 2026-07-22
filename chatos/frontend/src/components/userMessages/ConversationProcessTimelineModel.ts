// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import {
  getMessagePrimaryToolCalls,
  getMessageToolResultCallId,
  type MessageToolCallLike,
} from '../messageItem/messageReaders';
import type { Message } from '../../types';
import type { UserMessageTurn } from './types';

export const readRecord = (value: unknown): Record<string, unknown> | null => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
);

export const readString = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const formatTime = (date: Date): string => {
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) {
    return '-';
  }
  return date.toLocaleString();
};

const hasOwn = (record: Record<string, unknown>, key: string): boolean => (
  Object.prototype.hasOwnProperty.call(record, key)
);

const isDefined = (value: unknown): boolean => value !== undefined && value !== null;

export const isProcessMessage = (message: Message): boolean => {
  const metadata = readRecord(message.metadata);
  if (metadata?.historyProcessLoaded === true) {
    return true;
  }
  if (metadata?.historyProcessPlaceholder === false && readString(metadata?.historyProcessUserMessageId)) {
    return true;
  }
  return false;
};

export const selectTurnProcessMessages = (
  messages: Message[],
  item: UserMessageTurn | null,
): Message[] => {
  if (!item) {
    return [];
  }
  const userMessageId = item.userMessage.id;
  const finalAssistantMessageId = item.finalAssistantMessage?.id || '';
  const selected = new Map<string, Message>();

  messages.forEach((message) => {
    const withinTurnBoundary = (
      message.id !== userMessageId
      && message.id !== finalAssistantMessageId
      && (message.role === 'assistant' || message.role === 'tool')
    );
    if (isProcessMessage(message) || withinTurnBoundary) {
      selected.set(message.id, message);
    }
  });

  return Array.from(selected.values()).sort(
    (left, right) => left.createdAt.getTime() - right.createdAt.getTime(),
  );
};

const readTaskRunnerMessageKind = (message: Message): string => {
  const metadata = readRecord(message.metadata);
  const taskRunnerAsync = readRecord(metadata?.task_runner_async);
  return readString(taskRunnerAsync?.message_kind);
};

const processLabel = (message: Message): string => {
  const messageKind = readTaskRunnerMessageKind(message);
  if (messageKind === 'task_terminal_update') {
    return '任务状态更新';
  }
  if (message.role === 'tool') {
    return '工具结果';
  }
  if (message.role === 'assistant') {
    return '模型过程';
  }
  return message.role;
};

type TextSegment = {
  content: string;
  type: string;
};

const readContentSegments = (message: Message): TextSegment[] => {
  const metadata = readRecord(message.metadata);
  const rawSegments = Array.isArray(metadata?.contentSegments)
    ? metadata.contentSegments
    : Array.isArray(metadata?.content_segments)
      ? metadata.content_segments
      : [];

  return rawSegments
    .map((segment) => {
      const record = readRecord(segment);
      const content = readString(record?.content);
      const type = readString(record?.type);
      return content ? { content, type } : null;
    })
    .filter((segment): segment is TextSegment => segment !== null);
};

const processContentItems = (message: Message): Array<{ content: string; label: string }> => {
  const segmentItems = readContentSegments(message)
    .filter((segment) => segment.type === 'text' || segment.type === 'thinking')
    .map((segment) => ({
      content: segment.content,
      label: segment.type === 'thinking' ? '模型思考' : processLabel(message),
    }));
  if (segmentItems.length > 0) {
    return segmentItems;
  }
  const content = readString(message.content);
  return content ? [{ content, label: processLabel(message) }] : [];
};

const readToolMessageResult = (message: Message): unknown => {
  const metadata = readRecord(message.metadata);
  if (metadata && hasOwn(metadata, 'structured_result')) {
    return metadata.structured_result;
  }
  if (metadata && hasOwn(metadata, 'structuredResult')) {
    return metadata.structuredResult;
  }
  return message.content;
};

const readToolMessageError = (message: Message | undefined): string => {
  if (!message) {
    return '';
  }
  const metadata = readRecord(message.metadata);
  const isError = metadata?.isError === true || metadata?.is_error === true;
  return isError ? readString(message.content) || '工具返回错误' : '';
};

export type TimelineStatus = 'completed' | 'error' | 'pending';

export type TimelineItem =
  | {
    content: string;
    createdAt: Date;
    id: string;
    label: string;
    type: 'model';
  }
  | {
    createdAt: Date;
    error: string;
    hasResult: boolean;
    id: string;
    result: unknown;
    resultMessage?: Message;
    status: TimelineStatus;
    toolCall: MessageToolCallLike;
    type: 'tool_call';
  }
  | {
    callId: string;
    createdAt: Date;
    error: string;
    hasResult: boolean;
    id: string;
    result: unknown;
    status: TimelineStatus;
    type: 'tool_result';
  };

const resolveToolCallResult = (
  toolCall: MessageToolCallLike,
  resultMessage: Message | undefined,
): { hasResult: boolean; result: unknown } => {
  if (isDefined(toolCall.finalResult)) {
    return { hasResult: true, result: toolCall.finalResult };
  }
  if (isDefined(toolCall.result)) {
    return { hasResult: true, result: toolCall.result };
  }
  if (resultMessage) {
    return { hasResult: true, result: readToolMessageResult(resultMessage) };
  }
  return { hasResult: false, result: undefined };
};

export const buildTimelineItems = (processMessages: Message[]): TimelineItem[] => {
  const toolResultByCallId = new Map<string, Message>();
  const knownToolCallIds = new Set<string>();

  processMessages.forEach((message) => {
    if (message.role === 'tool') {
      const toolCallId = getMessageToolResultCallId(message);
      if (toolCallId && !toolResultByCallId.has(toolCallId)) {
        toolResultByCallId.set(toolCallId, message);
      }
      return;
    }

    getMessagePrimaryToolCalls(message).forEach((toolCall) => {
      if (toolCall.id) {
        knownToolCallIds.add(toolCall.id);
      }
    });
  });

  return processMessages.flatMap((message): TimelineItem[] => {
    if (message.role === 'tool') {
      const callId = getMessageToolResultCallId(message);
      if (callId && knownToolCallIds.has(callId)) {
        return [];
      }
      const error = readToolMessageError(message);
      return [{
        callId,
        createdAt: message.createdAt,
        error,
        hasResult: true,
        id: `tool-result-${message.id}`,
        result: readToolMessageResult(message),
        status: error ? 'error' : 'completed',
        type: 'tool_result',
      }];
    }

    const items: TimelineItem[] = [];
    processContentItems(message).forEach((contentItem, index) => {
      items.push({
        content: contentItem.content,
        createdAt: message.createdAt,
        id: `model-${message.id}-${index}`,
        label: contentItem.label,
        type: 'model',
      });
    });

    getMessagePrimaryToolCalls(message)
      .filter((toolCall) => toolCall.id || toolCall.name)
      .forEach((toolCall, index) => {
        const resultMessage = toolCall.id ? toolResultByCallId.get(toolCall.id) : undefined;
        const { hasResult, result } = resolveToolCallResult(toolCall, resultMessage);
        const error = toolCall.error || readToolMessageError(resultMessage);
        const status: TimelineStatus = error
          ? 'error'
          : (hasResult || toolCall.completed === true ? 'completed' : 'pending');

        items.push({
          createdAt: toolCall.createdAt || message.createdAt,
          error,
          hasResult,
          id: `tool-call-${message.id}-${toolCall.id || index}`,
          result,
          resultMessage,
          status,
          toolCall,
          type: 'tool_call',
        });
      });

    return items;
  });
};

export type TimelineSummary = {
  error: number;
  model: number;
  toolCall: number;
  toolResult: number;
};

export const buildTimelineSummary = (timelineItems: TimelineItem[]): TimelineSummary => (
  timelineItems.reduce(
    (acc, timelineItem) => {
      if (timelineItem.type === 'model') {
        acc.model += 1;
      }
      if (timelineItem.type === 'tool_call') {
        acc.toolCall += 1;
        if (timelineItem.hasResult) {
          acc.toolResult += 1;
        }
      }
      if (timelineItem.type === 'tool_result') {
        acc.toolResult += 1;
      }
      if ('status' in timelineItem && timelineItem.status === 'error') {
        acc.error += 1;
      }
      return acc;
    },
    { error: 0, model: 0, toolCall: 0, toolResult: 0 },
  )
);

export type DisplayValue = {
  kind: 'empty' | 'json' | 'text';
  text: string;
};

const parseJsonText = (value: string): { parsed: boolean; value: unknown } => {
  const trimmed = value.trim();
  if (!trimmed.startsWith('{') && !trimmed.startsWith('[')) {
    return { parsed: false, value };
  }
  try {
    return { parsed: true, value: JSON.parse(trimmed) };
  } catch {
    return { parsed: false, value };
  }
};

const stringifyValue = (value: unknown): string => {
  try {
    const formatted = JSON.stringify(value, null, 2);
    return formatted ?? String(value);
  } catch {
    return String(value);
  }
};

const clipSummary = (value: string, limit = 96): string => {
  const normalized = value.replace(/\s+/g, ' ').trim();
  return normalized.length > limit ? `${normalized.slice(0, limit)}...` : normalized;
};

type DisplayTextOptions = {
  emptyArrayText: string;
  emptyObjectText: string;
  emptyText: string;
};

export const buildDisplayValue = (
  value: unknown,
  options: DisplayTextOptions,
): DisplayValue => {
  if (value === undefined || value === null) {
    return { kind: 'empty', text: options.emptyText };
  }

  let normalized: unknown = value;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) {
      return { kind: 'empty', text: options.emptyText };
    }
    const parsed = parseJsonText(trimmed);
    if (!parsed.parsed) {
      return { kind: 'text', text: trimmed };
    }
    normalized = parsed.value;
  }

  if (Array.isArray(normalized)) {
    if (normalized.length === 0) {
      return { kind: 'empty', text: options.emptyArrayText };
    }
    return { kind: 'json', text: stringifyValue(normalized) };
  }

  const record = readRecord(normalized);
  if (record) {
    if (Object.keys(record).length === 0) {
      return { kind: 'empty', text: options.emptyObjectText };
    }
    return { kind: 'json', text: stringifyValue(record) };
  }

  return { kind: 'text', text: String(normalized) };
};

export const buildValueSummary = (
  value: unknown,
  options: DisplayTextOptions,
): string => {
  if (value === undefined || value === null) {
    return options.emptyText;
  }

  let normalized: unknown = value;
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) {
      return options.emptyText;
    }
    const parsed = parseJsonText(trimmed);
    if (!parsed.parsed) {
      return clipSummary(trimmed);
    }
    normalized = parsed.value;
  }

  if (Array.isArray(normalized)) {
    return normalized.length === 0 ? options.emptyArrayText : `数组 ${normalized.length} 项`;
  }

  const record = readRecord(normalized);
  if (record) {
    const fieldCount = Object.keys(record).length;
    return fieldCount === 0 ? options.emptyObjectText : `对象 ${fieldCount} 个字段`;
  }

  return clipSummary(String(normalized));
};
