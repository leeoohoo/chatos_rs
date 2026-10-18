import type {
  GenerationArtifact,
  GenerationPageRun,
  GenerationPlan,
  GenerationStep
} from './generation-plan-schema.js';

const executableStepStatuses = new Set(['ready', 'retryable', 'rejected', 'stale', 'rolled-back']);

export function isExecutableStepStatus(status: GenerationStep['status']): boolean {
  return executableStepStatuses.has(status);
}

export function nextExecutableStep(page: GenerationPageRun): GenerationStep | undefined {
  return page.steps.find((step) => isExecutableStepStatus(step.status));
}

export function nextAction(plan: GenerationPlan): Record<string, unknown> {
  if (plan.status === 'paused') return { type: 'resume-plan', tool: 'web_design_control_plan', action: 'resume' };
  if (plan.status === 'draft') {
    const page = plan.pageRuns.find((candidate) => candidate.status === 'unplanned');
    return page
      ? { type: 'plan-page', tool: 'web_design_plan_page', pageId: page.pageId }
      : { type: 'mark-ready', detail: 'The next page plan mutation will make this plan ready.' };
  }
  if (plan.status === 'ready') {
    const unplanned = plan.pageRuns.find((candidate) => candidate.status === 'unplanned');
    if (unplanned) return { type: 'plan-page', tool: 'web_design_plan_page', pageId: unplanned.pageId };
    const page = plan.pageRuns.find((candidate) => candidate.status === 'planned');
    return page
      ? { type: 'start-page', tool: 'web_design_control_plan', action: 'start-page', pageId: page.pageId }
      : { type: 'inspect-plan', tool: 'web_design_get_active_context' };
  }
  if (plan.status === 'running' && plan.activePageId) {
    const page = plan.pageRuns.find((candidate) => candidate.pageId === plan.activePageId)!;
    const active = page.activeStepId ? page.steps.find((step) => step.stepId === page.activeStepId) : undefined;
    if (active?.status === 'awaiting-review') {
      return { type: 'review-step', tool: 'web_design_control_plan', pageId: page.pageId, stepId: active.stepId, attemptId: active.activeAttemptId };
    }
    if (active) return { type: 'wait-for-step', pageId: page.pageId, stepId: active.stepId, status: active.status };
    const step = nextExecutableStep(page);
    if (step) {
      return {
        type: step.status === 'ready' ? 'run-step' : 'retry-step',
        tool: 'web_design_execute_step',
        pageId: page.pageId,
        stepId: step.stepId,
        target: step.target
      };
    }
    const handoff = page.steps.find((step) => step.kind === 'handoff');
    if (handoff?.status === 'accepted') return { type: 'complete-page-automatically', pageId: page.pageId };
  }
  return { type: 'inspect-plan', tool: 'web_design_get_active_context' };
}

const visibleDesignStepKinds = new Set<GenerationStep['kind']>([
  'structure', 'section', 'visual', 'responsive', 'polish'
]);

export function generationDeliveryGate(plan: GenerationPlan): Record<string, unknown> {
  const requiredNextAction = nextAction(plan);
  const acceptedVisibleStepCount = plan.pageRuns.reduce((count, page) => count + page.steps.filter((step) => (
    step.status === 'accepted' && visibleDesignStepKinds.has(step.kind)
  )).length, 0);
  const completedArtboardCount = plan.pageRuns.filter((page) => page.status === 'completed').length;
  const visibleSceneReady = acceptedVisibleStepCount > 0;
  const projectImplementationAllowed = completedArtboardCount > 0;
  const taskCompletionAllowed = plan.status === 'completed';

  let code = 'READY';
  let message = 'Every planned artboard has passed its visual handoff. The requested design scope may now be reported complete.';
  if (!visibleSceneReady) {
    code = 'NO_ACCEPTED_VISUAL_STEP';
    message = 'The design is still visually empty. Continue the returned nextAction until at least one visible Scene Candidate is reviewed and accepted. Do not edit product UI code or report a task outcome yet.';
  } else if (!projectImplementationAllowed) {
    code = 'NO_COMPLETED_ARTBOARD';
    message = 'Visible Scene work exists, but no artboard has passed Design Gate and handoff. Continue the returned nextAction; product UI implementation is not allowed yet.';
  } else if (!taskCompletionAllowed) {
    code = 'INCOMPLETE_DESIGN_SCOPE';
    message = 'At least one artboard is ready for its matching implementation, but the planned design scope is incomplete. Implement only completed artboards and continue the returned nextAction before reporting the whole task complete.';
  }

  return {
    status: taskCompletionAllowed ? 'ready' : 'blocked',
    code,
    message,
    visibleSceneReady,
    projectImplementationAllowed,
    taskCompletionAllowed,
    acceptedVisibleStepCount,
    completedArtboardCount,
    plannedArtboardCount: plan.pageRuns.length,
    requiredNextAction
  };
}

export function summarizeGenerationPlan(plan: GenerationPlan): Record<string, unknown> {
  const activePage = plan.activePageId ? plan.pageRuns.find((page) => page.pageId === plan.activePageId) : undefined;
  const activeStep = activePage?.activeStepId ? activePage.steps.find((step) => step.stepId === activePage.activeStepId) : undefined;
  return {
    planId: plan.planId,
    revision: plan.revision,
    scope: { projectId: plan.scope.projectId, documentId: plan.scope.documentId },
    mode: plan.mode,
    status: plan.status,
    objective: plan.sitePlan.objective,
    audience: plan.sitePlan.audience,
    pages: plan.pageRuns.map((page) => ({
      pageId: page.pageId,
      name: page.name,
      purpose: page.purpose,
      order: page.order,
      status: page.status,
      stepCounts: page.steps.reduce<Record<string, number>>((counts, step) => {
        counts[step.status] = (counts[step.status] ?? 0) + 1;
        return counts;
      }, {}),
      ...(page.design ? { design: page.design } : {})
    })),
    ...(activePage ? { activePage: { pageId: activePage.pageId, name: activePage.name, status: activePage.status } } : {}),
    ...(activeStep ? {
      activeStep: {
        stepId: activeStep.stepId,
        title: activeStep.title,
        kind: activeStep.kind,
        status: activeStep.status,
        target: activeStep.target,
        activeAttemptId: activeStep.activeAttemptId
      }
    } : {}),
    nextAction: nextAction(plan),
    deliveryGate: generationDeliveryGate(plan)
  };
}

export function compactArtifactReference(artifact: GenerationArtifact): Record<string, unknown> {
  return {
    id: artifact.artifactId,
    kind: artifact.kind,
    revision: artifact.revision,
    ...(artifact.viewportWidth === undefined ? {} : { viewportWidth: artifact.viewportWidth })
  };
}

export function compactSceneChange(summary: {
  revision: number;
  insertedPageIds: string[];
  removedPageIds: string[];
  insertedNodeIds: string[];
  updatedNodeIds: string[];
  removedNodeIds: string[];
  movedNodeIds: string[];
  insertedVariableCollectionIds: string[];
  updatedVariableCollectionIds: string[];
  removedVariableCollectionIds: string[];
  insertedResponsiveRuleIds: string[];
  removedResponsiveRuleIds: string[];
  updatedResponsiveRuleIds: string[];
  renamedPageIds: string[];
}): Record<string, unknown> {
  const affectedNodeIds = [...new Set([
    ...summary.insertedNodeIds,
    ...summary.updatedNodeIds,
    ...summary.removedNodeIds,
    ...summary.movedNodeIds
  ])];
  return {
    revision: summary.revision,
    counts: {
      pages: summary.insertedPageIds.length + summary.removedPageIds.length + summary.renamedPageIds.length,
      insertedNodes: summary.insertedNodeIds.length,
      updatedNodes: summary.updatedNodeIds.length,
      removedNodes: summary.removedNodeIds.length,
      movedNodes: summary.movedNodeIds.length,
      variables: summary.insertedVariableCollectionIds.length + summary.updatedVariableCollectionIds.length + summary.removedVariableCollectionIds.length,
      responsiveRules: summary.insertedResponsiveRuleIds.length + summary.updatedResponsiveRuleIds.length + summary.removedResponsiveRuleIds.length
    },
    affectedNodeIds: affectedNodeIds.slice(0, 24),
    ...(affectedNodeIds.length > 24 ? { affectedNodeIdsTruncated: true } : {})
  };
}

export function summarizeGenerationPlanState(plan: GenerationPlan): Record<string, unknown> {
  const activePage = plan.activePageId ? plan.pageRuns.find((page) => page.pageId === plan.activePageId) : undefined;
  const activeStep = activePage?.activeStepId ? activePage.steps.find((step) => step.stepId === activePage.activeStepId) : undefined;
  return {
    planId: plan.planId,
    revision: plan.revision,
    documentId: plan.scope.documentId,
    mode: plan.mode,
    status: plan.status,
    pages: plan.pageRuns.map((page) => ({
      pageId: page.pageId,
      name: page.name,
      status: page.status,
      stepCounts: page.steps.reduce<Record<string, number>>((counts, step) => {
        counts[step.status] = (counts[step.status] ?? 0) + 1;
        return counts;
      }, {})
    })),
    ...(activePage ? { activePage: { pageId: activePage.pageId, name: activePage.name, status: activePage.status } } : {}),
    ...(activeStep ? {
      activeStep: {
        stepId: activeStep.stepId,
        title: activeStep.title,
        kind: activeStep.kind,
        status: activeStep.status,
        target: activeStep.target,
        activeAttemptId: activeStep.activeAttemptId
      }
    } : {}),
    nextAction: nextAction(plan),
    deliveryGate: generationDeliveryGate(plan)
  };
}
