// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState, type FC } from 'react';
import { RefreshCw } from 'lucide-react';
import { getMessageTaskRunnerRunChanges } from '../../lib/api/client/messages';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type {
  MessageTaskRunnerRunChanges,
  MessageTaskRunnerRunDetailResponse,
} from '../../lib/api/client/types';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { sanitizeUserVisibleAppError } from '../../lib/domain/userVisibleError';
import { CollapsibleSection, CollapsibleText } from './CollapsibleSection';
import { FieldGrid, MarkdownCard, ModalShell } from './parts';
import { RunEventTimeline } from './RunEventTimeline';
import { RunProcessTimeline } from './RunProcessTimeline';
import { buildRunEventTimelineEntries } from './runEventTimelineUtils';
import { buildRunProcessTimelineItems } from './runProcessTimelineModel';
import { BrowserSessionEventsCard } from './BrowserSessionEventsCard';
import { PluginRunSnapshotCard } from './PluginRunSnapshotCard';
import { PluginRuntimeEventsCard } from './PluginRuntimeEventsCard';
import { PluginUiWorkbenchCard } from './PluginUiWorkbenchCard';
import { extractReportContent, formatDateTime, readString } from './utils';

interface MessageTaskRunDetailModalProps {
  detail: MessageTaskRunnerRunDetailResponse | null;
  messageId?: string;
  taskLookup?: MessageTaskRunnerLookupOptions;
  loadingMoreEvents?: boolean;
  refreshing?: boolean;
  onLoadMoreEvents?: () => void;
  onRefresh?: () => void;
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

export const MessageTaskRunDetailModal: FC<MessageTaskRunDetailModalProps> = ({
  detail,
  messageId,
  taskLookup,
  loadingMoreEvents = false,
  refreshing = false,
  onLoadMoreEvents,
  onRefresh,
  onClose,
}) => {
  const apiClient = useApiClient();
  const [changes, setChanges] = useState<MessageTaskRunnerRunChanges | null>(null);
  const [changesLoading, setChangesLoading] = useState(false);
  const [changesError, setChangesError] = useState<string | null>(null);
  useEffect(() => {
    setChanges(null);
    setChangesLoading(false);
    setChangesError(null);
  }, [detail?.run.id]);
  if (!detail) {
    return null;
  }
  const { run, task, events } = detail;
  const reportContent = extractReportContent(run.report);
  const modelRequestCount = events.filter((event) => event.event_type === 'model_request').length;
  const toolEventCount = events.filter((event) => event.event_type.includes('tool')).length;
  const processTimelineItems = buildRunProcessTimelineItems(events);
  const rawTimelineEntries = buildRunEventTimelineEntries(events);
  const eventsTotal = typeof detail.events_total === 'number'
    ? detail.events_total
    : events.length;
  const eventsHasMore = Boolean(detail.events_has_more);
  const resultSummary = readString(run.result_summary);
  const normalizedReportContent = readString(reportContent);
  const userVisibleError = run.error_message
    ? sanitizeUserVisibleAppError(run.error_message)
    : null;
  const hasDistinctReport = Boolean(
    normalizedReportContent
      && normalizedReportContent !== resultSummary,
  );
  const canLoadChanges = Boolean(
    messageId
      && taskLookup
      && readString(run.workspace_execution?.result_commit),
  );

  const loadChanges = async () => {
    if (!messageId || !taskLookup || changesLoading) {
      return;
    }
    setChangesLoading(true);
    setChangesError(null);
    try {
      setChanges(await getMessageTaskRunnerRunChanges(
        apiClient.getRequestFn(),
        messageId,
        run.id,
        taskLookup,
      ));
    } catch (error) {
      setChangesError(error instanceof Error ? error.message : '读取任务代码变更失败');
    } finally {
      setChangesLoading(false);
    }
  };

  return (
    <ModalShell
      title="运行详情"
      subtitle={task.title || run.task_id}
      onClose={onClose}
      widthClassName="max-w-6xl"
    >
      {onRefresh ? (
        <div className="flex justify-end">
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent disabled:cursor-wait disabled:opacity-60"
            disabled={refreshing}
            onClick={onRefresh}
          >
            <RefreshCw className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin' : ''}`} />
            {refreshing ? '正在刷新' : '刷新运行详情'}
          </button>
        </div>
      ) : null}
      <FieldGrid
        items={[
          ['运行 ID', run.id],
          ['任务', task.title || run.task_id],
          ['状态', run.status],
          ['模型阶段', run.model_phase_status],
          ['代码集成', run.workspace_execution?.integration_status || 'not_required'],
          ['执行批次分支', run.workspace_execution?.execution_branch_ref || '-'],
          ['冲突文件', run.workspace_execution?.conflict_files?.join('、') || '-'],
          ['模型', formatModelConfig(detail.model_config, run.model_config_id)],
          ['开始时间', formatDateTime(run.started_at)],
          ['结束时间', formatDateTime(run.finished_at)],
          ['已加载事件', `${events.length}/${eventsTotal}`],
          ['已加载模型请求', modelRequestCount],
          ['已加载工具事件', toolEventCount],
        ]}
      />

      <PluginRunSnapshotCard inputSnapshot={run.input_snapshot} />
      {canLoadChanges ? (
        <CollapsibleSection
          title="代码变更"
          summary={changes ? `${changes.files.length} 个文件` : '查看任务分支相对执行基线的真实差异'}
          defaultOpen={Boolean(changes || changesError)}
        >
          {!changes ? (
            <button
              type="button"
              className="inline-flex items-center rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent disabled:cursor-wait disabled:opacity-60"
              disabled={changesLoading}
              onClick={() => void loadChanges()}
            >
              {changesLoading ? '正在读取代码变更' : '查看代码变更'}
            </button>
          ) : (
            <div className="space-y-3">
              <CollapsibleText value={changes.files} code />
              <CollapsibleText value={changes.patch || '没有代码差异'} code />
              {changes.patch_truncated ? (
                <p className="text-xs text-amber-700 dark:text-amber-300">差异内容过长，已截断展示。</p>
              ) : null}
            </div>
          )}
          {changesError ? (
            <p role="alert" className="mt-2 text-xs text-destructive">{changesError}</p>
          ) : null}
        </CollapsibleSection>
      ) : null}
      <BrowserSessionEventsCard events={events} />
      <PluginRuntimeEventsCard events={events} />
      {messageId && taskLookup ? (
        <PluginUiWorkbenchCard events={events} messageId={messageId} lookup={taskLookup} />
      ) : null}

      {resultSummary ? (
        <CollapsibleSection title="最终结果" defaultOpen>
          <MarkdownCard content={resultSummary} />
        </CollapsibleSection>
      ) : null}

      {userVisibleError ? (
        <CollapsibleSection title="错误信息" defaultOpen>
          <CollapsibleText value={userVisibleError} />
        </CollapsibleSection>
      ) : null}

      {hasDistinctReport ? (
        <CollapsibleSection title="执行报告">
          <MarkdownCard content={normalizedReportContent} />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection
        title="运行事件时间线（诊断）"
        summary={events.length ? `已加载 ${events.length}/${eventsTotal} 条运行记录 · 聚合为 ${processTimelineItems.length} 个诊断步骤` : '暂无事件'}
        defaultOpen={events.length > 0}
      >
        <RunProcessTimeline items={processTimelineItems} />
        {eventsHasMore ? (
          <button
            type="button"
            className="mt-3 inline-flex items-center rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground"
            disabled={loadingMoreEvents}
            onClick={onLoadMoreEvents}
          >
            {loadingMoreEvents ? '加载中' : `加载更多诊断事件（剩余 ${Math.max(eventsTotal - events.length, 0)}）`}
          </button>
        ) : null}
      </CollapsibleSection>

      <CollapsibleSection
        title="原始运行事件（诊断）"
        summary={events.length ? `${events.length} 条事件 · 聚合为 ${rawTimelineEntries.length} 个节点` : '暂无事件'}
      >
        <RunEventTimeline entries={rawTimelineEntries} />
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
