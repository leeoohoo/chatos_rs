// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Project } from '../../types';
import { ProjectPlanPane } from './ProjectPlanPane';

const mocks = vi.hoisted(() => ({
  apiClient: {
    getProjectPlan: vi.fn(),
    listProjectRequirementWorkItems: vi.fn(),
    listProjectRequirementDocuments: vi.fn(),
    getProjectRequirementExecutionPlan: vi.fn(),
  },
  storeState: {
    refreshSessionById: vi.fn(),
    selectedModelId: null,
  },
}));

vi.mock('../../lib/api/ApiClientContext', () => ({
  useApiClient: () => mocks.apiClient,
}));

vi.mock('../../lib/store', () => ({
  useChatStore: (selector: (state: Record<string, unknown>) => unknown) => selector(
    mocks.storeState,
  ),
}));

vi.mock('./projectPlanPane/RequirementExecutionProcessModal', async () => {
  const actual = await vi.importActual<
    typeof import('./projectPlanPane/RequirementExecutionProcessModal')
  >('./projectPlanPane/RequirementExecutionProcessModal');
  return {
    ...actual,
    RequirementExecutionStartingModal: ({ requirement, onClose }: {
      requirement: { title: string };
      onClose: () => void;
    }) => (
      <div role="dialog" aria-label="执行计划工作台">
        <span>{requirement.title}</span>
        <button type="button" onClick={onClose}>关闭</button>
      </div>
    ),
    RequirementExecutionProcessModal: ({ process }: {
      process: { requirement: { title: string } };
    }) => (
      <div role="dialog" aria-label="执行计划工作台">
        <span>{process.requirement.title}</span>
      </div>
    ),
  };
});

afterEach(() => cleanup());
beforeEach(() => vi.clearAllMocks());

describe('ProjectPlanPane execution workbench context', () => {
  it('does not reopen a stale starter when another requirement has an existing plan', async () => {
    mocks.apiClient.getProjectPlan.mockResolvedValue({
      requirements: [
        { id: 'requirement-starter', title: 'Starter requirement', status: 'approved' },
        { id: 'requirement-existing', title: 'Existing requirement', status: 'reviewing' },
      ],
      workItems: [],
      workItemCounts: { total: 0, open: 0, done: 0, blocked: 0 },
      dependencyGraph: { nodes: [], edges: [] },
    });
    mocks.apiClient.listProjectRequirementWorkItems.mockResolvedValue([]);
    mocks.apiClient.listProjectRequirementDocuments.mockResolvedValue([]);
    mocks.apiClient.getProjectRequirementExecutionPlan.mockImplementation(
      async (_projectId: string, requirementId: string) => (
        requirementId === 'requirement-existing'
          ? {
            found: true,
            conversation_id: 'conversation-existing',
            execution_group_id: 'group-existing',
            message_id: 'message-existing',
            status: 'awaiting_confirmation',
            has_started_runs: false,
          }
          : { found: false }
      ),
    );
    const user = userEvent.setup();
    const now = new Date();
    const project: Project = {
      id: 'project-1',
      name: 'Project 1',
      rootPath: '/workspace',
      createdAt: now,
      updatedAt: now,
    };

    render(<ProjectPlanPane project={project} />);

    await user.click(await screen.findByRole('button', { name: /Starter requirement/ }));
    await user.click(await screen.findByRole('button', { name: '打开执行工作台' }));
    expect(
      screen.getByRole('dialog', { name: '执行计划工作台' }).textContent,
    ).toContain('Starter requirement');
    await user.click(screen.getByRole('button', { name: '关闭' }));

    await user.click(screen.getByRole('button', { name: /Existing requirement/ }));
    await waitFor(() => {
      expect(
        (screen.getByRole('button', { name: '查看执行计划' }) as HTMLButtonElement)
          .disabled,
      ).toBe(false);
    });
    await user.click(screen.getByRole('button', { name: '查看执行计划' }));

    const dialogText = screen.getByRole('dialog', { name: '执行计划工作台' }).textContent;
    expect(dialogText).toContain('Existing requirement');
    expect(dialogText).not.toContain('Starter requirement');
  });
});
