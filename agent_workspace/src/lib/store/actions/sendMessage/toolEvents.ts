import { debugLog } from '@/lib/utils';
import {
  ensureContentSegments,
  ensureStreamingMetadata,
  ensureStreamingToolCalls,
  type RawToolCallPayload,
  type RawToolResultPayload,
  type StreamingMessage,
  type StreamingToolCall,
} from './types';

const resolveToolCallId = (value: unknown): string | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }
  const candidate = value as RawToolResultPayload;
  const id = candidate.toolCallId || candidate.tool_call_id || candidate.id;
  return typeof id === 'string' && id.trim().length > 0 ? id : null;
};

const normalizeToolResultContent = (value: unknown): string => {
  if (typeof value === 'string') {
    return value;
  }
  if (value === undefined || value === null) {
    return '';
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

const toDate = (value: Date | string | undefined): Date => {
  if (value instanceof Date) {
    return value;
  }
  if (typeof value === 'string') {
    const parsed = new Date(value);
    if (!Number.isNaN(parsed.getTime())) {
      return parsed;
    }
  }
  return new Date();
};

const convertToolCallData = (
  tc: RawToolCallPayload,
  assistantMessageId: string,
): StreamingToolCall => ({
  id: tc?.id || tc?.tool_call_id || `tool_${Date.now()}_${Math.random()}`,
  messageId: assistantMessageId,
  name: tc?.function?.name || tc?.name || 'unknown_tool',
  arguments: tc?.function?.arguments || tc?.arguments || '{}',
  result: tc?.result || '',
  finalResult: normalizeToolResultContent(
    tc?.finalResult || tc?.final_result || tc?.result || '',
  ),
  streamLog: tc?.streamLog || tc?.stream_log || '',
  completed: tc?.completed === true,
  error: tc?.error || undefined,
  createdAt: toDate(tc?.createdAt || tc?.created_at),
});

export const extractToolCallsFromStartPayload = (
  data: unknown,
): RawToolCallPayload[] => {
  const rawToolCalls = (
    data && typeof data === 'object' && 'tool_calls' in data
      ? (data as { tool_calls?: unknown }).tool_calls
      : data
  );
  if (Array.isArray(rawToolCalls)) {
    return rawToolCalls as RawToolCallPayload[];
  }
  return rawToolCalls ? [rawToolCalls as RawToolCallPayload] : [];
};

export const applyToolStartToMessage = (
  message: StreamingMessage,
  toolCallsArray: RawToolCallPayload[],
  assistantMessageId: string,
): number => {
  if (!message) {
    return 0;
  }
  const metadata = ensureStreamingMetadata(message);
  const toolCalls = ensureStreamingToolCalls(metadata);
  const segments = ensureContentSegments(metadata);

  let addedCount = 0;

  toolCallsArray.forEach((tc) => {
    const toolCall = convertToolCallData(tc, assistantMessageId);
    toolCalls.push(toolCall);
    segments.push({
      content: '',
      type: 'tool_call' as const,
      toolCallId: toolCall.id,
    });
    addedCount += 1;
  });

  segments.push({ content: '', type: 'text' as const });
  metadata.currentSegmentIndex = segments.length - 1;
  return addedCount;
};

export const extractToolResultsFromEndPayload = (
  data: unknown,
): RawToolResultPayload[] => {
  const rawResults = (
    data && typeof data === 'object'
      ? (
        (data as { tool_results?: unknown }).tool_results
        || (data as { results?: unknown }).results
        || data
      )
      : data
  );
  return Array.isArray(rawResults)
    ? (rawResults as RawToolResultPayload[])
    : (rawResults ? [rawResults as RawToolResultPayload] : []);
};

export const applyToolEndResultsToMessage = (
  message: StreamingMessage,
  resultsArray: RawToolResultPayload[],
): void => {
  const toolCalls = message.metadata
    ? ensureStreamingToolCalls(ensureStreamingMetadata(message))
    : undefined;
  if (!toolCalls) {
    return;
  }

  resultsArray.forEach((result) => {
    const toolCallId = resolveToolCallId(result);
    if (!toolCallId) {
      return;
    }

    const toolCall = toolCalls.find((tc) => tc.id === toolCallId);
    if (!toolCall) {
      debugLog('❌ 未找到对应的工具调用:', toolCallId);
      return;
    }

    const resultContent = normalizeToolResultContent(
      result?.result || result?.content || result?.output || '',
    );
    if (result?.success === false || result?.is_error === true) {
      toolCall.error = result?.error || resultContent || '工具执行失败';
      toolCall.completed = true;
      return;
    }

    if (typeof resultContent === 'string' && resultContent.length > 0) {
      toolCall.finalResult = resultContent;
      toolCall.result = resultContent;
    } else if (normalizeToolResultContent(toolCall.result).trim() === '') {
      toolCall.result = resultContent;
    }
    toolCall.completed = true;
    if (toolCall.error) {
      delete toolCall.error;
    }
  });
};

export const applyToolStreamDataToMessage = (
  message: StreamingMessage,
  data: RawToolResultPayload,
): boolean => {
  const toolCalls = message.metadata
    ? ensureStreamingToolCalls(ensureStreamingMetadata(message))
    : undefined;
  if (!toolCalls) {
    return false;
  }

  const toolCallId = resolveToolCallId(data);
  if (!toolCallId) {
    return false;
  }

  const toolCall = toolCalls.find((tc) => tc.id === toolCallId);
  if (!toolCall) {
    return false;
  }

  const rawChunkContent = data?.content || data?.chunk || data?.data || '';
  const chunkContent = normalizeToolResultContent(rawChunkContent);
  const isDeltaStream = data?.is_stream === true;

  if (data?.is_error === true || data?.success !== true) {
    toolCall.error = chunkContent || '工具执行出错';
    toolCall.completed = true;
    return true;
  }

  if (isDeltaStream) {
    toolCall.streamLog = (toolCall.streamLog || '') + chunkContent;
    toolCall.result = (toolCall.result || '') + chunkContent;
  } else {
    if (typeof chunkContent === 'string' && chunkContent.length > 0) {
      toolCall.finalResult = chunkContent;
    }
    toolCall.result = chunkContent;
    toolCall.completed = true;
  }
  return true;
};

export const extractToolCallIdFromStreamData = (data: unknown): string | null => resolveToolCallId(data);
