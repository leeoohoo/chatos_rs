import React, { useEffect, useMemo, useState, memo } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';
import { AttachmentRenderer } from './AttachmentRenderer';
import { ToolCallRenderer } from './ToolCallRenderer';
import { cn, formatTime } from '../lib/utils';
import type { Message, Attachment, ToolCall } from '../types';

interface ToolCallTimelineProps {
  toolCalls: ToolCall[];
  allMessages: Message[];
  toolResultById?: Map<string, Message>;
}

const ToolCallTimeline: React.FC<ToolCallTimelineProps> = ({
  toolCalls,
  allMessages,
  toolResultById,
}) => {
  const [expanded, setExpanded] = useState(false);

  const resolveToolResult = (toolCall: ToolCall) => {
    if (toolCall.result !== undefined && toolCall.result !== null) return toolCall.result;
    const direct = toolResultById?.get(String(toolCall.id));
    if (direct?.content !== undefined && direct?.content !== null) return direct.content;
    const fallback = allMessages.find(msg => {
      if (msg.role !== 'tool') return false;
      const topLevelId = (msg as any).tool_call_id || (msg as any).toolCallId;
      const metadataId = msg.metadata?.tool_call_id || msg.metadata?.toolCallId;
      return topLevelId === toolCall.id || metadataId === toolCall.id;
    });
    return fallback?.content;
  };

  const getToolStatus = (toolCall: ToolCall) => {
    if (toolCall.error) return 'error';
    const result = resolveToolResult(toolCall);
    if (result !== undefined && result !== null) return 'success';
    return 'pending';
  };

  const summaryStatus = useMemo(() => {
    let hasError = false;
    let allDone = true;
    toolCalls.forEach(tc => {
      const status = getToolStatus(tc);
      if (status === 'error') hasError = true;
      if (status !== 'success') allDone = false;
    });
    if (hasError) return 'error';
    if (allDone) return 'success';
    return 'pending';
  }, [toolCalls, allMessages, toolResultById]);

  const summaryNames = useMemo(() => {
    const names = toolCalls.map(tc => tc?.name).filter(Boolean);
    if (names.length === 0) return '';
    const shown = names.slice(0, 2).map(name => `@${name}`).join(' · ');
    const more = names.length - 2;
    return more > 0 ? `${shown} · +${more}` : shown;
  }, [toolCalls]);

  const statusDotClass = summaryStatus === 'error'
    ? 'bg-red-500'
    : summaryStatus === 'success'
      ? 'bg-emerald-500'
      : 'bg-amber-500';

  return (
    <div className="rounded-md border border-border bg-muted/30">
      <div className="flex items-center justify-between px-3 py-2">
        <div className="flex items-center gap-2 text-xs text-muted-foreground min-w-0">
          <span className={`inline-flex h-2 w-2 rounded-full ${statusDotClass}`} />
          <span className="font-medium text-foreground">工具调用</span>
          <span>· {toolCalls.length} 个</span>
          {summaryNames && (
            <span className="hidden sm:inline truncate">{summaryNames}</span>
          )}
        </div>
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
          aria-label={expanded ? '收起工具时间线' : '展开工具时间线'}
          aria-expanded={expanded}
        >
          <span>{expanded ? '收起' : '展开'}</span>
          <svg className={`w-3 h-3 transition-transform ${expanded ? 'rotate-180' : ''}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
            <polyline points="6 9 12 15 18 9" />
          </svg>
        </button>
      </div>

      {expanded && (
        <div className="px-3 pb-3 space-y-2">
          {toolCalls.map((toolCall, index) => {
            const status = getToolStatus(toolCall);
            const dotClass = status === 'error'
              ? 'bg-red-500'
              : status === 'success'
                ? 'bg-emerald-500'
                : 'bg-amber-500';
            return (
              <div key={toolCall.id || `tool-${index}`} className="flex gap-3">
                <div className="relative flex flex-col items-center pt-1">
                  <span className={`h-2.5 w-2.5 rounded-full ${dotClass}`} />
                  {index < toolCalls.length - 1 && (
                    <span className="w-px flex-1 bg-border mt-1" />
                  )}
                </div>
                <div className="flex-1">
                  <ToolCallRenderer
                    toolCall={toolCall}
                    allMessages={allMessages}
                    toolResultById={toolResultById}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};



interface MessageItemProps {
  message: Message;
  isLast?: boolean;
  isStreaming?: boolean;
  onEdit?: (messageId: string, content: string) => void;
  onDelete?: (messageId: string) => void;
  onToggleTurnProcess?: (userMessageId: string) => void;
  allMessages?: Message[]; // 添加所有消息的引用
  toolResultById?: Map<string, Message>;
  toolResultKey?: string;
  customRenderer?: {
    renderMessage?: (message: Message) => React.ReactNode;
    renderAttachment?: (attachment: Attachment) => React.ReactNode;
  };
}

const MessageItemComponent: React.FC<MessageItemProps> = ({
  message,
  isLast = false,
  isStreaming = false,
  onEdit,
  onDelete,
  onToggleTurnProcess,
  allMessages = [],
  toolResultById,
  customRenderer,
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

  const historyProcess = isUser ? (message.metadata?.historyProcess as any) : null;
  const hasHistoryProcess = Boolean(historyProcess?.hasProcess);
  const historyProcessExpanded = historyProcess?.expanded === true;
  const historyProcessLoading = historyProcess?.loading === true;
  const historyToolCount = Number(historyProcess?.toolCallCount || 0);
  const historyThinkingCount = Number(historyProcess?.thinkingCount || 0);

  const turnProcessExpanded = message.metadata?.historyProcessExpanded === true;
  const collapseAssistantProcessByDefault = (
    isAssistant
    && Boolean(message.metadata?.historyFinalForUserMessageId || message.metadata?.historyProcessUserMessageId)
    && !turnProcessExpanded
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
        'group relative rounded-lg transition-colors',
        // 基础布局样式 - 所有消息都使用统一的左对齐布局
        !isAssistant && 'flex gap-3 px-4 py-4',
        // assistant消息使用简化布局（无头像无头部）
        isAssistant && 'px-4 py-2',
        // 角色特定样式 - 移除左右对齐差异，统一左对齐
        isUser && 'bg-user-message',
        isSystem && 'bg-muted border-l-4 border-primary',
        isTool && 'bg-blue-50 dark:bg-blue-950/20 border-l-4 border-blue-500',
        'hover:bg-opacity-80'
      )}
    >
      {/* 头像 - assistant消息不显示头像 */}
      {!isAssistant && (
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
      <div className="flex-1 min-w-0">
        {/* 消息头部 - assistant消息不显示头部 */}
        {!isAssistant && (
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
          <div className="space-y-3">
            {/* 使用新的内容分段渲染机制 */}
            {(() => {
              const contentSegments = message.metadata?.contentSegments || [];
              const hasContent = message.content && message.content.trim().length > 0;
              const isCurrentlyStreaming = isStreaming && isLast;
              
              // 如果有内容分段，使用分段渲染
              if (contentSegments.length > 0) {
                const nodes: React.ReactNode[] = [];
                let index = 0;
                while (index < contentSegments.length) {
                  const segment = contentSegments[index];

                  if (segment.type === 'tool_call') {
                    const groupedToolCalls: ToolCall[] = [];
                    let j = index;
                    while (j < contentSegments.length && contentSegments[j].type === 'tool_call') {
                      const seg = contentSegments[j];
                      const toolCall = seg.toolCallId ? toolCallsById.get(seg.toolCallId) : undefined;
                      if (toolCall) groupedToolCalls.push(toolCall);
                      j += 1;
                    }

                    if (collapseAssistantProcessByDefault) {
                      index = j;
                      continue;
                    }

                    if (groupedToolCalls.length > 0) {
                      nodes.push(
                        <ToolCallTimeline
                          key={`tool-group-${index}`}
                          toolCalls={groupedToolCalls}
                          allMessages={allMessages}
                          toolResultById={toolResultById}
                        />
                      );
                    }

                    index = j;
                    continue;
                  }

                  if (segment.type === 'text') {
                    nodes.push(
                      <div key={`segment-${index}`} className="prose prose-sm max-w-none">
                        <MarkdownRenderer
                          content={segment.content as string}
                          isStreaming={isCurrentlyStreaming && index === contentSegments.length - 1}
                          onApplyCode={handleApplyCode}
                        />
                      </div>
                    );
                    index += 1;
                    continue;
                  }

                  if (segment.type === 'thinking') {
                    if (collapseAssistantProcessByDefault) {
                      index += 1;
                      continue;
                    }

                    nodes.push(
                      <details
                        key={`thinking-${index}`}
                        className="group border border-gray-200 dark:border-gray-700 rounded-md bg-muted px-3 py-2"
                      >
                        <summary className="cursor-pointer text-xs text-gray-500 dark:text-gray-400 select-none">
                          Thinking
                        </summary>
                        <div className="mt-1">
                          <MarkdownRenderer
                            content={(segment.content as string) || ''}
                            isStreaming={isCurrentlyStreaming && index === contentSegments.length - 1}
                            onApplyCode={handleApplyCode}
                            className="thinking not-prose"
                          />
                        </div>
                      </details>
                    );
                    index += 1;
                    continue;
                  }

                  index += 1;
                }

                return <div className="space-y-0.5">{nodes}</div>;
              }
              
              // 回退到传统渲染方式（向后兼容）
              return (
                <div className="space-y-0.5">
                  {/* 渲染文本内容 */}
                  {hasContent && (
                    <div className="prose prose-sm max-w-none">
                      <MarkdownRenderer
                        content={message.content}
                        isStreaming={isCurrentlyStreaming}
                        onApplyCode={handleApplyCode}
                      />
                    </div>
                  )}
                  
                  {/* 渲染工具调用（历史消息兼容） - 修复：确保工具调用总是被渲染 */}
                  {!collapseAssistantProcessByDefault && toolCalls.length > 0 && (
                    <div className="space-y-0.5">
                      <ToolCallTimeline
                        toolCalls={toolCalls as ToolCall[]}
                        allMessages={allMessages}
                        toolResultById={toolResultById}
                      />
                     </div>
                  )}
                </div>
              );
            })()}
          </div>
        )}

        {/* Token使用信息 */}
        {message.tokensUsed && (
          <div className="mt-2 text-xs text-muted-foreground">
            Tokens used: {message.tokensUsed}
          </div>
        )}
      </div>

      {/* 操作按钮 */}
      {!isEditing && (
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
  const getTime = (value: unknown): number => {
    if (!value) return 0;
    if (value instanceof Date) return value.getTime();
    const parsed = new Date(value as any).getTime();
    return Number.isNaN(parsed) ? 0 : parsed;
  };

  const summarizeValue = (value: unknown): string => {
    if (value === null || value === undefined) return "";
    if (typeof value === "string") {
      if (!value) return "";
      const head = value.slice(0, 24);
      const tail = value.slice(-24);
      return `${value.length}:${head}:${tail}`;
    }
    try {
      const raw = JSON.stringify(value);
      if (!raw) return "";
      return `${raw.length}:${raw.slice(0, 24)}:${raw.slice(-24)}`;
    } catch {
      return String(value);
    }
  };

  const getToolCalls = (message: Message): any[] => {
    const topLevel = (message as any).toolCalls;
    if (Array.isArray(topLevel) && topLevel.length > 0) return topLevel;
    const fromMeta = message.metadata?.toolCalls;
    return Array.isArray(fromMeta) ? fromMeta : [];
  };

  const getToolCallsKey = (message: Message): string => {
    const toolCalls = getToolCalls(message);
    if (toolCalls.length === 0) return "";
    return toolCalls
      .map((toolCall: any) => {
        const id = String(toolCall?.id ?? "");
        const name = String(toolCall?.name ?? "");
        const completed = toolCall?.completed === true ? "1" : "0";
        const error = summarizeValue(toolCall?.error ?? "");
        const streamLog = summarizeValue(toolCall?.streamLog ?? toolCall?.stream_log ?? "");
        const finalResult = summarizeValue(toolCall?.finalResult ?? toolCall?.final_result ?? "");
        const result = summarizeValue(toolCall?.result ?? "");
        const args = summarizeValue(toolCall?.arguments ?? toolCall?.args ?? "");
        return `${id}~${name}~${completed}~${error}~${streamLog}~${finalResult}~${result}~${args}`;
      })
      .join("|");
  };

  const getContentSegmentsKey = (meta?: Message["metadata"]): string => {
    const segments = meta?.contentSegments;
    if (!Array.isArray(segments) || segments.length === 0) return "";
    return segments
      .map((segment: any, index: number) => {
        const type = String(segment?.type ?? "");
        const toolCallId = String(segment?.toolCallId ?? "");
        const content = summarizeValue(segment?.content ?? "");
        return `${index}:${type}:${toolCallId}:${content}`;
      })
      .join("|");
  };

  const getMetaKey = (meta?: Message["metadata"]): string => {
    if (!meta) return "";
    const attachmentsLen = meta.attachments?.length ?? 0;
    const summary = meta.summary ?? "";
    const model = meta.model ?? "";
    const hidden = (meta as any).hidden ? "1" : "0";
    const currentSegmentIndex = String(meta.currentSegmentIndex ?? "");
    const contentSegmentsKey = getContentSegmentsKey(meta);
    const process = (meta as any).historyProcess || {};
    const processKey = `${process.hasProcess ? '1' : '0'}:${process.toolCallCount ?? 0}:${process.thinkingCount ?? 0}:${process.expanded ? '1' : '0'}:${process.loading ? '1' : '0'}:${process.loaded ? '1' : '0'}`;
    const processUserId = String((meta as any).historyProcessUserMessageId ?? "");
    const processPlaceholder = (meta as any).historyProcessPlaceholder ? "1" : "0";
    const processLoaded = (meta as any).historyProcessLoaded ? "1" : "0";
    const processExpanded = (meta as any).historyProcessExpanded ? "1" : "0";
    const finalForUser = String((meta as any).historyFinalForUserMessageId ?? "");
    return `${attachmentsLen}|${summary}|${model}|${hidden}|${currentSegmentIndex}|${contentSegmentsKey}|${processKey}|${processUserId}|${processPlaceholder}|${processLoaded}|${processExpanded}|${finalForUser}`;
  };

  // 比较关键属性
  return (
    prevProps.message.id === nextProps.message.id &&
    prevProps.message.content === nextProps.message.content &&
    prevProps.message.rawContent === nextProps.message.rawContent &&
    getTime(prevProps.message.createdAt) === getTime(nextProps.message.createdAt) &&
    prevProps.message.status === nextProps.message.status &&
    prevProps.message.tokensUsed === nextProps.message.tokensUsed &&
    getTime(prevProps.message.updatedAt) === getTime(nextProps.message.updatedAt) &&
    prevProps.isLast === nextProps.isLast &&
    prevProps.isStreaming === nextProps.isStreaming &&
    getMetaKey(prevProps.message.metadata) === getMetaKey(nextProps.message.metadata) &&
    getToolCallsKey(prevProps.message) === getToolCallsKey(nextProps.message) &&
    (prevProps.toolResultKey ?? "") === (nextProps.toolResultKey ?? "")
  );
});

export default MessageItem;
