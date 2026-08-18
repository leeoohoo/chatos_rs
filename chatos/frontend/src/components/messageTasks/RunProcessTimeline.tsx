// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import {
  SummaryPill,
  TimelineDot,
  renderTimelineCard,
} from '../userMessages/ConversationProcessTimelineCards';
import {
  buildTimelineSummary,
  type TimelineItem,
} from '../userMessages/ConversationProcessTimelineModel';

interface RunProcessTimelineProps {
  items: TimelineItem[];
}

const currentActivity = (items: TimelineItem[]): string | null => {
  const pendingTool = [...items]
    .reverse()
    .find((item): item is Extract<TimelineItem, { type: 'tool_call' }> => (
      item.type === 'tool_call' && item.status === 'pending'
    ));
  if (pendingTool) {
    const toolName = pendingTool.toolCall.name.toLowerCase();
    return toolName.includes('process_wait') || toolName.includes('process_poll')
      ? '当前正在等待工具进程完成'
      : '当前正在调用工具';
  }

  const lastItem = items[items.length - 1];
  if (lastItem?.type === 'model' && lastItem.label === '模型请求') {
    return '当前正在等待模型响应';
  }
  return null;
};

export const RunProcessTimeline: FC<RunProcessTimelineProps> = ({ items }) => {
  if (!items.length) {
    return (
      <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
        暂无可展示的诊断事件。
      </div>
    );
  }

  const summary = buildTimelineSummary(items);
  const activity = currentActivity(items);

  return (
    <div className="space-y-4">
      {activity ? (
        <div
          role="status"
          className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
        >
          {activity}
        </div>
      ) : null}
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <SummaryPill label="模型事件" value={summary.model} />
        <SummaryPill label="工具调用事件" value={summary.toolCall} />
        <SummaryPill label="工具返回事件" value={summary.toolResult} />
        <SummaryPill label="错误" value={summary.error} />
      </div>
      <ol className="relative space-y-3 before:absolute before:bottom-4 before:left-[13px] before:top-4 before:w-px before:bg-border">
        {items.map((item) => (
          <li key={item.id} className="relative pl-9">
            <TimelineDot
              type={item.type}
              status={'status' in item ? item.status : undefined}
            />
            {renderTimelineCard(item)}
          </li>
        ))}
      </ol>
    </div>
  );
};
