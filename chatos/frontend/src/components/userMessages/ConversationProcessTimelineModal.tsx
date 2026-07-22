// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useMemo } from 'react';
import { X } from 'lucide-react';

import type { Message } from '../../types';
import type { UserMessageTurn } from './types';
import {
  SummaryPill,
  TimelineDot,
  renderTimelineCard,
} from './ConversationProcessTimelineCards';
import {
  buildTimelineItems,
  buildTimelineSummary,
  formatTime,
  selectTurnProcessMessages,
} from './ConversationProcessTimelineModel';

interface ConversationProcessTimelineModalProps {
  item: UserMessageTurn | null;
  messages: Message[];
  loading: boolean;
  error: string | null;
  onClose: () => void;
}

export const ConversationProcessTimelineModal: React.FC<ConversationProcessTimelineModalProps> = ({
  item,
  messages,
  loading,
  error,
  onClose,
}) => {
  const processMessages = useMemo(
    () => selectTurnProcessMessages(messages, item),
    [item, messages],
  );
  const timelineItems = useMemo(
    () => buildTimelineItems(processMessages),
    [processMessages],
  );
  const summary = useMemo(
    () => buildTimelineSummary(timelineItems),
    [timelineItems],
  );

  if (!item) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[70]">
      <button
        type="button"
        className="absolute inset-0 bg-black/45"
        aria-label="关闭"
        onClick={onClose}
      />
      <div className="absolute left-1/2 top-1/2 flex max-h-[88vh] w-[calc(100vw-24px)] max-w-5xl -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl sm:w-[calc(100vw-40px)]">
        <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-foreground">执行过程</h2>
            <p className="mt-0.5 truncate text-xs text-muted-foreground">
              {formatTime(item.userMessage.createdAt)} · {loading ? item.processMessageCount : processMessages.length} 条过程消息 · {timelineItems.length} 个事件
            </p>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
            onClick={onClose}
            aria-label="关闭"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {loading ? (
            <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
              正在加载执行过程...
            </div>
          ) : error ? (
            <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-3 text-sm text-destructive">
              {error}
            </div>
          ) : timelineItems.length === 0 ? (
            <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
              暂无可展示的过程消息。
            </div>
          ) : (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                <SummaryPill label="模型过程" value={summary.model} />
                <SummaryPill label="工具调用" value={summary.toolCall} />
                <SummaryPill label="工具返回" value={summary.toolResult} />
                <SummaryPill label="错误" value={summary.error} />
              </div>
              <ol className="relative space-y-3 before:absolute before:bottom-4 before:left-[13px] before:top-4 before:w-px before:bg-border">
                {timelineItems.map((timelineItem) => (
                  <li key={timelineItem.id} className="relative pl-9">
                    <TimelineDot
                      type={timelineItem.type}
                      status={'status' in timelineItem ? timelineItem.status : undefined}
                    />
                    {renderTimelineCard(timelineItem)}
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
