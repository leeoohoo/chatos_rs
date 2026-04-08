import { useMemo, useRef } from 'react';

import type { Message } from '../../types';
import {
  buildVisibleMessageState,
  normalizeMetaId,
  parseMessageForList,
  type ParsedMessageCacheEntry,
} from './derivedData';

export const useMessageListDerivedState = (messages: Message[]) => {
  const parsedMessageCacheRef = useRef<Map<string, ParsedMessageCacheEntry>>(new Map());

  const parsedMessages = useMemo(() => {
    const previousCache = parsedMessageCacheRef.current;
    const nextCache = new Map<string, ParsedMessageCacheEntry>();
    const list = (messages || []).map((message) => {
      const cacheKey = String(message.id);
      const metadataRef = (message as any)?.metadata;
      const updatedAt = (message as any)?.updatedAt;
      const cached = previousCache.get(cacheKey);

      if (
        cached
        && cached.ref === message
        && cached.metadataRef === metadataRef
        && cached.content === message.content
        && cached.status === message.status
        && cached.updatedAt === updatedAt
      ) {
        nextCache.set(cacheKey, cached);
        return cached.parsed;
      }

      const parsed = parseMessageForList(message);
      nextCache.set(cacheKey, {
        ref: message,
        metadataRef,
        content: message.content,
        status: message.status,
        updatedAt,
        parsed,
      });
      return parsed;
    });

    parsedMessageCacheRef.current = nextCache;
    return list;
  }, [messages]);

  const {
    visibleMessages,
    toolResultById,
    toolResultMetaById,
    assistantToolCallById,
    assistantToolCallMetaById,
    derivedProcessStatsByUserId,
    processSignalByUserMessageId,
    linkedUserExpandedByAssistantId,
  } = useMemo(() => buildVisibleMessageState(parsedMessages), [parsedMessages]);

  const dedupedVisibleMessages = useMemo(() => {
    if (!visibleMessages || visibleMessages.length <= 1) {
      return visibleMessages;
    }
    const seenIds = new Set<string>();
    const list: typeof visibleMessages = [];
    for (const message of visibleMessages) {
      const id = String(message.id || '');
      if (!id || seenIds.has(id)) {
        continue;
      }
      seenIds.add(id);
      list.push(message);
    }
    return list;
  }, [visibleMessages]);

  const toolResultKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of dedupedVisibleMessages) {
      const toolCalls = message.metadata?.toolCalls;
      if (!toolCalls || toolCalls.length === 0) {
        map.set(message.id, '');
        continue;
      }
      const key = toolCalls
        .map((toolCall) => {
          const meta = toolResultMetaById.get(String(toolCall.id));
          return `${toolCall.id}:${meta?.id ?? ''}:${meta?.time ?? 0}`;
        })
        .join('|');
      map.set(message.id, key);
    }
    return map;
  }, [dedupedVisibleMessages, toolResultMetaById]);

  const toolCallLookupKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of dedupedVisibleMessages) {
      const segments = Array.isArray(message.metadata?.contentSegments)
        ? message.metadata.contentSegments
        : [];
      const toolCallIds = segments
        .filter((segment: any) => segment?.type === 'tool_call')
        .map((segment: any) => normalizeMetaId(segment?.toolCallId))
        .filter(Boolean);
      if (toolCallIds.length === 0) {
        map.set(message.id, '');
        continue;
      }
      const key = [...new Set(toolCallIds)]
        .map((toolCallId) => {
          const meta = assistantToolCallMetaById.get(toolCallId);
          return `${toolCallId}:${meta?.messageId ?? ''}:${meta?.time ?? 0}`;
        })
        .join('|');
      map.set(message.id, key);
    }
    return map;
  }, [assistantToolCallMetaById, dedupedVisibleMessages]);

  return {
    dedupedVisibleMessages,
    toolResultById,
    assistantToolCallById,
    derivedProcessStatsByUserId,
    processSignalByUserMessageId,
    linkedUserExpandedByAssistantId,
    toolResultKeyByMessageId,
    toolCallLookupKeyByMessageId,
  };
};
