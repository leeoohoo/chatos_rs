import { debugLog } from '@/lib/utils';

const resolveToolCallId = (value: any): string | null => {
  const id = value?.toolCallId || value?.tool_call_id || value?.id;
  return typeof id === 'string' && id.trim().length > 0 ? id : null;
};

const convertToolCallData = (tc: any, assistantMessageId: string) => ({
  id: tc?.id || tc?.tool_call_id || `tool_${Date.now()}_${Math.random()}`,
  messageId: assistantMessageId,
  name: tc?.function?.name || tc?.name || 'unknown_tool',
  arguments: tc?.function?.arguments || tc?.arguments || '{}',
  result: tc?.result || '',
  finalResult: tc?.finalResult || tc?.final_result || tc?.result || '',
  streamLog: tc?.streamLog || tc?.stream_log || '',
  completed: tc?.completed === true,
  error: tc?.error || undefined,
  createdAt: tc?.createdAt || tc?.created_at || new Date(),
});

export const extractToolCallsFromStartPayload = (data: any): any[] => {
  const rawToolCalls = data?.tool_calls || data;
  return Array.isArray(rawToolCalls) ? rawToolCalls : [rawToolCalls];
};

export const applyToolStartToMessage = (
  message: any,
  toolCallsArray: any[],
  assistantMessageId: string,
): number => {
  if (!message) {
    return 0;
  }
  if (!message.metadata) {
    message.metadata = {} as any;
  }
  if (!message.metadata.toolCalls) {
    message.metadata.toolCalls = [] as any[];
  }

  const segments = message.metadata.contentSegments || [];
  let addedCount = 0;

  toolCallsArray.forEach((tc: any) => {
    const toolCall = convertToolCallData(tc, assistantMessageId);
    message.metadata!.toolCalls!.push(toolCall);
    segments.push({
      content: '',
      type: 'tool_call' as const,
      toolCallId: toolCall.id,
    });
    addedCount += 1;
  });

  segments.push({ content: '', type: 'text' as const });
  message.metadata!.currentSegmentIndex = segments.length - 1;
  return addedCount;
};

export const extractToolResultsFromEndPayload = (data: any): any[] => {
  const rawResults = data?.tool_results || data?.results || data;
  return Array.isArray(rawResults) ? rawResults : (rawResults ? [rawResults] : []);
};

export const applyToolEndResultsToMessage = (message: any, resultsArray: any[]): void => {
  if (!message?.metadata?.toolCalls) {
    return;
  }

  resultsArray.forEach((result: any) => {
    const toolCallId = resolveToolCallId(result);
    if (!toolCallId) {
      return;
    }

    const toolCall = message.metadata!.toolCalls!.find((tc: any) => tc.id === toolCallId);
    if (!toolCall) {
      debugLog('❌ 未找到对应的工具调用:', toolCallId);
      return;
    }

    const resultContent = result?.result || result?.content || result?.output || '';
    if (result?.success === false || result?.is_error === true) {
      toolCall.error = result?.error || resultContent || '工具执行失败';
      toolCall.completed = true;
      return;
    }

    if (typeof resultContent === 'string' && resultContent.length > 0) {
      toolCall.finalResult = resultContent;
      toolCall.result = resultContent;
    } else if (!toolCall.result || toolCall.result.trim() === '') {
      toolCall.result = resultContent;
    }
    toolCall.completed = true;
    if (toolCall.error) {
      delete toolCall.error;
    }
  });
};

export const applyToolStreamDataToMessage = (message: any, data: any): boolean => {
  if (!message?.metadata?.toolCalls) {
    return false;
  }

  const toolCallId = resolveToolCallId(data);
  if (!toolCallId) {
    return false;
  }

  const toolCall = message.metadata.toolCalls.find((tc: any) => tc.id === toolCallId);
  if (!toolCall) {
    return false;
  }

  const rawChunkContent = data?.content || data?.chunk || data?.data || '';
  const chunkContent = typeof rawChunkContent === 'string'
    ? rawChunkContent
    : JSON.stringify(rawChunkContent);
  const isDeltaStream = data?.is_stream === true;

  if (data?.is_error || !data?.success) {
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

export const extractToolCallIdFromStreamData = (data: any): string | null => resolveToolCallId(data);
