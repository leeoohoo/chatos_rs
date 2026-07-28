// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useState, type FC } from 'react';
import { CircleAlert, LoaderCircle, RotateCcw } from 'lucide-react';
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
  onRetry?: (
    task: MessageTaskRunnerTask,
    retryInstruction?: string,
    executionServiceId?: string,
  ) => unknown | Promise<unknown>;
  retrying?: boolean;
  retryError?: string | null;
  onClose: () => void;
}

interface MessageTaskProcessLogModalProps {
  task: MessageTaskRunnerTask | null;
  onClose: () => void;
}

const shortId = (value: string): string => (
  value.length > 16 ? `${value.slice(0, 8)}...${value.slice(-4)}` : value
);

export const canRetryMessageTask = (task: MessageTaskRunnerTask): boolean => (
  ['failed', 'blocked'].includes(readString(task.status)?.toLowerCase() || '')
  && Boolean(readString(task.last_run_id))
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
  onRetry,
  retrying = false,
  retryError = null,
  onClose,
}) => {
  const [retryInstruction, setRetryInstruction] = useState('');
  useEffect(() => {
    setRetryInstruction('');
  }, [task?.id]);

  if (!task) {
    return null;
  }
  const normalizedStatus = readString(task.status)?.toLowerCase();
  const isBlocked = normalizedStatus === 'blocked';
  const taskToolState = isRecord(task.task_tool_state) ? task.task_tool_state : {};
  const blockedReason = readString(task.last_run?.error_message)
    || readString(taskToolState.blocker_reason)
    || readString(task.result_summary)
    || '该节点进入了阻塞状态，但运行记录没有返回具体原因。';
  const blockerNeeds = readStringArray(taskToolState.blocker_needs);
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
  const outcomeItems = Array.isArray(taskToolState.outcome_items)
    ? taskToolState.outcome_items
    : [];

  return (
    <ModalShell
      title="任务详情"
      subtitle={task.title || task.id}
      onClose={onClose}
      widthClassName="max-w-5xl"
    >
      {canRetryMessageTask(task) && onRetry ? (
        <div className="space-y-3 rounded-md border border-amber-300 bg-amber-50 px-3 py-3 dark:border-amber-800 dark:bg-amber-950/30">
          <div className="flex items-start gap-2.5">
            {isBlocked ? (
              <CircleAlert className="mt-0.5 h-4 w-4 shrink-0 text-orange-600 dark:text-orange-300" />
            ) : null}
            <div className="min-w-0 flex-1">
              <div className="text-sm font-medium text-amber-900 dark:text-amber-100">
                {isBlocked ? '当前节点未完成' : '当前节点执行失败'}
              </div>
              <p className="mt-0.5 text-xs leading-5 text-amber-800 dark:text-amber-200">
                {isBlocked
                  ? '系统检测到仍有必需步骤没有完成，因此没有把本次运行算作成功。可以补充处理意见后重新运行；不需要补充时可直接重新处理。节点成功后，满足依赖条件的后续节点会自动继续。'
                  : '仅重新运行此节点；成功后，满足依赖条件的后续节点会继续调度。'}
              </p>
            </div>
          </div>

          {isBlocked ? (
            <div className="rounded-md border border-orange-200 bg-white/70 px-3 py-2 dark:border-orange-900/70 dark:bg-background/50">
              <div className="text-xs font-medium text-orange-900 dark:text-orange-100">阻塞原因</div>
              <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-5 text-orange-800 dark:text-orange-200">
                {blockedReason}
              </p>
              {blockerNeeds.length > 0 ? (
                <div className="mt-2 text-xs text-orange-800 dark:text-orange-200">
                  <span className="font-medium">继续处理需要：</span>
                  {blockerNeeds.join('、')}
                </div>
              ) : null}
            </div>
          ) : null}

          {isBlocked ? (
            <label className="block">
              <span className="text-xs font-medium text-amber-900 dark:text-amber-100">
                补充处理意见（可选）
              </span>
              <textarea
                className="mt-1.5 min-h-24 w-full resize-y rounded-md border border-amber-300 bg-background px-3 py-2 text-sm text-foreground outline-none focus:border-amber-500 focus:ring-2 focus:ring-amber-500/20 disabled:cursor-not-allowed disabled:opacity-60 dark:border-amber-800"
                value={retryInstruction}
                maxLength={4000}
                disabled={retrying}
                placeholder="例如：相关配置已经补齐；请使用新的接口约束继续处理，并重新完成验证。留空表示外部阻塞已经处理，直接重新检查。"
                onChange={(event) => setRetryInstruction(event.target.value)}
              />
            </label>
          ) : null}

          <div className="flex justify-end">
            <button
              type="button"
              className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-3 py-2 text-xs font-semibold text-white hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={retrying}
              onClick={() => {
                if (isBlocked) {
                  void onRetry(task, retryInstruction.trim());
                  return;
                }
                void onRetry(task);
              }}
            >
              {retrying
                ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                : <RotateCcw className="h-3.5 w-3.5" />}
              {retrying ? '正在重新处理' : isBlocked ? '重新处理此节点' : '重试此任务'}
            </button>
          </div>

          {retryError ? (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs leading-5 text-destructive"
            >
              <CircleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
              <span>{retryError}</span>
            </div>
          ) : null}
        </div>
      ) : null}

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
