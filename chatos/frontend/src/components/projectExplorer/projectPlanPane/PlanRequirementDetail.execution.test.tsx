// @vitest-environment jsdom
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { PlanRequirementDetail } from './PlanRequirementDetail';
import type { RequirementExecutionProcess } from './RequirementExecutionProcessModal';

afterEach(() => cleanup());

const requirement = {
  id: 'requirement-1',
  title: 'Requirement 1',
  status: 'reviewing' as const,
};

const renderDetail = (process: RequirementExecutionProcess | null) => {
  const onGenerateRequirementExecution = vi.fn();
  const onOpenRequirementExecution = vi.fn();
  render(
    <PlanRequirementDetail
      activeDetailTab="requirement"
      actionDisabled={false}
      dependencyMaps={{
        requirementDependents: new Map(),
        requirementPrerequisites: new Map(),
        workItemDependents: new Map(),
        workItemPrerequisites: new Map(),
      }}
      onActiveDetailTabChange={vi.fn()}
      onGenerateRequirementExecution={onGenerateRequirementExecution}
      onLoadMoreWorkItems={vi.fn()}
      onOpenRequirementExecution={onOpenRequirementExecution}
      onPreviewRequirement={vi.fn()}
      resolveRequirementTitle={(id) => id}
      resolveWorkItemTitle={(id) => id}
      selectedDocumentsLoading={false}
      selectedExecutionScopeRelatedIds={[]}
      selectedRequirement={requirement}
      selectedRequirementActionBusy={false}
      selectedRequirementCanShowAction
      selectedRequirementChildren={[]}
      selectedRequirementDependents={[]}
      selectedRequirementDocuments={[]}
      selectedRequirementExecutionProcess={process}
      selectedRequirementPrerequisites={[]}
      selectedWorkItems={[]}
      selectedWorkItemsLoading={false}
      visibleSelectedWorkItems={{
        hasMore: false,
        hiddenCount: 0,
        items: [],
        totalCount: 0,
      }}
    />,
  );
  return { onGenerateRequirementExecution, onOpenRequirementExecution };
};

describe('Plan requirement execution entry', () => {
  it('only opens an existing execution plan without starting a new planning batch', async () => {
    const user = userEvent.setup();
    const process: RequirementExecutionProcess = {
      requirement,
      projectId: 'project-1',
      conversationId: 'conversation-1',
      executionGroupId: 'execution-group-1',
      messageId: 'message-1',
      hasStartedRuns: false,
    };
    const actions = renderDetail(process);

    await user.click(screen.getByRole('button', { name: '查看执行计划' }));

    expect(actions.onOpenRequirementExecution).toHaveBeenCalledTimes(1);
    expect(actions.onGenerateRequirementExecution).not.toHaveBeenCalled();
  });

  it('opens the workbench before the user explicitly starts the planning agent', async () => {
    const user = userEvent.setup();
    const actions = renderDetail(null);

    await user.click(screen.getByRole('button', { name: '打开执行工作台' }));

    expect(actions.onGenerateRequirementExecution).toHaveBeenCalledWith(requirement);
    expect(actions.onOpenRequirementExecution).not.toHaveBeenCalled();
  });
});
