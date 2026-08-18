// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useApiClient } from '../../../lib/api/ApiClientContext';
import type { RealtimeChatStreamPayloadWrapper } from '../../../lib/realtime/types';
import { useConversationChatStreamRealtime } from '../../../lib/realtime/useConversationChatStreamRealtime';
import { normalizePersistedMessage } from '../../../lib/store/actions/sendMessage/persistedTurnMessages';
import type { Message } from '../../../types';
import type { MessageToolCallLike } from '../../messageItem/messageReaders';
import {
  buildTimelineItems,
  isProcessMessage,
  readRecord,
  readString,
  type TimelineItem,
} from '../../userMessages/ConversationProcessTimelineModel';

const PLANNER_PROCESS_REFRESH_INTERVAL_MS = 2_000;

const realtimeTimestamp = (payload: RealtimeChatStreamPayloadWrapper): Date => {
  const rawTimestamp = typeof payload.raw?.timestamp === 'string'
    ? payload.raw.timestamp
    : '';
  const timestamp = rawTimestamp ? new Date(rawTimestamp) : new Date();
  return Number.isNaN(timestamp.getTime()) ? new Date() : timestamp;
};

const realtimeEventType = (
  payload: RealtimeChatStreamPayloadWrapper,
  eventName: string,
): string => String(payload.raw?.type || payload.stream_type || eventName || '')
  .trim()
  .toLowerCase();

const readFiniteNumber = (value: unknown, fallback: number): number => {
  const numeric = typeof value === 'number' ? value : Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
};

const toolCallId = (record: Record<string, unknown>): string => readString(
  record.call_id
  || record.tool_call_id
  || record.toolCallId
  || record.id
  || record.invocation_id,
);

const toolCallName = (record: Record<string, unknown>): string => {
  const fn = readRecord(record.function);
  return readString(record.name || fn?.name) || '未知工具';
};

const toolCallArguments = (record: Record<string, unknown>): MessageToolCallLike['arguments'] => {
  const fn = readRecord(record.function);
  const value = record.arguments ?? fn?.arguments;
  return typeof value === 'string' || readRecord(value) ? value as MessageToolCallLike['arguments'] : {};
};

const modelRequestContent = (data: Record<string, unknown>): string => {
  const iteration = Math.max(1, readFiniteNumber(data.iteration, 1));
  const requestAttempt = Math.max(1, readFiniteNumber(data.request_attempt, 1));
  const model = readString(data.model) || '当前模型';
  const inputItemCount = Math.max(0, readFiniteNumber(data.input_item_count, 0));
  const toolCount = Math.max(0, readFiniteNumber(data.tool_count, 0));
  const timeout = Math.max(0, readFiniteNumber(data.read_timeout_seconds, 0));
  return [
    `正在进行第 ${iteration} 轮第 ${requestAttempt} 次流式模型请求（${model}）`,
    `输入项 ${inputItemCount}，可用工具 ${toolCount}`,
    timeout > 0 ? `流读取超时 ${timeout} 秒` : '',
  ].filter(Boolean).join('；');
};

const upsertModelRequest = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
): TimelineItem[] => {
  const data = readRecord(payload.raw?.data);
  if (!data || readString(data.phase).toLowerCase() !== 'model_request') return items;
  const iteration = Math.max(1, readFiniteNumber(data.iteration, 1));
  const requestAttempt = Math.max(1, readFiniteNumber(data.request_attempt, 1));
  const id = `live-model-request-${iteration}-${requestAttempt}`;
  const nextItem: TimelineItem = {
    content: modelRequestContent(data),
    createdAt: realtimeTimestamp(payload),
    id,
    label: '模型请求',
    type: 'model',
  };
  const existingIndex = items.findIndex((item) => item.id === id);
  if (existingIndex < 0) return [...items, nextItem];
  const next = [...items];
  next[existingIndex] = nextItem;
  return next;
};

const appendModelChunk = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
  label: '模型思考' | '模型输出',
): TimelineItem[] => {
  const content = typeof payload.raw?.content === 'string' ? payload.raw.content : '';
  if (!content) return items;
  const last = items[items.length - 1];
  if (last?.type === 'model' && last.label === label && last.status !== 'error') {
    return [
      ...items.slice(0, -1),
      { ...last, content: `${last.content}${content}` },
    ];
  }
  return [...items, {
    content,
    createdAt: realtimeTimestamp(payload),
    id: `live-model-${label === '模型思考' ? 'thinking' : 'output'}-${items.length}`,
    label,
    type: 'model',
  }];
};

const startToolCalls = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
): TimelineItem[] => {
  const data = readRecord(payload.raw?.data);
  const calls = Array.isArray(data?.tool_calls) ? data.tool_calls : [];
  const next = [...items];
  calls.forEach((value, index) => {
    const record = readRecord(value);
    if (!record) return;
    const callId = toolCallId(record) || `anonymous-${next.length}-${index}`;
    const createdAt = realtimeTimestamp(payload);
    const toolCall: MessageToolCallLike = {
      arguments: toolCallArguments(record),
      createdAt,
      id: callId,
      messageId: `live-tool-${callId}`,
      name: toolCallName(record),
    };
    const item: TimelineItem = {
      createdAt,
      error: '',
      hasResult: false,
      id: `live-tool-call-${callId}`,
      result: undefined,
      status: 'pending',
      toolCall,
      type: 'tool_call',
    };
    const existingIndex = next.findIndex((candidate) => (
      candidate.type === 'tool_call' && candidate.toolCall.id === callId
    ));
    if (existingIndex < 0) next.push(item);
    else next[existingIndex] = item;
  });
  return next;
};

const applyToolResult = (
  items: TimelineItem[],
  value: unknown,
  payload: RealtimeChatStreamPayloadWrapper,
  terminal: boolean,
): TimelineItem[] => {
  const record = readRecord(value);
  if (!record) return items;
  const callId = toolCallId(record);
  if (!callId) return items;
  const isError = record.is_error === true || record.success === false;
  const content = typeof record.content === 'string' ? record.content : '';
  const result = record.result !== undefined ? record.result : content;
  const existingIndex = items.findIndex((item) => (
    item.type === 'tool_call' && item.toolCall.id === callId
  ));
  if (existingIndex < 0) {
    return [...items, {
      callId,
      createdAt: realtimeTimestamp(payload),
      error: isError ? content || '工具返回错误' : '',
      hasResult: true,
      id: `live-tool-result-${callId}`,
      result,
      status: isError ? 'error' : (terminal ? 'completed' : 'pending'),
      type: 'tool_result',
    }];
  }
  const item = items[existingIndex];
  if (item.type !== 'tool_call') return items;
  const next = [...items];
  next[existingIndex] = {
    ...item,
    error: isError ? content || '工具返回错误' : '',
    hasResult: true,
    result,
    status: isError ? 'error' : (terminal ? 'completed' : 'pending'),
    toolCall: {
      ...item.toolCall,
      completed: terminal,
      error: isError ? content || '工具返回错误' : undefined,
      finalResult: terminal ? result : item.toolCall.finalResult,
      result,
      streamLog: content || item.toolCall.streamLog,
    },
  };
  return next;
};

const applyToolResults = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
  terminal: boolean,
): TimelineItem[] => {
  const data = readRecord(payload.raw?.data);
  const values = terminal && Array.isArray(data?.tool_results)
    ? data.tool_results
    : [payload.raw?.data];
  return values.reduce(
    (next, value) => applyToolResult(next, value, payload, terminal),
    items,
  );
};

const appendRealtimeError = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
): TimelineItem[] => {
  const data = readRecord(payload.raw?.data);
  const content = readString(payload.raw?.message)
    || readString(data?.message)
    || readString(data?.error)
    || '规划运行失败';
  return [...items, {
    content,
    createdAt: realtimeTimestamp(payload),
    id: `live-error-${items.length}`,
    label: '运行错误',
    status: 'error',
    type: 'model',
  }];
};

export const applyRequirementPlannerRealtimeEvent = (
  items: TimelineItem[],
  payload: RealtimeChatStreamPayloadWrapper,
  eventName: string,
): TimelineItem[] => {
  const eventType = realtimeEventType(payload, eventName);
  if (eventType === 'turn_phase') return upsertModelRequest(items, payload);
  if (eventType === 'chunk') return appendModelChunk(items, payload, '模型输出');
  if (eventType === 'thinking') return appendModelChunk(items, payload, '模型思考');
  if (eventType === 'tools_start') return startToolCalls(items, payload);
  if (eventType === 'tools_stream') return applyToolResults(items, payload, false);
  if (eventType === 'tools_end') return applyToolResults(items, payload, true);
  if (['error', 'failed', 'cancelled', 'canceled'].some((type) => (
    eventType === type || eventType.endsWith(`.${type}`)
  ))) return appendRealtimeError(items, payload);
  return items;
};

export const matchesRequirementPlannerRealtimeEvent = (
  payload: RealtimeChatStreamPayloadWrapper,
  conversationId: string,
  turnId: string,
  userMessageId: string,
): boolean => (
  readString(payload.conversation_id) === conversationId
  && readString(payload.conversation_turn_id) === turnId
  && (!readString(payload.user_message_id) || readString(payload.user_message_id) === userMessageId)
);

const timelineToolCallId = (item: TimelineItem): string => {
  if (item.type === 'tool_call') return readString(item.toolCall.id);
  if (item.type === 'tool_result') return readString(item.callId);
  return '';
};

export const mergeRequirementPlannerTimelineItems = (
  persistedItems: TimelineItem[],
  liveItems: TimelineItem[],
): TimelineItem[] => {
  const merged = [...persistedItems];
  const persistedModelContents = persistedItems
    .filter((item): item is Extract<TimelineItem, { type: 'model' }> => item.type === 'model')
    .map((item) => item.content);
  liveItems.forEach((liveItem) => {
    if (liveItem.type === 'model') {
      const persisted = ['模型输出', '模型思考'].includes(liveItem.label)
        && persistedModelContents.some((content) => content.includes(liveItem.content));
      if (!persisted) merged.push(liveItem);
      return;
    }
    const callId = timelineToolCallId(liveItem);
    const persistedIndex = callId ? merged.findIndex((item) => timelineToolCallId(item) === callId) : -1;
    if (persistedIndex < 0) {
      merged.push(liveItem);
      return;
    }
    const persistedItem = merged[persistedIndex];
    if ('status' in persistedItem && persistedItem.status === 'pending'
      && liveItem.status !== 'pending') {
      merged[persistedIndex] = liveItem;
    }
  });
  return Array.from(new Map(merged.map((item) => [item.id, item])).values())
    .sort((left, right) => left.createdAt.getTime() - right.createdAt.getTime());
};

export interface RequirementExecutionPlannerTimelineState {
  error: string | null;
  items: TimelineItem[];
  loading: boolean;
  processMessageCount: number;
  refresh: (silent?: boolean) => Promise<void>;
}

export const isRequirementExecutionPlannerTimelineMessage = (message: Message): boolean => (
  isProcessMessage(message)
  || message.role === 'assistant'
  || message.role === 'tool'
);

export const useRequirementExecutionPlannerTimeline = ({
  active,
  conversationId,
  turnId,
  userMessageId,
}: {
  active: boolean;
  conversationId: string;
  turnId: string;
  userMessageId: string;
}): RequirementExecutionPlannerTimelineState => {
  const apiClient = useApiClient();
  const apiClientRef = useRef(apiClient);
  const timelineIdentity = `${conversationId}\u0000${turnId}\u0000${userMessageId}`;
  const [messageSnapshot, setMessageSnapshot] = useState<{
    identity: string;
    messages: Message[];
  }>({ identity: timelineIdentity, messages: [] });
  const [loading, setLoading] = useState(true);
  const [liveTimelineSnapshot, setLiveTimelineSnapshot] = useState<{
    identity: string;
    items: TimelineItem[];
  }>({ identity: timelineIdentity, items: [] });
  const [error, setError] = useState<string | null>(null);
  const requestSequenceRef = useRef(0);
  const previousActiveRef = useRef(active);

  useEffect(() => {
    apiClientRef.current = apiClient;
  }, [apiClient]);

  const refresh = useCallback(async (silent = false) => {
    const requestSequence = requestSequenceRef.current + 1;
    requestSequenceRef.current = requestSequence;
    if (!silent) setLoading(true);
    try {
      const response = await apiClientRef.current.getConversationTurnMessagesByTurn(
        conversationId,
        turnId,
      );
      if (requestSequenceRef.current !== requestSequence) return;
      const normalized = (Array.isArray(response) ? response : [])
        .map((rawMessage) => normalizePersistedMessage(rawMessage, conversationId))
        .filter((message): message is Message => message !== null);
      setMessageSnapshot({ identity: timelineIdentity, messages: normalized });
      setError(null);
    } catch (err) {
      if (requestSequenceRef.current !== requestSequence) return;
      setError(err instanceof Error ? err.message : '读取规划运行过程失败');
    } finally {
      if (requestSequenceRef.current === requestSequence) {
        setLoading(false);
      }
    }
  }, [conversationId, timelineIdentity, turnId]);

  useEffect(() => {
    requestSequenceRef.current += 1;
    setMessageSnapshot({ identity: timelineIdentity, messages: [] });
    setLiveTimelineSnapshot({ identity: timelineIdentity, items: [] });
    setError(null);
    setLoading(true);
    void refresh(false);
  }, [conversationId, refresh, timelineIdentity, turnId]);

  useEffect(() => {
    const wasActive = previousActiveRef.current;
    previousActiveRef.current = active;
    if (wasActive && !active) {
      void refresh(true);
    }
  }, [active, refresh]);

  useEffect(() => {
    if (!active || !conversationId || !turnId || !userMessageId) return undefined;
    const intervalId = window.setInterval(() => {
      void refresh(true);
    }, PLANNER_PROCESS_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [active, conversationId, refresh, turnId, userMessageId]);

  const onRealtimeEvent = useCallback((
    payload: RealtimeChatStreamPayloadWrapper,
    eventName: string,
  ) => {
    if (!matchesRequirementPlannerRealtimeEvent(
      payload,
      conversationId,
      turnId,
      userMessageId,
    )) return;
    setLiveTimelineSnapshot((snapshot) => ({
      identity: timelineIdentity,
      items: applyRequirementPlannerRealtimeEvent(
        snapshot.identity === timelineIdentity ? snapshot.items : [],
        payload,
        eventName,
      ),
    }));
  }, [conversationId, timelineIdentity, turnId, userMessageId]);

  useConversationChatStreamRealtime({
    enabled: active && Boolean(conversationId && turnId && userMessageId),
    onEvent: onRealtimeEvent,
    sessionId: conversationId,
  });

  const messages = messageSnapshot.identity === timelineIdentity
    ? messageSnapshot.messages
    : [];
  const processMessages = useMemo(
    () => messages.filter(isRequirementExecutionPlannerTimelineMessage),
    [messages],
  );
  const persistedItems = useMemo(
    () => buildTimelineItems(processMessages),
    [processMessages],
  );
  const liveItems = liveTimelineSnapshot.identity === timelineIdentity
    ? liveTimelineSnapshot.items
    : [];
  const items = useMemo(
    () => mergeRequirementPlannerTimelineItems(persistedItems, liveItems),
    [liveItems, persistedItems],
  );

  return {
    error: messageSnapshot.identity === timelineIdentity ? error : null,
    items,
    loading: messageSnapshot.identity === timelineIdentity ? loading : true,
    processMessageCount: items.length,
    refresh,
  };
};
