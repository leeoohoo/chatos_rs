// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Cloud,
  GitBranch,
  Laptop,
  LoaderCircle,
  Maximize2,
  MessageSquareText,
  Minimize2,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  Send,
  Square,
  X,
  XCircle,
} from 'lucide-react';

import type {
  MessageTaskRunnerTask,
  ProjectRequirementExecuteResponse,
  ProjectRequirementExecutionPlanResponse,
  ProjectRequirementResponse,
} from '../../../lib/api/client/types';
import { ApiRequestError } from '../../../lib/api/client/shared';
import { useApiClient } from '../../../lib/api/ApiClientContext';
import { normalizeRawMessages } from '../../../lib/domain/messages';
import { useChatStore } from '../../../lib/store';
import { cn } from '../../../lib/utils';
import type { Message } from '../../../types';
import { MessageTaskChangesModal } from '../../messageTasks/MessageTaskChangesModal';
import {
  MessageTaskDetailModal,
  MessageTaskProcessLogModal,
} from '../../messageTasks/MessageTaskDetailModal';
import { MessageTaskGraphPanel } from '../../messageTasks/MessageTaskGraphPanel';
import { MessageTaskRunDetailModal } from '../../messageTasks/MessageTaskRunDetailModal';
import {
  resolveProjectExecutionConfirmationState,
  type ProjectExecutionConfirmationState,
} from '../../messageTasks/projectExecutionConfirmation';
import { useMessageTaskGraph } from '../../messageTasks/useMessageTaskGraph';
import { readString } from '../../messageTasks/utils';
import { readText } from './model';

export interface RequirementExecutionProcess {
  requirement: ProjectRequirementResponse;
  projectId: string;
  conversationId: string;
  executionGroupId: string;
  messageId: string;
  contactId?: string | null;
  executionPlane?: string | null;
  selectedModelId?: string | null;
  includePrerequisiteDependents?: boolean;
  planningFeedback?: string | null;
  planningFeedbackHistory?: string[];
  serverStatus?: string | null;
  confirmationStatus?: string | null;
  hasStartedRuns?: boolean;
  executionPaused?: boolean;
  tasksDiscarded?: boolean;
  initialMessage?: Message | null;
}

type ExecutionResponse = ProjectRequirementExecuteResponse | ProjectRequirementExecutionPlanResponse;

const readTextList = (value: unknown): string[] => (
  Array.isArray(value)
    ? value.map(readText).filter(Boolean)
    : []
);

export const REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS = 10_000;

export const isPendingRequirementExecutionPlanError = (error: unknown): boolean => {
  if (!(error instanceof Error)) {
    return false;
  }
  const code = error instanceof ApiRequestError ? error.code : undefined;
  if (code === 'local_execution_plan_source_missing') {
    return true;
  }
  const message = error.message.toLowerCase();
  return message.includes('需求执行规划消息不存在')
    || message.includes('需求执行会话不存在')
    || message.includes('local execution plan source message was not found');
};

export const shouldReplaceRequirementExecutionBatch = ({
  phase,
  planDiscarded,
  taskCount,
}: {
  phase: RequirementExecutionProcessPhase;
  planDiscarded: boolean;
  taskCount: number;
}): boolean => (
  !planDiscarded
  && !(phase === 'stopped' && taskCount === 0)
);

export const shouldStopRequirementExecutionBeforeReplacement = ({
  phase,
  replacePreviousBatch,
}: {
  phase: RequirementExecutionProcessPhase;
  replacePreviousBatch: boolean;
}): boolean => (
  replacePreviousBatch
  && phase !== 'stopped'
  && phase !== 'completed'
);

export const shouldShowCancelRequirementExecution = ({
  actuallyStarted,
  phase,
}: {
  actuallyStarted: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => (
  actuallyStarted
  && phase !== 'stopped'
  && phase !== 'completed'
);

export const shouldShowDiscardRequirementPlan = ({
  actuallyStarted,
  phase,
}: {
  actuallyStarted: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => (
  !actuallyStarted
  && phase !== 'stopped'
  && phase !== 'completed'
);

export const buildRequirementExecutionProcess = ({
  fallback,
  projectId,
  requirement,
  response,
}: {
  fallback?: RequirementExecutionProcess | null;
  projectId: string;
  requirement: ProjectRequirementResponse;
  response: ExecutionResponse;
}): RequirementExecutionProcess | null => {
  if ('found' in response && response.found === false) {
    return null;
  }
  const conversationId = readText(response.conversation_id)
    || readText(response.conversationId)
    || fallback?.conversationId
    || '';
  const [normalizedMessage] = response.message && conversationId
    ? normalizeRawMessages([response.message], conversationId)
    : [];
  const executionGroupId = readText(response.execution_group_id)
    || readText(response.executionGroupId)
    || readText(normalizedMessage?.metadata?.project_requirement_execution?.execution_group_id)
    || readText(normalizedMessage?.metadata?.conversation_turn_id)
    || fallback?.executionGroupId
    || '';
  const messageId = readText(response.message_id)
    || readText(response.messageId)
    || readText(normalizedMessage?.id)
    || fallback?.messageId
    || executionGroupId;
  if (!conversationId || !executionGroupId || !messageId) {
    return null;
  }
  const latestPlanningFeedback = readText(response.planning_feedback)
    || readText(response.planningFeedback)
    || readText(normalizedMessage?.metadata?.project_requirement_execution?.planning_feedback);
  const snakeCaseFeedbackHistory = readTextList(response.planning_feedback_history);
  const responseFeedbackHistory = snakeCaseFeedbackHistory.length > 0
    ? snakeCaseFeedbackHistory
    : readTextList(response.planningFeedbackHistory);
  const messageFeedbackHistory = readTextList(
    normalizedMessage?.metadata?.project_requirement_execution?.planning_feedback_history,
  );
  const fallbackFeedbackHistory = fallback?.planningFeedbackHistory?.length
    ? fallback.planningFeedbackHistory
    : fallback?.planningFeedback ? [fallback.planningFeedback] : [];
  const planningFeedbackHistory = (
    responseFeedbackHistory.length > 0
        ? responseFeedbackHistory
      : messageFeedbackHistory.length > 0
        ? messageFeedbackHistory
        : fallbackFeedbackHistory
  ).slice();
  if (
    latestPlanningFeedback
    && planningFeedbackHistory[planningFeedbackHistory.length - 1] !== latestPlanningFeedback
  ) {
    planningFeedbackHistory.push(latestPlanningFeedback);
  }
  const responseExecutionPaused = response.execution_paused ?? response.executionPaused;
  const metadataExecutionPaused = normalizedMessage
    ?.metadata
    ?.task_runner_async
    ?.execution_paused;
  return {
    requirement,
    projectId,
    conversationId,
    executionGroupId,
    messageId,
    contactId: readText(response.contact_id)
      || readText(response.contactId)
      || readText(normalizedMessage?.metadata?.project_requirement_execution?.contact_id)
      || fallback?.contactId
      || null,
    executionPlane: readText(response.execution_plane)
      || readText(response.executionPlane)
      || readText(normalizedMessage?.metadata?.project_requirement_execution?.execution_plane)
      || fallback?.executionPlane
      || null,
    selectedModelId: readText(response.model_config_id)
      || readText(response.modelConfigId)
      || readText(normalizedMessage?.metadata?.model_config_id)
      || fallback?.selectedModelId
      || null,
    includePrerequisiteDependents: response.include_prerequisite_dependents
      ?? response.includePrerequisiteDependents
      ?? fallback?.includePrerequisiteDependents
      ?? false,
    planningFeedback: latestPlanningFeedback
      || planningFeedbackHistory[planningFeedbackHistory.length - 1]
      || fallback?.planningFeedback
      || null,
    planningFeedbackHistory,
    serverStatus: readText(response.status) || fallback?.serverStatus || null,
    confirmationStatus: readText(response.confirmation_status)
      || readText(response.confirmationStatus)
      || fallback?.confirmationStatus
      || null,
    hasStartedRuns: response.has_started_runs
      ?? response.hasStartedRuns
      ?? fallback?.hasStartedRuns
      ?? false,
    executionPaused: typeof responseExecutionPaused === 'boolean'
      ? responseExecutionPaused
      : typeof metadataExecutionPaused === 'boolean'
        ? metadataExecutionPaused
        : fallback?.executionPaused ?? false,
    tasksDiscarded: fallback?.tasksDiscarded ?? false,
    initialMessage: normalizedMessage || fallback?.initialMessage || null,
  };
};

export type RequirementExecutionProcessPhase =
  | 'planning_context'
  | 'building_graph'
  | 'awaiting_confirmation'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'stopped';

const TERMINAL_TASK_STATUSES = new Set([
  'succeeded',
  'success',
  'completed',
  'done',
  'failed',
  'error',
  'blocked',
  'cancelled',
  'canceled',
]);

const FAILED_TASK_STATUSES = new Set(['failed', 'error', 'blocked']);
const ACTIVE_TASK_STATUSES = new Set([
  'queued',
  'pending',
  'running',
  'processing',
  'in_progress',
  'doing',
]);

const isStoppedExecutionStatus = (status?: string | null): boolean => (
  ['stopped', 'cancelled', 'canceled'].includes((status || '').trim().toLowerCase())
);

export const resolveRequirementExecutionProcessPhase = ({
  confirmationState,
  executionConfirmed = false,
  executionPaused = false,
  failureDetected,
  hasStartedRuns = false,
  planStopped,
  serverStatus = '',
  taskStatuses,
}: {
  confirmationState: ProjectExecutionConfirmationState;
  executionConfirmed?: boolean;
  executionPaused?: boolean;
  failureDetected: boolean;
  hasStartedRuns?: boolean;
  planStopped: boolean;
  serverStatus?: string;
  taskStatuses: string[];
}): RequirementExecutionProcessPhase => {
  const normalizedServerStatus = serverStatus.trim().toLowerCase();
  const overallStatus = confirmationState.overallStatus || normalizedServerStatus;
  if (
    planStopped
    || ['stopped', 'cancelled', 'canceled'].includes(normalizedServerStatus || overallStatus)
  ) {
    return 'stopped';
  }
  const allTasksTerminal = taskStatuses.length > 0
    && taskStatuses.every((status) => TERMINAL_TASK_STATUSES.has(status));
  if (
    (executionPaused || normalizedServerStatus === 'paused' || overallStatus === 'paused')
    && !allTasksTerminal
  ) {
    return 'paused';
  }
  const actuallyStarted = hasStartedRuns || confirmationState.hasStartedTasks;
  if (
    actuallyStarted
    && taskStatuses.some((status) => ACTIVE_TASK_STATUSES.has(status))
  ) {
    return 'running';
  }
  if (
    failureDetected
    || ['failed', 'error'].includes(normalizedServerStatus || overallStatus)
    || taskStatuses.some((status) => FAILED_TASK_STATUSES.has(status))
  ) {
    return 'failed';
  }
  if (
    ['completed', 'succeeded', 'success'].includes(
      normalizedServerStatus || overallStatus,
    )
  ) {
    return 'completed';
  }
  if (
    actuallyStarted
    && taskStatuses.length > 0
    && allTasksTerminal
  ) {
    return 'completed';
  }
  if (actuallyStarted || executionConfirmed) {
    return 'running';
  }
  if (
    confirmationState.graphReadyForConfirmation
    && taskStatuses.length > 0
  ) {
    return 'awaiting_confirmation';
  }
  if (taskStatuses.length > 0) {
    return 'building_graph';
  }
  return 'planning_context';
};

const createFallbackMessage = (process: RequirementExecutionProcess): Message => ({
  id: process.messageId,
  sessionId: process.conversationId,
  role: 'user',
  content: `为需求“${readText(process.requirement.title) || process.requirement.id}”生成执行计划`,
  status: 'completed',
  createdAt: new Date(),
  metadata: {
    hidden: true,
    conversation_turn_id: process.executionGroupId,
    project_requirement_execution: {
      project_id: process.projectId,
      requirement_id: process.requirement.id,
      requirement_title: readText(process.requirement.title),
      contact_id: process.contactId || undefined,
      execution_group_id: process.executionGroupId,
      execution_plane: process.executionPlane || undefined,
    },
    task_runner_async: {
      mode: 'project_requirement_execution',
      overall_status: process.serverStatus || 'planning',
      confirmation_status: process.confirmationStatus || process.serverStatus || 'planning',
      source_turn_id: process.executionGroupId,
      source_user_message_id: process.messageId,
      execution_paused: Boolean(process.executionPaused),
      project_id: process.projectId,
      requirement_id: process.requirement.id,
    },
  },
});

const withProcessStatus = (message: Message, process: RequirementExecutionProcess): Message => ({
  ...message,
  metadata: {
    ...message.metadata,
    conversation_turn_id: process.executionGroupId,
    project_requirement_execution: {
      ...message.metadata?.project_requirement_execution,
      project_id: process.projectId,
      requirement_id: process.requirement.id,
      execution_group_id: process.executionGroupId,
      execution_plane: process.executionPlane || undefined,
      contact_id: process.contactId || undefined,
    },
    task_runner_async: {
      ...message.metadata?.task_runner_async,
      mode: 'project_requirement_execution',
      overall_status: process.serverStatus || 'planning',
      confirmation_status: process.confirmationStatus || process.serverStatus || 'planning',
      source_turn_id: process.executionGroupId,
      source_user_message_id: process.messageId,
      execution_paused: Boolean(process.executionPaused),
    },
  },
});

const phaseCopy: Record<RequirementExecutionProcessPhase, { title: string; detail: string }> = {
  planning_context: {
    title: '正在读取需求、项目任务和技术文档',
    detail: '规划 Agent 正在建立执行上下文，Task Runner 尚未启动。',
  },
  building_graph: {
    title: '正在生成完整执行流程',
    detail: '任务节点会随着 Agent 创建而实时出现在右侧流程图中。',
  },
  awaiting_confirmation: {
    title: '执行流程已生成，等待你确认',
    detail: '你可以继续输入调整意见；只有点击“执行”后 Task Runner 才会启动。',
  },
  running: {
    title: 'Task Runner 正在执行',
    detail: '已收到你的执行确认，任务正在按照依赖顺序运行。',
  },
  paused: {
    title: '已暂停后续任务调度',
    detail: '当前已运行的任务会继续完成；新的任务节点不会启动，可以点击“继续调度”恢复。',
  },
  completed: {
    title: '本次执行已经完成',
    detail: '当前执行批次的任务已经全部结束。',
  },
  failed: {
    title: '规划或执行失败',
    detail: '请查看过程记录和任务节点，也可以在尚未启动执行时输入意见重新规划。',
  },
  stopped: {
    title: '当前执行已取消',
    detail: '整批执行已取消，可以重新生成或重新执行一份新的任务流程。',
  },
};

export const requirementExecutionModalShellClassName = (fullscreen: boolean): string => cn(
  'absolute flex flex-col overflow-hidden border border-border bg-card shadow-2xl',
  fullscreen
    ? 'inset-0 h-[100dvh] w-screen max-w-none rounded-none border-0'
    : 'left-1/2 top-1/2 h-[94dvh] w-[calc(100vw-20px)] max-w-[1500px] -translate-x-1/2 -translate-y-1/2 rounded-xl sm:w-[calc(100vw-36px)]',
);

const FullscreenToggleButton: React.FC<{
  fullscreen: boolean;
  onToggle: () => void;
}> = ({ fullscreen, onToggle }) => (
  <button
    type="button"
    className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
    onClick={onToggle}
    aria-label={fullscreen ? '退出全屏' : '全屏'}
    aria-pressed={fullscreen}
    title={fullscreen ? '退出全屏' : '全屏显示'}
  >
    {fullscreen ? <Minimize2 className="h-3.5 w-3.5" /> : <Maximize2 className="h-3.5 w-3.5" />}
    <span className="hidden sm:inline">{fullscreen ? '退出全屏' : '全屏'}</span>
  </button>
);

export const RequirementExecutionStartingModal: React.FC<{
  requirement: ProjectRequirementResponse;
  executionPlane?: string | null;
  starting: boolean;
  onClose: () => void;
  onStart: (planningFeedback: string) => void;
}> = ({ requirement, executionPlane, starting, onClose, onStart }) => {
  const isLocalExecution = (executionPlane || '').toLowerCase() === 'local_connector';
  const [fullscreen, setFullscreen] = useState(false);
  const [planningFeedback, setPlanningFeedback] = useState('');
  return (
    <div className="fixed inset-0 z-[50]" role="dialog" aria-modal="true" aria-label="执行计划工作台">
      <button
        type="button"
        aria-label="关闭执行计划工作台"
        className="absolute inset-0 bg-black/55"
        onClick={onClose}
      />
      <section
        className={requirementExecutionModalShellClassName(fullscreen)}
        data-fullscreen={fullscreen}
      >
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-4 py-3 sm:px-5">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-base font-semibold text-foreground">执行计划工作台</h2>
              <span className={cn(
                'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium',
                isLocalExecution
                  ? 'border-violet-200 bg-violet-50 text-violet-700 dark:border-violet-800 dark:bg-violet-950/30 dark:text-violet-200'
                  : 'border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-800 dark:bg-sky-950/30 dark:text-sky-200',
              )}
              >
                {isLocalExecution ? <Laptop className="h-3 w-3" /> : <Cloud className="h-3 w-3" />}
                {isLocalExecution ? '本地规划 / 本地执行' : '云端规划 / 云端执行'}
              </span>
            </div>
            <p className="mt-1 truncate text-sm text-muted-foreground">
              {readText(requirement.title) || requirement.id}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <FullscreenToggleButton
              fullscreen={fullscreen}
              onToggle={() => setFullscreen((current) => !current)}
            />
            <button
              type="button"
              className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
              onClick={onClose}
              aria-label="关闭"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden lg:grid-cols-[minmax(320px,0.78fr)_minmax(0,1.72fr)]">
          <aside className="flex min-h-0 flex-col border-b border-border bg-muted/10 lg:border-b-0 lg:border-r">
            <div className="shrink-0 border-b border-border px-4 py-3">
              <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
                {starting
                  ? <LoaderCircle className="h-4 w-4 animate-spin" />
                  : <Play className="h-4 w-4" />}
                {starting ? '正在建立规划上下文' : '等待用户开始生成执行流程'}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {starting
                  ? '规划 Agent 正在创建会话并读取需求、项目任务和技术文档，Task Runner 尚未启动。'
                  : '点击“开始生成执行流程”后只会启动规划 Agent；Task Runner 仍需在完整 DAG 生成后由你点击“执行”。'}
              </p>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
              <ol className="space-y-4">
                <li className="flex gap-3">
                  <span className={cn(
                    'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border',
                    starting
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border text-muted-foreground',
                  )}
                  >
                    {starting
                      ? <LoaderCircle className="h-3 w-3 animate-spin" />
                      : <span className="h-1.5 w-1.5 rounded-full bg-current" />}
                  </span>
                  <div>
                    <div className="text-xs font-medium text-foreground">
                      {starting ? '执行规划请求已接受' : '等待启动规划 Agent'}
                    </div>
                    <div className="mt-1 text-[11px] leading-5 text-muted-foreground">
                      {starting ? '正在建立规划批次标识' : '尚未创建规划会话或执行任务'}
                    </div>
                  </div>
                </li>
                <li className="flex gap-3">
                  <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-border text-muted-foreground">
                    <span className="h-1.5 w-1.5 rounded-full bg-current" />
                  </span>
                  <div>
                    <div className="text-xs font-medium text-foreground">读取任务与文档</div>
                    <div className="mt-1 text-[11px] leading-5 text-muted-foreground">上下文就绪后会逐个创建执行任务</div>
                  </div>
                </li>
              </ol>
            </div>
            <div className="shrink-0 border-t border-border bg-background px-4 py-3">
              <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                <MessageSquareText className="h-3.5 w-3.5" />
                调整执行计划
              </div>
              <textarea
                value={planningFeedback}
                disabled={starting}
                onChange={(event) => setPlanningFeedback(event.target.value)}
                placeholder="输入希望项目 Agent 遵循的规划要求，例如：先补测试，再拆分接口；把部署放到最后……"
                className="min-h-24 w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground focus:border-primary focus:ring-1 focus:ring-primary/20 disabled:cursor-wait disabled:bg-muted/40"
              />
              <div className="mt-1 text-[11px] leading-5 text-muted-foreground">
                这段内容会随首次规划请求一起发送给项目 Agent；留空也可以开始。
              </div>
            </div>
          </aside>

          <main className="flex min-h-0 min-w-0 flex-col">
            <div className="shrink-0 border-b border-border px-4 py-3">
              <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
                <GitBranch className="h-4 w-4" />
                实时执行流程图
              </div>
              <p className="mt-1 text-xs text-muted-foreground">Agent 创建第一个任务后，流程图会立即在这里更新。</p>
            </div>
            <div className="flex min-h-0 flex-1 items-center justify-center p-6">
              <div className="max-w-sm rounded-lg border border-dashed border-border bg-muted/10 px-6 py-8 text-center">
                {starting
                  ? <LoaderCircle className="mx-auto h-7 w-7 animate-spin text-primary" />
                  : <GitBranch className="mx-auto h-7 w-7 text-muted-foreground" />}
                <div className="mt-3 text-sm font-medium text-foreground">
                  {starting ? '等待第一个任务节点' : '执行流程尚未开始生成'}
                </div>
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {starting
                    ? '这里只展示规划结果，不会提前启动任何 task run。'
                    : '用户明确开始后，规划 Agent 创建的任务节点才会显示在这里。'}
                </div>
              </div>
            </div>
            <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/10 px-4 py-3">
              <span className="text-xs text-muted-foreground">
                {starting
                  ? '正在生成 DAG；此阶段不会启动 Task Runner。'
                  : '开始生成与最终执行是两个独立操作。'}
              </span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  disabled={starting}
                  className="inline-flex items-center gap-1.5 rounded-md bg-primary px-5 py-2 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60"
                  onClick={() => onStart(planningFeedback.trim())}
                >
                  {starting
                    ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                    : <Play className="h-3.5 w-3.5" />}
                  {starting ? '正在生成执行流程' : '开始生成执行流程'}
                </button>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-accent"
                  onClick={onClose}
                >
                  关闭
                </button>
              </div>
            </footer>
          </main>
        </div>
      </section>
    </div>
  );
};

type ProcessEntry = {
  id: string;
  title: string;
  detail?: string;
  state: 'done' | 'active' | 'pending' | 'error' | 'stopped';
};

export const runnerProcessEntryForPhase = (
  phase: RequirementExecutionProcessPhase,
): Pick<ProcessEntry, 'title' | 'detail' | 'state'> => {
  if (phase === 'stopped') {
    return {
      title: 'Task Runner 执行已取消',
      detail: '当前批次已经整体取消，不会继续调度或执行后续任务',
      state: 'stopped',
    };
  }
  if (phase === 'paused') {
    return {
      title: 'Task Runner 已暂停调度',
      detail: '运行中的节点可正常收尾，但不会启动新的节点',
      state: 'active',
    };
  }
  if (phase === 'completed') {
    return {
      title: 'Task Runner 执行完成',
      detail: '所有任务均已到达终态',
      state: 'done',
    };
  }
  if (phase === 'failed') {
    return {
      title: 'Task Runner 执行失败',
      detail: '当前批次存在失败或阻塞任务',
      state: 'error',
    };
  }
  return {
    title: 'Task Runner 已开始执行',
    detail: '正在按照右侧依赖顺序运行',
    state: 'active',
  };
};

export const resolveRequirementExecutionRecoveryActions = ({
  actuallyStarted,
  hasActiveRuns,
  phase,
}: {
  actuallyStarted: boolean;
  hasActiveRuns: boolean;
  phase: RequirementExecutionProcessPhase;
}): { canRegenerate: boolean; canRevise: boolean; canRerun: boolean } => ({
  canRegenerate: phase === 'failed' && !actuallyStarted && !hasActiveRuns,
  canRevise: !hasActiveRuns
    && (!actuallyStarted || ['completed', 'failed', 'stopped'].includes(phase)),
  canRerun: phase === 'stopped' && !hasActiveRuns,
});

export const isRequirementExecutionCancellationSettling = ({
  hasActiveRuns,
  phase,
}: {
  hasActiveRuns: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => phase === 'stopped' && hasActiveRuns;

export const RequirementExecutionProcessModal: React.FC<{
  process: RequirementExecutionProcess;
  onClose: () => void;
  onProcessChange: (process: RequirementExecutionProcess) => void;
}> = ({ process, onClose, onProcessChange }) => {
  const apiClient = useApiClient();
  const refreshSessionById = useChatStore((state) => state.refreshSessionById);
  const syncSessionMessagesInBackground = useChatStore(
    (state) => state.syncSessionMessagesInBackground,
  );
  const [liveProcess, setLiveProcess] = useState(process);
  const [message, setMessage] = useState<Message>(
    withProcessStatus(process.initialMessage || createFallbackMessage(process), process),
  );
  const [feedback, setFeedback] = useState('');
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [pausing, setPausing] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [revising, setRevising] = useState(false);
  const [rerunning, setRerunning] = useState(false);
  const [rerunConfirmOpen, setRerunConfirmOpen] = useState(false);
  const [failedTaskRetryOpen, setFailedTaskRetryOpen] = useState(false);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [cancelConfirmOpen, setCancelConfirmOpen] = useState(false);
  const [planStopped, setPlanStopped] = useState(
    isStoppedExecutionStatus(process.serverStatus),
  );
  const [planDiscarded, setPlanDiscarded] = useState(Boolean(process.tasksDiscarded));
  const [executionConfirmed, setExecutionConfirmed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [panelWidth, setPanelWidth] = useState(900);
  const graphContainerRef = useRef<HTMLDivElement>(null);
  const pollingRef = useRef(false);
  const activeExecutionGroupIdRef = useRef(process.executionGroupId);
  const stoppedExecutionGroupIdsRef = useRef(new Set<string>(
    isStoppedExecutionStatus(process.serverStatus) ? [process.executionGroupId] : [],
  ));

  useEffect(() => {
    const stopped = isStoppedExecutionStatus(process.serverStatus);
    activeExecutionGroupIdRef.current = process.executionGroupId;
    if (stopped) {
      stoppedExecutionGroupIdsRef.current.add(process.executionGroupId);
    } else {
      stoppedExecutionGroupIdsRef.current.delete(process.executionGroupId);
    }
    setLiveProcess(process);
    setMessage(withProcessStatus(
      process.initialMessage || createFallbackMessage(process),
      process,
    ));
    setFeedback('');
    setPlanStopped(stopped);
    setPlanDiscarded(Boolean(process.tasksDiscarded));
    setExecutionConfirmed(Boolean(process.hasStartedRuns));
    setActionError(null);
    setActionMessage(null);
    setSyncError(null);
    setRerunConfirmOpen(false);
    setFailedTaskRetryOpen(false);
    setDiscardConfirmOpen(false);
    setCancelConfirmOpen(false);
  }, [process.executionGroupId]);

  const taskLookup = useMemo(() => ({
    sessionId: liveProcess.conversationId,
    turnId: liveProcess.executionGroupId,
    sourceUserMessageId: liveProcess.messageId,
  }), [liveProcess.conversationId, liveProcess.executionGroupId, liveProcess.messageId]);

  const {
    graph,
    allTasks,
    loading,
    error: graphError,
    detailTask,
    processTask,
    runDetail,
    changesTask,
    outputChanges,
    outputDiff,
    selectedChangePath,
    loadingProcessTaskId,
    loadingRunId,
    loadingChangesRunId,
    loadingDiffPath,
    retryingTaskId,
    retryError,
    reloadGraph,
    openDetail,
    openProcessLog,
    openRun,
    openChanges,
    retryTask,
    selectChangeFile,
    loadMoreRunEvents,
    closeDetail,
    closeProcessLog,
    closeRun,
    closeChanges,
  } = useMessageTaskGraph({
    open: true,
    messageId: liveProcess.messageId,
    lookup: taskLookup,
    isTransientError: isPendingRequirementExecutionPlanError,
  });

  const confirmationState = useMemo(
    () => resolveProjectExecutionConfirmationState({ graph, message, tasks: allTasks }),
    [allTasks, graph, message],
  );
  const taskStatuses = useMemo(
    () => allTasks.map((task) => readString(task.status)?.toLowerCase() || ''),
    [allTasks],
  );
  const phase = useMemo(() => resolveRequirementExecutionProcessPhase({
    confirmationState,
    executionConfirmed,
    executionPaused: Boolean(liveProcess.executionPaused),
    failureDetected: false,
    hasStartedRuns: Boolean(liveProcess.hasStartedRuns),
    planStopped,
    serverStatus: liveProcess.serverStatus || '',
    taskStatuses,
  }), [
    confirmationState,
    executionConfirmed,
    liveProcess.executionPaused,
    liveProcess.hasStartedRuns,
    liveProcess.serverStatus,
    planStopped,
    taskStatuses,
  ]);
  const actuallyStarted = Boolean(
    liveProcess.hasStartedRuns || confirmationState.hasStartedTasks || executionConfirmed,
  );
  const hasActiveRuns = allTasks.some((task) => {
    const status = readString(task.status)?.toLowerCase() || '';
    return Boolean(readString(task.last_run_id)) && ACTIVE_TASK_STATUSES.has(status);
  });
  const runningTaskCount = allTasks.filter((task) => {
    const status = readString(task.status)?.toLowerCase() || '';
    return Boolean(readString(task.last_run_id))
      && ['running', 'processing', 'in_progress', 'doing'].includes(status);
  }).length;
  const queuedTaskCount = allTasks.filter((task) => {
    const status = readString(task.status)?.toLowerCase() || '';
    return Boolean(readString(task.last_run_id))
      && ['queued', 'pending'].includes(status);
  }).length;
  const retryableFailedTasks = useMemo(() => allTasks.filter((task) => (
    readString(task.status)?.toLowerCase() === 'failed'
    && Boolean(readString(task.last_run_id))
  )), [allTasks]);
  const graphReady = confirmationState.graphReadyForConfirmation
    && allTasks.length > 0
    && !actuallyStarted
    && phase !== 'stopped';
  const recoveryActions = resolveRequirementExecutionRecoveryActions({
    actuallyStarted,
    hasActiveRuns,
    phase,
  });
  const cancellationSettling = isRequirementExecutionCancellationSettling({
    hasActiveRuns,
    phase,
  });
  const canRegenerate = recoveryActions.canRegenerate && !planDiscarded;
  const canRevise = recoveryActions.canRevise;
  const canRerun = recoveryActions.canRerun && allTasks.length > 0 && !planDiscarded;
  const terminal = ['completed', 'failed', 'stopped'].includes(phase) && !hasActiveRuns;
  const isLocalExecution = (liveProcess.executionPlane || '').toLowerCase() === 'local_connector'
    || liveProcess.conversationId.startsWith('lc_');
  const phaseText = cancellationSettling
    ? {
      title: '正在取消当前执行',
      detail: runningTaskCount > 0
        ? `取消请求已提交，正在等待 ${runningTaskCount} 个运行中任务结束并回写状态。`
        : `取消请求已提交，正在等待 ${queuedTaskCount > 0 ? `${queuedTaskCount} 个排队任务` : '任务'}回写取消状态。`,
    }
    : phase === 'paused'
    ? {
      title: '已暂停后续任务调度',
      detail: runningTaskCount > 0
        ? `新的后置任务不会启动；当前 ${runningTaskCount} 个已运行任务会继续完成。`
        : `${queuedTaskCount > 0 ? `${queuedTaskCount} 个排队任务` : '后续任务'}不会启动，点击“继续调度”后恢复依赖调度。`,
    }
    : phaseCopy[phase];

  const processEntries = useMemo<ProcessEntry[]>(() => {
    const entries: ProcessEntry[] = [{
      id: 'accepted',
      title: '执行规划请求已接受',
      detail: isLocalExecution ? '由本地客户端规划' : '由云端规划服务处理',
      state: 'done',
    }];
    entries.push({
      id: 'context',
      title: '读取需求、项目任务和技术文档',
      detail: cancellationSettling
        ? '已发起取消，正在等待任务状态回写，不再继续读取或生成执行上下文'
        : phase === 'stopped'
        ? '当前批次已取消，不再继续读取或生成执行上下文'
        : allTasks.length > 0
          ? '上下文读取完成，开始创建执行任务'
          : '正在整理执行范围与约束',
      state: cancellationSettling
        ? 'active'
        : phase === 'stopped'
        ? 'stopped'
        : allTasks.length > 0
          ? 'done'
          : phase === 'failed' ? 'error' : 'active',
    });
    const planningFeedbackHistory = liveProcess.planningFeedbackHistory?.length
      ? liveProcess.planningFeedbackHistory
      : liveProcess.planningFeedback ? [liveProcess.planningFeedback] : [];
    planningFeedbackHistory.forEach((planningFeedback, index) => {
      entries.push({
        id: `feedback:${liveProcess.executionGroupId}:${index}`,
        title: planningFeedbackHistory.length > 1
          ? `已应用用户调整意见 ${index + 1}`
          : '已应用用户调整意见',
        detail: planningFeedback,
        state: 'done',
      });
    });
    allTasks
      .slice()
      .sort((left, right) => (
        (readString(left.created_at) || '').localeCompare(readString(right.created_at) || '')
      ))
      .forEach((task, index) => {
        entries.push({
          id: task.id,
          title: `已添加任务 ${index + 1}：${task.title || task.id}`,
          detail: readString(task.objective) || readString(task.description) || undefined,
          state: 'done',
        });
      });
    entries.push({
      id: 'dag',
      title: cancellationSettling
        ? '正在取消当前执行流程'
        : phase === 'stopped'
        ? '当前执行流程已取消'
        : graphReady || actuallyStarted || phase === 'completed'
          ? `完整流程图已生成（${allTasks.length} 个任务）`
          : '校验任务覆盖范围和依赖关系',
      detail: cancellationSettling
        ? '保留现有流程图用于查看；正在等待运行中和排队任务进入终态'
        : phase === 'stopped'
        ? '保留现有流程图用于查看，不再继续生成或调度任务'
        : graphReady
          ? '流程图已冻结等待确认，尚未启动任何 run'
          : phase === 'completed'
            ? '流程图和依赖关系已经全部执行完毕'
            : actuallyStarted
              ? '流程图已冻结，Task Runner 正在按照依赖关系执行'
              : '仍在等待 Agent 补全任务节点和依赖',
      state: cancellationSettling
        ? 'active'
        : phase === 'stopped'
        ? 'stopped'
        : graphReady || actuallyStarted || phase === 'completed'
          ? 'done'
          : phase === 'failed' ? 'error' : 'active',
    });
    if (graphReady) {
      entries.push({
        id: 'confirmation',
        title: '等待用户点击“执行”',
        detail: '在此之前 Task Runner 不会开始运行',
        state: 'active',
      });
    }
    if (actuallyStarted) {
      const runnerEntry = cancellationSettling
        ? {
          title: 'Task Runner 正在取消',
          detail: '后续调度已停止，正在等待运行中/排队任务收敛到终态',
          state: 'active' as const,
        }
        : runnerProcessEntryForPhase(phase);
      entries.push({
        id: 'runner',
        ...runnerEntry,
      });
    }
    return entries;
  }, [
    actuallyStarted,
    allTasks,
    cancellationSettling,
    graphReady,
    isLocalExecution,
    liveProcess.executionGroupId,
    liveProcess.planningFeedback,
    liveProcess.planningFeedbackHistory,
    phase,
  ]);

  const refreshPlanStatus = useCallback(async (silent = false) => {
    if (!silent) setSyncing(true);
    setSyncError(null);
    const requestedExecutionGroupId = liveProcess.executionGroupId;
    try {
      const response = await apiClient.getProjectRequirementExecutionPlan(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          conversationId: liveProcess.conversationId,
          executionGroupId: liveProcess.executionGroupId,
        },
      );
      const next = buildRequirementExecutionProcess({
        fallback: liveProcess,
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (
        activeExecutionGroupIdRef.current !== requestedExecutionGroupId
        || stoppedExecutionGroupIdsRef.current.has(requestedExecutionGroupId)
      ) {
        return;
      }
      if (next) {
        setLiveProcess(next);
        setMessage(withProcessStatus(next.initialMessage || message, next));
      }
    } catch (err) {
      if (
        activeExecutionGroupIdRef.current === requestedExecutionGroupId
        && !stoppedExecutionGroupIdsRef.current.has(requestedExecutionGroupId)
        && !isPendingRequirementExecutionPlanError(err)
      ) {
        setSyncError(err instanceof Error ? err.message : '读取规划批次状态失败');
      }
    } finally {
      if (!silent) setSyncing(false);
    }
  }, [apiClient, liveProcess, message]);

  const refreshAll = useCallback(async (silent = false) => {
    if (pollingRef.current) return;
    pollingRef.current = true;
    try {
      await Promise.all([
        reloadGraph(silent ? { silent: true } : undefined),
        refreshPlanStatus(silent),
      ]);
    } finally {
      pollingRef.current = false;
    }
  }, [refreshPlanStatus, reloadGraph]);

  const retryFailedTask = useCallback(async (task: MessageTaskRunnerTask) => {
    setActionError(null);
    setActionMessage(null);
    const retried = await retryTask(task);
    if (retried) {
      setActionMessage(liveProcess.executionPaused
        ? `任务“${task.title || task.id}”已重新进入暂停队列，将在继续执行后启动。`
        : `任务“${task.title || task.id}”已重新进入执行队列。`);
    }
  }, [
    liveProcess.executionPaused,
    retryTask,
  ]);

  useEffect(() => {
    if (
      failedTaskRetryOpen
      && retryableFailedTasks.length === 0
      && !retryingTaskId
    ) {
      setFailedTaskRetryOpen(false);
    }
  }, [failedTaskRetryOpen, retryableFailedTasks.length, retryingTaskId]);

  useEffect(() => {
    void refreshPlanStatus(true);
  }, [liveProcess.executionGroupId]);

  useEffect(() => {
    if (terminal) return undefined;
    const intervalId = window.setInterval(() => {
      void refreshAll(true);
    }, REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [refreshAll, terminal]);

  useEffect(() => {
    if (terminal) {
      setActionMessage(null);
    }
  }, [terminal]);

  useEffect(() => {
    const element = graphContainerRef.current;
    if (!element) return undefined;
    const updateWidth = () => setPanelWidth(Math.max(360, element.clientWidth));
    updateWidth();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateWidth);
      return () => window.removeEventListener('resize', updateWidth);
    }
    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const confirmExecution = async () => {
    if (!graphReady || !confirmationState.canConfirm) {
      setActionError('完整流程图尚未生成，或者当前批次已经存在运行记录');
      return;
    }
    setConfirming(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await apiClient.confirmProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
        },
      );
      const startedRuns = result.started_runs || result.startedRuns || [];
      const next = {
        ...liveProcess,
        serverStatus: readText(result.status) || 'execution_started',
        confirmationStatus: 'confirmed',
        hasStartedRuns: startedRuns.length > 0 || liveProcess.hasStartedRuns,
        executionPaused: false,
      };
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      setExecutionConfirmed(true);
      onProcessChange(next);
      setActionMessage('已确认执行。正式用户消息现在才会显示到会话，Task Runner 开始运行。');
      await refreshSessionById(liveProcess.conversationId);
      await syncSessionMessagesInBackground(liveProcess.conversationId);
      await refreshAll(false);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : '确认执行失败');
    } finally {
      setConfirming(false);
    }
  };

  const setExecutionPause = async (paused: boolean) => {
    if (!actuallyStarted || pausing || stopping) return;
    setPausing(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const payload = {
        execution_group_id: liveProcess.executionGroupId,
        conversation_id: liveProcess.conversationId,
        ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
      };
      const result = paused
        ? await apiClient.pauseProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          payload,
        )
        : await apiClient.resumeProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          payload,
        );
      const next = {
        ...liveProcess,
        serverStatus: readText(result.status) || (paused ? 'paused' : 'execution_started'),
        confirmationStatus: 'confirmed',
        executionPaused: paused,
      };
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      onProcessChange(next);
      setActionMessage(paused
        ? (runningTaskCount > 0
          ? `已暂停后续调度；${runningTaskCount} 个已运行任务仍会继续完成。`
          : '已暂停后续调度，不会启动新的任务节点。')
        : '已继续调度，Task Runner 将按照依赖顺序启动后续任务。');
      await refreshAll(false);
    } catch (err) {
      setActionError(err instanceof Error
        ? err.message
        : paused ? '暂停后续调度失败' : '继续调度失败');
    } finally {
      setPausing(false);
    }
  };

  const stopCurrentBatch = async (discardTasks = false) => {
    setStopping(true);
    setActionError(null);
    try {
      await apiClient.stopProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          ...(discardTasks ? { discard_tasks: true } : {}),
        },
      );
      stoppedExecutionGroupIdsRef.current.add(liveProcess.executionGroupId);
      const next = {
        ...liveProcess,
        serverStatus: 'stopped',
        executionPaused: false,
        tasksDiscarded: discardTasks,
      };
      setPlanStopped(true);
      setPlanDiscarded(discardTasks);
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      setSyncError(null);
      onProcessChange(next);
      setActionMessage(discardTasks
        ? '规划已停止，本批次创建的 Task Runner 任务和关联记录已删除。'
        : '当前执行已整体取消。');
      await reloadGraph();
    } catch (err) {
      setActionError(err instanceof Error
        ? err.message
        : discardTasks ? '取消规划并删除任务失败' : '取消本次执行失败');
    } finally {
      setDiscardConfirmOpen(false);
      setCancelConfirmOpen(false);
      setStopping(false);
    }
  };

  const replaceExecutionPlan = async (planningFeedback?: string) => {
    const normalizedFeedback = planningFeedback?.trim() || '';
    setRevising(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const replacePreviousBatch = shouldReplaceRequirementExecutionBatch({
        phase,
        planDiscarded,
        taskCount: allTasks.length,
      });
      if (shouldStopRequirementExecutionBeforeReplacement({
        phase,
        replacePreviousBatch,
      })) {
        await apiClient.stopProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          {
            execution_group_id: liveProcess.executionGroupId,
            conversation_id: liveProcess.conversationId,
            ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          },
        );
      }
      const response = await apiClient.executeProjectRequirement(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          ...(liveProcess.selectedModelId
            ? { model_config_id: liveProcess.selectedModelId }
            : {}),
          include_prerequisite_dependents: Boolean(
            liveProcess.includePrerequisiteDependents,
          ),
          ...(normalizedFeedback ? { planning_feedback: normalizedFeedback } : {}),
          ...(replacePreviousBatch ? {
            replaces_execution_group_id: liveProcess.executionGroupId,
            replaces_conversation_id: liveProcess.conversationId,
          } : {}),
        },
      );
      const next = buildRequirementExecutionProcess({
        fallback: liveProcess,
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (!next) {
        throw new Error('后端没有返回新的规划批次标识');
      }
      next.executionPaused = false;
      setFeedback('');
      activeExecutionGroupIdRef.current = next.executionGroupId;
      stoppedExecutionGroupIdsRef.current.delete(next.executionGroupId);
      setLiveProcess(next);
      setMessage(withProcessStatus(next.initialMessage || createFallbackMessage(next), next));
      setPlanStopped(false);
      setPlanDiscarded(false);
      setExecutionConfirmed(false);
      setSyncError(null);
      setActionError(null);
      setActionMessage(normalizedFeedback
        ? '已接收你的意见，正在重新生成执行流程。'
        : '已重新启动规划 Agent，正在生成新的执行流程。');
      onProcessChange(next);
    } catch (err) {
      setActionError(err instanceof Error
        ? err.message
        : normalizedFeedback ? '根据意见重新规划失败' : '重新生成执行流程失败');
    } finally {
      setRevising(false);
    }
  };

  const submitFeedback = async () => {
    const planningFeedback = feedback.trim();
    if (!planningFeedback || revising || !canRevise) return;
    await replaceExecutionPlan(planningFeedback);
  };

  const regenerateFailedPlan = async () => {
    if (!canRegenerate || revising) return;
    await replaceExecutionPlan();
  };

  const rerunStoppedBatch = async () => {
    if (!canRerun || rerunning) return;
    setRerunning(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const response = await apiClient.rerunProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
        },
      );
      const next = buildRequirementExecutionProcess({
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (!next) {
        throw new Error('后端没有返回新的重新执行批次标识');
      }
      next.executionPaused = false;
      setRerunConfirmOpen(false);
      setFeedback('');
      activeExecutionGroupIdRef.current = next.executionGroupId;
      stoppedExecutionGroupIdsRef.current.delete(next.executionGroupId);
      setLiveProcess(next);
      setMessage(withProcessStatus(next.initialMessage || createFallbackMessage(next), next));
      setPlanStopped(false);
      setPlanDiscarded(false);
      setExecutionConfirmed(Boolean(next.hasStartedRuns));
      setSyncError(null);
      setActionError(null);
      setActionMessage(next.hasStartedRuns
        ? '旧批次资源已清理，新的任务副本已经开始执行。'
        : '旧批次资源已清理，新任务图已经生成；自动启动未成功，请点击“执行”继续。');
      onProcessChange(next);
      await refreshSessionById(next.conversationId);
      await syncSessionMessagesInBackground(next.conversationId);
      await reloadGraph();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : '重新执行失败');
    } finally {
      setRerunning(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 z-[50]" role="dialog" aria-modal="true" aria-label="执行计划工作台">
        <button
          type="button"
          aria-label="关闭执行计划工作台"
          className="absolute inset-0 bg-black/55"
          onClick={onClose}
        />
        <section
          className={requirementExecutionModalShellClassName(fullscreen)}
          data-fullscreen={fullscreen}
        >
          <header className="flex shrink-0 items-start justify-between gap-4 border-b border-border px-4 py-3 sm:px-5">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <h2 className="text-base font-semibold text-foreground">执行计划工作台</h2>
                <span className={cn(
                  'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium',
                  isLocalExecution
                    ? 'border-violet-200 bg-violet-50 text-violet-700 dark:border-violet-800 dark:bg-violet-950/30 dark:text-violet-200'
                    : 'border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-800 dark:bg-sky-950/30 dark:text-sky-200',
                )}
                >
                  {isLocalExecution ? <Laptop className="h-3 w-3" /> : <Cloud className="h-3 w-3" />}
                  {isLocalExecution ? '本地规划 / 本地执行' : '云端规划 / 云端执行'}
                </span>
              </div>
              <p className="mt-1 truncate text-sm text-muted-foreground">
                {readText(liveProcess.requirement.title) || liveProcess.requirement.id}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                disabled={loading || syncing}
                onClick={() => void refreshAll(false)}
              >
                <RefreshCw className={cn('h-3.5 w-3.5', (loading || syncing) && 'animate-spin')} />
                刷新
              </button>
              <FullscreenToggleButton
                fullscreen={fullscreen}
                onToggle={() => setFullscreen((current) => !current)}
              />
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={onClose}
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </header>

          <div className="grid min-h-0 flex-1 lg:grid-cols-[390px_minmax(0,1fr)]">
            <aside className="flex min-h-0 flex-col border-b border-border bg-muted/10 lg:border-b-0 lg:border-r">
              <div className="border-b border-border px-4 py-3">
                <div className="flex items-start gap-2">
                  {cancellationSettling ? (
                    <LoaderCircle className="mt-0.5 h-4 w-4 shrink-0 animate-spin text-sky-500" />
                  ) : phase === 'failed' ? (
                    <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-500" />
                  ) : phase === 'completed' ? (
                    <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-500" />
                  ) : phase === 'stopped' ? (
                    <Square className="mt-0.5 h-4 w-4 shrink-0 fill-current text-slate-500" />
                  ) : phase === 'paused' ? (
                    <Pause className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
                  ) : (
                    <LoaderCircle className={cn(
                      'mt-0.5 h-4 w-4 shrink-0 text-sky-500',
                      !terminal && 'animate-spin',
                    )}
                    />
                  )}
                  <div>
                    <div className="text-sm font-semibold text-foreground">{phaseText.title}</div>
                    <p className="mt-1 text-xs leading-5 text-muted-foreground">{phaseText.detail}</p>
                  </div>
                </div>
              </div>

              <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
                <div className="mb-3 flex items-center justify-between">
                  <div className="text-xs font-semibold text-foreground">规划过程</div>
                  <span className="text-[11px] text-muted-foreground">{allTasks.length} 个任务</span>
                </div>
                <ol className="space-y-3">
                  {processEntries.map((entry, index) => (
                    <li key={entry.id} className="relative flex gap-3">
                      {index < processEntries.length - 1 ? (
                        <span className="absolute left-[9px] top-5 h-[calc(100%+0.75rem)] w-px bg-border" />
                      ) : null}
                      <span className={cn(
                        'relative z-10 mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border bg-background',
                        entry.state === 'done' && 'border-emerald-400 text-emerald-600',
                        entry.state === 'active' && 'border-sky-400 text-sky-600',
                        entry.state === 'error' && 'border-red-400 text-red-600',
                        entry.state === 'stopped' && 'border-slate-400 text-slate-500',
                        entry.state === 'pending' && 'border-border text-muted-foreground',
                      )}
                      >
                        {entry.state === 'done' ? (
                          <Check className="h-3 w-3" />
                        ) : entry.state === 'active' ? (
                          <LoaderCircle className="h-3 w-3 animate-spin" />
                        ) : entry.state === 'error' ? (
                          <X className="h-3 w-3" />
                        ) : entry.state === 'stopped' ? (
                          <Square className="h-2.5 w-2.5 fill-current" />
                        ) : (
                          <span className="h-1.5 w-1.5 rounded-full bg-current" />
                        )}
                      </span>
                      <div className="min-w-0 pb-1">
                        <div className="break-words text-xs font-medium text-foreground">{entry.title}</div>
                        {entry.detail ? (
                          <div className="mt-1 line-clamp-4 whitespace-pre-wrap text-[11px] leading-5 text-muted-foreground">
                            {entry.detail}
                          </div>
                        ) : null}
                      </div>
                    </li>
                  ))}
                </ol>
              </div>

              <div className="shrink-0 border-t border-border bg-background px-4 py-3">
                <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                  <MessageSquareText className="h-3.5 w-3.5" />
                  调整执行计划
                </div>
                <textarea
                  value={feedback}
                  disabled={!canRevise || revising}
                  onChange={(event) => setFeedback(event.target.value)}
                  placeholder={cancellationSettling
                    ? '正在取消当前批次，任务状态收敛后即可调整'
                    : !canRevise
                    ? '当前批次仍有活动任务，请先取消本次执行后再调整'
                    : phase === 'completed'
                      ? '输入新的调整意见，将基于已完成结果重新生成执行流程……'
                      : '输入你的想法，例如：先补测试，再修改接口；把前端和后端拆开执行……'}
                  className="min-h-24 w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground focus:border-primary disabled:cursor-not-allowed disabled:bg-muted/40"
                />
                <div className="mt-2 flex items-center justify-between gap-2">
                  <span className="text-[10px] text-muted-foreground">
                    发送后会保留历史记录并生成新的 DAG
                  </span>
                  <button
                    type="button"
                    className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                    disabled={!feedback.trim() || revising || !canRevise}
                    onClick={() => void submitFeedback()}
                  >
                    {revising ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Send className="h-3.5 w-3.5" />}
                    {revising ? '重新规划中' : '发送并调整'}
                  </button>
                </div>
              </div>
            </aside>

            <main className="flex min-h-0 min-w-0 flex-col">
              <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
                <div>
                  <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
                    <GitBranch className="h-4 w-4" />
                    实时执行流程图
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    Agent 每创建一个任务节点，右侧 DAG 都会自动更新。
                  </p>
                </div>
                <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
                  <span className="rounded-full border border-border bg-background px-2 py-1">节点 {allTasks.length}</span>
                  <span className="rounded-full border border-border bg-background px-2 py-1">依赖 {graph.edges.length}</span>
                  <span className="rounded-full border border-border bg-background px-2 py-1">
                    运行记录 {allTasks.filter((task) => Boolean(readString(task.last_run_id))).length}
                  </span>
                </div>
              </div>

              {(actionError || syncError || actionMessage) ? (
                <div className="shrink-0 px-4 pt-3">
                  {actionError || syncError ? (
                    <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                      {actionError || syncError}
                    </div>
                  ) : (
                    <div className="rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-200">
                      {actionMessage}
                    </div>
                  )}
                </div>
              ) : null}

              <div ref={graphContainerRef} className="min-h-0 flex-1 p-4">
                <MessageTaskGraphPanel
                  graph={graph}
                  loading={loading}
                  error={graphError}
                  loadingRunId={loadingRunId}
                  loadingChangesRunId={loadingChangesRunId}
                  panelWidth={panelWidth}
                  loadingProcessTaskId={loadingProcessTaskId}
                  onOpenDetail={openDetail}
                  onOpenProcessLog={openProcessLog}
                  onOpenRun={openRun}
                  onOpenChanges={openChanges}
                />
              </div>

              <footer className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-border bg-muted/10 px-4 py-3">
                <div className="text-xs text-muted-foreground">
                  {cancellationSettling
                    ? (runningTaskCount > 0
                      ? `正在取消本次执行，等待 ${runningTaskCount} 个运行中任务结束并回写状态。`
                      : queuedTaskCount > 0
                        ? `正在取消本次执行，等待 ${queuedTaskCount} 个排队任务回写取消状态。`
                        : '正在取消本次执行，等待任务回写取消状态。')
                    : graphReady
                    ? '流程图已就绪，当前没有任何任务 run。'
                    : phase === 'paused'
                      ? (runningTaskCount > 0
                        ? `暂停门禁已生效，等待 ${runningTaskCount} 个运行中任务结束。`
                        : '执行已暂停，不会启动新的任务节点。')
                    : phase === 'completed'
                      ? '全部任务已完成，执行结果已同步回项目 Plan。'
                    : phase === 'failed'
                      ? '当前批次存在失败任务，可查看详情、重试或重新规划。'
                    : actuallyStarted
                      ? 'Task Runner 已收到执行确认。'
                      : '完整 DAG 生成前，执行按钮保持禁用。'}
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  {!cancellationSettling && retryableFailedTasks.length > 0 ? (
                    <button
                      type="button"
                      aria-label={`重试失败任务，共 ${retryableFailedTasks.length} 个`}
                      className="inline-flex items-center gap-1.5 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs font-semibold text-red-700 hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200"
                      disabled={Boolean(retryingTaskId)}
                      onClick={() => setFailedTaskRetryOpen(true)}
                    >
                      {retryingTaskId
                        ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        : <RotateCcw className="h-3.5 w-3.5" />}
                      重试失败任务
                      <span className="rounded-full bg-red-600 px-1.5 py-0.5 text-[10px] leading-none text-white">
                        {retryableFailedTasks.length}
                      </span>
                    </button>
                  ) : null}
                  {canRegenerate ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
                      disabled={revising || rerunning || stopping || confirming}
                      onClick={() => void regenerateFailedPlan()}
                    >
                      {revising ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
                      {revising ? '重新生成中' : '重新生成执行流程'}
                    </button>
                  ) : null}
                  {canRerun ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
                      disabled={rerunning || revising || stopping || confirming}
                      onClick={() => setRerunConfirmOpen(true)}
                    >
                      {rerunning ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
                      {rerunning ? '重新执行中' : '重新执行'}
                    </button>
                  ) : null}
                  {actuallyStarted && phase !== 'stopped' && (hasActiveRuns || phase === 'paused') ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
                      disabled={pausing || stopping || confirming || revising || rerunning}
                      onClick={() => void setExecutionPause(!liveProcess.executionPaused)}
                    >
                      {pausing
                        ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                        : liveProcess.executionPaused
                          ? <Play className="h-3.5 w-3.5" />
                          : <Pause className="h-3.5 w-3.5" />}
                      {pausing
                        ? (liveProcess.executionPaused ? '恢复调度中' : '暂停调度中')
                        : liveProcess.executionPaused ? '继续调度' : '暂停后续任务'}
                    </button>
                  ) : null}
                  {shouldShowCancelRequirementExecution({ actuallyStarted, phase }) ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs font-medium text-red-700 hover:bg-red-100 disabled:opacity-60 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200"
                      disabled={stopping || pausing || confirming || revising || rerunning}
                      onClick={() => setCancelConfirmOpen(true)}
                    >
                      {stopping ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <XCircle className="h-3.5 w-3.5" />}
                      {stopping ? '取消中' : '取消本次执行'}
                    </button>
                  ) : null}
                  {shouldShowDiscardRequirementPlan({ actuallyStarted, phase }) ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                      disabled={stopping || pausing || confirming || revising}
                      onClick={() => setDiscardConfirmOpen(true)}
                    >
                      {stopping ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Square className="h-3.5 w-3.5" />}
                      {stopping ? '取消并清理中' : '取消规划并删除任务'}
                    </button>
                  ) : null}
                  {!actuallyStarted && phase !== 'failed' && phase !== 'stopped' ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-5 py-2 text-xs font-semibold text-white hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={!graphReady || confirming || stopping || pausing || revising}
                      onClick={() => void confirmExecution()}
                    >
                      {confirming ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
                      {confirming ? '启动中' : '执行'}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-accent"
                    onClick={onClose}
                  >
                    关闭
                  </button>
                </div>
              </footer>
            </main>
          </div>
        </section>
      </div>

      {failedTaskRetryOpen ? (
        <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4" role="dialog" aria-modal="true" aria-label="失败任务重试">
          <section className="flex max-h-[min(720px,88dvh)] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
            <header className="flex shrink-0 items-start justify-between gap-3 border-b border-border px-5 py-4">
              <div className="flex min-w-0 items-start gap-3">
                <span className="mt-0.5 rounded-full bg-red-100 p-2 text-red-700 dark:bg-red-950/40 dark:text-red-300">
                  <RotateCcw className="h-5 w-5" />
                </span>
                <div className="min-w-0">
                  <h3 className="text-base font-semibold text-foreground">重试失败任务</h3>
                  <p className="mt-1 text-sm leading-6 text-muted-foreground">
                    下面只列出当前执行批次中可以重试的失败节点。点击“重新开始”后，只重跑该节点；成功后，后续节点会继续按照依赖关系调度。
                  </p>
                </div>
              </div>
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => setFailedTaskRetryOpen(false)}
                aria-label="关闭失败任务列表"
              >
                <X className="h-4 w-4" />
              </button>
            </header>

            <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-5">
              {retryableFailedTasks.map((task) => {
                const taskId = readString(task.id) || '';
                const taskTitle = task.title || taskId;
                const retrying = retryingTaskId === taskId;
                const errorMessage = readString(task.last_run?.error_message);
                return (
                  <div
                    key={taskId}
                    role="group"
                    aria-label={`失败任务：${taskTitle}`}
                    className="rounded-lg border border-red-200 bg-red-50/60 p-4 dark:border-red-900 dark:bg-red-950/20"
                  >
                    <div className="flex flex-wrap items-start justify-between gap-4">
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="rounded-full border border-red-300 bg-background px-2 py-0.5 text-[11px] font-medium text-red-700 dark:border-red-800 dark:text-red-200">
                            执行失败
                          </span>
                          <span className="text-[11px] text-muted-foreground">
                            运行 {readString(task.last_run_id)}
                          </span>
                        </div>
                        <h4 className="mt-2 break-words text-sm font-semibold text-foreground">
                          {taskTitle}
                        </h4>
                        {(readString(task.objective) || readString(task.description)) ? (
                          <p className="mt-1 line-clamp-3 whitespace-pre-wrap text-xs leading-5 text-muted-foreground">
                            {readString(task.objective) || readString(task.description)}
                          </p>
                        ) : null}
                        {errorMessage ? (
                          <div className="mt-3 rounded-md border border-red-200 bg-background/80 px-3 py-2 text-xs leading-5 text-red-700 dark:border-red-900 dark:text-red-200">
                            {errorMessage}
                          </div>
                        ) : null}
                      </div>
                      <button
                        type="button"
                        className="inline-flex shrink-0 items-center gap-1.5 rounded-md bg-red-600 px-4 py-2 text-xs font-semibold text-white hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-60"
                        disabled={Boolean(retryingTaskId)}
                        onClick={() => void retryFailedTask(task)}
                      >
                        {retrying
                          ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                          : <Play className="h-3.5 w-3.5" />}
                        {retrying ? '正在重新开始' : '重新开始'}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>

            <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/10 px-5 py-3">
              <span className="text-xs text-muted-foreground">
                当前共有 {retryableFailedTasks.length} 个失败任务可以重试
              </span>
              <button
                type="button"
                className="rounded-md border border-border bg-background px-4 py-2 text-xs font-medium text-foreground hover:bg-accent"
                onClick={() => setFailedTaskRetryOpen(false)}
              >
                关闭
              </button>
            </footer>
          </section>
        </div>
      ) : null}

      {cancelConfirmOpen ? (
        <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4" role="alertdialog" aria-modal="true" aria-label="确认取消本次执行">
          <section className="w-full max-w-lg rounded-xl border border-border bg-card p-5 shadow-2xl">
            <div className="flex items-start gap-3">
              <span className="mt-0.5 rounded-full bg-red-100 p-2 text-red-700 dark:bg-red-950/40 dark:text-red-300">
                <AlertTriangle className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <h3 className="text-base font-semibold text-foreground">取消本次执行？</h3>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">
                  这会取消正在运行的任务，并阻止所有排队和后续依赖节点继续执行。这个操作不是暂停，取消后不能直接继续当前批次。
                </p>
                <p className="mt-2 text-xs leading-5 text-red-700 dark:text-red-300">
                  如需稍后继续，请关闭此提示并使用“暂停后续任务”；当前已运行任务仍会继续完成。
                </p>
              </div>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-60"
                disabled={stopping}
                onClick={() => setCancelConfirmOpen(false)}
              >
                返回
              </button>
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md bg-red-600 px-4 py-2 text-sm font-semibold text-white hover:bg-red-700 disabled:opacity-60"
                disabled={stopping}
                onClick={() => void stopCurrentBatch(false)}
              >
                {stopping ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <XCircle className="h-4 w-4" />}
                {stopping ? '正在取消本次执行' : '确认取消本次执行'}
              </button>
            </div>
          </section>
        </div>
      ) : null}

      {discardConfirmOpen ? (
        <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4" role="alertdialog" aria-modal="true" aria-label="确认取消规划并删除任务">
          <section className="w-full max-w-lg rounded-xl border border-border bg-card p-5 shadow-2xl">
            <div className="flex items-start gap-3">
              <span className="mt-0.5 rounded-full bg-red-100 p-2 text-red-700 dark:bg-red-950/40 dark:text-red-300">
                <AlertTriangle className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <h3 className="text-base font-semibold text-foreground">取消规划并删除已创建的任务？</h3>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">
                  系统会立即取消当前规划 Agent，并删除这个执行批次已经创建的 Task Runner 任务、运行记录和关联链接。
                </p>
                <p className="mt-2 text-xs leading-5 text-muted-foreground">
                  项目需求、项目任务和技术文档不会被删除。之后仍可重新发起规划。
                </p>
              </div>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-60"
                disabled={stopping}
                onClick={() => setDiscardConfirmOpen(false)}
              >
                取消
              </button>
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md bg-red-600 px-4 py-2 text-sm font-semibold text-white hover:bg-red-700 disabled:opacity-60"
                disabled={stopping}
                onClick={() => void stopCurrentBatch(true)}
              >
                {stopping ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <Square className="h-4 w-4" />}
                {stopping ? '正在取消并清理' : '确认取消并删除'}
              </button>
            </div>
          </section>
        </div>
      ) : null}

      {rerunConfirmOpen ? (
        <div className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4" role="alertdialog" aria-modal="true" aria-label="确认重新执行">
          <section className="w-full max-w-lg rounded-xl border border-border bg-card p-5 shadow-2xl">
            <div className="flex items-start gap-3">
              <span className="mt-0.5 rounded-full bg-amber-100 p-2 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
                <AlertTriangle className="h-5 w-5" />
              </span>
              <div className="min-w-0">
                <h3 className="text-base font-semibold text-foreground">确认重新执行整个任务流程？</h3>
                <p className="mt-2 text-sm leading-6 text-muted-foreground">
                  系统会复制当前完整 DAG 并立即执行新副本。新批次创建成功后，会删除旧批次的任务、运行记录、沙箱、临时分支、worktree 和本地临时目录。
                </p>
                <p className="mt-2 text-xs leading-5 text-amber-700 dark:text-amber-300">
                  此清理操作不可撤销，但项目需求和流程替换记录会保留。
                </p>
              </div>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-60"
                disabled={rerunning}
                onClick={() => setRerunConfirmOpen(false)}
              >
                取消
              </button>
              <button
                type="button"
                className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-4 py-2 text-sm font-semibold text-white hover:bg-amber-700 disabled:opacity-60"
                disabled={rerunning}
                onClick={() => void rerunStoppedBatch()}
              >
                {rerunning ? <LoaderCircle className="h-4 w-4 animate-spin" /> : <RotateCcw className="h-4 w-4" />}
                {rerunning ? '正在清理并重新执行' : '确认清理并重新执行'}
              </button>
            </div>
          </section>
        </div>
      ) : null}

      <MessageTaskDetailModal
        task={detailTask}
        relatedTasks={allTasks}
        retrying={Boolean(retryingTaskId)}
        retryError={retryError}
        onRetry={retryTask}
        onClose={closeDetail}
      />
      <MessageTaskProcessLogModal task={processTask} onClose={closeProcessLog} />
      <MessageTaskRunDetailModal
        detail={runDetail}
        loadingMoreEvents={Boolean(runDetail && loadingRunId === runDetail.run?.id)}
        onLoadMoreEvents={loadMoreRunEvents}
        onClose={closeRun}
      />
      <MessageTaskChangesModal
        task={changesTask}
        changes={outputChanges}
        diff={outputDiff}
        selectedPath={selectedChangePath}
        loadingChanges={Boolean(
          changesTask?.last_run_id && loadingChangesRunId === changesTask.last_run_id
        )}
        loadingDiff={Boolean(selectedChangePath && loadingDiffPath === selectedChangePath)}
        error={graphError}
        onSelectFile={selectChangeFile}
        onClose={closeChanges}
      />
    </>
  );
};
