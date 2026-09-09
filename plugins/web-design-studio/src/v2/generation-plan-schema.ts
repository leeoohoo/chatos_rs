export interface GenerationScope {
  projectId: string;
  documentId: string;
}

export type GenerationExecutionMode = 'guided' | 'auto-current-page' | 'review-sensitive';
export type GenerationPlanStatus = 'draft' | 'ready' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export type GenerationPageStatus = 'unplanned' | 'planned' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export type GenerationStepStatus =
  | 'planned'
  | 'ready'
  | 'generating'
  | 'validating'
  | 'awaiting-review'
  | 'accepted'
  | 'rejected'
  | 'retryable'
  | 'blocked'
  | 'stale'
  | 'skipped'
  | 'rolled-back';

export type GenerationTaskKind =
  | 'structure'
  | 'section'
  | 'visual'
  | 'design-gate'
  | 'interaction'
  | 'responsive'
  | 'polish'
  | 'handoff';

export type GenerationAttemptStatus =
  | 'generating'
  | 'validating'
  | 'awaiting-review'
  | 'committed'
  | 'failed'
  | 'rejected'
  | 'discarded';

export type GenerationArtifactKind =
  | 'candidate-transaction'
  | 'scene-diff'
  | 'layout'
  | 'page-snapshot'
  | 'region-crop'
  | 'visual-grounding'
  | 'visual-diff'
  | 'calibration'
  | 'quality-report';

export interface GenerationDesignIntent {
  artDirection: string;
  compositionIntent: string;
  typographyIntent: string;
  imageStrategy: string;
  contentHierarchy: string[];
  designAcceptanceCriteria: string[];
  interactionIntents: string[];
}

export interface GenerationArtifact {
  artifactId: string;
  kind: GenerationArtifactKind;
  revision: number;
  createdAt: string;
  viewportWidth?: number;
  nodeIds?: string[];
  uri?: string;
  sha256?: string;
  metadata?: Record<string, string | number | boolean>;
}

export interface GenerationAttemptError {
  code:
    | 'generation_error'
    | 'scope_violation'
    | 'layout_error'
    | 'render_error'
    | 'quality_reject'
    | 'revision_conflict'
    | 'cancelled';
  message: string;
  retryable: boolean;
  issueIds: string[];
}

export interface GenerationAttempt {
  attemptId: string;
  idempotencyKey: string;
  status: GenerationAttemptStatus;
  baseRevision: number;
  committedRevision?: number;
  artifacts: GenerationArtifact[];
  error?: GenerationAttemptError;
  createdAt: string;
  updatedAt: string;
}

export interface GenerationStepTarget {
  sectionKey?: string;
  nodeIds: string[];
  viewportWidths: number[];
}

export interface GenerationStep {
  stepId: string;
  pageId: string;
  title: string;
  kind: GenerationTaskKind;
  required: boolean;
  dependsOn: string[];
  target: GenerationStepTarget;
  status: GenerationStepStatus;
  activeAttemptId?: string;
  acceptedAttemptId?: string;
  attempts: GenerationAttempt[];
  createdAt: string;
  updatedAt: string;
}

export interface GenerationPageRun {
  pageId: string;
  name: string;
  purpose: string;
  order: number;
  design?: GenerationDesignIntent;
  status: GenerationPageStatus;
  activeStepId?: string;
  steps: GenerationStep[];
  createdAt: string;
  updatedAt: string;
}

export interface GenerationSitePlan {
  objective: string;
  audience: string[];
  pages: Array<{
    pageId: string;
    name: string;
    purpose: string;
    order: number;
  }>;
}

export interface GenerationPlan {
  schemaVersion: 1;
  planId: string;
  revision: number;
  scope: GenerationScope;
  mode: GenerationExecutionMode;
  status: GenerationPlanStatus;
  sitePlan: GenerationSitePlan;
  pageRuns: GenerationPageRun[];
  activePageId?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateGenerationStepInput {
  stepId: string;
  title: string;
  kind: GenerationTaskKind;
  required?: boolean;
  dependsOn?: string[];
  target?: Partial<GenerationStepTarget>;
}

export interface CreateGenerationPageInput {
  pageId: string;
  name: string;
  purpose: string;
  design: GenerationDesignIntent;
  steps: CreateGenerationStepInput[];
}

export interface CreateGenerationPlanInput {
  planId: string;
  scope: GenerationScope;
  mode?: GenerationExecutionMode;
  objective: string;
  audience: string[];
  pages: CreateGenerationPageInput[];
}

export interface CreateGenerationSitePlanInput {
  planId: string;
  scope: GenerationScope;
  mode?: GenerationExecutionMode;
  objective: string;
  audience: string[];
  pages: Array<{
    pageId: string;
    name: string;
    purpose: string;
  }>;
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;
const sha256Pattern = /^[a-f0-9]{64}$/;
const planStatuses = new Set<GenerationPlanStatus>(['draft', 'ready', 'running', 'paused', 'completed', 'failed', 'cancelled']);
const pageStatuses = new Set<GenerationPageStatus>(['unplanned', 'planned', 'running', 'paused', 'completed', 'failed', 'cancelled']);
const stepStatuses = new Set<GenerationStepStatus>(['planned', 'ready', 'generating', 'validating', 'awaiting-review', 'accepted', 'rejected', 'retryable', 'blocked', 'stale', 'skipped', 'rolled-back']);
const taskKinds = new Set<GenerationTaskKind>(['structure', 'section', 'visual', 'design-gate', 'interaction', 'responsive', 'polish', 'handoff']);
const attemptStatuses = new Set<GenerationAttemptStatus>(['generating', 'validating', 'awaiting-review', 'committed', 'failed', 'rejected', 'discarded']);
const artifactKinds = new Set<GenerationArtifactKind>(['candidate-transaction', 'scene-diff', 'layout', 'page-snapshot', 'region-crop', 'visual-grounding', 'visual-diff', 'calibration', 'quality-report']);

function assertIdentifier(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
}

function assertText(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !value.trim()) throw new Error(`${label} is required.`);
}

function assertTimestamp(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !Number.isFinite(Date.parse(value))) throw new Error(`${label} is invalid.`);
}

function assertUnique(values: string[], label: string): void {
  if (new Set(values).size !== values.length) throw new Error(`${label} must be unique.`);
}

function assertStringList(value: unknown, label: string, minimum = 0): asserts value is string[] {
  if (!Array.isArray(value) || value.length < minimum || value.some((item) => typeof item !== 'string' || !item.trim())) {
    throw new Error(`${label} needs at least ${minimum} non-empty values.`);
  }
}

function assertScope(scope: GenerationScope): void {
  if (!scope || typeof scope !== 'object') throw new Error('Generation scope is invalid.');
  assertIdentifier(scope.projectId, 'scope.projectId');
  assertIdentifier(scope.documentId, 'scope.documentId');
}

function assertDesignIntent(intent: GenerationDesignIntent, label: string): void {
  if (!intent || typeof intent !== 'object') throw new Error(`${label} is invalid.`);
  assertText(intent.artDirection, `${label}.artDirection`);
  assertText(intent.compositionIntent, `${label}.compositionIntent`);
  assertText(intent.typographyIntent, `${label}.typographyIntent`);
  assertText(intent.imageStrategy, `${label}.imageStrategy`);
  assertStringList(intent.contentHierarchy, `${label}.contentHierarchy`, 1);
  assertStringList(intent.designAcceptanceCriteria, `${label}.designAcceptanceCriteria`, 2);
  assertStringList(intent.interactionIntents, `${label}.interactionIntents`);
}

function assertArtifact(artifact: GenerationArtifact, label: string): void {
  if (!artifact || typeof artifact !== 'object') throw new Error(`${label} is invalid.`);
  assertIdentifier(artifact.artifactId, `${label}.artifactId`);
  if (!artifactKinds.has(artifact.kind)) throw new Error(`${label}.kind is invalid.`);
  if (!Number.isSafeInteger(artifact.revision) || artifact.revision < 0) throw new Error(`${label}.revision is invalid.`);
  assertTimestamp(artifact.createdAt, `${label}.createdAt`);
  if (artifact.viewportWidth !== undefined && (!Number.isSafeInteger(artifact.viewportWidth) || artifact.viewportWidth < 240 || artifact.viewportWidth > 10000)) {
    throw new Error(`${label}.viewportWidth is invalid.`);
  }
  if (artifact.nodeIds !== undefined) {
    assertStringList(artifact.nodeIds, `${label}.nodeIds`);
    for (const nodeId of artifact.nodeIds) assertIdentifier(nodeId, `${label}.nodeId`);
    assertUnique(artifact.nodeIds, `${label}.nodeIds`);
  }
  if (artifact.uri !== undefined && (typeof artifact.uri !== 'string' || !artifact.uri.trim())) throw new Error(`${label}.uri is invalid.`);
  if (artifact.sha256 !== undefined && !sha256Pattern.test(artifact.sha256)) throw new Error(`${label}.sha256 is invalid.`);
  if (artifact.metadata !== undefined && (!artifact.metadata || typeof artifact.metadata !== 'object' || Array.isArray(artifact.metadata))) {
    throw new Error(`${label}.metadata is invalid.`);
  }
}

function assertAttempt(attempt: GenerationAttempt, label: string): void {
  if (!attempt || typeof attempt !== 'object') throw new Error(`${label} is invalid.`);
  assertIdentifier(attempt.attemptId, `${label}.attemptId`);
  assertIdentifier(attempt.idempotencyKey, `${label}.idempotencyKey`);
  if (!attemptStatuses.has(attempt.status)) throw new Error(`${label}.status is invalid.`);
  if (!Number.isSafeInteger(attempt.baseRevision) || attempt.baseRevision < 0) throw new Error(`${label}.baseRevision is invalid.`);
  if (attempt.committedRevision !== undefined && (!Number.isSafeInteger(attempt.committedRevision) || attempt.committedRevision <= attempt.baseRevision)) {
    throw new Error(`${label}.committedRevision is invalid.`);
  }
  if (attempt.status === 'committed' && attempt.committedRevision === undefined) throw new Error(`${label} needs a committedRevision.`);
  if (attempt.status !== 'committed' && attempt.committedRevision !== undefined) throw new Error(`${label} cannot have a committedRevision.`);
  if (!Array.isArray(attempt.artifacts)) throw new Error(`${label}.artifacts is invalid.`);
  for (const [index, artifact] of attempt.artifacts.entries()) assertArtifact(artifact, `${label}.artifacts[${index}]`);
  assertUnique(attempt.artifacts.map((artifact) => artifact.artifactId), `${label}.artifact IDs`);
  if (attempt.error !== undefined) {
    if (!attempt.error || typeof attempt.error !== 'object') throw new Error(`${label}.error is invalid.`);
    if (!['generation_error', 'scope_violation', 'layout_error', 'render_error', 'quality_reject', 'revision_conflict', 'cancelled'].includes(attempt.error.code)) {
      throw new Error(`${label}.error.code is invalid.`);
    }
    assertText(attempt.error.message, `${label}.error.message`);
    if (typeof attempt.error.retryable !== 'boolean') throw new Error(`${label}.error.retryable is invalid.`);
    assertStringList(attempt.error.issueIds, `${label}.error.issueIds`);
  }
  assertTimestamp(attempt.createdAt, `${label}.createdAt`);
  assertTimestamp(attempt.updatedAt, `${label}.updatedAt`);
}

function transitivelyDependsOn(steps: GenerationStep[], stepId: string, dependencyId: string, visiting = new Set<string>()): boolean {
  if (visiting.has(stepId)) throw new Error(`Generation step dependency cycle includes ${stepId}.`);
  visiting.add(stepId);
  const step = steps.find((candidate) => candidate.stepId === stepId);
  if (!step) return false;
  if (step.dependsOn.includes(dependencyId)) return true;
  return step.dependsOn.some((parentId) => transitivelyDependsOn(steps, parentId, dependencyId, new Set(visiting)));
}

function assertPageRun(page: GenerationPageRun, label: string): void {
  if (!page || typeof page !== 'object') throw new Error(`${label} is invalid.`);
  assertIdentifier(page.pageId, `${label}.pageId`);
  assertText(page.name, `${label}.name`);
  assertText(page.purpose, `${label}.purpose`);
  if (!Number.isSafeInteger(page.order) || page.order < 0) throw new Error(`${label}.order is invalid.`);
  if (!pageStatuses.has(page.status)) throw new Error(`${label}.status is invalid.`);
  if (!Array.isArray(page.steps)) throw new Error(`${label}.steps is invalid.`);
  if (page.status === 'unplanned') {
    if (page.design !== undefined || page.steps.length > 0) throw new Error(`${label} cannot contain a page plan while unplanned.`);
  } else {
    if (page.design === undefined) throw new Error(`${label}.design is required after page planning.`);
    assertDesignIntent(page.design, `${label}.design`);
    if (page.steps.length === 0) throw new Error(`${label}.steps needs at least one step.`);
  }
  assertUnique(page.steps.map((step) => step.stepId), `${label} step IDs`);
  const stepIds = new Set(page.steps.map((step) => step.stepId));
  const idempotencyKeys: string[] = [];
  for (const [index, step] of page.steps.entries()) {
    const stepLabel = `${label}.steps[${index}]`;
    if (!step || typeof step !== 'object') throw new Error(`${stepLabel} is invalid.`);
    assertIdentifier(step.stepId, `${stepLabel}.stepId`);
    if (step.pageId !== page.pageId) throw new Error(`${stepLabel}.pageId must match its page.`);
    assertText(step.title, `${stepLabel}.title`);
    if (!taskKinds.has(step.kind)) throw new Error(`${stepLabel}.kind is invalid.`);
    if (typeof step.required !== 'boolean') throw new Error(`${stepLabel}.required is invalid.`);
    if (!Array.isArray(step.dependsOn)) throw new Error(`${stepLabel}.dependsOn is invalid.`);
    assertUnique(step.dependsOn, `${stepLabel}.dependsOn`);
    for (const dependency of step.dependsOn) {
      if (!stepIds.has(dependency)) throw new Error(`${stepLabel} depends on unknown step ${dependency}.`);
      if (dependency === step.stepId) throw new Error(`${stepLabel} cannot depend on itself.`);
    }
    if (!stepStatuses.has(step.status)) throw new Error(`${stepLabel}.status is invalid.`);
    if (!step.target || typeof step.target !== 'object') throw new Error(`${stepLabel}.target is invalid.`);
    if (step.target.sectionKey !== undefined) assertIdentifier(step.target.sectionKey, `${stepLabel}.target.sectionKey`);
    assertStringList(step.target.nodeIds, `${stepLabel}.target.nodeIds`);
    for (const nodeId of step.target.nodeIds) assertIdentifier(nodeId, `${stepLabel}.target.nodeId`);
    assertUnique(step.target.nodeIds, `${stepLabel}.target.nodeIds`);
    if (!Array.isArray(step.target.viewportWidths) || new Set(step.target.viewportWidths).size !== step.target.viewportWidths.length
      || step.target.viewportWidths.some((width) => !Number.isSafeInteger(width) || width < 240 || width > 10000)) {
      throw new Error(`${stepLabel}.target.viewportWidths is invalid.`);
    }
    if (!Array.isArray(step.attempts)) throw new Error(`${stepLabel}.attempts is invalid.`);
    for (const [attemptIndex, attempt] of step.attempts.entries()) {
      assertAttempt(attempt, `${stepLabel}.attempts[${attemptIndex}]`);
      idempotencyKeys.push(attempt.idempotencyKey);
    }
    assertUnique(step.attempts.map((attempt) => attempt.attemptId), `${stepLabel} attempt IDs`);
    const activeAttempt = step.activeAttemptId ? step.attempts.find((attempt) => attempt.attemptId === step.activeAttemptId) : undefined;
    if (step.activeAttemptId && !activeAttempt) throw new Error(`${stepLabel}.activeAttemptId is missing.`);
    if (['generating', 'validating', 'awaiting-review'].includes(step.status)) {
      if (!activeAttempt || activeAttempt.status !== step.status) throw new Error(`${stepLabel} active attempt status must match the step.`);
    } else if (step.activeAttemptId !== undefined) {
      throw new Error(`${stepLabel} cannot retain an active attempt.`);
    }
    if (step.acceptedAttemptId !== undefined) {
      const accepted = step.attempts.find((attempt) => attempt.attemptId === step.acceptedAttemptId);
      if (!accepted || accepted.status !== 'committed') throw new Error(`${stepLabel}.acceptedAttemptId is invalid.`);
    }
    if (step.status === 'accepted' && !step.acceptedAttemptId) throw new Error(`${stepLabel} needs an accepted attempt.`);
    assertTimestamp(step.createdAt, `${stepLabel}.createdAt`);
    assertTimestamp(step.updatedAt, `${stepLabel}.updatedAt`);
  }
  assertUnique(idempotencyKeys, `${label} attempt idempotency keys`);
  for (const step of page.steps) transitivelyDependsOn(page.steps, step.stepId, '__cycle_probe__');
  if (page.status === 'unplanned') {
    assertTimestamp(page.createdAt, `${label}.createdAt`);
    assertTimestamp(page.updatedAt, `${label}.updatedAt`);
    return;
  }
  const designGates = page.steps.filter((step) => step.kind === 'design-gate');
  const handoffs = page.steps.filter((step) => step.kind === 'handoff');
  if (designGates.length !== 1) throw new Error(`${label} needs exactly one design-gate step.`);
  if (handoffs.length !== 1) throw new Error(`${label} needs exactly one handoff step.`);
  const designGateId = designGates[0].stepId;
  for (const step of page.steps.filter((candidate) => candidate.kind === 'interaction')) {
    if (!transitivelyDependsOn(page.steps, step.stepId, designGateId)) throw new Error(`${label} interaction steps must depend on the design gate.`);
  }
  const handoff = handoffs[0];
  for (const required of page.steps.filter((step) => step.required && step.stepId !== handoff.stepId)) {
    if (!transitivelyDependsOn(page.steps, handoff.stepId, required.stepId)) throw new Error(`${label} handoff must depend on required step ${required.stepId}.`);
  }
  const activeSteps = page.steps.filter((step) => ['generating', 'validating', 'awaiting-review'].includes(step.status));
  if (activeSteps.length > 1) throw new Error(`${label} cannot have more than one active step.`);
  if (page.activeStepId !== undefined) {
    if (activeSteps.length !== 1 || activeSteps[0].stepId !== page.activeStepId) throw new Error(`${label}.activeStepId is invalid.`);
  } else if (activeSteps.length > 0) {
    throw new Error(`${label} needs activeStepId for its active step.`);
  }
  assertTimestamp(page.createdAt, `${label}.createdAt`);
  assertTimestamp(page.updatedAt, `${label}.updatedAt`);
}

export function assertGenerationPlan(value: unknown): asserts value is GenerationPlan {
  if (!value || typeof value !== 'object') throw new Error('Generation plan must be an object.');
  const plan = value as GenerationPlan;
  if (plan.schemaVersion !== 1) throw new Error('Generation plan schemaVersion must be 1.');
  assertIdentifier(plan.planId, 'planId');
  if (!Number.isSafeInteger(plan.revision) || plan.revision < 0) throw new Error('plan.revision is invalid.');
  assertScope(plan.scope);
  if (!['guided', 'auto-current-page', 'review-sensitive'].includes(plan.mode)) throw new Error('plan.mode is invalid.');
  if (!planStatuses.has(plan.status)) throw new Error('plan.status is invalid.');
  if (!plan.sitePlan || typeof plan.sitePlan !== 'object') throw new Error('plan.sitePlan is invalid.');
  assertText(plan.sitePlan.objective, 'plan.sitePlan.objective');
  assertStringList(plan.sitePlan.audience, 'plan.sitePlan.audience', 1);
  if (!Array.isArray(plan.sitePlan.pages) || plan.sitePlan.pages.length === 0) throw new Error('plan.sitePlan.pages needs at least one page.');
  if (!Array.isArray(plan.pageRuns) || plan.pageRuns.length !== plan.sitePlan.pages.length) throw new Error('plan.pageRuns must match sitePlan pages.');
  assertUnique(plan.sitePlan.pages.map((page) => page.pageId), 'sitePlan page IDs');
  assertUnique(plan.pageRuns.map((page) => page.pageId), 'page run IDs');
  assertUnique(plan.pageRuns.map((page) => String(page.order)), 'page run order');
  for (const [index, page] of plan.sitePlan.pages.entries()) {
    assertIdentifier(page.pageId, `plan.sitePlan.pages[${index}].pageId`);
    assertText(page.name, `plan.sitePlan.pages[${index}].name`);
    assertText(page.purpose, `plan.sitePlan.pages[${index}].purpose`);
    if (!Number.isSafeInteger(page.order) || page.order < 0) throw new Error(`plan.sitePlan.pages[${index}].order is invalid.`);
    const run = plan.pageRuns.find((candidate) => candidate.pageId === page.pageId);
    if (!run || run.name !== page.name || run.purpose !== page.purpose || run.order !== page.order) throw new Error(`Page run ${page.pageId} does not match its site plan entry.`);
  }
  for (const [index, page] of plan.pageRuns.entries()) assertPageRun(page, `plan.pageRuns[${index}]`);
  const activePages = plan.pageRuns.filter((page) => ['running', 'paused'].includes(page.status));
  if (activePages.length > 1) throw new Error('Generation plan cannot have more than one active page.');
  if (plan.activePageId !== undefined) {
    if (activePages.length !== 1 || activePages[0].pageId !== plan.activePageId) throw new Error('plan.activePageId is invalid.');
  } else if (activePages.length > 0) {
    throw new Error('Generation plan needs activePageId for its active page.');
  }
  if (plan.status === 'running' && (activePages.length !== 1 || activePages[0].status !== 'running')) throw new Error('A running plan needs one running page.');
  if (plan.status === 'paused' && (activePages.length !== 1 || activePages[0].status !== 'paused')) throw new Error('A paused plan needs one paused page.');
  if (['draft', 'ready', 'completed', 'cancelled'].includes(plan.status) && plan.activePageId !== undefined) throw new Error(`${plan.status} plan cannot have an active page.`);
  if (plan.status === 'completed' && plan.pageRuns.some((page) => page.status !== 'completed')) throw new Error('A completed plan needs every page completed.');
  assertTimestamp(plan.createdAt, 'plan.createdAt');
  assertTimestamp(plan.updatedAt, 'plan.updatedAt');
}

export function assertGenerationScopeMatches(actual: GenerationScope, expected: GenerationScope): void {
  assertScope(actual);
  assertScope(expected);
  if (actual.projectId !== expected.projectId || actual.documentId !== expected.documentId) {
    throw new Error('Generation scope does not match the active ChatOS project and document.');
  }
}

export function createGenerationPlan(input: CreateGenerationPlanInput, createdAt = new Date().toISOString()): GenerationPlan {
  assertTimestamp(createdAt, 'createdAt');
  assertIdentifier(input.planId, 'input.planId');
  assertScope(input.scope);
  assertText(input.objective, 'input.objective');
  assertStringList(input.audience, 'input.audience', 1);
  if (!Array.isArray(input.pages) || input.pages.length === 0) throw new Error('Generation plan needs at least one page.');
  assertUnique(input.pages.map((page) => page.pageId), 'input page IDs');
  const pageRuns: GenerationPageRun[] = input.pages.map((page, order) => {
    assertIdentifier(page.pageId, `input.pages[${order}].pageId`);
    assertText(page.name, `input.pages[${order}].name`);
    assertText(page.purpose, `input.pages[${order}].purpose`);
    assertDesignIntent(page.design, `input.pages[${order}].design`);
    if (!Array.isArray(page.steps) || page.steps.length === 0) throw new Error(`input.pages[${order}].steps needs at least one step.`);
    assertUnique(page.steps.map((step) => step.stepId), `input.pages[${order}] step IDs`);
    return {
      pageId: page.pageId,
      name: page.name,
      purpose: page.purpose,
      order,
      design: structuredClone(page.design),
      status: 'planned',
      steps: page.steps.map((step) => ({
        stepId: step.stepId,
        pageId: page.pageId,
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
        createdAt,
        updatedAt: createdAt
      })),
      createdAt,
      updatedAt: createdAt
    };
  });
  const plan: GenerationPlan = {
    schemaVersion: 1,
    planId: input.planId,
    revision: 0,
    scope: structuredClone(input.scope),
    mode: input.mode ?? 'auto-current-page',
    status: 'draft',
    sitePlan: {
      objective: input.objective,
      audience: [...input.audience],
      pages: pageRuns.map((page) => ({ pageId: page.pageId, name: page.name, purpose: page.purpose, order: page.order }))
    },
    pageRuns,
    createdAt,
    updatedAt: createdAt
  };
  assertGenerationPlan(plan);
  return plan;
}

export function createGenerationSitePlan(input: CreateGenerationSitePlanInput, createdAt = new Date().toISOString()): GenerationPlan {
  assertTimestamp(createdAt, 'createdAt');
  assertIdentifier(input.planId, 'input.planId');
  assertScope(input.scope);
  assertText(input.objective, 'input.objective');
  assertStringList(input.audience, 'input.audience', 1);
  if (!Array.isArray(input.pages) || input.pages.length === 0) throw new Error('Generation site plan needs at least one page.');
  assertUnique(input.pages.map((page) => page.pageId), 'input page IDs');
  const pageRuns: GenerationPageRun[] = input.pages.map((page, order) => {
    assertIdentifier(page.pageId, `input.pages[${order}].pageId`);
    assertText(page.name, `input.pages[${order}].name`);
    assertText(page.purpose, `input.pages[${order}].purpose`);
    return {
      pageId: page.pageId,
      name: page.name,
      purpose: page.purpose,
      order,
      status: 'unplanned',
      steps: [],
      createdAt,
      updatedAt: createdAt
    };
  });
  const plan: GenerationPlan = {
    schemaVersion: 1,
    planId: input.planId,
    revision: 0,
    scope: structuredClone(input.scope),
    mode: input.mode ?? 'auto-current-page',
    status: 'draft',
    sitePlan: {
      objective: input.objective,
      audience: [...input.audience],
      pages: pageRuns.map((page) => ({ pageId: page.pageId, name: page.name, purpose: page.purpose, order: page.order }))
    },
    pageRuns,
    createdAt,
    updatedAt: createdAt
  };
  assertGenerationPlan(plan);
  return plan;
}
