// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { Message } from '../../../types';
import type {
  MessageTaskRunnerTask,
  RequirementExecutionRecoveryAction,
} from '../../../lib/api/client/types';
import type {
  ProjectExecutionConfirmationState,
} from '../../messageTasks/projectExecutionConfirmation';
import { readString } from '../../messageTasks/utils';
import { readText } from './model';
import type { RequirementExecutionProcess } from './requirementExecutionProcessModel';

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
export const ACTIVE_TASK_STATUSES = new Set([
  'queued',
  'pending',
  'running',
  'processing',
  'in_progress',
  'doing',
]);

export const isStoppedExecutionStatus = (status?: string | null): boolean => (
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
    || ['stopping', 'stopped', 'cancelled', 'canceled'].includes(
      normalizedServerStatus || overallStatus,
    )
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
    || ['failed', 'error', 'blocked'].includes(normalizedServerStatus || overallStatus)
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

export const createFallbackMessage = (process: RequirementExecutionProcess): Message => ({
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
      recovery_action: process.recoveryAction || undefined,
      recovery_reason: process.recoveryReason || undefined,
      replace_previous_batch: process.replacePreviousBatch,
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

export const withProcessStatus = (
  message: Message,
  process: RequirementExecutionProcess,
): Message => ({
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
      recovery_action: process.recoveryAction || undefined,
      recovery_reason: process.recoveryReason || undefined,
      replace_previous_batch: process.replacePreviousBatch,
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

export const phaseCopy: Record<
  RequirementExecutionProcessPhase,
  { title: string; detail: string }
> = {
  planning_context: {
    title: '正在读取需求、项目任务和技术文档',
    detail: '正在整理执行范围和约束，任务执行尚未启动。',
  },
  building_graph: {
    title: '正在生成完整执行流程',
    detail: '新创建的任务节点会实时出现在右侧流程图中。',
  },
  awaiting_confirmation: {
    title: '执行流程已生成，等待你确认',
    detail: '你可以继续输入调整意见；只有点击“执行”后任务才会启动。',
  },
  running: {
    title: '任务正在执行',
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

export const resolveRequirementExecutionPhaseCopy = ({
  cancellationSettling,
  phase,
  queuedTaskCount,
  runningTaskCount,
}: {
  cancellationSettling: boolean;
  phase: RequirementExecutionProcessPhase;
  queuedTaskCount: number;
  runningTaskCount: number;
}): { title: string; detail: string } => {
  if (cancellationSettling) {
    return {
      title: '正在取消当前执行',
      detail: runningTaskCount > 0
        ? `取消请求已提交，正在等待 ${runningTaskCount} 个运行中任务结束并回写状态。`
        : `取消请求已提交，正在等待 ${queuedTaskCount > 0 ? `${queuedTaskCount} 个排队任务` : '任务'}回写取消状态。`,
    };
  }
  if (phase === 'paused') {
    return {
      title: '已暂停后续任务调度',
      detail: runningTaskCount > 0
        ? `新的后置任务不会启动；当前 ${runningTaskCount} 个已运行任务会继续完成。`
        : `${queuedTaskCount > 0 ? `${queuedTaskCount} 个排队任务` : '后续任务'}不会启动，点击“继续调度”后恢复依赖调度。`,
    };
  }
  return phaseCopy[phase];
};

export type ProcessEntry = {
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
      title: '本次执行已取消',
      detail: '当前批次已经整体取消，不会继续调度或执行后续任务',
      state: 'stopped',
    };
  }
  if (phase === 'paused') {
    return {
      title: '后续任务已暂停',
      detail: '运行中的节点可正常收尾，但不会启动新的节点',
      state: 'active',
    };
  }
  if (phase === 'completed') {
    return {
      title: '本次执行已完成',
      detail: '所有任务均已到达终态',
      state: 'done',
    };
  }
  if (phase === 'failed') {
    return {
      title: '本次执行失败',
      detail: '当前批次存在失败或阻塞任务',
      state: 'error',
    };
  }
  return {
    title: '任务已开始执行',
    detail: '正在按照右侧依赖顺序运行',
    state: 'active',
  };
};

export const buildRequirementExecutionProcessEntries = ({
  actuallyStarted,
  allTasks,
  cancellationSettling,
  graphReady,
  isLocalExecution,
  phase,
  process,
}: {
  actuallyStarted: boolean;
  allTasks: MessageTaskRunnerTask[];
  cancellationSettling: boolean;
  graphReady: boolean;
  isLocalExecution: boolean;
  phase: RequirementExecutionProcessPhase;
  process: RequirementExecutionProcess;
}): ProcessEntry[] => {
  const entries: ProcessEntry[] = [{
    id: 'accepted',
    title: '执行规划请求已接受',
    detail: isLocalExecution
      ? '由云端统一规划，确认后再通过 Local Connector 调度本机能力执行'
      : '由云端统一规划并继续推进执行准备',
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
  const planningFeedbackHistory = process.planningFeedbackHistory?.length
    ? process.planningFeedbackHistory
    : process.planningFeedback ? [process.planningFeedback] : [];
  planningFeedbackHistory.forEach((planningFeedback, index) => {
    entries.push({
      id: `feedback:${process.executionGroupId}:${index}`,
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
        ? '流程图已冻结等待确认，尚未启动任何任务'
        : phase === 'completed'
          ? '流程图和依赖关系已经全部执行完毕'
          : actuallyStarted
            ? '流程图已冻结，任务正在按照依赖关系执行'
            : '仍在补全任务节点和依赖关系',
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
      detail: '确认之前不会开始运行任务',
      state: 'active',
    });
  }
  if (actuallyStarted) {
    const runnerEntry = cancellationSettling
      ? {
        title: '正在取消本次执行',
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
};

export const resolveRequirementExecutionRecoveryActions = ({
  actuallyStarted,
  hasActiveRuns,
  phase,
  recoveryAction,
}: {
  actuallyStarted: boolean;
  hasActiveRuns: boolean;
  phase: RequirementExecutionProcessPhase;
  recoveryAction?: RequirementExecutionRecoveryAction | null;
}): { canRegenerate: boolean; canRevise: boolean; canRerun: boolean } => ({
  canRegenerate: recoveryAction === 'regenerate' && !hasActiveRuns,
  canRevise: !hasActiveRuns
    && (!actuallyStarted || phase === 'stopped'),
  canRerun: recoveryAction === 'rerun' && phase === 'stopped' && !hasActiveRuns,
});

export const isRequirementExecutionCancellationSettling = ({
  hasActiveRuns,
  phase,
}: {
  hasActiveRuns: boolean;
  phase: RequirementExecutionProcessPhase;
}): boolean => phase === 'stopped' && hasActiveRuns;
