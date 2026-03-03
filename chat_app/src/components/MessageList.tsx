import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { MessageItem, type DerivedProcessStats } from './MessageItem';
import { LoadingSpinner } from './LoadingSpinner';
// import { cn } from '../lib/utils';
import type { Message, MessageListProps } from '../types';

const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);
const normalizeMetaId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);
const MESSAGE_WINDOW_EXPAND_TOP_OFFSET = 120;
const ESTIMATED_MESSAGE_ROW_HEIGHT = 88;
const MESSAGE_WINDOW_MIN_SIZE = 120;
const MESSAGE_WINDOW_MAX_SIZE = 280;
const MESSAGE_WINDOW_OVERSCAN_ROWS = 24;
const MESSAGE_WINDOW_THRESHOLD_EXTRA = 48;

export const MessageList: React.FC<MessageListProps> = ({
  sessionId,
  messages,
  isLoading = false,
  isStreaming = false,
  hasMore = false,
  onLoadMore,
  onToggleTurnProcess,
  onMessageEdit,
  onMessageDelete,
  customRenderer,
}) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const scrollRafRef = useRef<number | null>(null);
  const expandingWindowRef = useRef(false);
  const prevVisibleCountRef = useRef(0);
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [isAtBottom, setIsAtBottom] = useState<boolean>(true);
  const [renderStartIndex, setRenderStartIndex] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const visibleMessages = useMemo(
    () => (messages || []).filter((message) => {
      const metadata = (message as any)?.metadata;
      if (metadata?.hidden) return false;
      if (message.role === 'tool') return false;
      return true;
    }),
    [messages]
  );
  const windowSize = useMemo(() => {
    const estimatedRows = Math.ceil((viewportHeight || 960) / ESTIMATED_MESSAGE_ROW_HEIGHT);
    const candidate = estimatedRows + MESSAGE_WINDOW_OVERSCAN_ROWS;
    return Math.min(MESSAGE_WINDOW_MAX_SIZE, Math.max(MESSAGE_WINDOW_MIN_SIZE, candidate));
  }, [viewportHeight]);
  const windowThreshold = windowSize + MESSAGE_WINDOW_THRESHOLD_EXTRA;
  const windowStep = Math.max(60, Math.floor(windowSize * 0.72));
  const shouldWindowMessages = visibleMessages.length > windowThreshold;
  const boundedRenderStartIndex = shouldWindowMessages
    ? Math.min(renderStartIndex, Math.max(0, visibleMessages.length - 1))
    : 0;
  const renderedMessages = shouldWindowMessages
    ? visibleMessages.slice(boundedRenderStartIndex)
    : visibleMessages;
  const lastVisibleIndex = visibleMessages.length - 1;
  const getTimeValue = (value: unknown): number => {
    if (!value) return 0;
    if (value instanceof Date) return value.getTime();
    const parsed = new Date(value as any).getTime();
    return Number.isNaN(parsed) ? 0 : parsed;
  };
  const toolResultById = useMemo(() => {
    const map = new Map<string, Message>();
    for (const msg of messages || []) {
      if (msg.role !== 'tool') continue;
      const raw = msg as any;
      const toolCallId = raw.tool_call_id || raw.toolCallId || msg.metadata?.tool_call_id || msg.metadata?.toolCallId;
      if (toolCallId) {
        map.set(String(toolCallId), msg);
      }
    }
    return map;
  }, [messages]);
  const toolResultMetaById = useMemo(() => {
    const map = new Map<string, { id: string; time: number }>();
    toolResultById.forEach((msg, toolCallId) => {
      const time = msg.updatedAt ? getTimeValue(msg.updatedAt) : getTimeValue(msg.createdAt);
      map.set(toolCallId, { id: msg.id, time });
    });
    return map;
  }, [toolResultById]);
  const {
    assistantToolCallById,
    assistantToolCallMetaById,
  } = useMemo(() => {
    const byId = new Map<string, any>();
    const metaById = new Map<string, { messageId: string; time: number }>();
    for (const message of messages || []) {
      if (message.role !== 'assistant') {
        continue;
      }
      const time = message.updatedAt ? getTimeValue(message.updatedAt) : getTimeValue(message.createdAt);
      const topLevel = Array.isArray((message as any).toolCalls) ? (message as any).toolCalls : [];
      const metadataLevel = Array.isArray(message.metadata?.toolCalls) ? message.metadata.toolCalls : [];
      [...metadataLevel, ...topLevel].forEach((toolCall: any) => {
        const id = normalizeMetaId(toolCall?.id);
        if (!id) {
          return;
        }
        if (!byId.has(id)) {
          byId.set(id, toolCall);
        }
        if (!metaById.has(id)) {
          metaById.set(id, { messageId: message.id, time });
        }
      });
    }
    return {
      assistantToolCallById: byId,
      assistantToolCallMetaById: metaById,
    };
  }, [messages]);
  const toolResultKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of visibleMessages) {
      const toolCalls = message.metadata?.toolCalls;
      if (!toolCalls || toolCalls.length === 0) {
        map.set(message.id, '');
        continue;
      }
      const key = toolCalls
        .map((tc) => {
          const meta = toolResultMetaById.get(String(tc.id));
          return `${tc.id}:${meta?.id ?? ''}:${meta?.time ?? 0}`;
        })
        .join('|');
      map.set(message.id, key);
    }
    return map;
  }, [visibleMessages, toolResultMetaById]);
  const toolCallLookupKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of visibleMessages) {
      const segments = Array.isArray(message.metadata?.contentSegments) ? message.metadata.contentSegments : [];
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
  }, [visibleMessages, assistantToolCallMetaById]);
  const {
    derivedProcessStatsByUserId,
    processSignalByUserMessageId,
  } = useMemo(() => {
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

    for (const message of messages || []) {
      if (message.role !== 'user') {
        continue;
      }
      userMessageIds.add(message.id);
      signalMap.set(message.id, '');
      mutableStats.set(message.id, {
        hasStreamingAssistant: false,
        thinkingCount: 0,
        processMessageCount: 0,
        toolCallIds: new Set<string>(),
      });
      const turnId = normalizeTurnId(
        (message as any)?.metadata?.conversation_turn_id
        || (message as any)?.metadata?.historyProcess?.turnId,
      );
      if (turnId && !turnToUserMessageId.has(turnId)) {
        turnToUserMessageId.set(turnId, message.id);
      }
      const finalAssistantMessageId = normalizeMetaId(
        (message as any)?.metadata?.historyProcess?.finalAssistantMessageId,
      );
      if (finalAssistantMessageId && !assistantIdToUserMessageId.has(finalAssistantMessageId)) {
        assistantIdToUserMessageId.set(finalAssistantMessageId, message.id);
      }
    }

    const appendSignal = (userMessageId: string, piece: string) => {
      if (!userMessageId || !piece) {
        return;
      }
      const prev = signalMap.get(userMessageId) || '';
      signalMap.set(userMessageId, prev ? `${prev}||${piece}` : piece);
    };

    const resolveLinkedUserMessageId = (message: Message, metadata: Record<string, any>): string => {
      if (message.role === 'assistant') {
        let linkedUserMessageId = normalizeMetaId(metadata.historyProcessUserMessageId);
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          const processTurnId = normalizeTurnId(
            metadata.historyProcessTurnId || metadata.conversation_turn_id,
          );
          if (processTurnId) {
            linkedUserMessageId = turnToUserMessageId.get(processTurnId) || '';
          }
        }
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          linkedUserMessageId = normalizeMetaId(metadata.historyFinalForUserMessageId);
        }
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          const finalTurnId = normalizeTurnId(
            metadata.historyFinalForTurnId || metadata.conversation_turn_id,
          );
          if (finalTurnId) {
            linkedUserMessageId = turnToUserMessageId.get(finalTurnId) || '';
          }
        }
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          linkedUserMessageId = assistantIdToUserMessageId.get(message.id) || '';
        }
        return userMessageIds.has(linkedUserMessageId) ? linkedUserMessageId : '';
      }

      let linkedUserMessageId = normalizeMetaId(metadata.historyProcessUserMessageId);
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        const processTurnId = normalizeTurnId(
          metadata.historyProcessTurnId || metadata.conversation_turn_id,
        );
        if (processTurnId) {
          linkedUserMessageId = turnToUserMessageId.get(processTurnId) || '';
        }
      }
      return userMessageIds.has(linkedUserMessageId) ? linkedUserMessageId : '';
    };

    for (const message of messages || []) {
      const metadata = (message as any)?.metadata || {};
      const linkedUserMessageId = resolveLinkedUserMessageId(message, metadata);
      if (!linkedUserMessageId) {
        continue;
      }

      if (message.role === 'assistant') {
        const segments = Array.isArray(metadata.contentSegments)
          ? metadata.contentSegments
          : [];
        const metadataToolCalls = Array.isArray(metadata.toolCalls)
          ? metadata.toolCalls
          : [];
        const topLevelToolCalls = Array.isArray((message as any).toolCalls)
          ? (message as any).toolCalls
          : [];
        const thinkingCount = segments.filter((segment: any) => (
          segment?.type === 'thinking'
          && typeof segment?.content === 'string'
          && segment.content.trim().length > 0
        )).length;
        const toolCallSegmentCount = segments.filter((segment: any) => (
          segment?.type === 'tool_call'
          && Boolean(segment?.toolCallId)
        )).length;
        appendSignal(
          linkedUserMessageId,
          `A:${message.id}:${message.status || ''}:${metadataToolCalls.length}:${toolCallSegmentCount}:${thinkingCount}:${segments.length}`,
        );

        const stats = mutableStats.get(linkedUserMessageId);
        if (!stats) {
          continue;
        }

        if (message.status === 'streaming') {
          stats.hasStreamingAssistant = true;
        }

        const isProcessAssistant = Boolean(metadata.historyProcessUserMessageId || metadata.historyProcessTurnId);
        if (isProcessAssistant && metadata.historyProcessPlaceholder !== true) {
          stats.processMessageCount += 1;
        }

        [...metadataToolCalls, ...topLevelToolCalls].forEach((toolCall: any) => {
          const id = normalizeMetaId(toolCall?.id);
          if (id) {
            stats.toolCallIds.add(id);
          }
        });

        segments.forEach((segment: any) => {
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
        continue;
      }

      appendSignal(
        linkedUserMessageId,
        `P:${message.id}:${message.role}:${metadata.historyProcessPlaceholder ? '1' : '0'}`,
      );
    }

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

    return {
      derivedProcessStatsByUserId: derivedStats,
      processSignalByUserMessageId: signalMap,
    };
  }, [messages]);

  useEffect(() => {
    const target = scrollRef.current;
    if (!target) {
      return;
    }

    const updateHeight = () => {
      setViewportHeight((prev) => {
        const next = target.clientHeight || 0;
        return prev === next ? prev : next;
      });
    };

    updateHeight();

    if (typeof ResizeObserver === 'undefined') {
      return undefined;
    }

    const observer = new ResizeObserver(() => {
      updateHeight();
    });
    observer.observe(target);

    return () => observer.disconnect();
  }, [sessionId]);

  useEffect(() => {
    const nextCount = visibleMessages.length;
    const previousCount = prevVisibleCountRef.current;
    prevVisibleCountRef.current = nextCount;

    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }

    const latestStart = Math.max(0, nextCount - windowSize);
    setRenderStartIndex((prev) => {
      // 初次进入长会话时直接启用窗口渲染
      if (previousCount === 0) {
        return latestStart;
      }
      // 用户已展开到最前面时，保持不折叠
      if (prev === 0) {
        return 0;
      }
      // 在底部连续对话时，窗口跟随最新消息，避免渲染集合持续膨胀
      if (isStreaming || autoScroll || isAtBottom) {
        return latestStart;
      }
      return Math.min(prev, latestStart);
    });
  }, [visibleMessages.length, isStreaming, autoScroll, isAtBottom, windowSize, windowThreshold]);

  useEffect(() => {
    const nextCount = visibleMessages.length;
    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }
    setRenderStartIndex(Math.max(0, nextCount - windowSize));
  }, [sessionId, visibleMessages.length, windowSize, windowThreshold]);

  const expandRenderedWindow = useCallback(() => {
    if (!shouldWindowMessages || boundedRenderStartIndex <= 0) {
      return;
    }

    const scroller = scrollRef.current;
    if (expandingWindowRef.current) {
      return;
    }
    if (!scroller) {
      setRenderStartIndex((prev) => Math.max(0, prev - windowStep));
      return;
    }

    expandingWindowRef.current = true;
    const prevScrollHeight = scroller.scrollHeight;
    setRenderStartIndex((prev) => Math.max(0, prev - windowStep));

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        const nextScroller = scrollRef.current;
        if (nextScroller) {
          const delta = nextScroller.scrollHeight - prevScrollHeight;
          if (delta > 0) {
            nextScroller.scrollTop += delta;
          }
        }
        expandingWindowRef.current = false;
      });
    });
  }, [boundedRenderStartIndex, shouldWindowMessages, windowStep]);

  const measureAtBottom = () => {
    const el = scrollRef.current;
    if (!el) return true;
    const threshold = 40;
    return el.scrollHeight - el.scrollTop - el.clientHeight <= threshold;
  };

  const scrollToBottom = (smooth = true) => {
    bottomRef.current?.scrollIntoView({ behavior: smooth ? 'smooth' : 'auto' });
  };

  useEffect(() => {
    if (isStreaming && autoScroll) {
      scrollToBottom(true);
    }
  }, [messages, isStreaming, autoScroll]);

  useEffect(() => {
    const next = measureAtBottom();
    setIsAtBottom(prev => (prev === next ? prev : next));
  }, [messages, isStreaming]);

  useEffect(() => {
    if (isStreaming && isAtBottom) {
      setAutoScroll(prev => (prev ? prev : true));
      return;
    }
    if (!isStreaming) {
      setAutoScroll(prev => (prev ? false : prev));
    }
  }, [isStreaming, isAtBottom]);

  const handleScroll = () => {
    if (scrollRafRef.current !== null) return;
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      const el = scrollRef.current;
      if (!el) return;
      const atBottom = measureAtBottom();
      setIsAtBottom(prev => (prev === atBottom ? prev : atBottom));
      if (!atBottom) {
        setAutoScroll(prev => (prev ? false : prev));
      }
      if (
        shouldWindowMessages
        && boundedRenderStartIndex > 0
        && el.scrollTop <= MESSAGE_WINDOW_EXPAND_TOP_OFFSET
      ) {
        expandRenderedWindow();
      }
    });
  };

  const handleJumpToBottom = () => {
    scrollToBottom(true);
    setAutoScroll(true);
  };

  useEffect(() => {
    return () => {
      if (scrollRafRef.current !== null) {
        cancelAnimationFrame(scrollRafRef.current);
        scrollRafRef.current = null;
      }
      expandingWindowRef.current = false;
    };
  }, []);

  if (visibleMessages.length === 0 && !isLoading && !hasMore) {
    return (
      <div className="flex items-center justify-center h-full">
        <div className="text-center space-y-4">
          <div className="w-16 h-16 mx-auto bg-muted rounded-full flex items-center justify-center">
            <svg className="w-8 h-8 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
            </svg>
          </div>
          <div>
            <h3 className="text-lg font-semibold text-foreground">Start a conversation</h3>
            <p className="text-sm text-muted-foreground mt-1">
              Send a message to begin your chat with AI
            </p>
          </div>
        </div>
      </div>
    );
  }


  return (
    <div className="flex flex-col h-full relative">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-4 py-6 space-y-1"
        style={{
          willChange: 'scroll-position',
          transform: 'translateZ(0)',
        }}
      >
        {hasMore && (
          <div className="flex justify-center mb-2">
            <button
              type="button"
              onClick={onLoadMore}
              className="text-sm px-3 py-1 rounded border border-border text-foreground hover:bg-accent"
            >
              加载更多
            </button>
          </div>
        )}
        {shouldWindowMessages && boundedRenderStartIndex > 0 && (
          <div className="flex justify-center mb-2">
            <button
              type="button"
              onClick={expandRenderedWindow}
              className="text-sm px-3 py-1 rounded border border-border text-foreground hover:bg-accent"
            >
              显示更早消息（{boundedRenderStartIndex}）
            </button>
          </div>
        )}
        {renderedMessages.map((message, index) => {
          const globalIndex = boundedRenderStartIndex + index;
          return (
          <MessageItem
            key={message.id}
            message={message}
            isLast={globalIndex === lastVisibleIndex}
            isStreaming={isStreaming && globalIndex === lastVisibleIndex}
            onEdit={onMessageEdit}
            onDelete={onMessageDelete}
            onToggleTurnProcess={onToggleTurnProcess}
            allMessages={messages}
            derivedProcessStatsByUserId={derivedProcessStatsByUserId}
            toolResultById={toolResultById}
            assistantToolCallsById={assistantToolCallById}
            toolResultKey={toolResultKeyByMessageId.get(message.id) || ''}
            toolCallLookupKey={toolCallLookupKeyByMessageId.get(message.id) || ''}
            processSignal={processSignalByUserMessageId.get(message.id) || ''}
            customRenderer={customRenderer}
          />
          );
        })}
        
        {isLoading && (
          <div className="flex justify-start">
            <div className="flex items-center space-x-2 bg-muted px-4 py-3 rounded-lg max-w-xs">
              <LoadingSpinner size="sm" />
              <span className="text-sm text-muted-foreground">AI is thinking...</span>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {!isAtBottom && (
        <button
          type="button"
          aria-label="回到底部"
          title="回到底部"
          onClick={handleJumpToBottom}
          className="absolute bottom-4 right-4 z-10 flex items-center gap-2 rounded-full bg-primary text-primary-foreground px-4 py-2 shadow-md hover:bg-primary/90"
        >
          <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 5v12" />
            <path d="M19 12l-7 7-7-7" />
          </svg>
          <span className="text-sm">回到底部</span>
        </button>
      )}
    </div>
  );
};

export default MessageList;
