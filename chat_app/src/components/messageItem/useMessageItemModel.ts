import { useMemo } from 'react';

import type { Message, ToolCall } from '../../types';
import {
  EMPTY_DERIVED_PROCESS_STATS,
  normalizeContentSegmentsForRender,
} from './helpers';
import type { DerivedProcessStats } from './types';
import {
  getMessageContentSegments,
  getMessageHistoryFinalForTurnId,
  getMessageHistoryFinalForUserMessageId,
  getMessageHistoryProcessThinkingCount,
  getMessageHistoryProcessToolCount,
  getMessageHistoryProcessTurnId,
  getMessageHistoryProcessUnavailableToolCount,
  getMessageHistoryProcessUserMessageId,
  getMessageKeepLastN,
  getMessagePrimaryToolCalls,
  hasMessageHistoryProcess,
  isMessageHistoryProcessExpanded,
  isMessageHistoryProcessLoading,
} from './messageReaders';

interface UseMessageItemModelOptions {
  message: Message;
  isStreaming: boolean;
  renderContext: 'chat' | 'process_drawer';
  derivedProcessStatsByUserId?: Map<string, DerivedProcessStats>;
  linkedUserExpandedForAssistant?: boolean;
}

export const useMessageItemModel = ({
  message,
  isStreaming,
  renderContext,
  derivedProcessStatsByUserId,
  linkedUserExpandedForAssistant,
}: UseMessageItemModelOptions) => {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const isSystem = message.role === 'system';
  const isTool = message.role === 'tool';

  const derivedProcessStats = useMemo(() => {
    if (!isUser) {
      return EMPTY_DERIVED_PROCESS_STATS;
    }

    return derivedProcessStatsByUserId?.get(message.id) || EMPTY_DERIVED_PROCESS_STATS;
  }, [
    isUser,
    message.id,
    derivedProcessStatsByUserId,
  ]);

  const hasHistoryProcess = Boolean(
    (isUser && (
      hasMessageHistoryProcess(message)
      || getMessageHistoryProcessToolCount(message) > 0
      || getMessageHistoryProcessThinkingCount(message) > 0
      || isMessageHistoryProcessLoading(message)
    ))
    || derivedProcessStats.hasProcess
    || derivedProcessStats.hasStreamingAssistant
    || derivedProcessStats.processMessageCount > 0
  );
  const historyProcessExpanded = isUser
    ? isMessageHistoryProcessExpanded(message)
    : false;
  const historyProcessLoading = isUser
    ? isMessageHistoryProcessLoading(message)
    : false;
  const historyToolCount = Math.max(
    getMessageHistoryProcessToolCount(message),
    derivedProcessStats.toolCallCount,
  );
  const historyThinkingCount = Math.max(
    getMessageHistoryProcessThinkingCount(message),
    derivedProcessStats.thinkingCount,
  );
  const historyUnavailableToolCount = getMessageHistoryProcessUnavailableToolCount(message);

  const isProcessAssistant = (
    isAssistant
    && Boolean(getMessageHistoryProcessUserMessageId(message) || getMessageHistoryProcessTurnId(message))
  );
  const linkedUserExpandedForFinalAssistant = useMemo(() => {
    if (typeof linkedUserExpandedForAssistant === 'boolean') {
      return linkedUserExpandedForAssistant;
    }
    return false;
  }, [linkedUserExpandedForAssistant]);

  const isTurnLinkedAssistant = (
    isAssistant
    && Boolean(
      getMessageHistoryFinalForUserMessageId(message)
      || getMessageHistoryFinalForTurnId(message)
      || getMessageHistoryProcessUserMessageId(message)
      || getMessageHistoryProcessTurnId(message)
    )
  );
  const collapseAssistantProcessByDefault = (
    isTurnLinkedAssistant
    && !isProcessAssistant
    && !linkedUserExpandedForFinalAssistant
    && renderContext !== 'process_drawer'
  );

  const attachments = message.metadata?.attachments || [];
  const keepLastN = getMessageKeepLastN(message);
  const toolCalls = getMessagePrimaryToolCalls(message);
  const renderContentSegments = useMemo(
    () => normalizeContentSegmentsForRender(getMessageContentSegments(message)),
    [message.metadata?.contentSegments],
  );
  const toolCallsById = useMemo(() => {
    if (!toolCalls || toolCalls.length === 0) return new Map<string, ToolCall>();
    const map = new Map<string, ToolCall>();
    for (const tc of toolCalls) {
      if (tc && tc.id) {
        map.set(tc.id, tc);
      }
    }
    return map;
  }, [toolCalls]);

  const hasVisibleTextSegment = renderContentSegments.some((segment) => (
    segment.type === 'text'
    && typeof segment.content === 'string'
    && segment.content.trim().length > 0
  ));
  const hasVisibleThinkingSegment = renderContentSegments.some((segment) => (
    segment.type === 'thinking'
    && typeof segment.content === 'string'
    && segment.content.trim().length > 0
  ));
  const hasVisibleToolCallSegment = renderContentSegments.some((segment) => segment.type === 'tool_call');
  const shouldHideEmptyStreamingAssistant = Boolean(
    isAssistant
    && isStreaming
    && message.status === 'streaming'
    && (!message.content || message.content.trim().length === 0)
    && !hasVisibleTextSegment
    && !hasVisibleThinkingSegment
    && !hasVisibleToolCallSegment
    && toolCalls.length === 0
  );

  return {
    isUser,
    isAssistant,
    isSystem,
    isTool,
    hasHistoryProcess,
    historyProcessExpanded,
    historyProcessLoading,
    historyToolCount,
    historyThinkingCount,
    historyUnavailableToolCount,
    collapseAssistantProcessByDefault,
    attachments,
    keepLastN,
    toolCalls,
    renderContentSegments,
    toolCallsById,
    shouldHideEmptyStreamingAssistant,
  };
};
