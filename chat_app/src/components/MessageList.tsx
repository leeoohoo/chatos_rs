import React from 'react';
import { MessageItem } from './MessageItem';
import { LoadingSpinner } from './LoadingSpinner';
import ImRuntimeContextDialog from './ImRuntimeContextDialog';
// import { cn } from '../lib/utils';
import type { MessageListProps } from '../types';
import { useMessageListDerivedState } from './messageList/useMessageListDerivedState';
import { useMessageListWindowing } from './messageList/useMessageListWindowing';

const MessageListComponent: React.FC<MessageListProps> = ({
  sessionId,
  messages,
  isLoading = false,
  isStreaming = false,
  isStopping = false,
  hasMore = false,
  onLoadMore,
  onToggleTurnProcess,
  onInspectRuntimeContext,
  onMessageEdit,
  onMessageDelete,
  customRenderer,
}) => {
  const [runtimeContextTarget, setRuntimeContextTarget] = React.useState<{
    sessionId: string;
    turnId?: string | null;
  } | null>(null);
  const {
    dedupedVisibleMessages,
    toolResultById,
    assistantToolCallById,
    derivedProcessStatsByUserId,
    processSignalByUserMessageId,
    linkedUserExpandedByAssistantId,
    toolResultKeyByMessageId,
    toolCallLookupKeyByMessageId,
  } = useMessageListDerivedState(messages || []);
  const {
    scrollRef,
    bottomRef,
    renderedMessages,
    shouldWindowMessages,
    boundedRenderStartIndex,
    lastVisibleIndex,
    isAtBottom,
    expandRenderedWindow,
    handleScroll,
    handleJumpToBottom,
  } = useMessageListWindowing({
    sessionId,
    visibleMessages: dedupedVisibleMessages,
    isLoading,
    hasMore,
    isStreaming,
  });

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
            onInspectRuntimeContext={(payload) => {
              setRuntimeContextTarget(payload);
              onInspectRuntimeContext?.(payload);
            }}
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

      <ImRuntimeContextDialog
        key={runtimeContextTarget ? `${runtimeContextTarget.sessionId}::${runtimeContextTarget.turnId || ''}` : 'closed'}
        open={Boolean(runtimeContextTarget?.sessionId)}
        sessionId={runtimeContextTarget?.sessionId || ''}
        turnId={runtimeContextTarget?.turnId || null}
        onClose={() => setRuntimeContextTarget(null)}
      />

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
  && prevProps.onInspectRuntimeContext === nextProps.onInspectRuntimeContext
  && prevProps.onMessageEdit === nextProps.onMessageEdit
  && prevProps.onMessageDelete === nextProps.onMessageDelete
  && prevProps.customRenderer === nextProps.customRenderer
);

export const MessageList = React.memo(MessageListComponent, areMessageListPropsEqual);
MessageList.displayName = 'MessageList';

export default MessageList;
