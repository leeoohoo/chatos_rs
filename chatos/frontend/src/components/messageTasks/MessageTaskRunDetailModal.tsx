// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import type { MessageTaskRunnerLookupOptions } from '../../lib/api/client/messages';
import type { MessageTaskRunnerRunDetailResponse } from '../../lib/api/client/types';
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
import { extractReportContent, formatDateTime, isRecord, readString } from './utils';

interface MessageTaskRunDetailModalProps {
  detail: MessageTaskRunnerRunDetailResponse | null;
  messageId?: string;
  taskLookup?: MessageTaskRunnerLookupOptions;
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

interface RunExecutionLocation {
  environmentMode: string | null;
  sandboxEnabled: boolean | null;
  sandboxProvider: string | null;
  sandboxId: string | null;
  leaseId: string | null;
  leaseExpiresAt: string | null;
  harnessRepoPath: string | null;
  harnessBaseBranch: string | null;
  harnessRunBranch: string | null;
  harnessStatus: string | null;
  harnessResultCommit: string | null;
}

const readRecord = (value: unknown): Record<string, unknown> | null => (
  isRecord(value) ? value : null
);

const extractRunExecutionLocation = (
  run: MessageTaskRunnerRunDetailResponse['run'],
): RunExecutionLocation => {
  const input = readRecord(run.input_snapshot);
  const inputSandbox = readRecord(input?.sandbox);
  const inputHarness = readRecord(input?.harness);
  const report = readRecord(run.report);
  const reportOutput = readRecord(report?.output);
  const outputSandbox = readRecord(reportOutput?.sandbox);
  const outputHarness = readRecord(reportOutput?.harness);
  const sandboxEnabled = typeof input?.sandbox_enabled === 'boolean'
    ? input.sandbox_enabled
    : typeof outputSandbox?.enabled === 'boolean'
      ? outputSandbox.enabled
      : inputSandbox
        ? true
        : null;

  return {
    environmentMode: readString(input?.execution_environment_mode),
    sandboxEnabled,
    sandboxProvider: readString(inputSandbox?.provider),
    sandboxId: readString(inputSandbox?.sandbox_id) || readString(outputSandbox?.sandbox_id),
    leaseId: readString(inputSandbox?.lease_id) || readString(outputSandbox?.lease_id),
    leaseExpiresAt: readString(inputSandbox?.expires_at),
    harnessRepoPath: readString(inputHarness?.repo_path) || readString(outputHarness?.repo_path),
    harnessBaseBranch: readString(inputHarness?.base_branch) || readString(outputHarness?.base_branch),
    harnessRunBranch: readString(inputHarness?.run_branch) || readString(outputHarness?.run_branch),
    harnessStatus: readString(outputHarness?.status) || readString(inputHarness?.status),
    harnessResultCommit: readString(outputHarness?.result_commit),
  };
};

const environmentModeLabel = (mode: string | null): string => {
  if (mode === 'cloud') return '云端';
  if (mode === 'local') return '本地';
  return mode || '-';
};

const sandboxStatusLabel = (location: RunExecutionLocation): string => {
  if (location.sandboxId && location.leaseId) return '已准备';
  if (location.sandboxEnabled === true) return '准备中';
  if (location.sandboxEnabled === false) return '未启用';
  return '-';
};

export const MessageTaskRunDetailModal: FC<MessageTaskRunDetailModalProps> = ({
  detail,
  messageId,
  taskLookup,
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
  const processTimelineItems = buildRunProcessTimelineItems(events);
  const rawTimelineEntries = buildRunEventTimelineEntries(events);
  const eventsTotal = typeof detail.events_total === 'number'
    ? detail.events_total
    : events.length;
  const eventsHasMore = Boolean(detail.events_has_more);
  const resultSummary = readString(run.result_summary);
  const normalizedReportContent = readString(reportContent);
  const sandboxOutputCounts = extractSandboxOutputCounts(run.report);
  const executionLocation = extractRunExecutionLocation(run);
  const userVisibleError = run.error_message
    ? sanitizeUserVisibleAppError(run.error_message)
    : null;
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
          ['已加载模型请求', modelRequestCount],
          ['已加载工具事件', toolEventCount],
        ]}
      />

      <CollapsibleSection title="执行位置" defaultOpen>
        <FieldGrid
          items={[
            ['执行环境', environmentModeLabel(executionLocation.environmentMode)],
            ['沙箱状态', sandboxStatusLabel(executionLocation)],
            ['沙箱提供方', executionLocation.sandboxProvider],
            ['Sandbox ID', executionLocation.sandboxId],
            ['Lease ID', executionLocation.leaseId],
            ['租约到期', executionLocation.leaseExpiresAt
              ? formatDateTime(executionLocation.leaseExpiresAt)
              : null],
            ['Harness 仓库', executionLocation.harnessRepoPath],
            ['Harness 基线分支', executionLocation.harnessBaseBranch],
            ['Harness 运行分支', executionLocation.harnessRunBranch],
            ['Harness 状态', executionLocation.harnessStatus],
            ['Harness 结果提交', executionLocation.harnessResultCommit],
          ]}
        />
      </CollapsibleSection>

      <PluginRunSnapshotCard inputSnapshot={run.input_snapshot} />
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
