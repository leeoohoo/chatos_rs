// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RequirementExecutionProcessModal } from './RequirementExecutionProcessModal';

const mocks = vi.hoisted(() => ({
  executeProjectRequirement: vi.fn(),
  getProjectRequirementExecutionPlan: vi.fn(),
  onProcessChange: vi.fn(),
  reloadGraph: vi.fn(),
  stopProjectRequirementExecution: vi.fn(),
}));

vi.mock('../../../lib/api/ApiClientContext', () => ({
  useApiClient: () => ({
    executeProjectRequirement: mocks.executeProjectRequirement,
    getProjectRequirementExecutionPlan: mocks.getProjectRequirementExecutionPlan,
    stopProjectRequirementExecution: mocks.stopProjectRequirementExecution,
  }),
}));

vi.mock('../../../lib/store', () => ({
  useChatStore: (selector: (state: Record<string, unknown>) => unknown) => selector({
    refreshSessionById: vi.fn(),
    syncSessionMessagesInBackground: vi.fn(),
  }),
}));

vi.mock('../../messageTasks/projectExecutionConfirmation', () => ({
  resolveProjectExecutionConfirmationState: () => ({
    isProjectExecution: true,
    awaitingConfirmation: false,
    graphReadyForConfirmation: false,
    hasStartedTasks: false,
    canConfirm: false,
    projectId: 'project-1',
    requirementId: 'requirement-1',
    executionGroupId: 'execution-group-failed',
    conversationId: 'conversation-1',
    contactId: 'contact-1',
    overallStatus: 'failed',
  }),
}));

vi.mock('../../messageTasks/useMessageTaskGraph', () => ({
  useMessageTaskGraph: () => ({
    graph: { edges: [], nodes: [] },
    allTasks: [],
    loading: false,
    error: null,
    detailTask: null,
    processTask: null,
    runDetail: null,
    changesTask: null,
    outputChanges: [],
    outputDiff: null,
    selectedChangePath: null,
    loadingProcessTaskId: null,
    loadingRunId: null,
    loadingChangesRunId: null,
    loadingDiffPath: null,
    reloadGraph: mocks.reloadGraph,
    openDetail: vi.fn(),
    openProcessLog: vi.fn(),
    openRun: vi.fn(),
    openChanges: vi.fn(),
    selectChangeFile: vi.fn(),
    loadMoreRunEvents: vi.fn(),
    closeDetail: vi.fn(),
    closeProcessLog: vi.fn(),
    closeRun: vi.fn(),
    closeChanges: vi.fn(),
  }),
}));

vi.mock('../../messageTasks/MessageTaskGraphPanel', () => ({
  MessageTaskGraphPanel: () => <div>empty graph</div>,
}));

vi.mock('../../messageTasks/MessageTaskChangesModal', () => ({
  MessageTaskChangesModal: () => null,
}));

vi.mock('../../messageTasks/MessageTaskDetailModal', () => ({
  MessageTaskDetailModal: () => null,
  MessageTaskProcessLogModal: () => null,
}));

vi.mock('../../messageTasks/MessageTaskRunDetailModal', () => ({
  MessageTaskRunDetailModal: () => null,
}));

afterEach(() => cleanup());

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getProjectRequirementExecutionPlan.mockResolvedValue({
    found: true,
    conversation_id: 'conversation-1',
    execution_group_id: 'execution-group-failed',
    message_id: 'message-failed',
    status: 'failed',
    has_started_runs: false,
    recovery_action: 'regenerate',
    recovery_reason: 'failed_before_execution_started',
    replace_previous_batch: true,
    planning_feedback_history: ['先补测试', '接口任务放在最后'],
  });
  mocks.stopProjectRequirementExecution.mockResolvedValue({
    status: 'stopped',
    recovery_action: 'regenerate',
    recovery_reason: 'stopped_without_task_graph',
    replace_previous_batch: true,
  });
  mocks.executeProjectRequirement.mockResolvedValue({
    conversation_id: 'conversation-1',
    execution_group_id: 'execution-group-replacement',
    message_id: 'message-replacement',
    status: 'planning',
    has_started_runs: false,
    recovery_action: 'none',
    replace_previous_batch: true,
  });
});

describe('failed requirement execution planning recovery', () => {
  it('regenerates a failed zero-task plan without requiring new feedback', async () => {
    const user = userEvent.setup();
    render(
      <RequirementExecutionProcessModal
        process={{
          requirement: { id: 'requirement-1', title: 'Requirement 1' },
          projectId: 'project-1',
          conversationId: 'conversation-1',
          executionGroupId: 'execution-group-failed',
          messageId: 'message-failed',
          contactId: 'contact-1',
          selectedModelId: 'model-1',
          planningFeedbackHistory: ['先补测试', '接口任务放在最后'],
          serverStatus: 'failed',
          hasStartedRuns: false,
          recoveryAction: 'regenerate',
          recoveryReason: 'failed_before_execution_started',
          replacePreviousBatch: true,
        }}
        onClose={vi.fn()}
        onProcessChange={mocks.onProcessChange}
      />,
    );

    await user.click(screen.getByRole('button', { name: '重新生成执行流程' }));

    await waitFor(() => expect(mocks.stopProjectRequirementExecution).toHaveBeenCalledWith(
      'project-1',
      'requirement-1',
      {
        execution_group_id: 'execution-group-failed',
        conversation_id: 'conversation-1',
        contact_id: 'contact-1',
      },
    ));
    expect(mocks.executeProjectRequirement).toHaveBeenCalledWith(
      'project-1',
      'requirement-1',
      {
        contact_id: 'contact-1',
        model_config_id: 'model-1',
        include_prerequisite_dependents: false,
        replaces_execution_group_id: 'execution-group-failed',
        replaces_conversation_id: 'conversation-1',
      },
    );
    expect(mocks.onProcessChange).toHaveBeenCalledWith(expect.objectContaining({
      executionGroupId: 'execution-group-replacement',
      planningFeedbackHistory: ['先补测试', '接口任务放在最后'],
      hasStartedRuns: false,
    }));
  });

  it('shows regenerate for a reopened stopped zero-task plan with stale recovery metadata', async () => {
    const user = userEvent.setup();
    mocks.getProjectRequirementExecutionPlan.mockResolvedValue({
      found: true,
      conversation_id: 'conversation-1',
      execution_group_id: 'execution-group-stopped',
      message_id: 'message-stopped',
      status: 'stopped',
      task_count: 0,
      has_started_runs: false,
      recovery_action: 'none',
      recovery_reason: 'not_recoverable_in_current_state',
      replace_previous_batch: true,
    });
    render(
      <RequirementExecutionProcessModal
        process={{
          requirement: { id: 'requirement-1', title: 'Requirement 1' },
          projectId: 'project-1',
          conversationId: 'conversation-1',
          executionGroupId: 'execution-group-stopped',
          messageId: 'message-stopped',
          contactId: 'contact-1',
          selectedModelId: 'model-1',
          serverStatus: 'stopped',
          hasStartedRuns: false,
          recoveryAction: 'none',
          recoveryReason: 'not_recoverable_in_current_state',
          replacePreviousBatch: true,
        }}
        onClose={vi.fn()}
        onProcessChange={mocks.onProcessChange}
      />,
    );

    const regenerateButton = await screen.findByRole('button', {
      name: '重新生成执行流程',
    });
    await user.click(regenerateButton);

    expect(mocks.stopProjectRequirementExecution).not.toHaveBeenCalled();
    expect(mocks.executeProjectRequirement).toHaveBeenCalledWith(
      'project-1',
      'requirement-1',
      {
        contact_id: 'contact-1',
        model_config_id: 'model-1',
        include_prerequisite_dependents: false,
        replaces_execution_group_id: 'execution-group-stopped',
        replaces_conversation_id: 'conversation-1',
      },
    );
  });
});
