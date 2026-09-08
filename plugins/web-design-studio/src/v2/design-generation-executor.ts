import { diffSceneDocuments, type SceneDocumentDiff } from './scene-diff.js';
import type { SceneDocument } from './scene-schema.js';
import type { SceneTransaction, SceneTransactionSummary } from './scene-transaction.js';
import type { DesignGenerationPlan, DesignGenerationStep, DesignScope } from './design-protocol.js';

export interface DesignGenerationRepository {
  read(documentId: string): Promise<SceneDocument>;
  apply(documentId: string, transaction: SceneTransaction): Promise<{ document: SceneDocument; summary: SceneTransactionSummary }>;
}

export interface DesignLayoutArtifact {
  viewportWidth: number;
  rootNodeId: string;
  errorDiagnostics: number;
  warningDiagnostics: number;
}

export interface DesignSnapshotArtifact {
  viewportWidth: number;
  snapshotId: string;
  sha256?: string;
}

export interface DesignCritiqueArtifact {
  verdict: 'pass' | 'revise';
  issueIds: string[];
  summary: string;
}

export interface DesignGenerationHandlers {
  createTransaction(
    step: Extract<DesignGenerationStep, { kind: 'create-design-system' | 'apply-section-transaction' }>,
    document: SceneDocument,
    scope: DesignScope
  ): Promise<SceneTransaction> | SceneTransaction;
  solveLayout(
    step: Extract<DesignGenerationStep, { kind: 'solve-layout' }>,
    document: SceneDocument,
    scope: DesignScope
  ): Promise<DesignLayoutArtifact[]> | DesignLayoutArtifact[];
  renderSnapshots(
    step: Extract<DesignGenerationStep, { kind: 'render-snapshots' }>,
    document: SceneDocument,
    scope: DesignScope
  ): Promise<DesignSnapshotArtifact[]> | DesignSnapshotArtifact[];
  critique(
    step: Extract<DesignGenerationStep, { kind: 'critique-and-revise' }>,
    document: SceneDocument,
    scope: DesignScope
  ): Promise<DesignCritiqueArtifact> | DesignCritiqueArtifact;
}

export type DesignGenerationStepState = {
  stepId: string;
  kind: DesignGenerationStep['kind'];
  status: 'pending' | 'completed' | 'failed' | 'blocked';
  startedRevision?: number;
  completedRevision?: number;
  transactionSummary?: SceneTransactionSummary;
  diff?: SceneDocumentDiff;
  layouts?: DesignLayoutArtifact[];
  snapshots?: DesignSnapshotArtifact[];
  critique?: DesignCritiqueArtifact;
  error?: string;
};

export interface DesignGenerationRun {
  planId: string;
  scope: DesignScope;
  status: 'completed' | 'failed';
  baseRevision: number;
  finalRevision: number;
  steps: DesignGenerationStepState[];
}

function assertScopeMatches(plan: DesignGenerationPlan, document: SceneDocument): void {
  if (document.documentId !== plan.scope.documentId) throw new Error('Generation plan documentId does not match the stored Scene document.');
}

function assertDependencies(plan: DesignGenerationPlan): void {
  const ids = new Set(plan.steps.map((step) => step.stepId));
  if (ids.size !== plan.steps.length) throw new Error('Generation plan step IDs must be unique.');
  const preceding = new Set<string>();
  for (const step of plan.steps) {
    if (!step.stepId || !Array.isArray(step.dependsOn)) throw new Error('Generation plan contains an invalid step.');
    for (const dependency of step.dependsOn) {
      if (!ids.has(dependency)) throw new Error(`Generation step ${step.stepId} depends on unknown step ${dependency}.`);
      if (!preceding.has(dependency)) throw new Error(`Generation step ${step.stepId} has a forward or cyclic dependency on ${dependency}.`);
    }
    preceding.add(step.stepId);
  }
}

function assertViewportArtifacts(expected: number[], actual: Array<{ viewportWidth: number }>, label: string): void {
  if (!Array.isArray(actual)) throw new Error(`${label} must return an artifact list.`);
  const widths = actual.map((artifact) => artifact.viewportWidth);
  if (widths.length !== expected.length || new Set(widths).size !== widths.length || expected.some((width) => !widths.includes(width))) {
    throw new Error(`${label} must return exactly one artifact for every requested viewport.`);
  }
}

export async function executeDesignGenerationPlan(
  plan: DesignGenerationPlan,
  repository: DesignGenerationRepository,
  handlers: DesignGenerationHandlers
): Promise<DesignGenerationRun> {
  if (!plan || typeof plan !== 'object' || !plan.scope || !Array.isArray(plan.steps) || plan.steps.length === 0) throw new Error('Generation plan is invalid.');
  assertDependencies(plan);
  let document = await repository.read(plan.scope.documentId);
  assertScopeMatches(plan, document);
  if (document.revision !== plan.baseRevision) throw new Error(`Generation plan revision conflict. Current revision is ${document.revision}.`);
  const states: DesignGenerationStepState[] = plan.steps.map((step) => ({ stepId: step.stepId, kind: step.kind, status: 'pending' }));
  const completed = new Set<string>();

  for (const [index, step] of plan.steps.entries()) {
    const state = states[index];
    if (step.dependsOn.some((dependency) => !completed.has(dependency))) {
      state.status = 'blocked';
      state.error = 'A required generation step did not complete.';
      continue;
    }
    state.startedRevision = document.revision;
    try {
      if (step.kind === 'create-design-system' || step.kind === 'apply-section-transaction') {
        const before = document;
        const transaction = await handlers.createTransaction(step, structuredClone(document), structuredClone(plan.scope));
        if (transaction.author !== 'ai') throw new Error(`Generation transaction ${transaction.transactionId} must be authored by AI.`);
        if (transaction.baseRevision !== document.revision) throw new Error(`Generation transaction ${transaction.transactionId} must use current revision ${document.revision}.`);
        const applied = await repository.apply(plan.scope.documentId, transaction);
        document = applied.document;
        state.transactionSummary = applied.summary;
        state.diff = diffSceneDocuments(before, document);
      } else if (step.kind === 'solve-layout') {
        const layouts = await handlers.solveLayout(step, structuredClone(document), structuredClone(plan.scope));
        assertViewportArtifacts(step.viewportWidths, layouts, 'solveLayout');
        if (layouts.some((layout) => layout.errorDiagnostics > 0)) throw new Error('Layout solving returned error diagnostics.');
        state.layouts = structuredClone(layouts);
      } else if (step.kind === 'render-snapshots') {
        const snapshots = await handlers.renderSnapshots(step, structuredClone(document), structuredClone(plan.scope));
        assertViewportArtifacts(step.viewportWidths, snapshots, 'renderSnapshots');
        if (snapshots.some((snapshot) => !snapshot.snapshotId)) throw new Error('Snapshot artifact needs a stable snapshotId.');
        state.snapshots = structuredClone(snapshots);
      } else {
        const critique = await handlers.critique(step, structuredClone(document), structuredClone(plan.scope));
        if (!critique || !['pass', 'revise'].includes(critique.verdict) || !Array.isArray(critique.issueIds) || !critique.summary?.trim()) {
          throw new Error('Critique artifact is invalid.');
        }
        if (critique.verdict === 'pass' && critique.issueIds.length > 0) throw new Error('A passing critique cannot retain unresolved issue IDs.');
        if (critique.verdict === 'revise' && critique.issueIds.length === 0) throw new Error('A revision critique needs at least one issue ID.');
        state.critique = structuredClone(critique);
      }
      state.status = 'completed';
      state.completedRevision = document.revision;
      completed.add(step.stepId);
    } catch (error) {
      state.status = 'failed';
      state.error = error instanceof Error ? error.message : String(error);
      for (const pending of states.slice(index + 1)) {
        pending.status = 'blocked';
        pending.error = `Blocked by failed step ${step.stepId}.`;
      }
      return {
        planId: plan.planId,
        scope: structuredClone(plan.scope),
        status: 'failed',
        baseRevision: plan.baseRevision,
        finalRevision: document.revision,
        steps: states
      };
    }
  }
  return {
    planId: plan.planId,
    scope: structuredClone(plan.scope),
    status: 'completed',
    baseRevision: plan.baseRevision,
    finalRevision: document.revision,
    steps: states
  };
}
