import type { ContentSegment, Message } from '../../types';
import type { DerivedProcessStats } from '../messageItem/types';
import {
  getMessageAllToolCalls,
  getMessageContentSegments,
  getMessageConversationTurnId,
  getMessageHistoryFinalForTurnId,
  getMessageHistoryFinalForUserMessageId,
  getMessageHistoryProcessFinalAssistantMessageId,
  getMessageHistoryProcessTurnId,
  getMessageHistoryProcessUserMessageId,
  getMessageMetadataToolCalls,
  getMessageMetadataRecord,
  getMessageToolResultCallId,
  isMessageHistoryProcessExpanded,
  isMessageHistoryProcessPlaceholder,
  normalizeMetaId,
  normalizeTurnId,
  type MessageToolCallLike,
} from '../messageItem/messageReaders';

type ParsedMessageForList = {
  message: Message;
  id: string;
  role: Message['role'];
  status: string;
  visible: boolean;
  time: number;
  metadataHidden: boolean;
  segments: ContentSegment[];
  metadataToolCallCount: number;
  assistantToolCalls: Array<{ id: string; toolCall: MessageToolCallLike }>;
  toolResultCallId: string;
  thinkingSegmentCount: number;
  toolCallSegmentCount: number;
  conversationTurnId: string;
  historyProcessTurnId: string;
  historyProcessUserMessageId: string;
  historyFinalForUserMessageId: string;
  historyFinalForTurnId: string;
  historyProcessPlaceholder: boolean;
  userExpanded: boolean;
  userTurnId: string;
  userFinalAssistantMessageId: string;
};

export type ParsedMessageCacheEntry = {
  ref: Message;
  metadataRef: unknown;
  content: string;
  status: unknown;
  updatedAt: unknown;
  parsed: ParsedMessageForList;
};

const getTimeValue = (value: unknown): number => {
  if (!value) return 0;
  if (value instanceof Date) return value.getTime();
  if (typeof value !== 'string' && typeof value !== 'number') return 0;
  const parsed = new Date(value).getTime();
  return Number.isNaN(parsed) ? 0 : parsed;
};

const isContentSegment = (value: unknown): value is ContentSegment => (
  value !== null && typeof value === 'object' && !Array.isArray(value)
);

const readMessageContentLength = (message: Message): number => (
  typeof message?.content === 'string' ? message.content.trim().length : 0
);

const readNonProcessAssistantDedupKey = (parsed: ParsedMessageForList): string => {
  if (parsed.role !== 'assistant') {
    return '';
  }
  if (parsed.historyProcessUserMessageId || parsed.historyProcessTurnId) {
    return '';
  }

  const turnId = normalizeTurnId(parsed.historyFinalForTurnId || parsed.conversationTurnId);
  if (turnId) {
    return `turn:${turnId}`;
  }

  const userId = normalizeMetaId(parsed.historyFinalForUserMessageId);
  if (userId) {
    return `user:${userId}`;
  }

  return '';
};

export const parseMessageForList = (message: Message): ParsedMessageForList => {
  const metadataRecord = getMessageMetadataRecord(message);
  const segments = getMessageContentSegments(message).filter(isContentSegment);
  const allToolCalls = getMessageAllToolCalls(message);
  const metadataToolCallCount = getMessageMetadataToolCalls(message).length;
  const assistantToolCalls: Array<{ id: string; toolCall: MessageToolCallLike }> = [];

  if (message.role === 'assistant') {
    allToolCalls.forEach((toolCall) => {
      const id = normalizeMetaId(toolCall.id);
      if (id) {
        assistantToolCalls.push({ id, toolCall });
      }
    });
  }

  let thinkingSegmentCount = 0;
  let toolCallSegmentCount = 0;
  if (message.role === 'assistant') {
    segments.forEach((segment) => {
      if (
        segment.type === 'thinking'
        && typeof segment.content === 'string'
        && segment.content.trim().length > 0
      ) {
        thinkingSegmentCount += 1;
        return;
      }
      if (segment.type === 'tool_call' && Boolean(segment.toolCallId)) {
        toolCallSegmentCount += 1;
      }
    });
  }

  const conversationTurnId = getMessageConversationTurnId(message);
  const historyProcessTurnId = getMessageHistoryProcessTurnId(message);
  const historyProcessUserMessageId = getMessageHistoryProcessUserMessageId(message);
  const historyFinalForUserMessageId = getMessageHistoryFinalForUserMessageId(message);
  const historyFinalForTurnId = getMessageHistoryFinalForTurnId(message);
  const historyProcessPlaceholder = isMessageHistoryProcessPlaceholder(message);
  const userExpanded = isMessageHistoryProcessExpanded(message);
  const userTurnId = normalizeTurnId(
    conversationTurnId || historyProcessTurnId,
  );
  const userFinalAssistantMessageId = getMessageHistoryProcessFinalAssistantMessageId(message);

  return {
    message,
    id: message.id,
    role: message.role,
    status: String(message.status || ''),
    visible: metadataRecord?.hidden !== true && message.role !== 'tool',
    time: message.updatedAt ? getTimeValue(message.updatedAt) : getTimeValue(message.createdAt),
    metadataHidden: metadataRecord?.hidden === true,
    segments,
    metadataToolCallCount,
    assistantToolCalls,
    toolResultCallId: getMessageToolResultCallId(message),
    thinkingSegmentCount,
    toolCallSegmentCount,
    conversationTurnId,
    historyProcessTurnId,
    historyProcessUserMessageId,
    historyFinalForUserMessageId,
    historyFinalForTurnId,
    historyProcessPlaceholder,
    userExpanded,
    userTurnId,
    userFinalAssistantMessageId,
  };
};

export const buildVisibleMessageState = (parsedMessages: ParsedMessageForList[]) => {
  const visibleCandidates: ParsedMessageForList[] = [];
  const toolResultMap = new Map<string, Message>();
  const toolResultMetaMap = new Map<string, { id: string; time: number }>();
  const assistantToolById = new Map<string, MessageToolCallLike>();
  const assistantToolMetaById = new Map<string, { messageId: string; time: number }>();

  const signalMap = new Map<string, string>();
  const userMessageIds = new Set<string>();
  const turnToUserMessageId = new Map<string, string>();
  const assistantIdToUserMessageId = new Map<string, string>();
  const mutableStats = new Map<string, {
    hasStreamingAssistant: boolean;
    thinkingCount: number;
    processMessageCount: number;
    toolCallIds: Set<string>;
  }>();

  const userExpandedById = new Map<string, boolean>();
  const turnExpandedById = new Map<string, boolean>();
  const finalAssistantExpandedById = new Map<string, boolean>();

  parsedMessages.forEach((parsed) => {
    if (parsed.visible) {
      visibleCandidates.push(parsed);
    }

    if (parsed.toolResultCallId) {
      toolResultMap.set(parsed.toolResultCallId, parsed.message);
      toolResultMetaMap.set(parsed.toolResultCallId, { id: parsed.id, time: parsed.time });
    }

    if (parsed.role === 'assistant') {
      parsed.assistantToolCalls.forEach(({ id, toolCall }) => {
        if (!assistantToolById.has(id)) {
          assistantToolById.set(id, toolCall);
        }
        if (!assistantToolMetaById.has(id)) {
          assistantToolMetaById.set(id, { messageId: parsed.id, time: parsed.time });
        }
      });
    }

    if (parsed.role === 'user') {
      userMessageIds.add(parsed.id);
      signalMap.set(parsed.id, '');
      mutableStats.set(parsed.id, {
        hasStreamingAssistant: false,
        thinkingCount: 0,
        processMessageCount: 0,
        toolCallIds: new Set<string>(),
      });

      if (parsed.userTurnId && !turnToUserMessageId.has(parsed.userTurnId)) {
        turnToUserMessageId.set(parsed.userTurnId, parsed.id);
      }
      if (
        parsed.userFinalAssistantMessageId
        && !assistantIdToUserMessageId.has(parsed.userFinalAssistantMessageId)
      ) {
        assistantIdToUserMessageId.set(parsed.userFinalAssistantMessageId, parsed.id);
      }

      userExpandedById.set(parsed.id, parsed.userExpanded);
      if (parsed.userTurnId) {
        turnExpandedById.set(parsed.userTurnId, parsed.userExpanded);
      }
      if (parsed.userFinalAssistantMessageId) {
        finalAssistantExpandedById.set(parsed.userFinalAssistantMessageId, parsed.userExpanded);
      }
    }
  });

  const appendSignal = (userMessageId: string, piece: string) => {
    if (!userMessageId || !piece) {
      return;
    }
    const prev = signalMap.get(userMessageId) || '';
    signalMap.set(userMessageId, prev ? `${prev}||${piece}` : piece);
  };

  const resolveLinkedUserMessageId = (parsed: ParsedMessageForList): string => {
    if (parsed.role === 'assistant') {
      let linkedUserMessageId = parsed.historyProcessUserMessageId;
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        const processTurnId = parsed.historyProcessTurnId || parsed.conversationTurnId;
        if (processTurnId) {
          linkedUserMessageId = turnToUserMessageId.get(processTurnId) || '';
        }
      }
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        linkedUserMessageId = parsed.historyFinalForUserMessageId;
      }
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        const finalTurnId = parsed.historyFinalForTurnId || parsed.conversationTurnId;
        if (finalTurnId) {
          linkedUserMessageId = turnToUserMessageId.get(finalTurnId) || '';
        }
      }
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        linkedUserMessageId = assistantIdToUserMessageId.get(parsed.id) || '';
      }
      return userMessageIds.has(linkedUserMessageId) ? linkedUserMessageId : '';
    }

    let linkedUserMessageId = parsed.historyProcessUserMessageId;
    if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
      const processTurnId = parsed.historyProcessTurnId || parsed.conversationTurnId;
      if (processTurnId) {
        linkedUserMessageId = turnToUserMessageId.get(processTurnId) || '';
      }
    }
    return userMessageIds.has(linkedUserMessageId) ? linkedUserMessageId : '';
  };

  parsedMessages.forEach((parsed) => {
    const linkedUserMessageId = resolveLinkedUserMessageId(parsed);
    if (!linkedUserMessageId) {
      return;
    }

    if (parsed.role === 'assistant') {
      appendSignal(
        linkedUserMessageId,
        `A:${parsed.id}:${parsed.status}:${parsed.metadataToolCallCount}:${parsed.toolCallSegmentCount}:${parsed.thinkingSegmentCount}:${parsed.segments.length}`,
      );

      const stats = mutableStats.get(linkedUserMessageId);
      if (!stats) {
        return;
      }

      if (parsed.status === 'streaming') {
        stats.hasStreamingAssistant = true;
      }

      const isProcessAssistant = Boolean(parsed.historyProcessUserMessageId || parsed.historyProcessTurnId);
      if (isProcessAssistant && !parsed.historyProcessPlaceholder) {
        stats.processMessageCount += 1;
      }

      parsed.assistantToolCalls.forEach(({ id }) => {
        stats.toolCallIds.add(id);
      });

      parsed.segments.forEach((segment) => {
        if (segment.type === 'tool_call') {
          const id = normalizeMetaId(segment.toolCallId);
          if (id) {
            stats.toolCallIds.add(id);
          }
          return;
        }
        if (
          segment.type === 'thinking'
          && typeof segment.content === 'string'
          && segment.content.trim().length > 0
        ) {
          stats.thinkingCount += 1;
        }
      });
      return;
    }

    appendSignal(
      linkedUserMessageId,
      `P:${parsed.id}:${parsed.role}:${parsed.historyProcessPlaceholder ? '1' : '0'}`,
    );
  });

  const derivedStats = new Map<string, DerivedProcessStats>();
  mutableStats.forEach((stats, userMessageId) => {
    const toolCallCount = stats.toolCallIds.size;
    derivedStats.set(userMessageId, {
      hasProcess: toolCallCount > 0 || stats.thinkingCount > 0 || stats.processMessageCount > 0,
      hasStreamingAssistant: stats.hasStreamingAssistant,
      toolCallCount,
      thinkingCount: stats.thinkingCount,
      processMessageCount: stats.processMessageCount,
    });
  });

  const expandedByAssistantId = new Map<string, boolean>();
  parsedMessages.forEach((parsed) => {
    if (parsed.role !== 'assistant') {
      return;
    }
    if (parsed.historyProcessUserMessageId || parsed.historyProcessTurnId) {
      return;
    }

    const linkedUserMessageId = parsed.historyFinalForUserMessageId;
    if (linkedUserMessageId && userExpandedById.has(linkedUserMessageId)) {
      expandedByAssistantId.set(parsed.id, userExpandedById.get(linkedUserMessageId) === true);
      return;
    }

    const linkedTurnId = parsed.historyFinalForTurnId || parsed.conversationTurnId;
    if (linkedTurnId && turnExpandedById.has(linkedTurnId)) {
      expandedByAssistantId.set(parsed.id, turnExpandedById.get(linkedTurnId) === true);
      return;
    }

    if (finalAssistantExpandedById.has(parsed.id)) {
      expandedByAssistantId.set(parsed.id, finalAssistantExpandedById.get(parsed.id) === true);
    }
  });

  const visible = (() => {
    if (visibleCandidates.length <= 1) {
      return visibleCandidates.map((item) => item.message);
    }

    const bestFinalAssistantByKey = new Map<string, ParsedMessageForList>();

    visibleCandidates.forEach((parsed) => {
      const dedupKey = readNonProcessAssistantDedupKey(parsed);
      if (!dedupKey) {
        return;
      }

      const existing = bestFinalAssistantByKey.get(dedupKey);
      if (!existing) {
        bestFinalAssistantByKey.set(dedupKey, parsed);
        return;
      }

      const parsedStatus = String(parsed.status || '');
      const existingStatus = String(existing.status || '');
      const parsedIsTerminal = parsedStatus === 'completed' || parsedStatus === 'error';
      const existingIsTerminal = existingStatus === 'completed' || existingStatus === 'error';
      const parsedContentLength = readMessageContentLength(parsed.message);
      const existingContentLength = readMessageContentLength(existing.message);

      const shouldReplace = (
        Number(parsedIsTerminal) > Number(existingIsTerminal)
        || (
          parsedIsTerminal === existingIsTerminal
          && (
            parsedContentLength > existingContentLength
            || (parsedContentLength === existingContentLength && parsed.time >= existing.time)
          )
        )
      );

      if (shouldReplace) {
        bestFinalAssistantByKey.set(dedupKey, parsed);
      }
    });

    return visibleCandidates.filter((parsed) => {
      const dedupKey = readNonProcessAssistantDedupKey(parsed);
      if (!dedupKey) {
        return true;
      }
      return bestFinalAssistantByKey.get(dedupKey)?.id === parsed.id;
    }).map((parsed) => parsed.message);
  })();

  return {
    visibleMessages: visible,
    toolResultById: toolResultMap,
    toolResultMetaById: toolResultMetaMap,
    assistantToolCallById: assistantToolById,
    assistantToolCallMetaById: assistantToolMetaById,
    derivedProcessStatsByUserId: derivedStats,
    processSignalByUserMessageId: signalMap,
    linkedUserExpandedByAssistantId: expandedByAssistantId,
  };
};
