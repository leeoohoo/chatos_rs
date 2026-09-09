import {
  type GenerationCandidateRecord,
  type GenerationSoftProtectionConflict
} from './generation-candidate-store.js';
import {
  assertGenerationScopeMatches,
  type GenerationArtifact,
  type GenerationAttemptError,
  type GenerationPageRun,
  type GenerationPlan,
  type GenerationScope,
  type GenerationStep
} from './generation-plan-schema.js';
import type { GenerationPlanAction } from './generation-state-machine.js';
import {
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneDocument,
  type SceneNode
} from './scene-schema.js';
import { applySceneTransaction, type SceneTransaction, type SceneTransactionSummary } from './scene-transaction.js';
import { findGenerationSoftProtectionConflicts, type GenerationSoftProtectedField } from './generation-soft-protection.js';

export interface GenerationStepPlanRepository {
  read(scope: GenerationScope): Promise<GenerationPlan>;
  apply(scope: GenerationScope, expectedRevision: number, action: GenerationPlanAction, timestamp?: string): Promise<{ plan: GenerationPlan; replayed: boolean }>;
}

export interface GenerationStepSceneRepository {
  read(documentId: string): Promise<SceneDocument>;
  apply(documentId: string, transaction: SceneTransaction): Promise<{ document: SceneDocument; summary: SceneTransactionSummary }>;
  findAppliedTransaction?(documentId: string, transactionId: string): Promise<{ transaction: SceneTransaction; summary: SceneTransactionSummary } | undefined>;
}

export interface GenerationStepCandidateRepository {
  create(candidate: GenerationCandidateRecord): Promise<GenerationCandidateRecord>;
  read(scope: GenerationScope, attemptId: string): Promise<GenerationCandidateRecord>;
  remove(scope: GenerationScope, attemptId: string): Promise<void>;
}

export interface GenerationSoftProtectionRepository {
  read(scope: GenerationScope, document: SceneDocument): Promise<GenerationSoftProtectedField[]> | GenerationSoftProtectedField[];
}

export interface GenerationStepVerification {
  passed: boolean;
  artifacts: GenerationArtifact[];
  qualitySummary: string;
  issueIds: string[];
  error?: GenerationAttemptError;
}

export interface GenerationStepHandlers {
  generate(input: {
    plan: GenerationPlan;
    page: GenerationPageRun;
    step: GenerationStep;
    document: SceneDocument;
    scope: GenerationScope;
    attemptId: string;
  }): Promise<SceneTransaction> | SceneTransaction;
  verify(input: {
    plan: GenerationPlan;
    page: GenerationPageRun;
    step: GenerationStep;
    baseDocument: SceneDocument;
    candidateDocument: SceneDocument;
    transaction: SceneTransaction;
    scope: GenerationScope;
    attemptId: string;
  }): Promise<GenerationStepVerification> | GenerationStepVerification;
}

export interface GenerationStepExecutorRepositories {
  plans: GenerationStepPlanRepository;
  scenes: GenerationStepSceneRepository;
  candidates: GenerationStepCandidateRepository;
  protections?: GenerationSoftProtectionRepository;
}

export interface PrepareGenerationStepInput {
  scope: GenerationScope;
  expectedPlanRevision: number;
  pageId: string;
  stepId: string;
  attemptId: string;
  idempotencyKey: string;
}

export type PrepareGenerationStepResult =
  | { status: 'prepared'; plan: GenerationPlan; candidate: GenerationCandidateRecord; candidateDocument: SceneDocument }
  | { status: 'failed'; plan: GenerationPlan; error: GenerationAttemptError }
  | { status: 'in-progress'; plan: GenerationPlan; attemptId: string };

export interface CommitGenerationStepInput {
  scope: GenerationScope;
  expectedPlanRevision: number;
  pageId: string;
  stepId: string;
  attemptId: string;
  approveSoftProtectionConflicts?: boolean;
}

export type CommitGenerationStepResult =
  | { status: 'committed'; plan: GenerationPlan; document: SceneDocument; summary: SceneTransactionSummary; recovered: boolean }
  | { status: 'requires-protection-review'; plan: GenerationPlan; conflicts: GenerationSoftProtectionConflict[] }
  | { status: 'stale'; plan: GenerationPlan; currentSceneRevision: number };

function pageAndStep(plan: GenerationPlan, pageId: string, stepId: string): { page: GenerationPageRun; step: GenerationStep } {
  const page = plan.pageRuns.find((candidate) => candidate.pageId === pageId);
  if (!page) throw new Error(`Generation page not found: ${pageId}`);
  const step = page.steps.find((candidate) => candidate.stepId === stepId);
  if (!step) throw new Error(`Generation step not found: ${stepId}`);
  return { page, step };
}

function nodeChildren(node: SceneNode): SceneNode[] {
  const children: SceneNode[] = [];
  if (isSceneContainer(node)) children.push(...node.children);
  if (isSceneSlotContainer(node)) children.push(...Object.values(node.slots).flat());
  return children;
}

function detachedNodeIds(root: SceneNode): string[] {
  const ids: string[] = [];
  const visit = (node: SceneNode): void => {
    ids.push(node.id);
    for (const child of nodeChildren(node)) visit(child);
  };
  visit(root);
  return ids;
}

function targetSubtreeIds(document: SceneDocument, targetNodeIds: string[]): Set<string> {
  const index = indexSceneDocument(document);
  const result = new Set<string>();
  const visit = (node: SceneNode): void => {
    if (result.has(node.id)) return;
    result.add(node.id);
    for (const child of nodeChildren(node)) visit(child);
  };
  for (const nodeId of targetNodeIds) {
    const entry = index.get(nodeId);
    if (!entry) throw new Error(`Generation step target node not found: ${nodeId}`);
    visit(entry.node);
  }
  return result;
}

export function validateGenerationStepTransaction(
  document: SceneDocument,
  page: GenerationPageRun,
  step: GenerationStep,
  transaction: SceneTransaction
): void {
  if (transaction.author !== 'ai') throw new Error('Generation candidate transaction must be authored by AI.');
  if (transaction.baseRevision !== document.revision) throw new Error(`Generation candidate transaction must use Scene revision ${document.revision}.`);
  if (!Array.isArray(transaction.operations) || transaction.operations.length === 0) throw new Error('Generation candidate transaction needs at least one operation.');
  const scenePage = document.pages.find((candidate) => candidate.id === page.pageId);
  if (!scenePage) throw new Error(`Generation Scene page not found: ${page.pageId}`);
  const index = indexSceneDocument(document);
  const allowed = targetSubtreeIds(document, step.target.nodeIds);
  const inserted = new Set<string>();

  for (const operation of transaction.operations) {
    if (operation.op === 'insert-page' || operation.op === 'remove-page') throw new Error(`Generation step cannot ${operation.op} a page.`);
    if (operation.op === 'rename-page') throw new Error('Generation step cannot rename a page.');
    if (operation.op === 'set-responsive-node-overrides' || operation.op === 'remove-responsive-rule') {
      throw new Error('Generation step cannot replace or remove a complete responsive rule.');
    }
    if (operation.op === 'insert-variable-collection') {
      if (!['structure', 'visual'].includes(step.kind)) throw new Error(`${step.kind} step cannot insert design variables.`);
      continue;
    }
    if (operation.op === 'insert-responsive-rule') {
      if (step.kind !== 'responsive') throw new Error(`${step.kind} step cannot insert responsive rules.`);
      continue;
    }
    if (operation.op === 'insert-node') {
      if (step.kind === 'interaction') throw new Error('Interaction steps cannot create visual Scene nodes. Design visible states on their own artboards before linking them.');
      const parentIsPage = operation.parentId === scenePage.id;
      if (!parentIsPage && !allowed.has(operation.parentId) && !inserted.has(operation.parentId)) {
        throw new Error(`Generation insertion parent ${operation.parentId} is outside the current step scope.`);
      }
      if (parentIsPage && step.target.nodeIds.length > 0) throw new Error('A targeted generation step cannot insert an unrelated page root.');
      for (const nodeId of detachedNodeIds(operation.node)) {
        if (index.has(nodeId) || inserted.has(nodeId)) throw new Error(`Generation candidate reuses Scene node ${nodeId}.`);
        inserted.add(nodeId);
        allowed.add(nodeId);
      }
      continue;
    }
    if (operation.op === 'set-variable-collections') {
      throw new Error('Generation candidates must update variables through bounded insert-variable-collection operations.');
    }
    if (operation.op === 'update-node') {
      const prototypePatches = operation.patches.filter((patch) => patch.path[0] === 'prototypeLink');
      if (prototypePatches.length > 0 && step.kind !== 'interaction') {
        throw new Error('Prototype links may be edited only by an interaction step after the visual design gate.');
      }
      if (step.kind === 'interaction' && prototypePatches.length !== operation.patches.length) {
        throw new Error('Interaction steps may edit only prototype links; visual changes need their own design step.');
      }
    } else if (step.kind === 'interaction') {
      throw new Error('Interaction steps may only update prototype links on existing Scene nodes.');
    }
    if (!allowed.has(operation.nodeId) && !inserted.has(operation.nodeId)) {
      throw new Error(`Generation operation targets ${operation.nodeId} outside the current step scope.`);
    }
    if (operation.op === 'move-node' && !allowed.has(operation.parentId) && !inserted.has(operation.parentId)) {
      throw new Error(`Generation move destination ${operation.parentId} is outside the current step scope.`);
    }
  }
}

function generationError(error: unknown, code: GenerationAttemptError['code'] = 'generation_error', retryable = true): GenerationAttemptError {
  return {
    code,
    message: error instanceof Error ? error.message : String(error),
    retryable,
    issueIds: []
  };
}

function transactionsEqual(left: SceneTransaction, right: SceneTransaction): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function failAttempt(
  repositories: GenerationStepExecutorRepositories,
  scope: GenerationScope,
  planRevision: number,
  pageId: string,
  stepId: string,
  attemptId: string,
  error: GenerationAttemptError,
  artifacts: GenerationArtifact[] | undefined,
  timestamp: string
): Promise<GenerationPlan> {
  return (await repositories.plans.apply(scope, planRevision, {
    type: 'fail-step', pageId, stepId, attemptId, error, artifacts
  }, timestamp)).plan;
}

export async function prepareGenerationStep(
  input: PrepareGenerationStepInput,
  repositories: GenerationStepExecutorRepositories,
  handlers: GenerationStepHandlers,
  timestamp = new Date().toISOString()
): Promise<PrepareGenerationStepResult> {
  const initialPlan = await repositories.plans.read(input.scope);
  assertGenerationScopeMatches(initialPlan.scope, input.scope);
  if (initialPlan.revision !== input.expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${initialPlan.revision}.`);
  const document = await repositories.scenes.read(input.scope.documentId);
  if (document.documentId !== input.scope.documentId) throw new Error('Generation Scene document does not match the active scope.');
  const started = await repositories.plans.apply(input.scope, input.expectedPlanRevision, {
    type: 'start-step', pageId: input.pageId, stepId: input.stepId,
    attemptId: input.attemptId, idempotencyKey: input.idempotencyKey, baseRevision: document.revision
  }, timestamp);
  if (started.replayed) {
    try {
      const candidate = await repositories.candidates.read(input.scope, input.attemptId);
      const candidateDocument = applySceneTransaction(document, candidate.transaction, timestamp).document;
      return { status: 'prepared', plan: started.plan, candidate, candidateDocument };
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return { status: 'in-progress', plan: started.plan, attemptId: input.attemptId };
      throw error;
    }
  }

  let plan = started.plan;
  const { page, step } = pageAndStep(plan, input.pageId, input.stepId);
  let transaction: SceneTransaction;
  try {
    transaction = await handlers.generate({
      plan: structuredClone(plan), page: structuredClone(page), step: structuredClone(step),
      document: structuredClone(document), scope: structuredClone(input.scope), attemptId: input.attemptId
    });
    validateGenerationStepTransaction(document, page, step, transaction);
  } catch (error) {
    const failure = generationError(error, 'generation_error');
    plan = await failAttempt(repositories, input.scope, plan.revision, input.pageId, input.stepId, input.attemptId, failure, undefined, timestamp);
    return { status: 'failed', plan, error: failure };
  }

  let candidateDocument: SceneDocument;
  try {
    candidateDocument = applySceneTransaction(document, transaction, timestamp).document;
  } catch (error) {
    const failure = generationError(error, 'scope_violation');
    plan = await failAttempt(repositories, input.scope, plan.revision, input.pageId, input.stepId, input.attemptId, failure, undefined, timestamp);
    return { status: 'failed', plan, error: failure };
  }

  plan = (await repositories.plans.apply(input.scope, plan.revision, {
    type: 'begin-validation', pageId: input.pageId, stepId: input.stepId, attemptId: input.attemptId,
    artifacts: [{
      artifactId: `candidate-transaction:${input.attemptId}`,
      kind: 'candidate-transaction',
      revision: candidateDocument.revision,
      createdAt: timestamp,
      metadata: { transactionId: transaction.transactionId }
    }]
  }, timestamp)).plan;

  let verification: GenerationStepVerification;
  try {
    const current = pageAndStep(plan, input.pageId, input.stepId);
    verification = await handlers.verify({
      plan: structuredClone(plan), page: structuredClone(current.page), step: structuredClone(current.step),
      baseDocument: structuredClone(document), candidateDocument: structuredClone(candidateDocument),
      transaction: structuredClone(transaction), scope: structuredClone(input.scope), attemptId: input.attemptId
    });
  } catch (error) {
    const failure = generationError(error, 'render_error');
    plan = await failAttempt(repositories, input.scope, plan.revision, input.pageId, input.stepId, input.attemptId, failure, undefined, timestamp);
    return { status: 'failed', plan, error: failure };
  }
  if (!verification || !Array.isArray(verification.artifacts) || !verification.qualitySummary?.trim() || !Array.isArray(verification.issueIds)) {
    const failure = generationError('Generation verification result is invalid.', 'render_error');
    plan = await failAttempt(repositories, input.scope, plan.revision, input.pageId, input.stepId, input.attemptId, failure, undefined, timestamp);
    return { status: 'failed', plan, error: failure };
  }
  if (!verification.passed) {
    const failure = verification.error ?? {
      code: 'quality_reject', message: verification.qualitySummary, retryable: true, issueIds: [...verification.issueIds]
    };
    plan = await failAttempt(repositories, input.scope, plan.revision, input.pageId, input.stepId, input.attemptId, failure, verification.artifacts, timestamp);
    return { status: 'failed', plan, error: failure };
  }

  const protections = await repositories.protections?.read(input.scope, document) ?? [];
  const protectionConflicts = findGenerationSoftProtectionConflicts(document, transaction, protections);
  const candidate: GenerationCandidateRecord = {
    schemaVersion: 1,
    candidateId: `candidate:${input.attemptId}`,
    planId: plan.planId,
    scope: structuredClone(input.scope),
    pageId: input.pageId,
    stepId: input.stepId,
    attemptId: input.attemptId,
    idempotencyKey: input.idempotencyKey,
    baseRevision: document.revision,
    transaction: structuredClone(transaction),
    artifacts: structuredClone(verification.artifacts),
    protectionConflicts,
    qualitySummary: verification.qualitySummary,
    issueIds: [...verification.issueIds],
    createdAt: timestamp,
    updatedAt: timestamp
  };
  await repositories.candidates.create(candidate);
  plan = (await repositories.plans.apply(input.scope, plan.revision, {
    type: 'await-review', pageId: input.pageId, stepId: input.stepId, attemptId: input.attemptId,
    artifacts: verification.artifacts
  }, timestamp)).plan;
  return { status: 'prepared', plan, candidate: structuredClone(candidate), candidateDocument };
}

export async function commitGenerationStep(
  input: CommitGenerationStepInput,
  repositories: GenerationStepExecutorRepositories,
  timestamp = new Date().toISOString()
): Promise<CommitGenerationStepResult> {
  let plan = await repositories.plans.read(input.scope);
  assertGenerationScopeMatches(plan.scope, input.scope);
  if (plan.revision !== input.expectedPlanRevision) throw new Error(`Generation plan revision conflict. Current revision is ${plan.revision}.`);
  const { page, step } = pageAndStep(plan, input.pageId, input.stepId);
  if (plan.activePageId !== page.pageId || page.activeStepId !== step.stepId || step.status !== 'awaiting-review' || step.activeAttemptId !== input.attemptId) {
    throw new Error('Generation candidate is not the active reviewed step.');
  }
  const candidate = await repositories.candidates.read(input.scope, input.attemptId);
  if (candidate.planId !== plan.planId || candidate.pageId !== input.pageId || candidate.stepId !== input.stepId) {
    throw new Error('Generation candidate identity does not match the active plan step.');
  }
  if (candidate.protectionConflicts.length > 0 && input.approveSoftProtectionConflicts !== true) {
    return { status: 'requires-protection-review', plan, conflicts: structuredClone(candidate.protectionConflicts) };
  }

  let document = await repositories.scenes.read(input.scope.documentId);
  let summary: SceneTransactionSummary;
  let recovered = false;
  if (document.revision === candidate.baseRevision) {
    const applied = await repositories.scenes.apply(input.scope.documentId, candidate.transaction);
    document = applied.document;
    summary = applied.summary;
  } else {
    const applied = await repositories.scenes.findAppliedTransaction?.(input.scope.documentId, candidate.transaction.transactionId);
    if (!applied || !transactionsEqual(applied.transaction, candidate.transaction)) {
      plan = (await repositories.plans.apply(input.scope, plan.revision, {
        type: 'mark-step-stale', pageId: input.pageId, stepId: input.stepId,
        reason: `Scene revision changed from ${candidate.baseRevision} to ${document.revision} before candidate commit.`
      }, timestamp)).plan;
      return { status: 'stale', plan, currentSceneRevision: document.revision };
    }
    summary = applied.summary;
    if (summary.revision !== document.revision) throw new Error('Recovered generation transaction is not the current Scene revision.');
    recovered = true;
  }

  plan = (await repositories.plans.apply(input.scope, plan.revision, {
    type: 'accept-step', pageId: input.pageId, stepId: input.stepId, attemptId: input.attemptId,
    committedRevision: document.revision
  }, timestamp)).plan;
  await repositories.candidates.remove(input.scope, input.attemptId).catch(() => undefined);
  return { status: 'committed', plan, document, summary, recovered };
}
