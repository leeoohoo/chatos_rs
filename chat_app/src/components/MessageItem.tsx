import React, { useEffect, useMemo, useState, memo } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';
import { AttachmentRenderer } from './AttachmentRenderer';
import { cn, formatTime } from '../lib/utils';
import type { Message, Attachment, ToolCall } from '../types';
import { MessageContentRenderer } from './messageItem/MessageContentRenderer';
import {
  EMPTY_DERIVED_PROCESS_STATS,
  normalizeContentSegmentsForRender,
} from './messageItem/helpers';
import type { DerivedProcessStats } from './messageItem/types';
export type { DerivedProcessStats } from './messageItem/types';

const normalizeText = (value: unknown): string => (
  typeof value === 'string' ? value.trim() : ''
);

interface MessageItemProps {
  message: Message;
  isLast?: boolean;
  isStreaming?: boolean;
  onEdit?: (messageId: string, content: string) => void;
  onDelete?: (messageId: string) => void;
  onToggleTurnProcess?: (userMessageId: string) => void;
  onInspectRuntimeContext?: (payload: { sessionId: string; turnId?: string | null }) => void;
  renderContext?: 'chat' | 'process_drawer';
  derivedProcessStatsByUserId?: Map<string, DerivedProcessStats>;
  toolResultById?: Map<string, Message>;
  assistantToolCallsById?: Map<string, ToolCall>;
  toolResultKey?: string;
  toolCallLookupKey?: string;
  processSignal?: string;
  customRenderer?: {
    renderMessage?: (message: Message) => React.ReactNode;
    renderAttachment?: (attachment: Attachment) => React.ReactNode;
  };
  linkedUserExpandedForAssistant?: boolean;
}

const MessageItemComponent: React.FC<MessageItemProps> = ({
  message,
  isLast = false,
  isStreaming = false,
  onEdit,
  onDelete,
  onToggleTurnProcess,
  onInspectRuntimeContext,
  renderContext = 'chat',
  derivedProcessStatsByUserId,
  toolResultById,
  assistantToolCallsById,
  customRenderer,
  linkedUserExpandedForAssistant,
}) => {
  const [isEditing, setIsEditing] = useState(false);
  const [editContent, setEditContent] = useState(message.content);
  useEffect(() => {
    if (!isEditing) {
      setEditContent(message.content);
    }
  }, [isEditing, message.content]);

  // 处理代码应用
  const handleApplyCode = (code: string, _language: string) => {
    // 复制代码到剪贴板
    navigator.clipboard.writeText(code).catch(err => {
      console.error('复制失败:', err);
    });
  };

  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const isSystem = message.role === 'system';
  const isTool = message.role === 'tool';
  const isChatRender = renderContext === 'chat';

  const historyProcess = isUser ? (message.metadata?.historyProcess as any) : null;

  // 部分历史数据只把过程信息保存在最终assistant消息里，user消息的historyProcess.hasProcess可能为false
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

  const hasHistoryProcess = !isChatRender && Boolean(
    historyProcess?.hasProcess
    || historyProcess?.loading
    || derivedProcessStats.hasProcess
    || derivedProcessStats.hasStreamingAssistant
    || derivedProcessStats.processMessageCount > 0
  );
  const historyProcessExpanded = isUser
    ? historyProcess?.expanded === true
    : false;
  const historyProcessLoading = isUser
    ? historyProcess?.loading === true
    : false;
  const historyToolCount = Math.max(
    Number(historyProcess?.toolCallCount || 0),
    derivedProcessStats.toolCallCount,
  );
  const historyThinkingCount = Math.max(
    Number(historyProcess?.thinkingCount || 0),
    derivedProcessStats.thinkingCount,
  );

  const isProcessAssistant = (
    isAssistant
    && Boolean(message.metadata?.historyProcessUserMessageId || message.metadata?.historyProcessTurnId)
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
      message.metadata?.historyFinalForUserMessageId
      || message.metadata?.historyFinalForTurnId
      || message.metadata?.historyProcessUserMessageId
      || message.metadata?.historyProcessTurnId
    )
  );
  const collapseAssistantProcessByDefault = (
    isTurnLinkedAssistant
    && !isProcessAssistant
    && !linkedUserExpandedForFinalAssistant
    && renderContext !== 'process_drawer'
  );
  const imRunMetadata = useMemo(() => {
    const raw = message.metadata?.im_run;
    return raw && typeof raw === 'object' ? raw as Record<string, unknown> : null;
  }, [message.metadata?.im_run]);
  const runtimeContextSessionId = normalizeText(
    imRunMetadata?.legacy_session_id ?? imRunMetadata?.execution_session_id,
  );
  const runtimeContextTurnId = normalizeText(
    imRunMetadata?.legacy_turn_id ?? imRunMetadata?.execution_turn_id,
  );
  const canInspectRuntimeContext = Boolean(
    isChatRender
    && isAssistant
    && runtimeContextSessionId,
  );

  // 隐藏tool角色的消息，因为它们应该作为工具调用的结果显示
  if (isTool) {
    return null;
  }

  // 使用自定义渲染器
  if (customRenderer?.renderMessage) {
    return <div>{customRenderer.renderMessage(message)}</div>;
  }

  const handleEdit = () => {
    if (onEdit && editContent.trim() !== message.content) {
      onEdit(message.id, editContent.trim());
    }
    setIsEditing(false);
  };

  const handleCancelEdit = () => {
    setEditContent(message.content);
    setIsEditing(false);
  };

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(message.content);
    } catch (error) {
      console.error('Failed to copy message:', error);
    }
  };

  const attachments = message.metadata?.attachments || [];
  // 获取工具调用数据 - 同时检查顶层和metadata中的toolCalls（兼容不同的数据格式）
  const toolCalls = (message as any).toolCalls || message.metadata?.toolCalls || [];
  const renderContentSegments = useMemo(
    () => normalizeContentSegmentsForRender(Array.isArray(message.metadata?.contentSegments) ? message.metadata.contentSegments : []),
    [message.metadata?.contentSegments],
  );
  const toolCallsById = useMemo(() => {
    if (!toolCalls || toolCalls.length === 0) return new Map<string, any>();
    const map = new Map<string, any>();
    for (const tc of toolCalls) {
      if (tc && tc.id) {
        map.set(tc.id, tc);
      }
    }
    return map;
  }, [toolCalls, toolCalls?.length]);

  return (
    <div
      className={cn(
        'group relative w-full transition-colors',
        isChatRender && 'flex',
        isChatRender && isUser && 'justify-end px-4 py-2',
        isChatRender && isAssistant && 'justify-start px-4 py-2',
        !isChatRender && !isAssistant && 'flex gap-3 px-4 py-4 rounded-lg',
        !isChatRender && isAssistant && 'px-4 py-2',
        !isChatRender && isUser && 'bg-user-message',
        !isChatRender && isSystem && 'bg-muted border-l-4 border-primary',
        !isChatRender && isTool && 'bg-blue-50 dark:bg-blue-950/20 border-l-4 border-blue-500',
        !isChatRender && 'hover:bg-opacity-80',
      )}
    >
      {isChatRender && isAssistant && (
        <div className="flex-shrink-0">
          <div className={cn(
            'w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium',
            'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-100'
          )}>
            A
          </div>
        </div>
      )}

      {/* 头像 - assistant消息不显示头像 */}
      {!isChatRender && !isAssistant && (
        <div className="flex-shrink-0">
          <div className={cn(
            'w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium',
            isUser && 'bg-primary text-primary-foreground',
            isSystem && 'bg-muted text-muted-foreground',
            isTool && 'bg-blue-500 text-white'
          )}>
            {isUser ? 'U' : isTool ? 'T' : 'S'}
          </div>
        </div>
      )}

      {/* 消息内容 */}
      <div
        className={cn(
          'min-w-0',
          isChatRender
            ? 'max-w-[82%]'
            : 'flex-1',
        )}
      >
        {/* 消息头部 - assistant消息不显示头部 */}
        {!isChatRender && !isAssistant && (
          <div className="flex items-center gap-2 mb-1">
            <span className="text-sm font-medium">
              {isUser ? 'You' : isTool ? 'Tool Result' : 'System'}
            </span>
            <span className="text-xs text-muted-foreground">
              {formatTime(message.createdAt)}
            </span>
            {message.metadata?.model && (
              <span className="text-xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
                {message.metadata.model}
              </span>
            )}
          </div>
        )}

        {/* 特殊渲染：会话摘要提示 */}
        {isUser && hasHistoryProcess && (
          <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
            <button
              type="button"
              onClick={() => onToggleTurnProcess?.(message.id)}
              disabled={historyProcessLoading || !onToggleTurnProcess}
              className="px-2 py-0.5 rounded border border-border bg-muted text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
            >
              {historyProcessLoading
                ? 'Loading...'
                : historyProcessExpanded
                  ? 'Hide process'
                  : 'Show process'}
            </button>
            <span className="px-2 py-0.5 rounded bg-muted text-muted-foreground">
              Tools: {historyToolCount}
            </span>
            <span className="px-2 py-0.5 rounded bg-muted text-muted-foreground">
              Thinking: {historyThinkingCount}
            </span>
          </div>
        )}

        {message.metadata?.type === 'session_summary' && (
          <div className="mb-3 border border-amber-300 dark:border-amber-600/50 bg-amber-50 dark:bg-amber-950/20 rounded-md p-3">
            <div className="text-xs text-amber-900 dark:text-amber-200 font-medium mb-1">
              上下文已压缩为摘要{typeof (message.metadata as any)?.keepLastN === 'number' ? `（保留最近 ${ (message.metadata as any).keepLastN } 条）` : ''}
            </div>
            <details className="group">
              <summary className="cursor-pointer text-xs text-muted-foreground select-none">
                查看摘要内容
              </summary>
              <div className="mt-2 prose prose-sm max-w-none">
                <MarkdownRenderer
                  content={(message.rawContent || message.metadata?.summary || '').toString()}
                  isStreaming={false}
                  onApplyCode={() => {}}
                />
              </div>
            </details>
          </div>
        )}

        {/* 附件 */}
        {attachments.length > 0 && (
          <div className="mb-3 space-y-2">
            {attachments.map((attachment) => (
              <AttachmentRenderer
                key={attachment.id}
                attachment={attachment}
                isUser={isUser}
                customRenderer={customRenderer?.renderAttachment}
              />
            ))}
          </div>
        )}

        {/* 动态渲染消息内容和工具调用 */}
        {isEditing ? (
          <div className="space-y-2">
            <textarea
              value={editContent}
              onChange={(e) => setEditContent(e.target.value)}
              className="w-full p-2 border rounded-md resize-none focus:outline-none focus:ring-2 focus:ring-primary"
              rows={3}
              autoFocus
            />
            <div className="flex gap-2">
              <button
                onClick={handleEdit}
                className="px-3 py-1 text-sm bg-primary text-primary-foreground rounded hover:bg-primary/90"
              >
                Save
              </button>
              <button
                onClick={handleCancelEdit}
                className="px-3 py-1 text-sm bg-muted text-muted-foreground rounded hover:bg-muted/80"
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <div
            className={cn(
              'space-y-3',
              isChatRender && 'rounded-2xl px-4 py-3 shadow-sm',
              isChatRender && isUser && 'bg-sky-100 text-slate-900 dark:bg-sky-900/40 dark:text-slate-50',
              isChatRender && isAssistant && 'bg-white border border-slate-200 text-slate-900 dark:bg-slate-900 dark:border-slate-700 dark:text-slate-50',
              isChatRender && isSystem && 'bg-muted',
            )}
          >
            <MessageContentRenderer
              message={message}
              isLast={isLast}
              isStreaming={isStreaming}
              renderContentSegments={renderContentSegments}
              toolCalls={toolCalls as ToolCall[]}
              toolCallsById={toolCallsById}
              assistantToolCallsById={assistantToolCallsById}
              toolResultById={toolResultById}
              collapseAssistantProcessByDefault={isChatRender ? true : collapseAssistantProcessByDefault}
              hideInternalProcess={isChatRender}
              onApplyCode={handleApplyCode}
            />
          </div>
        )}

        {/* Token使用信息 */}
        {message.tokensUsed && !isChatRender && (
          <div className="mt-2 text-xs text-muted-foreground">
            Tokens used: {message.tokensUsed}
          </div>
        )}

        {isChatRender && (
          <div
            className={cn(
              'mt-1 px-1 text-[11px] text-muted-foreground',
              isUser ? 'text-right' : 'text-left',
            )}
          >
            <div className={cn('flex items-center gap-2', isUser ? 'justify-end' : 'justify-start')}>
              <span>{formatTime(message.createdAt)}</span>
              {canInspectRuntimeContext ? (
                <button
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    onInspectRuntimeContext?.({
                      sessionId: runtimeContextSessionId,
                      turnId: runtimeContextTurnId || null,
                    });
                  }}
                  className="rounded-full border border-slate-300 px-2 py-0.5 text-[11px] text-slate-600 transition-colors hover:bg-slate-100 hover:text-slate-900 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-slate-100"
                >
                  查看本次模型上下文
                </button>
              ) : null}
            </div>
          </div>
        )}
      </div>

      {/* 操作按钮 */}
      {!isEditing && !isChatRender && (
        <div className="absolute top-2 right-2 flex gap-1 bg-background border rounded-md shadow-sm opacity-0 pointer-events-none group-hover:opacity-100 group-hover:pointer-events-auto transition-opacity">
          <button
            onClick={handleCopy}
            className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
            title="Copy message"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
          </button>
          
          {isUser && onEdit && (
            <button
              onClick={() => setIsEditing(true)}
              className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-foreground transition-colors"
              title="Edit message"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
              </svg>
            </button>
          )}
          
          {onDelete && (
            <button
              onClick={() => onDelete(message.id)}
              className="p-1.5 hover:bg-destructive/10 rounded text-muted-foreground hover:text-destructive transition-colors"
              title="Delete message"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
            </button>
          )}
        </div>
      )}
    </div>
  );
};

// 使用memo优化性能，只在关键props变化时重新渲染
export const MessageItem = memo(MessageItemComponent, (prevProps, nextProps) => {
  return (
    prevProps.message === nextProps.message &&
    prevProps.isLast === nextProps.isLast &&
    prevProps.isStreaming === nextProps.isStreaming &&
    (prevProps.renderContext ?? 'chat') === (nextProps.renderContext ?? 'chat') &&
    (prevProps.processSignal ?? "") === (nextProps.processSignal ?? "") &&
    (prevProps.toolCallLookupKey ?? "") === (nextProps.toolCallLookupKey ?? "") &&
    (prevProps.toolResultKey ?? "") === (nextProps.toolResultKey ?? "") &&
    (prevProps.linkedUserExpandedForAssistant ?? null) === (nextProps.linkedUserExpandedForAssistant ?? null)
  );
});

export default MessageItem;
