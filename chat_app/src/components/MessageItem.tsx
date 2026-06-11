import React, { useEffect, useState, memo } from 'react';
import { AttachmentRenderer } from './AttachmentRenderer';
import { cn } from '../lib/utils';
import type { ToolCall } from '../types';
import { MessageContentRenderer } from './messageItem/MessageContentRenderer';
import { HistoryProcessSummary } from './messageItem/HistoryProcessSummary';
import { MessageActions } from './messageItem/MessageActions';
import { MessageAvatar } from './messageItem/MessageAvatar';
import { MessageEditForm } from './messageItem/MessageEditForm';
import { MessageHeader } from './messageItem/MessageHeader';
import { SessionSummaryCard } from './messageItem/SessionSummaryCard';
import type { MessageItemProps } from './messageItem/messageItemTypes';
import { useMessageItemModel } from './messageItem/useMessageItemModel';
export type { DerivedProcessStats } from './messageItem/types';
export type { MessageItemProps } from './messageItem/messageItemTypes';

const readDisplayName = (value: unknown): string | null => {
  if (typeof value !== 'string') {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
};

const resolveTaskRunnerAssistantDisplayName = (
  message: MessageItemProps['message'],
  fallbackContactName: string | null | undefined,
): string | null => {
  const taskRunnerAsync = message.metadata?.task_runner_async;
  return readDisplayName(taskRunnerAsync?.contact_display_name)
    || readDisplayName(taskRunnerAsync?.agent_name_snapshot)
    || readDisplayName(taskRunnerAsync?.contact_name)
    || readDisplayName(fallbackContactName);
};

const MessageItemComponent: React.FC<MessageItemProps> = ({
  message,
  isLast = false,
  isStreaming = false,
  assistantContactName = null,
  onEdit,
  onDelete,
  onToggleTurnProcess,
  hideHistoryProcessSummary = false,
  derivedProcessStatsByUserId,
  toolResultById,
  assistantToolCallsById,
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

  const {
    isUser,
    isAssistant,
    isSystem,
    isTool,
    isTaskRunnerAsyncAssistant,
    hasHistoryProcess,
    historyToolCount,
    historyThinkingCount,
    historyUnavailableToolCount,
    collapseAssistantProcessByDefault,
    attachments,
    keepLastN,
    toolCalls,
    renderContentSegments,
    toolCallsById,
    shouldHideEmptyStreamingAssistant,
  } = useMessageItemModel({
    message,
    isStreaming,
    derivedProcessStatsByUserId,
  });
  const showAssistantChrome = isAssistant && isTaskRunnerAsyncAssistant;
  const useCompactAssistantLayout = isAssistant && !showAssistantChrome;
  const assistantDisplayName = showAssistantChrome
    ? resolveTaskRunnerAssistantDisplayName(message, assistantContactName)
    : null;

  // 隐藏tool角色的消息，因为它们应该作为工具调用的结果显示
  if (isTool) {
    return null;
  }

  if (shouldHideEmptyStreamingAssistant) {
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

  return (
    <div
      className={cn(
        'group relative rounded-lg transition-colors',
        // 基础布局样式 - 所有消息都使用统一的左对齐布局
        (!isAssistant || showAssistantChrome) && 'flex gap-3 px-4 py-4',
        // assistant消息使用简化布局（无头像无头部）
        useCompactAssistantLayout && 'px-4 py-2',
        // 角色特定样式 - 移除左右对齐差异，统一左对齐
        isUser && 'bg-user-message',
        isSystem && 'bg-muted border-l-4 border-primary',
        isTool && 'bg-blue-50 dark:bg-blue-950/20 border-l-4 border-blue-500',
        'hover:bg-opacity-80'
      )}
    >
      {/* 头像 - assistant消息不显示头像 */}
      {(!isAssistant || showAssistantChrome) && (
        <MessageAvatar
          isUser={isUser}
          isAssistant={isAssistant}
          isSystem={isSystem}
          isTool={isTool}
          assistantDisplayName={assistantDisplayName}
        />
      )}

      {/* 消息内容 */}
      <div className="flex-1 min-w-0">
        {/* 消息头部 - assistant消息不显示头部 */}
        {(!isAssistant || showAssistantChrome) && (
          <MessageHeader
            message={message}
            isUser={isUser}
            isAssistant={isAssistant}
            isTool={isTool}
            assistantDisplayName={assistantDisplayName}
          />
        )}

        {/* 特殊渲染：会话摘要提示 */}
        {isUser && hasHistoryProcess && !hideHistoryProcessSummary && (
          <HistoryProcessSummary
            userMessageId={message.id}
            historyToolCount={historyToolCount}
            historyThinkingCount={historyThinkingCount}
            historyUnavailableToolCount={historyUnavailableToolCount}
            onToggleTurnProcess={onToggleTurnProcess}
          />
        )}

        {message.metadata?.type === 'session_summary' && (
          <SessionSummaryCard
            message={message}
            keepLastN={keepLastN}
          />
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
          <MessageEditForm
            editContent={editContent}
            onEditContentChange={setEditContent}
            onSave={handleEdit}
            onCancel={handleCancelEdit}
          />
        ) : (
          <div className="space-y-3">
            <MessageContentRenderer
              message={message}
              isLast={isLast}
              isStreaming={isStreaming}
              renderContentSegments={renderContentSegments}
              toolCalls={toolCalls as ToolCall[]}
              toolCallsById={toolCallsById}
              assistantToolCallsById={assistantToolCallsById}
              toolResultById={toolResultById}
              collapseAssistantProcessByDefault={collapseAssistantProcessByDefault}
              onApplyCode={handleApplyCode}
            />
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
        <MessageActions
          isUser={isUser}
          canEdit={Boolean(onEdit)}
          canDelete={Boolean(onDelete)}
          onCopy={handleCopy}
          onStartEdit={() => setIsEditing(true)}
          onDelete={() => onDelete?.(message.id)}
        />
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
    (prevProps.assistantContactName ?? null) === (nextProps.assistantContactName ?? null) &&
    (prevProps.processSignal ?? "") === (nextProps.processSignal ?? "") &&
    (prevProps.toolCallLookupKey ?? "") === (nextProps.toolCallLookupKey ?? "") &&
    (prevProps.toolResultKey ?? "") === (nextProps.toolResultKey ?? "")
  );
});

export default MessageItem;
