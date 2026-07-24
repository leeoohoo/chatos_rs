// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RequirementExecutionProcessModal } from './RequirementExecutionProcessModal';

const mocks = vi.hoisted(() => ({
  getPlan: vi.fn(),
  pause: vi.fn(),
  resume: vi.fn(),
  stop: vi.fn(),
  reloadGraph: vi.fn(),
}));

vi.mock('../../../lib/api/ApiClientContext', () => ({
  useApiClient: () => ({
    getProjectRequirementExecutionPlan: mocks.getPlan,
    pauseProjectRequirementExecution: mocks.pause,
    resumeProjectRequirementExecution: mocks.resume,
    stopProjectRequirementExecution: mocks.stop,
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
    hasStartedTasks: true,
    canConfirm: false,
    projectId: 'project-1',
    requirementId: 'requirement-1',
    executionGroupId: 'execution-group-1',
    conversationId: 'conversation-1',
    contactId: 'contact-1',
    overallStatus: 'processing',
  }),
}));

vi.mock('../../messageTasks/useMessageTaskGraph', () => ({
  useMessageTaskGraph: () => ({
    graph: { edges: [], nodes: [] },
    allTasks: [
      { id: 'task-running', title: 'Running', status: 'running', last_run_id: 'run-1' },
      { id: 'task-queued', title: 'Queued', status: 'queued', last_run_id: 'run-2' },
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
    retryTask: vi.fn(),
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
vi.mock('../../messageTasks/MessageTaskChangesModal', () => ({ MessageTaskChangesModal: () => null }));
vi.mock('../../messageTasks/MessageTaskDetailModal', () => ({
  MessageTaskDetailModal: () => null,
  MessageTaskProcessLogModal: () => null,
}));
vi.mock('../../messageTasks/MessageTaskRunDetailModal', () => ({ MessageTaskRunDetailModal: () => null }));

afterEach(() => cleanup());

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getPlan.mockResolvedValue({
    found: true,
    conversation_id: 'conversation-1',
    execution_group_id: 'execution-group-1',
    message_id: 'message-1',
    status: 'paused',
    execution_paused: true,
    has_started_runs: true,
  });
  mocks.pause.mockResolvedValue({ status: 'paused', execution_paused: true });
  mocks.resume.mockResolvedValue({ status: 'execution_started', execution_paused: false });
  mocks.stop.mockResolvedValue({ status: 'stopped' });
  mocks.reloadGraph.mockResolvedValue(undefined);
});

const process = {
  requirement: { id: 'requirement-1', title: 'Requirement 1' },
  projectId: 'project-1',
  conversationId: 'conversation-1',
  executionGroupId: 'execution-group-1',
  messageId: 'message-1',
  contactId: 'contact-1',
  serverStatus: 'execution_started',
  hasStartedRuns: true,
};

describe('execution pause and cancel controls', () => {
  it('pauses without cancelling and requires confirmation before cancelling the whole batch', async () => {
    const user = userEvent.setup();
    mocks.getPlan
      .mockReset()
      .mockResolvedValueOnce({
        found: true,
        conversation_id: 'conversation-1',
        execution_group_id: 'execution-group-1',
        message_id: 'message-1',
        status: 'execution_started',
        execution_paused: false,
        has_started_runs: true,
      })
      .mockResolvedValue({
        found: true,
        conversation_id: 'conversation-1',
        execution_group_id: 'execution-group-1',
        message_id: 'message-1',
        status: 'paused',
        execution_paused: true,
        has_started_runs: true,
      });
    render(
      <RequirementExecutionProcessModal
        process={process}
        onClose={vi.fn()}
        onProcessChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '暂停后续任务' }));
    expect(mocks.pause).toHaveBeenCalledWith('project-1', 'requirement-1', {
      execution_group_id: 'execution-group-1',
      conversation_id: 'conversation-1',
      contact_id: 'contact-1',
    });
    await waitFor(() => expect(screen.getByRole('button', { name: '继续调度' })).toBeTruthy());
    expect(screen.getByText('已暂停后续任务调度')).toBeTruthy();
    expect(screen.getByText(/当前 1 个已运行任务会继续完成/)).toBeTruthy();
    expect(mocks.stop).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: '取消本次执行' }));
    const dialog = screen.getByRole('alertdialog', { name: '确认取消本次执行' });
    expect(mocks.stop).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: '确认取消本次执行' }));
    expect(mocks.stop).toHaveBeenCalledTimes(1);
  });

  it('resumes a persisted paused batch through the resume endpoint', async () => {
    const user = userEvent.setup();
    render(
      <RequirementExecutionProcessModal
        process={{ ...process, serverStatus: 'paused', executionPaused: true }}
        onClose={vi.fn()}
        onProcessChange={vi.fn()}
      />,
    );

    await user.click(screen.getByRole('button', { name: '继续调度' }));
    expect(mocks.resume).toHaveBeenCalledTimes(1);
    expect(mocks.pause).not.toHaveBeenCalled();
  });
});
