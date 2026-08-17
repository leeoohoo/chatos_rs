// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { ApiRequestError } from '../../../lib/api/client/shared';
import type { ProjectExecutionConfirmationState } from '../../messageTasks/projectExecutionConfirmation';
import {
  buildRequirementExecutionProcess,
  isRequirementExecutionCancellationSettling,
  isRequirementExecutionRerunCancellationSettlingError,
  isPendingRequirementExecutionPlanError,
  isRequirementExecutionRuntimeReady,
  REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS,
  resolveRequirementExecutionRecoveryActions,
  resolveRequirementExecutionProcessPhase,
  runnerProcessEntryForPhase,
  shouldShowCancelRequirementExecution,
  shouldShowDiscardRequirementPlan,
  shouldReplaceRequirementExecutionBatch,
  shouldStopRequirementExecutionBeforeReplacement,
} from './RequirementExecutionProcessModal';

const confirmationState = (
  overrides: Partial<ProjectExecutionConfirmationState> = {},
): ProjectExecutionConfirmationState => ({
  isProjectExecution: true,
  awaitingConfirmation: false,
  graphReadyForConfirmation: false,
  hasStartedTasks: false,
  canConfirm: false,
  projectId: 'project-1',
  requirementId: 'requirement-1',
  executionGroupId: 'execution-group-1',
  conversationId: 'conversation-1',
  contactId: 'contact-1',
  overallStatus: 'planning',
  ...overrides,
});

describe('requirement execution process phase', () => {
  it('does not gate client-managed local projects on server image initialization', () => {
    expect(isRequirementExecutionRuntimeReady({
      clientManagedRuntime: true,
      conversationId: 'conversation-1',
      executionPlane: 'cloud',
      status: 'analyzing',
    })).toBe(true);
    expect(isRequirementExecutionRuntimeReady({
      clientManagedRuntime: false,
      conversationId: 'conversation-1',
      executionPlane: 'cloud',
      status: 'analyzing',
    })).toBe(false);
  });

  it('polls every ten seconds and treats a not-yet-persisted plan as pending', () => {
    expect(REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS).toBe(10_000);
    expect(isPendingRequirementExecutionPlanError(new ApiRequestError(
      '需求执行规划消息不存在',
      { status: 404 },
    ))).toBe(true);
    expect(isPendingRequirementExecutionPlanError(new ApiRequestError(
      'Local execution plan source message was not found',
      { status: 404, code: 'local_execution_plan_source_missing' },
    ))).toBe(true);
    expect(isPendingRequirementExecutionPlanError(new ApiRequestError(
      '需求不存在',
      { status: 404 },
    ))).toBe(false);
    expect(isPendingRequirementExecutionPlanError(
      new Error('需求执行规划消息不存在'),
    )).toBe(true);
  });

  it('treats rerun active-run conflicts as cancellation settling', () => {
    expect(isRequirementExecutionRerunCancellationSettlingError(new ApiRequestError(
      '旧执行批次仍有 1 个 Task Runner 任务正在取消，已重新发送取消请求，请等待取消完成后再重新执行。',
      { status: 409 },
    ))).toBe(true);
    expect(isRequirementExecutionRerunCancellationSettlingError(new ApiRequestError(
      '复制 Task Runner 执行图失败',
      { status: 502 },
    ))).toBe(false);
  });

  it('starts a fresh plan after the previous stopped batch was discarded', () => {
    expect(shouldReplaceRequirementExecutionBatch({
      planDiscarded: true,
      replacePreviousBatch: true,
    })).toBe(false);
    expect(shouldReplaceRequirementExecutionBatch({
      planDiscarded: false,
      replacePreviousBatch: false,
    })).toBe(false);
    expect(shouldReplaceRequirementExecutionBatch({
      planDiscarded: false,
      replacePreviousBatch: true,
    })).toBe(true);
  });

  it('renders a stopped execution as static instead of active', () => {
    expect(runnerProcessEntryForPhase('stopped')).toEqual({
      title: '本次执行已取消',
      detail: '当前批次已经整体取消，不会继续调度或执行后续任务',
      state: 'stopped',
    });
  });

  it('keeps cancel available after execution failure and discard available after planning failure', () => {
    expect(shouldShowCancelRequirementExecution({
      actuallyStarted: true,
      phase: 'failed',
    })).toBe(true);
    expect(shouldShowDiscardRequirementPlan({
      actuallyStarted: false,
      phase: 'failed',
    })).toBe(true);
    expect(shouldShowCancelRequirementExecution({
      actuallyStarted: true,
      phase: 'completed',
    })).toBe(false);
    expect(shouldShowCancelRequirementExecution({
      actuallyStarted: true,
      phase: 'stopped',
    })).toBe(false);
  });

  it('treats persisted pause as a separate non-terminal execution phase', () => {
    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({
        hasStartedTasks: true,
        overallStatus: 'paused',
      }),
      executionPaused: true,
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['running', 'queued'],
    })).toBe('paused');
    expect(runnerProcessEntryForPhase('paused')).toEqual({
      title: '后续任务已暂停',
      detail: '运行中的节点可正常收尾，但不会启动新的节点',
      state: 'active',
    });
    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({
        hasStartedTasks: true,
        overallStatus: 'paused',
      }),
      executionPaused: true,
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['succeeded', 'completed'],
    })).toBe('completed');
  });

  it('enables feedback and rerun only after a batch is fully stopped', () => {
    expect(isRequirementExecutionCancellationSettling({
      hasActiveRuns: true,
      phase: 'stopped',
    })).toBe(true);
    expect(isRequirementExecutionCancellationSettling({
      hasActiveRuns: false,
      phase: 'stopped',
    })).toBe(false);
    expect(isRequirementExecutionCancellationSettling({
      hasActiveRuns: true,
      phase: 'running',
    })).toBe(false);

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: false,
      phase: 'stopped',
      recoveryAction: 'rerun',
    })).toEqual({ canRegenerate: true, canRevise: true, canRerun: true });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: true,
      phase: 'stopped',
      recoveryAction: 'rerun',
    })).toEqual({ canRegenerate: false, canRevise: false, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: false,
      phase: 'failed',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: false, canRevise: false, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: false,
      hasActiveRuns: false,
      phase: 'failed',
      recoveryAction: 'regenerate',
    })).toEqual({ canRegenerate: true, canRevise: true, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: false,
      hasActiveRuns: false,
      phase: 'awaiting_confirmation',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: true, canRevise: true, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: false,
      hasActiveRuns: true,
      phase: 'awaiting_confirmation',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: false, canRevise: false, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: true,
      phase: 'running',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: false, canRevise: false, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: false,
      phase: 'completed',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: false, canRevise: false, canRerun: false });
  });

  it('keeps a completed batch as history when feedback starts a new plan', () => {
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'completed',
      replacePreviousBatch: true,
    })).toBe(false);
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'planning_context',
      replacePreviousBatch: true,
    })).toBe(true);
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'building_graph',
      replacePreviousBatch: true,
    })).toBe(true);
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'awaiting_confirmation',
      replacePreviousBatch: true,
    })).toBe(true);
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'running',
      replacePreviousBatch: true,
    })).toBe(true);
    expect(shouldStopRequirementExecutionBeforeReplacement({
      phase: 'stopped',
      replacePreviousBatch: true,
    })).toBe(false);
  });

  it('restores a persisted cloud or local planning batch for reopening from Plan', () => {
    const process = buildRequirementExecutionProcess({
      projectId: 'project-1',
      requirement: { id: 'requirement-1', title: 'Requirement 1' },
      response: {
        found: true,
        execution_plane: 'local_connector',
        conversation_id: 'lc_session_1',
        execution_group_id: 'lc_execution_group_1',
        message_id: 'lc_message_1',
        status: 'awaiting_confirmation',
        has_started_runs: false,
        execution_paused: true,
        recovery_action: 'none',
        recovery_reason: 'not_recoverable_in_current_state',
        replace_previous_batch: true,
        planning_feedback: '再拆分接口',
        planning_feedback_history: ['先补测试', '再拆分接口'],
      },
    });

    expect(process).toMatchObject({
      conversationId: 'lc_session_1',
      executionGroupId: 'lc_execution_group_1',
      messageId: 'lc_message_1',
      planningFeedback: '再拆分接口',
      planningFeedbackHistory: ['先补测试', '再拆分接口'],
      hasStartedRuns: false,
      executionPaused: true,
      recoveryAction: 'none',
      recoveryReason: 'not_recoverable_in_current_state',
      replacePreviousBatch: true,
    });
  });

  it('normalizes stale stopped zero-task recovery payloads into regenerate', () => {
    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: false,
      phase: 'stopped',
      recoveryAction: 'regenerate',
    })).toEqual({ canRegenerate: true, canRevise: true, canRerun: false });

    expect(resolveRequirementExecutionRecoveryActions({
      actuallyStarted: true,
      hasActiveRuns: false,
      phase: 'stopped',
      recoveryAction: 'none',
    })).toEqual({ canRegenerate: true, canRevise: true, canRerun: false });

    const process = buildRequirementExecutionProcess({
      projectId: 'project-1',
      requirement: { id: 'requirement-1', title: 'Requirement 1' },
      response: {
        found: true,
        execution_plane: 'cloud',
        conversation_id: 'conversation-1',
        execution_group_id: 'execution-group-stopped',
        message_id: 'message-stopped',
        status: 'stopped',
        task_count: 0,
        has_started_runs: false,
        recovery_action: 'none',
        recovery_reason: 'not_recoverable_in_current_state',
        replace_previous_batch: true,
      },
    });

    expect(process).toMatchObject({
      serverStatus: 'stopped',
      taskCount: 0,
      hasStartedRuns: false,
      recoveryAction: 'regenerate',
      replacePreviousBatch: true,
    });
  });

  it('appends a new legacy feedback value to the previous process history', () => {
    const process = buildRequirementExecutionProcess({
      fallback: {
        requirement: { id: 'requirement-1', title: 'Requirement 1' },
        projectId: 'project-1',
        conversationId: 'session-1',
        executionGroupId: 'group-old',
        messageId: 'message-old',
        planningFeedback: '第一条意见',
        planningFeedbackHistory: ['第一条意见'],
      },
      projectId: 'project-1',
      requirement: { id: 'requirement-1', title: 'Requirement 1' },
      response: {
        conversation_id: 'session-1',
        execution_group_id: 'group-new',
        message_id: 'message-new',
        planning_feedback: '第二条意见',
      },
    });

    expect(process?.planningFeedbackHistory).toEqual(['第一条意见', '第二条意见']);
  });

  it('shows planning and partial DAG generation as separate real states', () => {
    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState(),
      failureDetected: false,
      planStopped: false,
      taskStatuses: [],
    })).toBe('planning_context');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState(),
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['ready'],
    })).toBe('building_graph');
  });

  it('allows the modal to enter confirmation only after the complete deferred graph is ready', () => {
    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({
        awaitingConfirmation: true,
        graphReadyForConfirmation: true,
        canConfirm: true,
        overallStatus: 'awaiting_confirmation',
      }),
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['ready', 'todo'],
    })).toBe('awaiting_confirmation');
  });

  it('reports running, failed, stopped, and completed terminal states', () => {
    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ hasStartedTasks: true, overallStatus: 'processing' }),
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['running'],
    })).toBe('running');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ overallStatus: 'processing' }),
      executionConfirmed: true,
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['ready'],
    })).toBe('running');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ overallStatus: 'failed' }),
      failureDetected: true,
      planStopped: false,
      taskStatuses: [],
    })).toBe('failed');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ overallStatus: 'blocked' }),
      failureDetected: false,
      planStopped: false,
      taskStatuses: [],
    })).toBe('failed');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ hasStartedTasks: true, overallStatus: 'failed' }),
      failureDetected: true,
      planStopped: false,
      taskStatuses: ['failed', 'queued', 'running'],
    })).toBe('running');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ overallStatus: 'stopped' }),
      failureDetected: false,
      planStopped: true,
      taskStatuses: [],
    })).toBe('stopped');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ hasStartedTasks: true, overallStatus: 'stopping' }),
      failureDetected: false,
      planStopped: false,
      serverStatus: 'stopping',
      taskStatuses: ['running'],
    })).toBe('stopped');

    expect(resolveRequirementExecutionProcessPhase({
      confirmationState: confirmationState({ overallStatus: 'completed' }),
      failureDetected: false,
      planStopped: false,
      taskStatuses: ['succeeded', 'completed'],
    })).toBe('completed');
  });
});
