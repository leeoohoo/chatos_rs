// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import type { MessageTaskRunnerRunDetailResponse } from '../../lib/api/client/types';
import { CollapsibleSection, CollapsibleText } from './CollapsibleSection';
import { FieldGrid, MarkdownCard, ModalShell } from './parts';
import { RunEventTimeline } from './RunEventTimeline';
import { buildRunEventTimelineEntries } from './runEventTimelineUtils';
import { extractReportContent, formatDateTime, isRecord, readString } from './utils';

interface MessageTaskRunDetailModalProps {
  detail: MessageTaskRunnerRunDetailResponse | null;
  loadingMoreEvents?: boolean;
  onLoadMoreEvents?: () => void;
  onClose: () => void;
}

const shortId = (value: string): string => (
  value.length > 16 ? `${value.slice(0, 8)}...${value.slice(-4)}` : value
);

const formatModelConfig = (
  modelConfig: MessageTaskRunnerRunDetailResponse['model_config'],
  fallbackId?: string | null,
): string => {
  const id = readString(modelConfig?.id) || readString(fallbackId);
  const name = readString(modelConfig?.name);
  const provider = readString(modelConfig?.provider);
  const model = readString(modelConfig?.model);
  const providerModel = provider && model ? `${provider}/${model}` : provider || model;
  const label = [name, providerModel]
    .filter((item, index, items): item is string => Boolean(item) && items.indexOf(item) === index)
    .join(' · ');
  if (label) {
    return id ? `${label} (${shortId(id)})` : label;
  }
  return id ? `模型配置暂不可用 (${shortId(id)})` : '-';
};

const extractSandboxOutputCounts = (report: unknown): Record<string, unknown> | null => {
  if (!isRecord(report)) {
    return null;
  }
  const output = isRecord(report.output) ? report.output : null;
  const sandbox = output && isRecord(output.sandbox) ? output.sandbox : null;
  const counts = sandbox && isRecord(sandbox.file_change_counts)
    ? sandbox.file_change_counts
    : null;
  return counts;
};

export const MessageTaskRunDetailModal: FC<MessageTaskRunDetailModalProps> = ({
  detail,
  loadingMoreEvents = false,
  onLoadMoreEvents,
  onClose,
}) => {
  if (!detail) {
    return null;
  }
  const { run, task, events } = detail;
  const reportContent = extractReportContent(run.report);
  const modelRequestCount = events.filter((event) => event.event_type === 'model_request').length;
  const toolEventCount = events.filter((event) => event.event_type.includes('tool')).length;
  const timelineEntries = buildRunEventTimelineEntries(events);
  const eventsTotal = typeof detail.events_total === 'number'
    ? detail.events_total
    : events.length;
  const eventsHasMore = Boolean(detail.events_has_more);
  const resultSummary = readString(run.result_summary);
  const normalizedReportContent = readString(reportContent);
  const sandboxOutputCounts = extractSandboxOutputCounts(run.report);
  const hasDistinctReport = Boolean(
    normalizedReportContent
      && normalizedReportContent !== resultSummary,
  );

  return (
    <ModalShell
      title="运行详情"
      subtitle={task.title || run.task_id}
      onClose={onClose}
      widthClassName="max-w-6xl"
    >
      <FieldGrid
        items={[
          ['运行 ID', run.id],
          ['任务', task.title || run.task_id],
          ['状态', run.status],
          ['模型', formatModelConfig(detail.model_config, run.model_config_id)],
          ['开始时间', formatDateTime(run.started_at)],
          ['结束时间', formatDateTime(run.finished_at)],
          ['已加载事件', `${events.length}/${eventsTotal}`],
          ['当前页模型请求', modelRequestCount],
          ['当前页工具事件', toolEventCount],
        ]}
      />

      {resultSummary ? (
        <CollapsibleSection title="最终结果" defaultOpen>
          <MarkdownCard content={resultSummary} />
        </CollapsibleSection>
      ) : null}

      {run.error_message ? (
        <CollapsibleSection title="错误信息" defaultOpen>
          <CollapsibleText value={run.error_message} />
        </CollapsibleSection>
      ) : null}

      {hasDistinctReport ? (
        <CollapsibleSection title="执行报告">
          <MarkdownCard content={normalizedReportContent} />
        </CollapsibleSection>
      ) : null}

      {sandboxOutputCounts ? (
        <CollapsibleSection title="文件变更" defaultOpen>
          <FieldGrid
            items={[
              ['新增', sandboxOutputCounts.added ?? 0],
              ['修改', sandboxOutputCounts.modified ?? 0],
              ['删除', sandboxOutputCounts.deleted ?? 0],
              ['总计', sandboxOutputCounts.total ?? 0],
            ]}
          />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection
        title="运行事件"
        summary={events.length ? `已加载 ${events.length}/${eventsTotal} 条事件 · 聚合为 ${timelineEntries.length} 个节点` : '暂无事件'}
        defaultOpen={events.length > 0}
      >
        <RunEventTimeline entries={timelineEntries} />
        {eventsHasMore ? (
          <button
            type="button"
            className="mt-3 inline-flex items-center rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground"
            disabled={loadingMoreEvents}
            onClick={onLoadMoreEvents}
          >
            {loadingMoreEvents ? '加载中' : `加载更多事件（剩余 ${Math.max(eventsTotal - events.length, 0)}）`}
          </button>
        ) : null}
      </CollapsibleSection>

      <CollapsibleSection title="运行快照">
        <div className="space-y-3">
          <CollapsibleSection title="输入快照">
            <CollapsibleText value={run.input_snapshot || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="上下文快照">
            <CollapsibleText value={run.context_snapshot || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="用量">
            <CollapsibleText value={run.usage || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="完整 Report">
            <CollapsibleText value={run.report || '-'} code />
          </CollapsibleSection>
        </div>
      </CollapsibleSection>
    </ModalShell>
  );
};
