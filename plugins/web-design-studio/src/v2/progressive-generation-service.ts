import { randomUUID } from 'node:crypto';
import {
  createGenerationSitePlan,
  type CreateGenerationStepInput,
  type GenerationArtifact,
  type GenerationDesignIntent,
  type GenerationExecutionMode,
  type GenerationPageRun,
  type GenerationPlan,
  type GenerationScope,
  type GenerationStep
} from './generation-plan-schema.js';
import { GenerationCandidateStore, type GenerationCandidateRecord } from './generation-candidate-store.js';
import { GenerationPlanStore } from './generation-plan-store.js';
import { GenerationSoftProtectionStore } from './generation-soft-protection-store.js';
import { createBlankSceneDocument, createSceneNodeBase, type SceneDocument, type SceneFrameNode } from './scene-schema.js';
import { SceneDocumentStore } from './scene-store.js';
import { commitGenerationStep, prepareGenerationStep } from './generation-step-executor.js';
import type { SceneTransactionOperation } from './scene-transaction.js';

export interface ProgressiveGenerationRepositories {
  plans: GenerationPlanStore;
  scenes: SceneDocumentStore;
  candidates: GenerationCandidateStore;
  protections: GenerationSoftProtectionStore;
}

export interface ProgressiveGenerationServiceOptions {
  projectId: string;
  repositories: ProgressiveGenerationRepositories;
  assertDocumentInScope(documentId: string): Promise<{ name: string }>;
}

export interface SitePlanInput {
  documentId: string;
  expectedPlanRevision?: number;
  planId?: string;
  mode?: GenerationExecutionMode;
  objective: string;
  audience: string[];
  pages: Array<{ pageId: string; name: string; purpose: string }>;
}

export interface PagePlanInput {
  documentId: string;
  expectedPlanRevision: number;
  pageId: string;
  design: GenerationDesignIntent;
  steps: CreateGenerationStepInput[];
}

export interface SubmittedStepVerification {
  passed: boolean;
  qualitySummary: string;
  issueIds: string[];
  artifacts: GenerationArtifact[];
  error?: {
    code: 'generation_error' | 'scope_violation' | 'layout_error' | 'render_error' | 'quality_reject' | 'revision_conflict' | 'cancelled';
    message: string;
    retryable: boolean;
    issueIds: string[];
  };
}

export interface ExecuteProgressiveStepInput {
  documentId: string;
  expectedPlanRevision: number;
  stepId?: string;
  attemptId?: string;
  idempotencyKey: string;
  transactionId: string;
  operations: SceneTransactionOperation[];
  visualInputs: GenerationArtifact[];
  verification: SubmittedStepVerification;
}

const executableStepStatuses = new Set(['ready', 'retryable', 'rejected', 'stale', 'rolled-back']);

function isMissing(error: unknown): boolean {
  return (error as NodeJS.ErrnoException)?.code === 'ENOENT';
}

function rootNodeId(pageId: string): string {
  return `root:${pageId}`;
}

function requireIdentifier(value: string, label: string): string {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value)) throw new Error(`${label} is invalid.`);
  return value;
}

function scopeFor(projectId: string, documentId: string): GenerationScope {
  return { projectId: requireIdentifier(projectId, 'runtime projectId'), documentId: requireIdentifier(documentId, 'documentId') };
}

function normalizePageSteps(pageId: string, steps: CreateGenerationStepInput[]): CreateGenerationStepInput[] {
  const rootId = rootNodeId(pageId);
  return steps.map((step) => ({
    ...structuredClone(step),
    target: {
      ...structuredClone(step.target ?? {}),
      nodeIds: [...(step.target?.nodeIds?.length ? step.target.nodeIds : [rootId])],
      viewportWidths: [...(step.target?.viewportWidths ?? [])]
    }
  }));
}

function artifactKinds(artifacts: GenerationArtifact[]): Set<GenerationArtifact['kind']> {
  return new Set(artifacts.map((artifact) => artifact.kind));
}

function assertUniqueArtifacts(artifacts: GenerationArtifact[], label: string): void {
  if (!Array.isArray(artifacts)) throw new Error(`${label} must be an array.`);
  const ids = artifacts.map((artifact) => artifact.artifactId);
  if (new Set(ids).size !== ids.length) throw new Error(`${label} artifact IDs must be unique.`);
}

function assertVisualInputs(artifacts: GenerationArtifact[], revision: number): void {
  assertUniqueArtifacts(artifacts, 'visualInputs');
  if (artifacts.some((artifact) => artifact.revision !== revision)) {
    throw new Error(`Every visual input must reference the current Scene revision ${revision}.`);
  }
  const kinds = artifactKinds(artifacts);
  if (!kinds.has('page-snapshot') && !kinds.has('region-crop')) {
    throw new Error('The current step needs a page snapshot or region crop as visual input.');
  }
  if (!kinds.has('visual-grounding')) throw new Error('The current step needs visual grounding for stable Scene node IDs.');
}

function assertPassingVerification(verification: SubmittedStepVerification, candidateRevision: number): void {
  if (!verification || typeof verification !== 'object') throw new Error('Step verification is required.');
  if (!verification.qualitySummary?.trim()) throw new Error('Step verification needs a quality summary.');
  if (!Array.isArray(verification.issueIds)) throw new Error('Step verification issueIds must be an array.');
  assertUniqueArtifacts(verification.artifacts, 'verification');
  if (!verification.passed) return;
  if (verification.artifacts.some((artifact) => artifact.revision !== candidateRevision)) {
    throw new Error(`Every passing verification artifact must reference Candidate revision ${candidateRevision}.`);
  }
  const kinds = artifactKinds(verification.artifacts);
  const required: GenerationArtifact['kind'][] = ['layout', 'visual-grounding', 'visual-diff', 'calibration', 'quality-report'];
  for (const kind of required) if (!kinds.has(kind)) throw new Error(`Passing verification is missing ${kind}.`);
  if (!kinds.has('page-snapshot') && !kinds.has('region-crop')) {
    throw new Error('Passing verification needs a rendered page snapshot or region crop.');
  }
}

function nextExecutableStep(page: GenerationPageRun): GenerationStep | undefined {
  return page.steps.find((step) => executableStepStatuses.has(step.status));
}

function nextAction(plan: GenerationPlan): Record<string, unknown> {
  if (plan.status === 'paused') return { type: 'resume-plan', tool: 'web_design_resume_plan' };
  if (plan.status === 'draft') {
    const page = plan.pageRuns.find((candidate) => candidate.status === 'unplanned');
    return page
      ? { type: 'plan-page', tool: 'web_design_plan_page', pageId: page.pageId }
      : { type: 'mark-ready', detail: 'The next page plan mutation will make this plan ready.' };
  }
  if (plan.status === 'ready') {
    const page = plan.pageRuns.find((candidate) => candidate.status === 'planned');
    return page
      ? { type: 'start-page', tool: 'web_design_start_page', pageId: page.pageId }
      : { type: 'inspect-plan', tool: 'web_design_get_plan' };
  }
  if (plan.status === 'running' && plan.activePageId) {
    const page = plan.pageRuns.find((candidate) => candidate.pageId === plan.activePageId)!;
    const active = page.activeStepId ? page.steps.find((step) => step.stepId === page.activeStepId) : undefined;
    if (active?.status === 'awaiting-review') {
      return { type: 'review-step', tool: 'web_design_inspect_step', pageId: page.pageId, stepId: active.stepId, attemptId: active.activeAttemptId };
    }
    if (active) return { type: 'wait-for-step', pageId: page.pageId, stepId: active.stepId, status: active.status };
    const step = nextExecutableStep(page);
    if (step) {
      return {
        type: step.status === 'ready' ? 'run-step' : 'retry-step',
        tool: step.status === 'ready' ? 'web_design_run_next_step' : 'web_design_retry_step',
        pageId: page.pageId,
        stepId: step.stepId
      };
    }
    const handoff = page.steps.find((step) => step.kind === 'handoff');
    if (handoff?.status === 'accepted') return { type: 'complete-page', tool: 'web_design_complete_page', pageId: page.pageId };
  }
  return { type: 'inspect-plan', tool: 'web_design_get_plan' };
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
    nextAction: nextAction(plan)
  };
}

function rootFrame(pageId: string, width: number): SceneFrameNode {
  const root: SceneFrameNode = {
    ...createSceneNodeBase('frame', 'Page root', { x: 0, y: 0, width, height: 768 }, 'system'),
    type: 'frame',
    id: rootNodeId(pageId),
    role: 'page-root',
    layout: {
      mode: 'auto' as const,
      direction: 'vertical' as const,
      wrap: false,
      padding: { top: 0, right: 0, bottom: 0, left: 0 },
      gap: { row: 0, column: 0 },
      alignItems: 'stretch' as const,
      justifyContent: 'start' as const,
      sizingX: 'fixed' as const,
      sizingY: 'hug' as const,
      minHeight: 768,
      position: 'flow' as const,
      clipContent: false
    },
    children: []
  };
  return root;
}

export class ProgressiveGenerationService {
  constructor(private readonly options: ProgressiveGenerationServiceOptions) {
    requireIdentifier(options.projectId, 'runtime projectId');
  }

  private async scope(documentId: string): Promise<{ scope: GenerationScope; documentName: string }> {
    const document = await this.options.assertDocumentInScope(documentId);
    return { scope: scopeFor(this.options.projectId, documentId), documentName: document.name };
  }

  async planSite(input: SitePlanInput): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(input.documentId);
    let plan: GenerationPlan;
    try {
      const current = await this.options.repositories.plans.read(scope);
      if (!Number.isSafeInteger(input.expectedPlanRevision)) {
        throw new Error(`expectedPlanRevision is required to revise existing plan ${current.planId}.`);
      }
      plan = (await this.options.repositories.plans.apply(scope, input.expectedPlanRevision!, {
        type: 'revise-site', objective: input.objective, audience: input.audience, pages: input.pages
      })).plan;
    } catch (error) {
      if (!isMissing(error)) throw error;
      plan = await this.options.repositories.plans.create(createGenerationSitePlan({
        planId: input.planId ?? `plan:${randomUUID()}`,
        scope,
        mode: input.mode,
        objective: input.objective,
        audience: input.audience,
        pages: input.pages
      }));
    }
    return { plan: summarizeGenerationPlan(plan) };
  }

  async planPage(input: PagePlanInput): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(input.documentId);
    let transition = await this.options.repositories.plans.apply(scope, input.expectedPlanRevision, {
      type: 'plan-page', pageId: input.pageId, design: input.design, steps: normalizePageSteps(input.pageId, input.steps)
    });
    if (transition.plan.status === 'draft') {
      transition = await this.options.repositories.plans.apply(scope, transition.plan.revision, { type: 'mark-ready' });
    }
    return { plan: summarizeGenerationPlan(transition.plan) };
  }

  async getPlan(documentId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    return { plan: summarizeGenerationPlan(await this.options.repositories.plans.read(scope)) };
  }

  private async ensureScene(plan: GenerationPlan, documentName: string): Promise<SceneDocument> {
    let scene: SceneDocument;
    try {
      scene = await this.options.repositories.scenes.read(plan.scope.documentId);
    } catch (error) {
      if (!isMissing(error)) throw error;
      scene = createBlankSceneDocument(documentName);
      scene.documentId = plan.scope.documentId;
      scene.pages = plan.pageRuns.map((page) => ({ id: page.pageId, name: page.name, children: [] }));
      scene = await this.options.repositories.scenes.create(scene);
    }
    return scene;
  }

  async startPage(documentId: string, expectedPlanRevision: number, pageId: string, viewportWidth = 1440): Promise<Record<string, unknown>> {
    if (!Number.isSafeInteger(viewportWidth) || viewportWidth < 240 || viewportWidth > 10000) throw new Error('viewportWidth is invalid.');
    const { scope, documentName } = await this.scope(documentId);
    let plan = await this.options.repositories.plans.read(scope);
    if (plan.revision !== expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${plan.revision}.`);
    if (!(plan.status === 'running' && plan.activePageId === pageId)) {
      plan = (await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'start-page', pageId })).plan;
    }
    let scene = await this.ensureScene(plan, documentName);
    let page = scene.pages.find((candidate) => candidate.id === pageId);
    if (!page) {
      const planned = plan.pageRuns.find((candidate) => candidate.pageId === pageId);
      if (!planned) throw new Error(`Generation page not found: ${pageId}`);
      const inserted = await this.options.repositories.scenes.apply(scene.documentId, {
        transactionId: `system:add-page:${plan.planId}:${pageId}`,
        baseRevision: scene.revision,
        author: 'system',
        operations: [{ op: 'insert-page', index: scene.pages.length, page: { id: pageId, name: planned.name, children: [] } }]
      });
      scene = inserted.document;
      page = scene.pages.find((candidate) => candidate.id === pageId)!;
    }
    const rootId = rootNodeId(pageId);
    if (!page.children.some((node) => node.id === rootId)) {
      scene = (await this.options.repositories.scenes.apply(scene.documentId, {
        transactionId: `system:add-root:${plan.planId}:${pageId}`,
        baseRevision: scene.revision,
        author: 'system',
        operations: [{ op: 'insert-node', parentId: pageId, index: page.children.length, node: rootFrame(pageId, viewportWidth) }]
      })).document;
    }
    return {
      plan: summarizeGenerationPlan(plan),
      scene: { documentId: scene.documentId, revision: scene.revision, pageId, rootNodeId: rootId, viewportWidth },
      nextAction: nextAction(plan)
    };
  }

  private async execute(input: ExecuteProgressiveStepInput, retry: boolean): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(input.documentId);
    const plan = await this.options.repositories.plans.read(scope);
    if (plan.revision !== input.expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${plan.revision}.`);
    if (plan.status !== 'running' || !plan.activePageId) throw new Error('Start one page before running a generation step.');
    const page = plan.pageRuns.find((candidate) => candidate.pageId === plan.activePageId)!;
    if (page.activeStepId) throw new Error(`Generation step ${page.activeStepId} is already active.`);
    const step = input.stepId ? page.steps.find((candidate) => candidate.stepId === input.stepId) : nextExecutableStep(page);
    if (!step) throw new Error('The active page has no executable generation step.');
    if (retry && step.status === 'ready') throw new Error(`Generation step ${step.stepId} has not failed and does not need retry.`);
    if (!retry && step.status !== 'ready') throw new Error(`Use web_design_retry_step for ${step.stepId} in ${step.status} state.`);
    const scene = await this.options.repositories.scenes.read(scope.documentId);
    assertVisualInputs(input.visualInputs, scene.revision);
    const serializedBytes = Buffer.byteLength(JSON.stringify(input.operations), 'utf8');
    if (!Array.isArray(input.operations) || input.operations.length === 0 || input.operations.length > 64 || serializedBytes > 262_144) {
      throw new Error('A generation step needs 1–64 focused Scene operations and must stay below 262144 bytes.');
    }
    assertPassingVerification(input.verification, scene.revision + 1);
    const attemptId = input.attemptId ?? `attempt:${randomUUID()}`;
    const result = await prepareGenerationStep({
      scope,
      expectedPlanRevision: plan.revision,
      pageId: page.pageId,
      stepId: step.stepId,
      attemptId,
      idempotencyKey: input.idempotencyKey
    }, this.options.repositories, {
      generate: () => ({
        transactionId: input.transactionId,
        baseRevision: scene.revision,
        author: 'ai',
        operations: structuredClone(input.operations)
      }),
      verify: () => ({
        ...structuredClone(input.verification),
        artifacts: [...structuredClone(input.visualInputs), ...structuredClone(input.verification.artifacts)]
      })
    });
    if (result.status !== 'prepared') {
      return { status: result.status, plan: summarizeGenerationPlan(result.plan), ...(result.status === 'failed' ? { error: result.error } : {}) };
    }
    if (result.plan.mode !== 'auto-current-page') {
      return { status: 'awaiting-review', plan: summarizeGenerationPlan(result.plan), candidate: this.candidateSummary(result.candidate) };
    }
    const committed = await commitGenerationStep({
      scope,
      expectedPlanRevision: result.plan.revision,
      pageId: page.pageId,
      stepId: step.stepId,
      attemptId
    }, this.options.repositories);
    if (committed.status === 'committed') {
      return {
        status: 'committed',
        plan: summarizeGenerationPlan(committed.plan),
        scene: { documentId: committed.document.documentId, revision: committed.document.revision },
        transaction: committed.summary,
        recovered: committed.recovered
      };
    }
    return {
      status: committed.status,
      plan: summarizeGenerationPlan(committed.plan),
      ...(committed.status === 'requires-protection-review'
        ? { conflicts: committed.conflicts, candidate: this.candidateSummary(result.candidate) }
        : { currentSceneRevision: committed.currentSceneRevision })
    };
  }

  async runNextStep(input: ExecuteProgressiveStepInput): Promise<Record<string, unknown>> {
    if (input.stepId !== undefined) throw new Error('web_design_run_next_step chooses the next ready step; use retry for a specific failed step.');
    return this.execute(input, false);
  }

  async retryStep(input: ExecuteProgressiveStepInput & { stepId: string }): Promise<Record<string, unknown>> {
    return this.execute(input, true);
  }

  async repairStep(input: ExecuteProgressiveStepInput & { stepId: string }): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(input.documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const step = plan.pageRuns.flatMap((page) => page.steps).find((candidate) => candidate.stepId === input.stepId);
    if (!step) throw new Error(`Generation step not found: ${input.stepId}`);
    const issueIds = step.attempts.at(-1)?.error?.issueIds ?? [];
    if (issueIds.length === 0) throw new Error(`Generation step ${step.stepId} has no recorded visual issue IDs for targeted repair.`);
    return this.execute(input, true);
  }

  private candidateSummary(candidate: GenerationCandidateRecord): Record<string, unknown> {
    return {
      candidateId: candidate.candidateId,
      pageId: candidate.pageId,
      stepId: candidate.stepId,
      attemptId: candidate.attemptId,
      baseRevision: candidate.baseRevision,
      qualitySummary: candidate.qualitySummary,
      issueIds: candidate.issueIds,
      protectionConflicts: candidate.protectionConflicts,
      artifacts: candidate.artifacts
    };
  }

  async inspectStep(documentId: string, stepId: string, attemptId?: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    const step = page?.steps.find((candidate) => candidate.stepId === stepId);
    if (!page || !step) throw new Error(`Generation step not found: ${stepId}`);
    const selectedAttempt = attemptId
      ? step.attempts.find((attempt) => attempt.attemptId === attemptId)
      : step.activeAttemptId
        ? step.attempts.find((attempt) => attempt.attemptId === step.activeAttemptId)
        : step.attempts.at(-1);
    if (!selectedAttempt) throw new Error(`Generation step ${stepId} has no attempt to inspect.`);
    let candidate: GenerationCandidateRecord | undefined;
    try { candidate = await this.options.repositories.candidates.read(scope, selectedAttempt.attemptId); }
    catch (error) { if (!isMissing(error)) throw error; }
    return {
      plan: summarizeGenerationPlan(plan),
      page: { pageId: page.pageId, name: page.name },
      step: { stepId: step.stepId, title: step.title, kind: step.kind, status: step.status, target: step.target },
      attempt: selectedAttempt,
      ...(candidate ? { candidate: this.candidateSummary(candidate) } : {}),
      nextAction: nextAction(plan)
    };
  }

  async acceptStep(documentId: string, expectedPlanRevision: number, stepId: string, attemptId: string, approveSoftProtectionConflicts = false): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    if (!page) throw new Error(`Generation step not found: ${stepId}`);
    const result = await commitGenerationStep({
      scope, expectedPlanRevision, pageId: page.pageId, stepId, attemptId, approveSoftProtectionConflicts
    }, this.options.repositories);
    if (result.status === 'committed') {
      return {
        status: 'committed', plan: summarizeGenerationPlan(result.plan),
        scene: { documentId: result.document.documentId, revision: result.document.revision }, transaction: result.summary, recovered: result.recovered
      };
    }
    return {
      status: result.status, plan: summarizeGenerationPlan(result.plan),
      ...(result.status === 'requires-protection-review' ? { conflicts: result.conflicts } : { currentSceneRevision: result.currentSceneRevision })
    };
  }

  async rejectStep(documentId: string, expectedPlanRevision: number, stepId: string, attemptId: string, reason: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    if (!page) throw new Error(`Generation step not found: ${stepId}`);
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, {
      type: 'reject-step', pageId: page.pageId, stepId, attemptId, reason
    });
    await this.options.repositories.candidates.remove(scope, attemptId).catch(() => undefined);
    return { status: 'rejected', plan: summarizeGenerationPlan(changed.plan) };
  }

  async skipStep(documentId: string, expectedPlanRevision: number, stepId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    if (!page) throw new Error(`Generation step not found: ${stepId}`);
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'skip-step', pageId: page.pageId, stepId });
    return { status: 'skipped', plan: summarizeGenerationPlan(changed.plan) };
  }

  async rollbackStep(documentId: string, expectedPlanRevision: number, stepId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    if (plan.revision !== expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${plan.revision}.`);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    const step = page?.steps.find((candidate) => candidate.stepId === stepId);
    if (!page || !step || step.status !== 'accepted' || !step.acceptedAttemptId) {
      throw new Error('Only an accepted generation step can be rolled back.');
    }
    const attempt = step.attempts.find((candidate) => candidate.attemptId === step.acceptedAttemptId)!;
    const transactionId = attempt.artifacts.find((artifact) => artifact.kind === 'candidate-transaction')?.metadata?.transactionId;
    if (typeof transactionId !== 'string' || !transactionId) throw new Error('Accepted generation step is missing its committed transaction ID.');
    if (attempt.committedRevision === undefined) throw new Error('Accepted generation step is missing its committed Scene revision.');
    let scene = await this.options.repositories.scenes.read(documentId);
    const history = await this.options.repositories.scenes.history(documentId);
    let recovered = false;
    if (scene.revision === attempt.committedRevision && history.nextUndoTransactionId === transactionId) {
      scene = await this.options.repositories.scenes.undo(documentId, scene.revision, 'system');
    } else if (scene.revision === attempt.committedRevision + 1 && history.nextRedoTransactionId === transactionId) {
      recovered = true;
    } else {
      throw new Error(`Step ${stepId} is not the latest Scene transaction and cannot be rolled back without overwriting later work.`);
    }
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'rollback-step', pageId: page.pageId, stepId });
    return {
      status: 'rolled-back',
      plan: summarizeGenerationPlan(changed.plan),
      scene: { documentId: scene.documentId, revision: scene.revision },
      transactionId,
      recovered
    };
  }

  async completePage(documentId: string, expectedPlanRevision: number, pageId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'complete-page', pageId });
    return { status: 'completed', plan: summarizeGenerationPlan(changed.plan), pageId };
  }

  async pause(documentId: string, expectedPlanRevision: number): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const result = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'pause-plan' });
    return { plan: summarizeGenerationPlan(result.plan) };
  }

  async resume(documentId: string, expectedPlanRevision: number): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const result = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'resume-plan' });
    return { plan: summarizeGenerationPlan(result.plan) };
  }
}
