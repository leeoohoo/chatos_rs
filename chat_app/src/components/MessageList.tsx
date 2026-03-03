import React, { useEffect, useMemo, useRef, useState } from 'react';
import { MessageItem } from './MessageItem';
import { LoadingSpinner } from './LoadingSpinner';
// import { cn } from '../lib/utils';
import type { Message, MessageListProps } from '../types';

const normalizeTurnId = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

export const MessageList: React.FC<MessageListProps> = ({
  messages,
  isLoading = false,
  isStreaming = false,
  hasMore = false,
  onLoadMore,
  onToggleTurnProcess,
  activeTurnProcessUserMessageId,
  loadingTurnProcessUserMessageId,
  onMessageEdit,
  onMessageDelete,
  customRenderer,
}) => {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const scrollRafRef = useRef<number | null>(null);
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [isAtBottom, setIsAtBottom] = useState<boolean>(true);
  const visibleMessages = useMemo(
    () => (messages || []).filter((message) => {
      const metadata = (message as any)?.metadata;
      if (metadata?.hidden) return false;
      if (message.role === 'tool') return false;
      // 过程消息统一由右侧抽屉展示，不在主聊天流中展开
      if (metadata?.historyProcessUserMessageId || metadata?.historyProcessTurnId) return false;
      return true;
    }),
    [messages]
  );
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

  const processSignalByUserMessageId = useMemo(() => {
    const signalMap = new Map<string, string>();
    const userMessageIds = new Set<string>();
    const turnToUserMessageId = new Map<string, string>();

    for (const message of messages || []) {
      if (message.role !== 'user') {
        continue;
      }
      userMessageIds.add(message.id);
      signalMap.set(message.id, '');
      const turnId = normalizeTurnId(
        (message as any)?.metadata?.conversation_turn_id
        || (message as any)?.metadata?.historyProcess?.turnId,
      );
      if (turnId && !turnToUserMessageId.has(turnId)) {
        turnToUserMessageId.set(turnId, message.id);
      }
    }

    const appendSignal = (userMessageId: string, piece: string) => {
      if (!userMessageId || !piece) {
        return;
      }
      const prev = signalMap.get(userMessageId) || '';
      signalMap.set(userMessageId, prev ? `${prev}||${piece}` : piece);
    };

    for (const message of messages || []) {
      const metadata = (message as any)?.metadata || {};

      if (message.role === 'assistant') {
        let linkedUserMessageId = typeof metadata.historyFinalForUserMessageId === 'string'
          ? metadata.historyFinalForUserMessageId
          : '';
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          const linkedTurnId = normalizeTurnId(
            metadata.historyFinalForTurnId || metadata.conversation_turn_id,
          );
          if (linkedTurnId) {
            linkedUserMessageId = turnToUserMessageId.get(linkedTurnId) || '';
          }
        }
        if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
          continue;
        }

        const segments = Array.isArray(metadata.contentSegments)
          ? metadata.contentSegments
          : [];
        const toolCalls = Array.isArray(metadata.toolCalls)
          ? metadata.toolCalls
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
          `A:${message.id}:${message.status || ''}:${toolCalls.length}:${toolCallSegmentCount}:${thinkingCount}:${segments.length}`,
        );
        continue;
      }

      const processUserMessageId = typeof metadata.historyProcessUserMessageId === 'string'
        ? metadata.historyProcessUserMessageId
        : '';
      const processTurnId = normalizeTurnId(
        metadata.historyProcessTurnId || metadata.conversation_turn_id,
      );
      let linkedUserMessageId = processUserMessageId;
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        if (processTurnId) {
          linkedUserMessageId = turnToUserMessageId.get(processTurnId) || '';
        }
      }
      if (!linkedUserMessageId || !userMessageIds.has(linkedUserMessageId)) {
        continue;
      }

      appendSignal(
        linkedUserMessageId,
        `P:${message.id}:${message.role}:${metadata.historyProcessPlaceholder ? '1' : '0'}`,
      );
    }

    return signalMap;
  }, [messages]);

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
      const atBottom = measureAtBottom();
      setIsAtBottom(prev => (prev === atBottom ? prev : atBottom));
      if (!atBottom) {
        setAutoScroll(prev => (prev ? false : prev));
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
        {visibleMessages.map((message, index) => (
          <MessageItem
            key={message.id}
            message={message}
            isLast={index === lastVisibleIndex}
            isStreaming={isStreaming && index === lastVisibleIndex}
            onEdit={onMessageEdit}
            onDelete={onMessageDelete}
            onToggleTurnProcess={onToggleTurnProcess}
            activeTurnProcessUserMessageId={activeTurnProcessUserMessageId}
            loadingTurnProcessUserMessageId={loadingTurnProcessUserMessageId}
            allMessages={messages}
            toolResultById={toolResultById}
            toolResultKey={toolResultKeyByMessageId.get(message.id) || ''}
            processSignal={processSignalByUserMessageId.get(message.id) || ''}
            customRenderer={customRenderer}
          />
        ))}
        
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
