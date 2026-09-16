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

export interface ProgressiveGenerationImage {
  label: string;
  data: string;
  mimeType: 'image/png';
}

export interface VerifyProgressiveCandidateInput {
  scope: GenerationScope;
  page: GenerationPageRun;
  step: GenerationStep;
  baseDocument: SceneDocument;
  candidateDocument: SceneDocument;
  visualInputs: GenerationArtifact[];
}

export type VerifiedProgressiveCandidate = SubmittedStepVerification & {
  __images?: ProgressiveGenerationImage[];
};

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
  verifyCandidate(input: VerifyProgressiveCandidateInput): Promise<VerifiedProgressiveCandidate>;
  captureVisualInputs(input: {
    documentId: string;
    pageId: string;
    revision: number;
    viewportWidths: number[];
  }): Promise<GenerationArtifact[]>;
  loadArtifactImages?(scope: GenerationScope, artifacts: GenerationArtifact[]): Promise<ProgressiveGenerationImage[]>;
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
  requestId?: string;
  operations: SceneTransactionOperation[];
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

function assertVisualInputs(artifacts: GenerationArtifact[], revision: number, viewportWidths: number[]): void {
  assertUniqueArtifacts(artifacts, 'visualInputs');
  if (artifacts.some((artifact) => artifact.revision !== revision)) {
    throw new Error(`Every visual input must reference the current Scene revision ${revision}.`);
  }
  const kinds = artifactKinds(artifacts);
  if (!kinds.has('page-snapshot') && !kinds.has('region-crop')) {
    throw new Error('The current step needs a page snapshot or region crop as visual input.');
  }
  if (!kinds.has('visual-grounding')) throw new Error('The current step needs visual grounding for stable Scene node IDs.');
  for (const viewportWidth of viewportWidths) {
    if (!artifacts.some((artifact) => artifact.kind === 'page-snapshot' && artifact.viewportWidth === viewportWidth)) {
      throw new Error(`The current step needs a full page snapshot at viewport ${viewportWidth}.`);
    }
    if (!artifacts.some((artifact) => artifact.kind === 'visual-grounding' && artifact.viewportWidth === viewportWidth)) {
      throw new Error(`The current step needs visual grounding at viewport ${viewportWidth}.`);
    }
  }
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

function compactArtifactReference(artifact: GenerationArtifact): Record<string, unknown> {
  return {
    id: artifact.artifactId,
    kind: artifact.kind,
    revision: artifact.revision,
    ...(artifact.viewportWidth === undefined ? {} : { viewportWidth: artifact.viewportWidth })
  };
}

function compactSceneChange(summary: {
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

function summarizeGenerationPlanState(plan: GenerationPlan): Record<string, unknown> {
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
      // The page root follows the current artboard viewport. Its frame width is
      // only an initial/reference size and must never lock the artboard.
      sizingX: 'fill' as const,
      sizingY: 'hug' as const,
      minHeight: 1,
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
    const viewportWidth = Math.max(1440, ...input.steps.flatMap((step) => step.target?.viewportWidths ?? []));
    const started = await this.startPage(input.documentId, transition.plan.revision, input.pageId, viewportWidth);
    return {
      status: 'planned-and-started',
      plan: started.plan,
      scene: started.scene,
      plannedPage: { pageId: input.pageId, viewportWidth, stepCount: input.steps.length }
    };
  }

  async getPlan(documentId: string, detail: 'compact' | 'full' = 'compact'): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    return { plan: detail === 'full' ? summarizeGenerationPlan(plan) : summarizeGenerationPlanState(plan) };
  }

  async getActiveContext(documentId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const summary = summarizeGenerationPlanState(plan);
    const page = plan.activePageId ? plan.pageRuns.find((candidate) => candidate.pageId === plan.activePageId) : undefined;
    const step = page?.activeStepId ? page.steps.find((candidate) => candidate.stepId === page.activeStepId) : undefined;
    if (!page || !step || step.status !== 'awaiting-review' || !step.activeAttemptId) return { plan: summary };
    const candidate = await this.options.repositories.candidates.read(scope, step.activeAttemptId);
    const images = this.options.loadArtifactImages ? await this.options.loadArtifactImages(scope, candidate.artifacts) : [];
    return {
      plan: summary,
      resumeReview: {
        page: { pageId: page.pageId, name: page.name },
        step: { stepId: step.stepId, title: step.title, kind: step.kind, target: step.target },
        candidate: this.candidateSummary(candidate),
        instruction: 'Review the replayed Candidate and Diff images, then use web_design_control_plan. Do not request the same images again.'
      },
      ...(images.length > 0 ? { __images: images } : {})
    };
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
      plan: summarizeGenerationPlanState(plan),
      scene: { documentId: scene.documentId, revision: scene.revision, pageId, rootNodeId: rootId, viewportWidth },
      nextAction: nextAction(plan)
    };
  }

  async executeStep(input: ExecuteProgressiveStepInput): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(input.documentId);
    const plan = await this.options.repositories.plans.read(scope);
    if (plan.revision !== input.expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${plan.revision}.`);
    if (plan.status !== 'running' || !plan.activePageId) throw new Error('Start one page before running a generation step.');
    const page = plan.pageRuns.find((candidate) => candidate.pageId === plan.activePageId)!;
    if (page.activeStepId) throw new Error(`Generation step ${page.activeStepId} is already active.`);
    const step = input.stepId ? page.steps.find((candidate) => candidate.stepId === input.stepId) : nextExecutableStep(page);
    if (!step) throw new Error('The active page has no executable generation step.');
    if (!executableStepStatuses.has(step.status)) throw new Error(`Generation step ${step.stepId} cannot execute from ${step.status}.`);
    const scene = await this.options.repositories.scenes.read(scope.documentId);
    const viewportWidths = step.target.viewportWidths.length > 0 ? step.target.viewportWidths : [1440];
    const visualInputs = await this.options.captureVisualInputs({
      documentId: scope.documentId,
      pageId: page.pageId,
      revision: scene.revision,
      viewportWidths
    });
    assertVisualInputs(visualInputs, scene.revision, viewportWidths);
    const serializedBytes = Buffer.byteLength(JSON.stringify(input.operations), 'utf8');
    if (!Array.isArray(input.operations) || input.operations.length === 0 || input.operations.length > 256 || serializedBytes > 262_144) {
      throw new Error('A generation step needs 1–256 focused Scene operations and must stay below 262144 bytes. Use insert-simple-tree for a large editable hierarchy.');
    }
    const requestId = requireIdentifier(input.requestId ?? randomUUID(), 'requestId');
    const attemptId = `attempt:${requestId}`;
    const idempotencyKey = `execute:${step.stepId}:${requestId}`;
    const transactionId = `transaction:${step.stepId}:${requestId}`;
    let verificationImages: ProgressiveGenerationImage[] = [];
    const result = await prepareGenerationStep({
      scope,
      expectedPlanRevision: plan.revision,
      pageId: page.pageId,
      stepId: step.stepId,
      attemptId,
      idempotencyKey
    }, this.options.repositories, {
      generate: () => ({
        transactionId,
        baseRevision: scene.revision,
        author: 'ai',
        operations: structuredClone(input.operations)
      }),
      verify: async ({ page: candidatePage, step: candidateStep, baseDocument, candidateDocument }) => {
        const verified = await this.options.verifyCandidate({
          scope: structuredClone(scope),
          page: structuredClone(candidatePage),
          step: structuredClone(candidateStep),
          baseDocument: structuredClone(baseDocument),
          candidateDocument: structuredClone(candidateDocument),
          visualInputs: structuredClone(visualInputs)
        });
        verificationImages = [...(verified.__images ?? [])];
        assertPassingVerification(verified, candidateDocument.revision);
        return {
          ...structuredClone(verified),
          artifacts: [...structuredClone(visualInputs), ...structuredClone(verified.artifacts)]
        };
      }
    });
    if (result.status !== 'prepared') {
      return {
        status: result.status,
        plan: summarizeGenerationPlanState(result.plan),
        ...(result.status === 'failed' ? { error: result.error } : {}),
        ...(verificationImages.length > 0 ? { __images: verificationImages } : {})
      };
    }
    const planSummary = summarizeGenerationPlanState(result.plan);
    planSummary.nextAction = {
      type: 'review-returned-candidate',
      detail: 'Review the Candidate and Diff images in this result, then use web_design_control_plan to accept or reject it.'
    };
    return {
      status: 'awaiting-review',
      plan: planSummary,
      candidate: this.candidateSummary(result.candidate),
      nextAction: {
        type: 'review-candidate-images',
        tool: 'web_design_control_plan',
        detail: 'Inspect the returned Candidate screenshots and visual diffs directly, then accept or reject. Rejected and mechanically failed Steps are retried by calling web_design_execute_step with corrected operations.'
      },
      ...(verificationImages.length > 0 ? { __images: verificationImages } : {})
    };
  }

  private candidateSummary(candidate: GenerationCandidateRecord): Record<string, unknown> {
    const candidateRevision = candidate.baseRevision + 1;
    const artifacts = candidate.artifacts.filter((artifact) => artifact.revision === candidateRevision);
    return {
      candidateId: candidate.candidateId,
      pageId: candidate.pageId,
      stepId: candidate.stepId,
      attemptId: candidate.attemptId,
      baseRevision: candidate.baseRevision,
      qualitySummary: candidate.qualitySummary,
      issueIds: candidate.issueIds,
      protectionConflicts: candidate.protectionConflicts,
      reviewArtifacts: artifacts
        .filter((artifact) => ['page-snapshot', 'visual-grounding', 'visual-diff', 'quality-report'].includes(artifact.kind))
        .map(compactArtifactReference)
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
    const images = candidate && this.options.loadArtifactImages
      ? await this.options.loadArtifactImages(scope, candidate.artifacts)
      : [];
    return {
      plan: summarizeGenerationPlanState(plan),
      page: { pageId: page.pageId, name: page.name },
      step: { stepId: step.stepId, title: step.title, kind: step.kind, status: step.status, target: step.target },
      attempt: selectedAttempt,
      ...(candidate ? { candidate: { ...this.candidateSummary(candidate), artifacts: candidate.artifacts } } : {}),
      nextAction: nextAction(plan),
      ...(images.length > 0 ? { __images: images } : {})
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
      let committedPlan = result.plan;
      const acceptedStep = committedPlan.pageRuns.find((candidate) => candidate.pageId === page.pageId)?.steps.find((candidate) => candidate.stepId === stepId);
      let completedPage = false;
      if (acceptedStep?.kind === 'handoff') {
        committedPlan = (await this.options.repositories.plans.apply(scope, committedPlan.revision, {
          type: 'complete-page', pageId: page.pageId
        })).plan;
        completedPage = true;
      }
      return {
        status: completedPage ? 'page-completed' : 'committed',
        plan: summarizeGenerationPlanState(committedPlan),
        scene: { documentId: result.document.documentId, revision: result.document.revision },
        change: compactSceneChange(result.summary),
        affectedRootNodeId: rootNodeId(page.pageId),
        recovered: result.recovered,
        ...(completedPage ? {
          contextCheckpoint: {
            kind: 'artboard-complete',
            pageId: page.pageId,
            planId: committedPlan.planId,
            planRevision: committedPlan.revision,
            sceneRevision: result.document.revision,
            instruction: 'Keep this compact checkpoint and the next required action; completed artboard construction details may be compacted from working context.'
          }
        } : {})
      };
    }
    return {
      status: result.status, plan: summarizeGenerationPlanState(result.plan),
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
    return { status: 'rejected', plan: summarizeGenerationPlanState(changed.plan) };
  }

  async skipStep(documentId: string, expectedPlanRevision: number, stepId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const plan = await this.options.repositories.plans.read(scope);
    const page = plan.pageRuns.find((candidate) => candidate.steps.some((step) => step.stepId === stepId));
    if (!page) throw new Error(`Generation step not found: ${stepId}`);
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'skip-step', pageId: page.pageId, stepId });
    return { status: 'skipped', plan: summarizeGenerationPlanState(changed.plan) };
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
      plan: summarizeGenerationPlanState(changed.plan),
      scene: { documentId: scene.documentId, revision: scene.revision },
      transactionId,
      recovered
    };
  }

  async completePage(documentId: string, expectedPlanRevision: number, pageId: string): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const changed = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'complete-page', pageId });
    return { status: 'completed', plan: summarizeGenerationPlanState(changed.plan), pageId };
  }

  async pause(documentId: string, expectedPlanRevision: number): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const result = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'pause-plan' });
    return { plan: summarizeGenerationPlanState(result.plan) };
  }

  async resume(documentId: string, expectedPlanRevision: number): Promise<Record<string, unknown>> {
    const { scope } = await this.scope(documentId);
    const result = await this.options.repositories.plans.apply(scope, expectedPlanRevision, { type: 'resume-plan' });
    return { plan: summarizeGenerationPlanState(result.plan) };
  }
}
