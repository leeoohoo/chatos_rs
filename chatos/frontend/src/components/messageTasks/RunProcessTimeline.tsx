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

export const RunProcessTimeline: FC<RunProcessTimelineProps> = ({ items }) => {
  if (!items.length) {
    return (
      <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm text-muted-foreground">
        暂无可展示的执行过程。
      </div>
    );
  }

  const summary = buildTimelineSummary(items);

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <SummaryPill label="模型过程" value={summary.model} />
        <SummaryPill label="工具调用" value={summary.toolCall} />
        <SummaryPill label="工具返回" value={summary.toolResult} />
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
