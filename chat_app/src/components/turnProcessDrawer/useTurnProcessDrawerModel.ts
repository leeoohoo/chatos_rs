import { useMemo } from 'react';

import type { ContentSegment, Message, ToolCall } from '../../types';
import {
  buildRenderableToolCall,
  getMessageAllToolCalls,
  getMessageContentSegments,
  getMessageConversationTurnId,
  getMessageHistoryFinalForTurnId,
  getMessageHistoryFinalForUserMessageId,
  getMessageHistoryProcessThinkingCount,
  getMessageHistoryProcessToolCount,
  getMessageHistoryProcessTurnId,
  getMessageHistoryProcessUnavailableToolCount,
  getMessageHistoryProcessUserMessageId,
  getMessagePrimaryToolCalls,
  getMessageToolResultCallId,
  getMessageUnavailableTools,
  isMessageHistoryProcessPlaceholder,
  normalizeMetaId,
  normalizeTurnId,
  type MessageUnavailableToolLike,
} from '../messageItem/messageReaders';

type UnavailableToolViewItem = MessageUnavailableToolLike;

const isToolCallSegment = (segment: ContentSegment): boolean => segment.type === 'tool_call';
const isThinkingSegment = (segment: ContentSegment): boolean => segment.type === 'thinking';
const isContentSegment = (value: unknown): value is ContentSegment => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
);

const buildFallbackProcessMessage = (
  finalAssistantMessage: Message | null,
  userMessageId: string,
  turnId: string,
): Message | null => {
  if (!finalAssistantMessage || finalAssistantMessage.role !== 'assistant') {
    return null;
  }

  const metadata = finalAssistantMessage.metadata || {};
  const toolCalls = getMessagePrimaryToolCalls(finalAssistantMessage);
  const unavailableTools = getMessageUnavailableTools(finalAssistantMessage);
  const segments = getMessageContentSegments(finalAssistantMessage).filter(isContentSegment);
  const processSegments = segments.filter((segment) => (
    isThinkingSegment(segment) || isToolCallSegment(segment)
  ));

  const hasProcessContent = processSegments.length > 0 || toolCalls.length > 0 || unavailableTools.length > 0;
  if (!hasProcessContent) {
    return null;
  }

  const normalizedSegments = processSegments.length > 0
    ? processSegments
    : toolCalls
      .filter((toolCall) => Boolean(toolCall.id))
      .map((toolCall) => ({
        type: 'tool_call' as const,
        toolCallId: String(toolCall.id),
        content: '',
      }));

  if (normalizedSegments.length === 0) {
    return null;
  }

  return {
    ...finalAssistantMessage,
    id: `${finalAssistantMessage.id}__process_fallback`,
    content: '',
    metadata: {
      ...metadata,
      contentSegments: normalizedSegments,
      historyProcessUserMessageId: userMessageId,
      ...(turnId ? { historyProcessTurnId: turnId } : {}),
      historyProcessLoaded: true,
      historyProcessPlaceholder: false,
      ...(unavailableTools.length > 0 ? { unavailableTools } : {}),
    },
  };
};

interface UseTurnProcessDrawerModelOptions {
  panelOpen: boolean;
  userMessageId: string | null;
  messages: Message[];
}

export const useTurnProcessDrawerModel = ({
  panelOpen,
  userMessageId,
  messages,
}: UseTurnProcessDrawerModelOptions) => {
  const userMessage = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return null;
    }
    return messages.find((message) => message.id === userMessageId && message.role === 'user') || null;
  }, [messages, panelOpen, userMessageId]);

  const turnId = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return '';
    }

    const direct = normalizeTurnId(
      (userMessage ? getMessageConversationTurnId(userMessage) : '')
      || (userMessage ? getMessageHistoryProcessTurnId(userMessage) : ''),
    );
    if (direct) {
      return direct;
    }

    const finalAssistant = messages.find((message) => (
      message.role === 'assistant'
      && !getMessageHistoryProcessUserMessageId(message)
      && !getMessageHistoryProcessTurnId(message)
      && getMessageHistoryFinalForUserMessageId(message) === userMessageId
    ));
    return normalizeTurnId(
      (finalAssistant ? getMessageHistoryFinalForTurnId(finalAssistant) : '')
      || (finalAssistant ? getMessageConversationTurnId(finalAssistant) : ''),
    );
  }, [messages, panelOpen, userMessage, userMessageId]);

  const processMessages = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return [] as Message[];
    }

    return messages.filter((message) => (
      (
        getMessageHistoryProcessUserMessageId(message) === userMessageId
        || (turnId && getMessageHistoryProcessTurnId(message) === turnId)
      )
      && !isMessageHistoryProcessPlaceholder(message)
    ));
  }, [messages, panelOpen, turnId, userMessageId]);

  const finalAssistantMessage = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return null;
    }

    return messages.find((message) => (
      message.role === 'assistant' && (
        (
          !getMessageHistoryProcessUserMessageId(message)
          && !getMessageHistoryProcessTurnId(message)
          && getMessageHistoryFinalForUserMessageId(message) === userMessageId
        )
        || (turnId && (
          !getMessageHistoryProcessUserMessageId(message)
          && !getMessageHistoryProcessTurnId(message)
          && (
            getMessageHistoryFinalForTurnId(message) === turnId
            || getMessageConversationTurnId(message) === turnId
          )
        ))
      )
    )) || null;
  }, [messages, panelOpen, turnId, userMessageId]);

  const fallbackProcessMessage = useMemo(() => {
    if (!panelOpen || !userMessageId) {
      return null;
    }
    return buildFallbackProcessMessage(finalAssistantMessage, userMessageId, turnId);
  }, [finalAssistantMessage, panelOpen, turnId, userMessageId]);

  const assistantProcessMessages = useMemo(() => {
    const base = processMessages.filter((message) => message.role === 'assistant');
    if (!fallbackProcessMessage) {
      return base;
    }

    if (base.length === 0) {
      return [fallbackProcessMessage];
    }

    const baseToolCallIds = new Set<string>();
    const baseThinkingContents = new Set<string>();

    base.forEach((message) => {
      const toolCalls = getMessagePrimaryToolCalls(message);
      toolCalls.forEach((toolCall) => {
        const id = normalizeTurnId(toolCall.id);
        if (id) {
          baseToolCallIds.add(id);
        }
      });

      const segments = getMessageContentSegments(message).filter(isContentSegment);
      segments.forEach((segment) => {
        if (isToolCallSegment(segment)) {
          const id = normalizeTurnId(segment.toolCallId);
          if (id) {
            baseToolCallIds.add(id);
          }
          return;
        }
        if (isThinkingSegment(segment) && typeof segment.content === 'string') {
          const key = segment.content.trim();
          if (key) {
            baseThinkingContents.add(key);
          }
        }
      });
    });

    const fallbackSegments = getMessageContentSegments(fallbackProcessMessage).filter(isContentSegment);
    const missingSegments = fallbackSegments.filter((segment) => {
      if (isToolCallSegment(segment)) {
        const id = normalizeTurnId(segment.toolCallId);
        return id ? !baseToolCallIds.has(id) : false;
      }
      if (isThinkingSegment(segment) && typeof segment.content === 'string') {
        const key = segment.content.trim();
        return key ? !baseThinkingContents.has(key) : false;
      }
      return false;
    });

    if (missingSegments.length === 0) {
      return base;
    }

    const missingToolCallIds = new Set(
      missingSegments
        .filter((segment) => isToolCallSegment(segment))
        .map((segment) => normalizeTurnId(segment.toolCallId))
        .filter(Boolean),
    );
    const fallbackToolCalls = getMessagePrimaryToolCalls(fallbackProcessMessage);
    const missingToolCalls = fallbackToolCalls.filter((toolCall) => (
      missingToolCallIds.has(normalizeTurnId(toolCall.id))
    ));

    return [
      ...base,
      {
        ...fallbackProcessMessage,
        id: `${fallbackProcessMessage.id}__delta`,
        metadata: {
          ...(fallbackProcessMessage.metadata || {}),
          contentSegments: missingSegments,
          ...(missingToolCalls.length > 0 ? { toolCalls: missingToolCalls } : {}),
        },
      },
    ];
  }, [fallbackProcessMessage, processMessages]);

  const toolResultById = useMemo(() => {
    const map = new Map<string, Message>();
    for (const message of messages || []) {
      if (message.role !== 'tool') continue;
      const toolCallId = getMessageToolResultCallId(message);
      if (toolCallId) {
        map.set(String(toolCallId), message);
      }
    }
    return map;
  }, [messages]);

  const assistantToolCallsById = useMemo(() => {
    const map = new Map<string, ToolCall>();
    for (const message of messages || []) {
      if (message.role !== 'assistant') continue;
      getMessageAllToolCalls(message).forEach((toolCall) => {
        const id = normalizeMetaId(toolCall.id);
        if (!id || map.has(id)) {
          return;
        }
        map.set(id, buildRenderableToolCall(toolCall, message));
      });
    }
    return map;
  }, [messages]);

  const assistantUnavailableTools = useMemo(() => {
    const deduped = new Map<string, UnavailableToolViewItem>();
    for (const message of assistantProcessMessages) {
      const items = getMessageUnavailableTools(message);
      items.forEach((item) => {
        const key = `${item.serverName}::${item.toolName}::${item.reason}`;
        if (!deduped.has(key)) {
          deduped.set(key, item);
        }
      });
    }
    return Array.from(deduped.values());
  }, [assistantProcessMessages]);

  const historyToolCount = userMessage ? getMessageHistoryProcessToolCount(userMessage) : 0;
  const historyThinkingCount = userMessage ? getMessageHistoryProcessThinkingCount(userMessage) : 0;
  const historyUnavailableCount = Math.max(
    userMessage ? getMessageHistoryProcessUnavailableToolCount(userMessage) : 0,
    assistantUnavailableTools.length,
  );

  return {
    userMessage,
    assistantProcessMessages,
    toolResultById,
    assistantToolCallsById,
    assistantUnavailableTools,
    historyToolCount,
    historyThinkingCount,
    historyUnavailableCount,
  };
};
