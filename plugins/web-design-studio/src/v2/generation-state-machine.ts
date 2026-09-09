import {
  assertGenerationPlan,
  type CreateGenerationStepInput,
  type GenerationArtifact,
  type GenerationAttempt,
  type GenerationAttemptError,
  type GenerationDesignIntent,
  type GenerationPageRun,
  type GenerationPlan,
  type GenerationStep
} from './generation-plan-schema.js';

export type GenerationPlanAction =
  | { type: 'revise-site'; objective: string; audience: string[]; pages: Array<{ pageId: string; name: string; purpose: string }> }
  | { type: 'plan-page'; pageId: string; design: GenerationDesignIntent; steps: CreateGenerationStepInput[] }
  | { type: 'mark-ready' }
  | { type: 'start-page'; pageId: string }
  | { type: 'start-step'; pageId: string; stepId: string; attemptId: string; idempotencyKey: string; baseRevision: number }
  | { type: 'begin-validation'; pageId: string; stepId: string; attemptId: string; artifacts?: GenerationArtifact[] }
  | { type: 'await-review'; pageId: string; stepId: string; attemptId: string; artifacts: GenerationArtifact[] }
  | { type: 'accept-step'; pageId: string; stepId: string; attemptId: string; committedRevision: number; artifacts?: GenerationArtifact[] }
  | { type: 'fail-step'; pageId: string; stepId: string; attemptId: string; error: GenerationAttemptError; artifacts?: GenerationArtifact[] }
  | { type: 'reject-step'; pageId: string; stepId: string; attemptId: string; reason: string }
  | { type: 'skip-step'; pageId: string; stepId: string }
  | { type: 'mark-step-stale'; pageId: string; stepId: string; reason: string }
  | { type: 'rollback-step'; pageId: string; stepId: string }
  | { type: 'complete-page'; pageId: string }
  | { type: 'pause-plan' }
  | { type: 'resume-plan' }
  | { type: 'cancel-plan'; reason: string };

export interface GenerationTransitionResult {
  plan: GenerationPlan;
  changed: boolean;
  replayed: boolean;
}

const activeStepStatuses = new Set(['generating', 'validating', 'awaiting-review']);
const dependencyCompleteStatuses = new Set(['accepted', 'skipped']);
const restartableStatuses = new Set(['ready', 'retryable', 'rejected', 'stale', 'rolled-back']);

function pageFor(plan: GenerationPlan, pageId: string): GenerationPageRun {
  const page = plan.pageRuns.find((candidate) => candidate.pageId === pageId);
  if (!page) throw new Error(`Generation page not found: ${pageId}`);
  return page;
}

function stepFor(page: GenerationPageRun, stepId: string): GenerationStep {
  const step = page.steps.find((candidate) => candidate.stepId === stepId);
  if (!step) throw new Error(`Generation step not found: ${stepId}`);
  return step;
}

function attemptFor(step: GenerationStep, attemptId: string): GenerationAttempt {
  const attempt = step.attempts.find((candidate) => candidate.attemptId === attemptId);
  if (!attempt) throw new Error(`Generation attempt not found: ${attemptId}`);
  return attempt;
}

function assertText(value: string, label: string): void {
  if (!value?.trim()) throw new Error(`${label} is required.`);
}

function appendArtifacts(attempt: GenerationAttempt, artifacts: GenerationArtifact[] | undefined): void {
  if (!artifacts?.length) return;
  const known = new Set(attempt.artifacts.map((artifact) => artifact.artifactId));
  for (const artifact of artifacts) {
    if (known.has(artifact.artifactId)) throw new Error(`Generation artifact already exists: ${artifact.artifactId}`);
    known.add(artifact.artifactId);
    attempt.artifacts.push(structuredClone(artifact));
  }
}

function dependenciesComplete(page: GenerationPageRun, step: GenerationStep): boolean {
  return step.dependsOn.every((dependencyId) => dependencyCompleteStatuses.has(stepFor(page, dependencyId).status));
}

function refreshReadySteps(page: GenerationPageRun, timestamp: string): void {
  for (const step of page.steps) {
    if (step.status === 'planned' && dependenciesComplete(page, step)) {
      step.status = 'ready';
      step.updatedAt = timestamp;
    }
  }
}

function activeAttempt(plan: GenerationPlan): { page: GenerationPageRun; step: GenerationStep; attempt: GenerationAttempt } | undefined {
  if (!plan.activePageId) return undefined;
  const page = pageFor(plan, plan.activePageId);
  if (!page.activeStepId) return undefined;
  const step = stepFor(page, page.activeStepId);
  if (!step.activeAttemptId) throw new Error('Active generation step is missing its attempt.');
  return { page, step, attempt: attemptFor(step, step.activeAttemptId) };
}

function findIdempotencyKey(plan: GenerationPlan, idempotencyKey: string): { page: GenerationPageRun; step: GenerationStep; attempt: GenerationAttempt } | undefined {
  for (const page of plan.pageRuns) {
    for (const step of page.steps) {
      const attempt = step.attempts.find((candidate) => candidate.idempotencyKey === idempotencyKey);
      if (attempt) return { page, step, attempt };
    }
  }
  return undefined;
}

function setStepInactive(page: GenerationPageRun, step: GenerationStep): void {
  step.activeAttemptId = undefined;
  page.activeStepId = undefined;
}

function markDependentStepsStale(page: GenerationPageRun, sourceStepId: string, timestamp: string): void {
  const stale = new Set([sourceStepId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const step of page.steps) {
      if (stale.has(step.stepId) || !step.dependsOn.some((dependency) => stale.has(dependency))) continue;
      stale.add(step.stepId);
      changed = true;
    }
  }
  stale.delete(sourceStepId);
  for (const step of page.steps.filter((candidate) => stale.has(candidate.stepId))) {
    if (activeStepStatuses.has(step.status)) throw new Error(`Cannot invalidate active dependent step ${step.stepId}.`);
    if (step.status === 'planned') continue;
    step.status = 'stale';
    step.acceptedAttemptId = undefined;
    step.activeAttemptId = undefined;
    step.updatedAt = timestamp;
  }
}

function assertVisualReviewArtifacts(attempt: GenerationAttempt): void {
  const kinds = new Set(attempt.artifacts.map((artifact) => artifact.kind));
  if (!kinds.has('page-snapshot') && !kinds.has('region-crop')) {
    throw new Error('A generation candidate needs a rendered page snapshot or region crop before review.');
  }
  if (!kinds.has('quality-report')) throw new Error('A generation candidate needs a quality report before review.');
}

function cloneForTransition(source: GenerationPlan): GenerationPlan {
  assertGenerationPlan(source);
  return structuredClone(source);
}

function plannedSteps(pageId: string, steps: CreateGenerationStepInput[], timestamp: string): GenerationStep[] {
  if (!Array.isArray(steps) || steps.length === 0) throw new Error('A page plan needs at least one step.');
  return steps.map((step) => ({
    stepId: step.stepId,
    pageId,
    title: step.title,
    kind: step.kind,
    required: step.required ?? true,
    dependsOn: [...(step.dependsOn ?? [])],
    target: {
      sectionKey: step.target?.sectionKey,
      nodeIds: [...(step.target?.nodeIds ?? [])],
      viewportWidths: [...(step.target?.viewportWidths ?? [])]
    },
    status: 'planned',
    attempts: [],
    createdAt: timestamp,
    updatedAt: timestamp
  }));
}

export function transitionGenerationPlan(
  source: GenerationPlan,
  action: GenerationPlanAction,
  timestamp = new Date().toISOString()
): GenerationTransitionResult {
  if (!Number.isFinite(Date.parse(timestamp))) throw new Error('Generation transition timestamp is invalid.');
  const plan = cloneForTransition(source);

  if (action.type === 'start-step') {
    const replay = findIdempotencyKey(plan, action.idempotencyKey);
    if (replay) {
      if (replay.page.pageId !== action.pageId || replay.step.stepId !== action.stepId
        || replay.attempt.attemptId !== action.attemptId || replay.attempt.baseRevision !== action.baseRevision) {
        throw new Error(`Generation idempotency key ${action.idempotencyKey} is already used by another attempt.`);
      }
      return { plan: structuredClone(source), changed: false, replayed: true };
    }
  }

  if (action.type === 'revise-site') {
    if (!['draft', 'ready'].includes(plan.status) || plan.activePageId) throw new Error('The site plan can be revised only while generation is idle.');
    assertText(action.objective, 'site objective');
    if (!Array.isArray(action.audience) || action.audience.length === 0 || action.audience.some((item) => !item?.trim())) {
      throw new Error('Site audience needs at least one non-empty value.');
    }
    if (!Array.isArray(action.pages) || action.pages.length === 0) throw new Error('Site plan needs at least one page.');
    const ids = action.pages.map((page) => page.pageId);
    if (new Set(ids).size !== ids.length) throw new Error('Site plan page IDs must be unique.');
    const retained = new Set(ids);
    const protectedRemoved = plan.pageRuns.filter((page) => !retained.has(page.pageId)
      && (page.status !== 'unplanned' || page.steps.some((step) => step.attempts.length > 0)));
    if (protectedRemoved.length > 0) throw new Error(`Cannot remove pages with planned or generated work: ${protectedRemoved.map((page) => page.pageId).join(', ')}`);
    const existing = new Map(plan.pageRuns.map((page) => [page.pageId, page]));
    plan.pageRuns = action.pages.map((entry, order) => {
      assertText(entry.pageId, 'pageId');
      assertText(entry.name, 'page name');
      assertText(entry.purpose, 'page purpose');
      const previous = existing.get(entry.pageId);
      if (previous) return { ...previous, name: entry.name, purpose: entry.purpose, order, updatedAt: timestamp };
      return {
        pageId: entry.pageId,
        name: entry.name,
        purpose: entry.purpose,
        order,
        status: 'unplanned',
        steps: [],
        createdAt: timestamp,
        updatedAt: timestamp
      };
    });
    plan.sitePlan = {
      objective: action.objective,
      audience: [...action.audience],
      pages: plan.pageRuns.map((page) => ({ pageId: page.pageId, name: page.name, purpose: page.purpose, order: page.order }))
    };
    if (plan.status === 'ready' && !plan.pageRuns.some((page) => page.status === 'planned')) plan.status = 'draft';
  } else if (action.type === 'plan-page') {
    if (!['draft', 'ready'].includes(plan.status) || plan.activePageId) throw new Error('A page can be planned only while the site plan is idle.');
    const page = pageFor(plan, action.pageId);
    if (!['unplanned', 'planned'].includes(page.status) || page.steps.some((step) => step.attempts.length > 0)) {
      throw new Error(`Generation page ${page.pageId} already contains execution history and cannot be replaced.`);
    }
    page.design = structuredClone(action.design);
    page.steps = plannedSteps(page.pageId, action.steps, timestamp);
    page.status = 'planned';
    page.updatedAt = timestamp;
  } else if (action.type === 'mark-ready') {
    if (plan.status !== 'draft') throw new Error('Only a draft generation plan can become ready.');
    if (!plan.pageRuns.some((page) => page.status === 'planned')) throw new Error('Plan at least one page before making the site plan ready.');
    plan.status = 'ready';
  } else if (action.type === 'start-page') {
    if (!['ready', 'failed'].includes(plan.status)) throw new Error('Generation plan is not ready to start a page.');
    if (plan.activePageId) throw new Error(`Generation page ${plan.activePageId} is already active.`);
    const page = pageFor(plan, action.pageId);
    if (!['planned', 'failed'].includes(page.status)) throw new Error(`Generation page ${page.pageId} cannot start from ${page.status}.`);
    page.status = 'running';
    page.updatedAt = timestamp;
    refreshReadySteps(page, timestamp);
    plan.activePageId = page.pageId;
    plan.status = 'running';
  } else if (action.type === 'start-step') {
    if (plan.status !== 'running' || plan.activePageId !== action.pageId) throw new Error('Generation step must belong to the active running page.');
    if (!Number.isSafeInteger(action.baseRevision) || action.baseRevision < 0) throw new Error('Generation attempt baseRevision is invalid.');
    assertText(action.attemptId, 'attemptId');
    assertText(action.idempotencyKey, 'idempotencyKey');
    const page = pageFor(plan, action.pageId);
    if (page.activeStepId) throw new Error(`Generation step ${page.activeStepId} is already active.`);
    const step = stepFor(page, action.stepId);
    if (!restartableStatuses.has(step.status)) throw new Error(`Generation step ${step.stepId} cannot start from ${step.status}.`);
    if (!dependenciesComplete(page, step)) throw new Error(`Generation step ${step.stepId} has incomplete dependencies.`);
    const attempt: GenerationAttempt = {
      attemptId: action.attemptId,
      idempotencyKey: action.idempotencyKey,
      status: 'generating',
      baseRevision: action.baseRevision,
      artifacts: [],
      createdAt: timestamp,
      updatedAt: timestamp
    };
    step.attempts.push(attempt);
    step.status = 'generating';
    step.activeAttemptId = attempt.attemptId;
    step.updatedAt = timestamp;
    page.activeStepId = step.stepId;
    page.updatedAt = timestamp;
  } else if (action.type === 'begin-validation') {
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    const attempt = attemptFor(step, action.attemptId);
    if (plan.status !== 'running' || page.status !== 'running' || step.status !== 'generating' || attempt.status !== 'generating') {
      throw new Error('Only a generating attempt can begin validation.');
    }
    appendArtifacts(attempt, action.artifacts);
    attempt.status = 'validating';
    attempt.updatedAt = timestamp;
    step.status = 'validating';
    step.updatedAt = timestamp;
  } else if (action.type === 'await-review') {
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    const attempt = attemptFor(step, action.attemptId);
    if (plan.status !== 'running' || step.status !== 'validating' || attempt.status !== 'validating') {
      throw new Error('Only a validating attempt can await review.');
    }
    appendArtifacts(attempt, action.artifacts);
    assertVisualReviewArtifacts(attempt);
    attempt.status = 'awaiting-review';
    attempt.updatedAt = timestamp;
    step.status = 'awaiting-review';
    step.updatedAt = timestamp;
  } else if (action.type === 'accept-step') {
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    const attempt = attemptFor(step, action.attemptId);
    if (plan.status !== 'running' || step.status !== 'awaiting-review' || attempt.status !== 'awaiting-review') {
      throw new Error('Only a reviewed generation attempt can be accepted.');
    }
    if (!Number.isSafeInteger(action.committedRevision) || action.committedRevision <= attempt.baseRevision) throw new Error('Committed revision is invalid.');
    appendArtifacts(attempt, action.artifacts);
    assertVisualReviewArtifacts(attempt);
    attempt.status = 'committed';
    attempt.committedRevision = action.committedRevision;
    attempt.updatedAt = timestamp;
    step.status = 'accepted';
    step.activeAttemptId = undefined;
    step.acceptedAttemptId = attempt.attemptId;
    step.updatedAt = timestamp;
    page.activeStepId = undefined;
    page.updatedAt = timestamp;
    refreshReadySteps(page, timestamp);
  } else if (action.type === 'fail-step') {
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    const attempt = attemptFor(step, action.attemptId);
    if (!activeStepStatuses.has(step.status) || !['generating', 'validating', 'awaiting-review'].includes(attempt.status)) {
      throw new Error('Only an active generation attempt can fail.');
    }
    appendArtifacts(attempt, action.artifacts);
    attempt.status = 'failed';
    attempt.error = structuredClone(action.error);
    attempt.updatedAt = timestamp;
    step.status = action.error.retryable ? 'retryable' : 'blocked';
    step.updatedAt = timestamp;
    setStepInactive(page, step);
    page.updatedAt = timestamp;
    if (!action.error.retryable) {
      page.status = 'failed';
      plan.status = 'failed';
      plan.activePageId = undefined;
    }
  } else if (action.type === 'reject-step') {
    assertText(action.reason, 'rejection reason');
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    const attempt = attemptFor(step, action.attemptId);
    if (step.status !== 'awaiting-review' || attempt.status !== 'awaiting-review') throw new Error('Only a reviewed candidate can be rejected.');
    attempt.status = 'rejected';
    attempt.error = { code: 'quality_reject', message: action.reason, retryable: true, issueIds: [] };
    attempt.updatedAt = timestamp;
    step.status = 'rejected';
    step.updatedAt = timestamp;
    setStepInactive(page, step);
    page.updatedAt = timestamp;
  } else if (action.type === 'skip-step') {
    const page = pageFor(plan, action.pageId);
    if (plan.activePageId !== page.pageId || page.activeStepId) throw new Error('A step can be skipped only on the idle active page.');
    const step = stepFor(page, action.stepId);
    if (step.required) throw new Error(`Required generation step ${step.stepId} cannot be skipped.`);
    if (!['ready', 'planned', 'retryable', 'rejected', 'stale'].includes(step.status)) throw new Error(`Generation step ${step.stepId} cannot be skipped from ${step.status}.`);
    if (!dependenciesComplete(page, step)) throw new Error(`Generation step ${step.stepId} has incomplete dependencies.`);
    step.status = 'skipped';
    step.updatedAt = timestamp;
    refreshReadySteps(page, timestamp);
  } else if (action.type === 'mark-step-stale') {
    assertText(action.reason, 'stale reason');
    const page = pageFor(plan, action.pageId);
    const step = stepFor(page, action.stepId);
    if (activeStepStatuses.has(step.status)) {
      const attempt = attemptFor(step, step.activeAttemptId!);
      attempt.status = 'discarded';
      attempt.error = { code: 'revision_conflict', message: action.reason, retryable: true, issueIds: [] };
      attempt.updatedAt = timestamp;
      setStepInactive(page, step);
    }
    step.status = 'stale';
    step.acceptedAttemptId = undefined;
    step.updatedAt = timestamp;
    markDependentStepsStale(page, step.stepId, timestamp);
    page.updatedAt = timestamp;
  } else if (action.type === 'rollback-step') {
    const page = pageFor(plan, action.pageId);
    if (page.activeStepId) throw new Error('Cannot roll back while another generation step is active.');
    const step = stepFor(page, action.stepId);
    if (step.status !== 'accepted' || !step.acceptedAttemptId) throw new Error('Only an accepted generation step can be rolled back.');
    step.status = 'rolled-back';
    step.acceptedAttemptId = undefined;
    step.updatedAt = timestamp;
    markDependentStepsStale(page, step.stepId, timestamp);
    page.updatedAt = timestamp;
  } else if (action.type === 'complete-page') {
    if (plan.status !== 'running' || plan.activePageId !== action.pageId) throw new Error('Only the active running page can be completed.');
    const page = pageFor(plan, action.pageId);
    if (page.activeStepId) throw new Error('Cannot complete a page while a step is active.');
    const handoff = page.steps.find((step) => step.kind === 'handoff');
    if (!handoff || handoff.status !== 'accepted') throw new Error('Page handoff must be accepted before completion.');
    const incomplete = page.steps.filter((step) => step.required ? step.status !== 'accepted' : !['accepted', 'skipped'].includes(step.status));
    if (incomplete.length > 0) throw new Error(`Page has incomplete steps: ${incomplete.map((step) => step.stepId).join(', ')}`);
    page.status = 'completed';
    page.updatedAt = timestamp;
    plan.activePageId = undefined;
    plan.status = plan.pageRuns.every((candidate) => candidate.status === 'completed') ? 'completed' : 'ready';
  } else if (action.type === 'pause-plan') {
    if (plan.status !== 'running' || !plan.activePageId) throw new Error('Only a running generation plan can be paused.');
    if (activeAttempt(plan)) throw new Error('Finish, fail, or discard the active step before pausing.');
    const page = pageFor(plan, plan.activePageId);
    page.status = 'paused';
    page.updatedAt = timestamp;
    plan.status = 'paused';
  } else if (action.type === 'resume-plan') {
    if (plan.status !== 'paused' || !plan.activePageId) throw new Error('Only a paused generation plan can resume.');
    const page = pageFor(plan, plan.activePageId);
    page.status = 'running';
    page.updatedAt = timestamp;
    plan.status = 'running';
  } else {
    assertText(action.reason, 'cancellation reason');
    const active = activeAttempt(plan);
    if (active) {
      active.attempt.status = 'discarded';
      active.attempt.error = { code: 'cancelled', message: action.reason, retryable: false, issueIds: [] };
      active.attempt.updatedAt = timestamp;
      active.step.status = 'blocked';
      active.step.activeAttemptId = undefined;
      active.step.updatedAt = timestamp;
      active.page.activeStepId = undefined;
    }
    if (plan.activePageId) {
      const page = pageFor(plan, plan.activePageId);
      page.status = 'cancelled';
      page.updatedAt = timestamp;
    }
    plan.activePageId = undefined;
    plan.status = 'cancelled';
  }

  plan.updatedAt = timestamp;
  assertGenerationPlan(plan);
  return { plan, changed: true, replayed: false };
}
