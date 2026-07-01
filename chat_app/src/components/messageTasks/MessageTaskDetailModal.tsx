// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import type {
  MessageTaskRunnerModelConfigSummary,
  MessageTaskRunnerRunSummary,
  MessageTaskRunnerTask,
  MessageTaskRunnerTaskSummary,
} from '../../lib/api/client/types';
import { CollapsibleSection, CollapsibleText } from './CollapsibleSection';
import { FieldGrid, MarkdownCard, ModalShell, StatusBadge, valueOrDash } from './parts';
import { formatDateTime, isRecord, readString, readStringArray } from './utils';

interface MessageTaskDetailModalProps {
  task: MessageTaskRunnerTask | null;
  relatedTasks?: MessageTaskRunnerTask[];
  onClose: () => void;
}

interface MessageTaskProcessLogModalProps {
  task: MessageTaskRunnerTask | null;
  onClose: () => void;
}

const shortId = (value: string): string => (
  value.length > 16 ? `${value.slice(0, 8)}...${value.slice(-4)}` : value
);

const formatModelConfig = (
  modelConfig?: MessageTaskRunnerModelConfigSummary | null,
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

const formatRunSummary = (
  run?: MessageTaskRunnerRunSummary | null,
  fallbackId?: string | null,
): string => {
  const id = readString(run?.id) || readString(fallbackId);
  if (!run) {
    return id ? `运行记录暂不可用 (${shortId(id)})` : '-';
  }
  const status = readString(run.status) || '未知状态';
  const time = formatDateTime(
    readString(run.finished_at) || readString(run.started_at) || readString(run.updated_at),
  );
  const parts = time === '-' ? [status] : [status, time];
  return id ? `${parts.join(' · ')} (${shortId(id)})` : parts.join(' · ');
};

const formatTaskSummary = (
  task?: MessageTaskRunnerTaskSummary | null,
  fallbackId?: string | null,
): string => {
  const id = readString(task?.id) || readString(fallbackId);
  const title = readString(task?.title);
  const status = readString(task?.status);
  if (!title) {
    return id ? `任务名称暂不可用 (${shortId(id)})` : '-';
  }
  const parts = status ? [title, status] : [title];
  return id ? `${parts.join(' · ')} (${shortId(id)})` : parts.join(' · ');
};

export const MessageTaskDetailModal: FC<MessageTaskDetailModalProps> = ({
  task,
  relatedTasks = [],
  onClose,
}) => {
  if (!task) {
    return null;
  }
  const prerequisiteIds = readStringArray(task.prerequisite_task_ids);
  const prerequisiteSummaries = Array.isArray(task.prerequisite_tasks)
    ? task.prerequisite_tasks
    : [];
  const prerequisiteSummaryById = new Map(
    prerequisiteSummaries
      .filter((item) => readString(item.id))
      .map((item) => [item.id, item]),
  );
  const relatedTaskById = new Map(
    relatedTasks
      .filter((item) => readString(item.id))
      .map((item) => [item.id, item]),
  );
  const prerequisiteSummaryIds = prerequisiteSummaries
    .map((item) => readString(item.id))
    .filter((item): item is string => Boolean(item));
  const orderedPrerequisiteIds = prerequisiteIds.length
    ? prerequisiteIds
    : prerequisiteSummaryIds;
  const extraPrerequisiteIds = prerequisiteSummaryIds
    .filter((item) => !orderedPrerequisiteIds.includes(item));
  const prerequisiteItems = [...orderedPrerequisiteIds, ...extraPrerequisiteIds].map((taskId) => {
    const prerequisiteTask = prerequisiteSummaryById.get(taskId) || relatedTaskById.get(taskId);
    return {
      id: taskId,
      title: readString(prerequisiteTask?.title),
      status: readString(prerequisiteTask?.status),
    };
  });
  const toolState = isRecord(task.task_tool_state) ? task.task_tool_state : {};
  const outcomeItems = Array.isArray(toolState.outcome_items) ? toolState.outcome_items : [];

  return (
    <ModalShell
      title="任务详情"
      subtitle={task.title || task.id}
      onClose={onClose}
      widthClassName="max-w-5xl"
    >
      <FieldGrid
        items={[
          ['任务 ID', task.id],
          ['状态', task.status],
          ['创建人', task.creator_display_name || task.creator_username || task.creator_user_id],
          ['模型', formatModelConfig(task.default_model_config, task.default_model_config_id)],
          ['优先级', task.priority],
          ['最近运行', formatRunSummary(task.last_run, task.last_run_id)],
          ['创建时间', formatDateTime(task.created_at)],
          ['更新时间', formatDateTime(task.updated_at)],
        ]}
      />

      {task.result_summary ? (
        <CollapsibleSection title="执行结果" defaultOpen>
          <MarkdownCard content={task.result_summary} />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection title="任务内容" defaultOpen>
        <div className="space-y-3">
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">目标</div>
            <CollapsibleText value={task.objective || '-'} />
          </div>
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">描述</div>
            <CollapsibleText value={task.description || '-'} />
          </div>
        </div>
      </CollapsibleSection>

      {task.process_log ? (
        <CollapsibleSection title="执行过程">
          <CollapsibleText value={task.process_log} />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection
        title="前置任务"
        summary={prerequisiteItems.length ? `${prerequisiteItems.length} 个前置任务` : '无'}
      >
        {prerequisiteItems.length ? (
          <div className="space-y-2">
            {prerequisiteItems.map((item) => (
              <div
                key={item.id}
                className="rounded-md border border-border bg-muted/30 px-3 py-2"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="break-words text-sm font-medium text-foreground">
                    {item.title || '任务名称暂不可用'}
                  </span>
                  <span className="rounded border border-border bg-background px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">
                    {shortId(item.id)}
                  </span>
                  {item.status ? <StatusBadge status={item.status} /> : null}
                </div>
                <div className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
                  {item.id}
                </div>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">无前置任务</p>
        )}
      </CollapsibleSection>

      <CollapsibleSection
        title="MCP / 工作区 / 服务器"
        summary={valueOrDash(isRecord(task.mcp_config) ? task.mcp_config.workspace_dir : null)}
      >
        <CollapsibleText value={task.mcp_config || '-'} code />
      </CollapsibleSection>

      <CollapsibleSection
        title="过程产物"
        summary={outcomeItems.length ? `${outcomeItems.length} 条` : '无'}
      >
        <CollapsibleText value={task.task_tool_state || '-'} code />
      </CollapsibleSection>

      <CollapsibleSection title="来源信息">
        <FieldGrid
          items={[
            ['会话 ID', task.source_session_id],
            ['轮次 ID', task.source_turn_id],
            ['源消息 ID', task.source_user_message_id],
            ['父任务', formatTaskSummary(task.parent_task, task.parent_task_id)],
            ['来源运行', formatRunSummary(task.source_run, task.source_run_id)],
          ]}
        />
      </CollapsibleSection>

      <CollapsibleSection title="原始输入">
        <CollapsibleText value={task.input_payload || '-'} code />
      </CollapsibleSection>
    </ModalShell>
  );
};

export const MessageTaskProcessLogModal: FC<MessageTaskProcessLogModalProps> = ({
  task,
  onClose,
}) => {
  if (!task) {
    return null;
  }

  return (
    <ModalShell
      title="执行过程"
      subtitle={task.title || task.id}
      onClose={onClose}
      widthClassName="max-w-4xl"
    >
      <CollapsibleText
        value={task.process_log || '暂无执行过程'}
        maxHeightClassName="max-h-[68vh]"
      />
    </ModalShell>
  );
};
