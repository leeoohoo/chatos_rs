import type { Message } from '../../types';
import type { DerivedProcessStats } from '../MessageItem';

export const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const normalizeMetaId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

type ParsedMessageForList = {
  message: Message;
  id: string;
  role: Message['role'];
  status: string;
  visible: boolean;
  time: number;
  metadata: Record<string, any>;
  segments: any[];
  metadataToolCalls: any[];
  topLevelToolCalls: any[];
  assistantToolCalls: Array<{ id: string; toolCall: any }>;
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
  const parsed = new Date(value as any).getTime();
  return Number.isNaN(parsed) ? 0 : parsed;
};

export const parseMessageForList = (message: Message): ParsedMessageForList => {
  const raw = message as any;
  const metadata = ((raw?.metadata || {}) as Record<string, any>);
  const segments = Array.isArray(metadata.contentSegments) ? metadata.contentSegments : [];
  const metadataToolCalls = Array.isArray(metadata.toolCalls) ? metadata.toolCalls : [];
  const topLevelToolCalls = Array.isArray(raw.toolCalls) ? raw.toolCalls : [];
  const assistantToolCalls: Array<{ id: string; toolCall: any }> = [];

  if (message.role === 'assistant') {
    [...metadataToolCalls, ...topLevelToolCalls].forEach((toolCall: any) => {
      const id = normalizeMetaId(toolCall?.id);
      if (id) {
        assistantToolCalls.push({ id, toolCall });
      }
    });
  }

  let thinkingSegmentCount = 0;
  let toolCallSegmentCount = 0;
  if (message.role === 'assistant') {
    segments.forEach((segment: any) => {
      if (
        segment?.type === 'thinking'
        && typeof segment?.content === 'string'
        && segment.content.trim().length > 0
      ) {
        thinkingSegmentCount += 1;
        return;
      }
      if (segment?.type === 'tool_call' && Boolean(segment?.toolCallId)) {
        toolCallSegmentCount += 1;
      }
    });
  }

  const toolResultCallIdRaw = raw.tool_call_id || raw.toolCallId || metadata.tool_call_id || metadata.toolCallId;
  const conversationTurnId = normalizeTurnId(
    metadata.conversation_turn_id || metadata.conversationTurnId,
  );
  const historyProcessTurnId = normalizeTurnId(metadata.historyProcessTurnId || metadata.historyProcess?.turnId);
  const historyProcessUserMessageId = normalizeMetaId(metadata.historyProcessUserMessageId);
  const historyFinalForUserMessageId = normalizeMetaId(metadata.historyFinalForUserMessageId);
  const historyFinalForTurnId = normalizeTurnId(metadata.historyFinalForTurnId);
  const historyProcessPlaceholder = metadata.historyProcessPlaceholder === true;
  const userExpanded = metadata?.historyProcess?.expanded === true;
  const userTurnId = normalizeTurnId(
    metadata.conversation_turn_id || metadata.conversationTurnId || metadata.historyProcess?.turnId,
  );
  const userFinalAssistantMessageId = normalizeMetaId(metadata?.historyProcess?.finalAssistantMessageId);

  return {
    message,
    id: message.id,
    role: message.role,
    status: String(message.status || ''),
    visible: !metadata?.hidden && message.role !== 'tool',
    time: message.updatedAt ? getTimeValue(message.updatedAt) : getTimeValue(message.createdAt),
    metadata,
    segments,
    metadataToolCalls,
    topLevelToolCalls,
    assistantToolCalls,
    toolResultCallId: toolResultCallIdRaw ? String(toolResultCallIdRaw) : '',
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
  const visible: Message[] = [];
  const toolResultMap = new Map<string, Message>();
  const toolResultMetaMap = new Map<string, { id: string; time: number }>();
  const assistantToolById = new Map<string, any>();
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
      visible.push(parsed.message);
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
        `A:${parsed.id}:${parsed.status}:${parsed.metadataToolCalls.length}:${parsed.toolCallSegmentCount}:${parsed.thinkingSegmentCount}:${parsed.segments.length}`,
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

      parsed.segments.forEach((segment: any) => {
        if (segment?.type === 'tool_call') {
          const id = normalizeMetaId(segment?.toolCallId);
          if (id) {
            stats.toolCallIds.add(id);
          }
          return;
        }
        if (
          segment?.type === 'thinking'
          && typeof segment?.content === 'string'
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
