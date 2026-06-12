import { useMemo } from 'react';

import type { Message, ToolCall } from '../../types';
import {
  isTaskRunnerAsyncPlanMessage,
  isTaskRunnerCallbackMessage,
} from '../../lib/domain/messages';
import {
  EMPTY_DERIVED_PROCESS_STATS,
  getCollapsedTextContentForRender,
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
} from './messageReaders';

interface UseMessageItemModelOptions {
  message: Message;
  isStreaming: boolean;
  derivedProcessStatsByUserId?: Map<string, DerivedProcessStats>;
}

export const useMessageItemModel = ({
  message,
  isStreaming,
  derivedProcessStatsByUserId,
}: UseMessageItemModelOptions) => {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const isSystem = message.role === 'system';
  const isTool = message.role === 'tool';
  const isTaskRunnerAsyncAssistant = Boolean(
    isAssistant
    && (
      isTaskRunnerCallbackMessage(message)
      || isTaskRunnerAsyncPlanMessage(message)
    ),
  );

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
    ))
    || derivedProcessStats.hasProcess
    || derivedProcessStats.hasStreamingAssistant
    || derivedProcessStats.processMessageCount > 0
  );
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
    && !isTaskRunnerAsyncAssistant
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
  const reviewOutcomeRaw = typeof message.metadata?.task_turn_review === 'object'
    ? (message.metadata?.task_turn_review as { outcome?: unknown }).outcome
    : null;
  const reviewOutcome = typeof reviewOutcomeRaw === 'string'
    ? reviewOutcomeRaw.trim().toLowerCase()
    : '';
  const hasVisibleReviewOutcome = (
    reviewOutcome === 'pass'
    || reviewOutcome === 'needs_more_work'
    || reviewOutcome === 'unknown'
    || reviewOutcome === 'not_attempted'
  );
  const hasVisibleCollapsedAssistantText = (
    collapseAssistantProcessByDefault
      ? getCollapsedTextContentForRender(renderContentSegments).trim().length > 0
      : false
  );
  const hasRenderableAssistantBody = renderContentSegments.length > 0
    ? (
      collapseAssistantProcessByDefault
        ? (hasVisibleCollapsedAssistantText || hasVisibleReviewOutcome)
        : (
          hasVisibleTextSegment
          || hasVisibleThinkingSegment
          || hasVisibleToolCallSegment
          || hasVisibleReviewOutcome
        )
    )
    : (
      (typeof message.content === 'string' && message.content.trim().length > 0)
      || (!collapseAssistantProcessByDefault && toolCalls.length > 0)
      || hasVisibleReviewOutcome
    );
  const shouldHideEmptyStreamingAssistant = Boolean(
    isAssistant
    && (isStreaming || isTaskRunnerAsyncAssistant)
    && message.status === 'streaming'
    && (!message.content || message.content.trim().length === 0)
    && !hasVisibleTextSegment
    && !hasVisibleThinkingSegment
    && !hasVisibleToolCallSegment
    && toolCalls.length === 0
  );
  const shouldHideEmptyNonTaskRunnerAssistant = Boolean(
    isAssistant
    && !isTaskRunnerAsyncAssistant
    && !hasRenderableAssistantBody
  );

  return {
    isUser,
    isAssistant,
    isSystem,
    isTool,
    isTaskRunnerAsyncAssistant,
    hasHistoryProcess,
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
    shouldHideEmptyNonTaskRunnerAssistant,
  };
};
