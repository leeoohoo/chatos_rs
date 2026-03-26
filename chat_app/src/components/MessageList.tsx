import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { MessageItem } from './MessageItem';
import { LoadingSpinner } from './LoadingSpinner';
// import { cn } from '../lib/utils';
import type { MessageListProps } from '../types';
import {
  buildVisibleMessageState,
  normalizeMetaId,
  parseMessageForList,
  type ParsedMessageCacheEntry,
} from './messageList/derivedData';
const MESSAGE_WINDOW_EXPAND_TOP_OFFSET = 120;
const ESTIMATED_MESSAGE_ROW_HEIGHT = 120;
const MESSAGE_WINDOW_MIN_SIZE = 20;
const MESSAGE_WINDOW_MAX_SIZE = 72;
const MESSAGE_WINDOW_OVERSCAN_ROWS = 6;
const MESSAGE_WINDOW_THRESHOLD_EXTRA = 8;

const MessageListComponent: React.FC<MessageListProps> = ({
  sessionId,
  messages,
  isLoading = false,
  isStreaming = false,
  isStopping = false,
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
  const initialScrollRafRef = useRef<number | null>(null);
  const streamEndScrollRafRef = useRef<number | null>(null);
  const streamEndSettleRafRef = useRef<number | null>(null);
  const autoScrollRafRef = useRef<number | null>(null);
  const prevIsStreamingRef = useRef<boolean>(isStreaming);
  const pendingSessionInitialScrollRef = useRef<boolean>(true);
  const expandingWindowRef = useRef(false);
  const prevVisibleCountRef = useRef(0);
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [isAtBottom, setIsAtBottom] = useState<boolean>(true);
  const [renderStartIndex, setRenderStartIndex] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
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

  const windowSize = useMemo(() => {
    const estimatedRows = Math.ceil((viewportHeight || 960) / ESTIMATED_MESSAGE_ROW_HEIGHT);
    const candidate = estimatedRows + MESSAGE_WINDOW_OVERSCAN_ROWS;
    return Math.min(MESSAGE_WINDOW_MAX_SIZE, Math.max(MESSAGE_WINDOW_MIN_SIZE, candidate));
  }, [viewportHeight]);
  const windowThreshold = windowSize + MESSAGE_WINDOW_THRESHOLD_EXTRA;
  const windowStep = Math.max(32, Math.floor(windowSize * 0.6));
  const shouldWindowMessages = dedupedVisibleMessages.length > windowThreshold;
  const boundedRenderStartIndex = shouldWindowMessages
    ? Math.min(renderStartIndex, Math.max(0, dedupedVisibleMessages.length - 1))
    : 0;
  const renderedMessages = useMemo(
    () => (shouldWindowMessages
      ? dedupedVisibleMessages.slice(boundedRenderStartIndex)
      : dedupedVisibleMessages),
    [shouldWindowMessages, dedupedVisibleMessages, boundedRenderStartIndex],
  );
  const lastVisibleIndex = dedupedVisibleMessages.length - 1;
  const toolResultKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of renderedMessages) {
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
  }, [renderedMessages, toolResultMetaById]);
  const toolCallLookupKeyByMessageId = useMemo(() => {
    const map = new Map<string, string>();
    for (const message of renderedMessages) {
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
  }, [renderedMessages, assistantToolCallMetaById]);

  useEffect(() => {
    pendingSessionInitialScrollRef.current = true;
    if (initialScrollRafRef.current !== null) {
      cancelAnimationFrame(initialScrollRafRef.current);
      initialScrollRafRef.current = null;
    }
    setIsAtBottom(true);
    setAutoScroll(false);
  }, [sessionId]);

  useEffect(() => {
    if (!pendingSessionInitialScrollRef.current) {
      return;
    }

    if (dedupedVisibleMessages.length === 0 && !isLoading && !hasMore) {
      pendingSessionInitialScrollRef.current = false;
      return;
    }

    if (initialScrollRafRef.current !== null) {
      cancelAnimationFrame(initialScrollRafRef.current);
      initialScrollRafRef.current = null;
    }

    initialScrollRafRef.current = requestAnimationFrame(() => {
      initialScrollRafRef.current = null;
      const el = scrollRef.current;
      if (!el) {
        return;
      }
      el.scrollTop = el.scrollHeight;
      setIsAtBottom(true);
      setAutoScroll(isStreaming);
      pendingSessionInitialScrollRef.current = false;
    });

    return () => {
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
    };
  }, [sessionId, dedupedVisibleMessages.length, isLoading, hasMore, isStreaming]);

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
    const nextCount = dedupedVisibleMessages.length;
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
      // 用户正在查看更早消息时保持位置；回到底部后自动恢复紧凑窗口，避免长列表常驻造成滚动卡顿
      if (prev === 0 && !isAtBottom) {
        return 0;
      }
      // 连续流式或自动滚动时，窗口跟随最新消息；手动展开过程消息时保持当前位置
      if (isStreaming || autoScroll || isAtBottom) {
        return latestStart;
      }
      return Math.min(prev, latestStart);
    });
  }, [dedupedVisibleMessages.length, isStreaming, autoScroll, isAtBottom, windowSize, windowThreshold]);

  useEffect(() => {
    const nextCount = dedupedVisibleMessages.length;
    if (nextCount <= windowThreshold) {
      setRenderStartIndex(0);
      return;
    }
    setRenderStartIndex(Math.max(0, nextCount - windowSize));
  }, [sessionId, dedupedVisibleMessages.length, windowSize, windowThreshold]);

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

  const cancelPendingStreamEndScroll = useCallback(() => {
    if (streamEndScrollRafRef.current !== null) {
      cancelAnimationFrame(streamEndScrollRafRef.current);
      streamEndScrollRafRef.current = null;
    }
    if (streamEndSettleRafRef.current !== null) {
      cancelAnimationFrame(streamEndSettleRafRef.current);
      streamEndSettleRafRef.current = null;
    }
  }, []);

  const scrollToBottom = useCallback((smooth = true) => {
    const el = scrollRef.current;
    if (!el) {
      return;
    }
    if (smooth) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
      return;
    }
    el.scrollTop = el.scrollHeight;
  }, []);

  const scheduleAutoScrollToBottom = useCallback(() => {
    if (autoScrollRafRef.current !== null) {
      return;
    }
    autoScrollRafRef.current = requestAnimationFrame(() => {
      autoScrollRafRef.current = null;
      scrollToBottom(false);
    });
  }, [scrollToBottom]);

  const scheduleStreamEndBottomLock = useCallback((frames = 8) => {
    cancelPendingStreamEndScroll();
    if (frames <= 0) {
      return;
    }

    const lockBottom = (remaining: number) => {
      scrollToBottom(false);
      setIsAtBottom(true);
      if (remaining <= 1) {
        return;
      }
      streamEndSettleRafRef.current = requestAnimationFrame(() => {
        streamEndSettleRafRef.current = null;
        lockBottom(remaining - 1);
      });
    };

    streamEndScrollRafRef.current = requestAnimationFrame(() => {
      streamEndScrollRafRef.current = null;
      lockBottom(frames);
    });
  }, [cancelPendingStreamEndScroll, scrollToBottom]);

  useEffect(() => {
    const root = scrollRef.current;
    const target = bottomRef.current;
    if (!root || !target || typeof IntersectionObserver === 'undefined') {
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        const atBottom = Boolean(entries[0]?.isIntersecting);
        setIsAtBottom((prev) => (prev === atBottom ? prev : atBottom));
        if (!atBottom) {
          setAutoScroll((prev) => (prev ? false : prev));
          return;
        }
        if (isStreaming) {
          setAutoScroll((prev) => (prev ? prev : true));
        }
      },
      {
        root,
        threshold: 0.98,
      },
    );

    observer.observe(target);
    return () => observer.disconnect();
  }, [sessionId, isStreaming, renderedMessages.length]);

  useEffect(() => {
    if (isStreaming && autoScroll) {
      scheduleAutoScrollToBottom();
    }
  }, [messages, isStreaming, autoScroll, scheduleAutoScrollToBottom]);

  useEffect(() => {
    const wasStreaming = prevIsStreamingRef.current;
    prevIsStreamingRef.current = isStreaming;
    if (!wasStreaming || isStreaming) {
      return;
    }

    if (!(autoScroll || isAtBottom)) {
      return;
    }

    scheduleStreamEndBottomLock();

    return cancelPendingStreamEndScroll;
  }, [
    isStreaming,
    autoScroll,
    isAtBottom,
    scheduleStreamEndBottomLock,
    cancelPendingStreamEndScroll,
  ]);

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
    if (!shouldWindowMessages) {
      return;
    }
    if (scrollRafRef.current !== null) return;
    scrollRafRef.current = requestAnimationFrame(() => {
      scrollRafRef.current = null;
      const el = scrollRef.current;
      if (!el) return;
      if (
        boundedRenderStartIndex > 0
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
      if (initialScrollRafRef.current !== null) {
        cancelAnimationFrame(initialScrollRafRef.current);
        initialScrollRafRef.current = null;
      }
      if (streamEndScrollRafRef.current !== null) {
        cancelAnimationFrame(streamEndScrollRafRef.current);
        streamEndScrollRafRef.current = null;
      }
      if (streamEndSettleRafRef.current !== null) {
        cancelAnimationFrame(streamEndSettleRafRef.current);
        streamEndSettleRafRef.current = null;
      }
      if (autoScrollRafRef.current !== null) {
        cancelAnimationFrame(autoScrollRafRef.current);
        autoScrollRafRef.current = null;
      }
      expandingWindowRef.current = false;
    };
  }, []);

  if (dedupedVisibleMessages.length === 0 && !isLoading && !hasMore) {
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
        onScroll={shouldWindowMessages ? handleScroll : undefined}
        className="flex-1 overflow-y-auto px-4 py-6 space-y-1"
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
            derivedProcessStatsByUserId={derivedProcessStatsByUserId}
            toolResultById={toolResultById}
            assistantToolCallsById={assistantToolCallById}
            linkedUserExpandedForAssistant={linkedUserExpandedByAssistantId.get(message.id)}
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
              <span className="text-sm text-muted-foreground">{isStopping ? 'AI is stopping...' : 'AI is thinking...'}</span>
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

const areMessageListPropsEqual = (prevProps: MessageListProps, nextProps: MessageListProps): boolean => (
  prevProps.sessionId === nextProps.sessionId
  && prevProps.messages === nextProps.messages
  && (prevProps.isLoading ?? false) === (nextProps.isLoading ?? false)
  && (prevProps.isStreaming ?? false) === (nextProps.isStreaming ?? false)
  && (prevProps.isStopping ?? false) === (nextProps.isStopping ?? false)
  && (prevProps.hasMore ?? false) === (nextProps.hasMore ?? false)
  && prevProps.onLoadMore === nextProps.onLoadMore
  && prevProps.onToggleTurnProcess === nextProps.onToggleTurnProcess
  && prevProps.onMessageEdit === nextProps.onMessageEdit
  && prevProps.onMessageDelete === nextProps.onMessageDelete
  && prevProps.customRenderer === nextProps.customRenderer
);

export const MessageList = React.memo(MessageListComponent, areMessageListPropsEqual);
MessageList.displayName = 'MessageList';

export default MessageList;
