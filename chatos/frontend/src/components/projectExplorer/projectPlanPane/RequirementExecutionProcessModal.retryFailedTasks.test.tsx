// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RequirementExecutionProcessModal } from './RequirementExecutionProcessModal';

const mocks = vi.hoisted(() => ({
  failedTask: {
    id: 'task-failed-1',
    title: '实现 OMS 订单录入、审核与内联建档流程',
    objective: '实现订单录入和审核流程',
    status: 'failed',
    last_run_id: 'run-failed-1',
    last_run: {
      id: 'run-failed-1',
      task_id: 'task-failed-1',
      model_config_id: 'model-1',
      status: 'failed',
      error_message: 'project runtime has multiple application services (mdm-service, web-prototype); execution_service_id must be selected by the user or program',
    },
    mcp_config: {
      requires_execution: true,
      execution_service_id: null,
    },
  },
  getProjectRequirementExecutionPlan: vi.fn(),
  getProjectRuntimeEnvironment: vi.fn(),
  reloadGraph: vi.fn(),
  retryTask: vi.fn(),
  apiClient: null as unknown as {
    getProjectRequirementExecutionPlan: ReturnType<typeof vi.fn>;
    getProjectRuntimeEnvironment: ReturnType<typeof vi.fn>;
  },
}));

mocks.apiClient = {
  getProjectRequirementExecutionPlan: mocks.getProjectRequirementExecutionPlan,
  getProjectRuntimeEnvironment: mocks.getProjectRuntimeEnvironment,
};

vi.mock('../../../lib/api/ApiClientContext', () => ({
  useApiClient: () => mocks.apiClient,
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
    hasStartedTasks: true,
    canConfirm: false,
    projectId: 'project-1',
    requirementId: 'requirement-1',
    executionGroupId: 'execution-group-1',
    conversationId: 'conversation-1',
    contactId: 'contact-1',
    overallStatus: 'failed',
  }),
}));

vi.mock('../../messageTasks/useMessageTaskGraph', () => ({
  useMessageTaskGraph: () => ({
    graph: { edges: [], nodes: [{ task: mocks.failedTask }] },
    allTasks: [
      mocks.failedTask,
      {
        id: 'task-success-1',
        title: '已经成功的任务',
        status: 'succeeded',
        last_run_id: 'run-success-1',
      },
    ],
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
    retryingTaskId: null,
    reloadGraph: mocks.reloadGraph,
    openDetail: vi.fn(),
    openProcessLog: vi.fn(),
    openRun: vi.fn(),
    openChanges: vi.fn(),
    retryTask: mocks.retryTask,
    selectChangeFile: vi.fn(),
    loadMoreRunEvents: vi.fn(),
    closeDetail: vi.fn(),
    closeProcessLog: vi.fn(),
    closeRun: vi.fn(),
    closeChanges: vi.fn(),
  }),
}));

vi.mock('../../messageTasks/MessageTaskGraphPanel', () => ({
  MessageTaskGraphPanel: () => <div>task graph</div>,
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
  mocks.retryTask.mockResolvedValue(true);
  mocks.getProjectRequirementExecutionPlan.mockResolvedValue({
    found: true,
    conversation_id: 'conversation-1',
    execution_group_id: 'execution-group-1',
    message_id: 'message-1',
    status: 'failed',
    has_started_runs: true,
  });
  mocks.getProjectRuntimeEnvironment.mockResolvedValue({
    environment: { status: 'ready' },
    images: [
      {
        service_id: 'mdm-service',
        display_name: 'MDM Service',
        service_role: 'application',
        status: 'ready',
      },
      {
        service_id: 'web-prototype',
        display_name: 'Web Prototype',
        service_role: 'application',
        status: 'ready',
      },
      {
        service_id: 'postgresql',
        display_name: 'PostgreSQL',
        service_role: 'dependency',
        status: 'ready',
      },
    ],
  });
});

describe('failed task retry entry in execution workbench', () => {
  it('opens a bottom retry list and restarts only the selected failed task', async () => {
    const user = userEvent.setup();
    render(
      <RequirementExecutionProcessModal
        process={{
          requirement: { id: 'requirement-1', title: 'Requirement 1' },
          projectId: 'project-1',
          conversationId: 'conversation-1',
          executionGroupId: 'execution-group-1',
          messageId: 'message-1',
          contactId: 'contact-1',
          serverStatus: 'failed',
          hasStartedRuns: true,
        }}
        onClose={vi.fn()}
        onProcessChange={vi.fn()}
      />,
    );

    expect(screen.getByRole('button', { name: '取消本次执行' })).toBeTruthy();

    await user.click(screen.getByRole('button', { name: '重试失败任务，共 1 个' }));

    const retryDialog = screen.getByRole('dialog', { name: '失败任务重试' });
    expect(within(retryDialog).getByText(mocks.failedTask.title)).toBeTruthy();
    expect(within(retryDialog).queryByText('已经成功的任务')).toBeNull();

    const taskRow = within(retryDialog).getByRole('group', {
      name: `失败任务：${mocks.failedTask.title}`,
    });
    const retryButton = within(taskRow).getByRole('button', { name: '重新开始' });
    expect((retryButton as HTMLButtonElement).disabled).toBe(false);
    expect(within(taskRow).queryByRole('combobox')).toBeNull();
    await user.click(retryButton);

    expect(mocks.retryTask).toHaveBeenCalledTimes(1);
    expect(mocks.retryTask).toHaveBeenCalledWith(mocks.failedTask);
  });
});
