import { createHash, createHmac, randomBytes, randomUUID, timingSafeEqual } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertIdentifier, type DiagramDocument, type DiagramKind } from './schema.js';
import type { DiagramQualityProfile } from './quality.js';

const GENERATION_PERMIT_TTL_MS = 30 * 60 * 1000;
const signingSecret = randomBytes(32);

export interface DiagramGuideModeContract {
  qualityProfile: DiagramQualityProfile;
  maxPrimaryItems: number;
  maxEdges: number;
  minStructureItems: number;
  maxStructureItems?: number;
}

export interface DiagramGuideContract {
  schemaVersion: 1;
  guideId: string;
  guideVersion: string;
  kind: DiagramKind;
  defaultMode: string;
  modes: Record<string, DiagramGuideModeContract>;
  checklist: string[];
}

export interface GenerationPlan {
  goal: string;
  scope: string;
  excludedDetails: string[];
  estimatedPrimaryItemCount: number;
  estimatedEdgeCount: number;
  structure: string[];
  splitPlan: string[];
  splitRationale: string;
  checklistAcknowledgements: string[];
}

export interface GenerationPermitPayload {
  tokenType: 'generation-permit';
  permitId: string;
  guideId: string;
  guideVersion: string;
  guideHash: string;
  kind: DiagramKind;
  mode: string;
  artifactKey: string;
  operation: 'create' | 'revise';
  documentId?: string;
  title: string;
  planHash: string;
  qualityProfile: DiagramQualityProfile;
  maxPrimaryItems: number;
  maxEdges: number;
  minStructureItems: number;
  maxStructureItems?: number;
  scopeFingerprint: string;
  issuedAt: string;
  expiresAt: string;
}

export interface GenerationContractIssue {
  code: string;
  message: string;
}

function normalizeText(value: string): string {
  return value.trim().replace(/\s+/g, ' ');
}

function hashText(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function canonicalize(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalize);
  if (!value || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value as Record<string, unknown>)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => [key, canonicalize(item)])
  );
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(canonicalize(value));
}

function signPayload(payload: object): string {
  const encoded = Buffer.from(canonicalJson(payload), 'utf8').toString('base64url');
  const signature = createHmac('sha256', signingSecret).update(encoded).digest('base64url');
  return `${encoded}.${signature}`;
}

function verifySignedPayload<T extends { tokenType: string; expiresAt: string }>(token: string, tokenType: T['tokenType']): T {
  const [encoded, signature, extra] = token.split('.');
  if (!encoded || !signature || extra !== undefined) throw new Error('Generation authorization token is malformed.');
  const expected = createHmac('sha256', signingSecret).update(encoded).digest();
  let actual: Buffer;
  try {
    actual = Buffer.from(signature, 'base64url');
  } catch {
    throw new Error('Generation authorization signature is malformed.');
  }
  if (actual.length !== expected.length || !timingSafeEqual(actual, expected)) {
    throw new Error('Generation authorization signature is invalid.');
  }
  let payload: T;
  try {
    payload = JSON.parse(Buffer.from(encoded, 'base64url').toString('utf8')) as T;
  } catch {
    throw new Error('Generation authorization payload is invalid.');
  }
  if (payload.tokenType !== tokenType) throw new Error(`Expected a ${tokenType} token.`);
  const expiresAt = Date.parse(payload.expiresAt);
  if (!Number.isFinite(expiresAt) || expiresAt <= Date.now()) throw new Error('Generation authorization token has expired.');
  return payload;
}

function validateContract(value: unknown, expectedKind: DiagramKind): DiagramGuideContract {
  if (!value || typeof value !== 'object') throw new Error(`Generation guide contract for ${expectedKind} is invalid.`);
  const contract = value as Partial<DiagramGuideContract>;
  if (contract.schemaVersion !== 1 || contract.kind !== expectedKind) throw new Error(`Generation guide contract kind mismatch for ${expectedKind}.`);
  if (typeof contract.guideId !== 'string' || typeof contract.guideVersion !== 'string' || typeof contract.defaultMode !== 'string') {
    throw new Error(`Generation guide contract metadata for ${expectedKind} is incomplete.`);
  }
  if (!contract.modes || typeof contract.modes !== 'object' || !contract.modes[contract.defaultMode]) {
    throw new Error(`Generation guide contract for ${expectedKind} has no valid default mode.`);
  }
  if (!Array.isArray(contract.checklist) || contract.checklist.length === 0 || contract.checklist.some((item) => typeof item !== 'string')) {
    throw new Error(`Generation guide checklist for ${expectedKind} is invalid.`);
  }
  for (const [modeName, mode] of Object.entries(contract.modes)) {
    if (!mode || !['balanced', 'architecture-overview', 'architecture-detail'].includes(mode.qualityProfile)) {
      throw new Error(`Generation guide mode ${modeName} has an invalid quality profile.`);
    }
    for (const [label, count] of Object.entries({
      maxPrimaryItems: mode.maxPrimaryItems,
      maxEdges: mode.maxEdges,
      minStructureItems: mode.minStructureItems
    })) {
      if (!Number.isSafeInteger(count) || count < 0) throw new Error(`Generation guide mode ${modeName} has invalid ${label}.`);
    }
    if (mode.maxStructureItems !== undefined
      && (!Number.isSafeInteger(mode.maxStructureItems) || mode.maxStructureItems < mode.minStructureItems)) {
      throw new Error(`Generation guide mode ${modeName} has invalid maxStructureItems.`);
    }
  }
  return contract as DiagramGuideContract;
}

function guideDirectories(kind: DiagramKind): string[] {
  const currentDirectory = path.dirname(fileURLToPath(import.meta.url));
  const folder = `diagram-${kind}`;
  return [
    path.resolve(currentDirectory, '..', 'skills', folder),
    path.resolve(process.cwd(), 'skills', folder),
    path.resolve(process.cwd(), 'plugins', 'diagram-studio', 'skills', folder)
  ];
}

async function readGuideFiles(kind: DiagramKind): Promise<{
  directory: string;
  instructions: string;
  examples: string;
  contract: DiagramGuideContract;
}> {
  let lastError: unknown;
  for (const directory of guideDirectories(kind)) {
    try {
      const [instructions, examples, contractBody] = await Promise.all([
        fs.readFile(path.join(directory, 'SKILL.md'), 'utf8'),
        fs.readFile(path.join(directory, 'references', 'examples.md'), 'utf8'),
        fs.readFile(path.join(directory, 'contract.json'), 'utf8')
      ]);
      return {
        directory,
        instructions,
        examples,
        contract: validateContract(JSON.parse(contractBody), kind)
      };
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(`Generation guide for ${kind} could not be loaded: ${lastError instanceof Error ? lastError.message : String(lastError)}`);
}

export function runtimeScopeFingerprint(rootDirectory: string): string {
  return hashText(canonicalJson({
    scope: process.env.CHATOS_CONTEXT_SCOPE ?? 'device',
    scopeId: process.env.CHATOS_CONTEXT_SCOPE_ID ?? '',
    projectId: process.env.CHATOS_PROJECT_ID ?? '',
    workspaceId: process.env.CHATOS_WORKSPACE_ID ?? '',
    userId: process.env.CHATOS_USER_ID ?? process.env.CHATOS_ACCOUNT_ID ?? '',
    dataDirectory: path.resolve(rootDirectory)
  }));
}

export async function prepareGenerationPermit(argumentsValue: {
  kind: DiagramKind;
  mode?: string;
  artifactKey: string;
  operation: 'create' | 'revise';
  documentId?: string;
  title: string;
  plan: GenerationPlan;
  scopeFingerprint: string;
}): Promise<{ generationPermit: string; permit: GenerationPermitPayload; planHash: string }> {
  assertIdentifier(argumentsValue.artifactKey, 'artifactKey');
  const title = normalizeText(argumentsValue.title);
  if (!title || title.length > 240) throw new Error('Generation title must contain 1 to 240 characters.');
  if (argumentsValue.operation !== 'create' && argumentsValue.operation !== 'revise') throw new Error('Generation operation must be create or revise.');
  if (argumentsValue.operation === 'revise') {
    if (!argumentsValue.documentId) throw new Error('documentId is required when revising a diagram.');
    assertIdentifier(argumentsValue.documentId, 'documentId');
  } else if (argumentsValue.documentId) {
    throw new Error('documentId is only valid for a revise operation.');
  }
  const plan = validatePlan(argumentsValue.plan);
  const guideFiles = await readGuideFiles(argumentsValue.kind);
  const { contract } = guideFiles;
  const guideHash = hashText(canonicalJson({
    instructions: guideFiles.instructions,
    examples: guideFiles.examples,
    contract
  }));
  const mode = argumentsValue.mode?.trim() || contract.defaultMode;
  const modeContract = contract.modes[mode];
  if (!modeContract) throw new Error(`Unsupported ${argumentsValue.kind} guide mode ${mode}. Available modes: ${Object.keys(contract.modes).join(', ')}.`);
  validatePlanAgainstContract(plan, contract, modeContract);
  const planHash = hashText(canonicalJson(plan));
  const issuedAt = new Date();
  const permit: GenerationPermitPayload = {
    tokenType: 'generation-permit',
    permitId: randomUUID(),
    guideId: contract.guideId,
    guideVersion: contract.guideVersion,
    guideHash,
    kind: contract.kind,
    mode,
    artifactKey: argumentsValue.artifactKey,
    operation: argumentsValue.operation,
    ...(argumentsValue.documentId ? { documentId: argumentsValue.documentId } : {}),
    title,
    planHash,
    qualityProfile: modeContract.qualityProfile,
    maxPrimaryItems: modeContract.maxPrimaryItems,
    maxEdges: modeContract.maxEdges,
    minStructureItems: modeContract.minStructureItems,
    ...(modeContract.maxStructureItems === undefined ? {} : { maxStructureItems: modeContract.maxStructureItems }),
    scopeFingerprint: argumentsValue.scopeFingerprint,
    issuedAt: issuedAt.toISOString(),
    expiresAt: new Date(issuedAt.getTime() + GENERATION_PERMIT_TTL_MS).toISOString()
  };
  return { generationPermit: signPayload(permit), permit, planHash };
}

function validatePlan(value: unknown): GenerationPlan {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Generation plan must be an object.');
  const plan = value as Partial<GenerationPlan>;
  for (const [label, textValue] of Object.entries({ goal: plan.goal, scope: plan.scope, splitRationale: plan.splitRationale })) {
    if (typeof textValue !== 'string' || normalizeText(textValue).length === 0 || textValue.length > 4000) {
      throw new Error(`Generation plan ${label} must contain 1 to 4000 characters.`);
    }
  }
  for (const [label, list, minimum] of [
    ['excludedDetails', plan.excludedDetails, 1],
    ['structure', plan.structure, 0],
    ['splitPlan', plan.splitPlan, 0],
    ['checklistAcknowledgements', plan.checklistAcknowledgements, 1]
  ] as const) {
    if (!Array.isArray(list) || list.length < minimum || list.length > 100 || list.some((item) => typeof item !== 'string' || normalizeText(item).length === 0)) {
      throw new Error(`Generation plan ${label} is invalid.`);
    }
  }
  for (const [label, count] of Object.entries({
    estimatedPrimaryItemCount: plan.estimatedPrimaryItemCount,
    estimatedEdgeCount: plan.estimatedEdgeCount
  })) {
    if (!Number.isSafeInteger(count) || (count ?? -1) < 0) throw new Error(`Generation plan ${label} must be a non-negative integer.`);
  }
  return {
    goal: normalizeText(plan.goal as string),
    scope: normalizeText(plan.scope as string),
    excludedDetails: (plan.excludedDetails as string[]).map(normalizeText),
    estimatedPrimaryItemCount: plan.estimatedPrimaryItemCount as number,
    estimatedEdgeCount: plan.estimatedEdgeCount as number,
    structure: (plan.structure as string[]).map(normalizeText),
    splitPlan: (plan.splitPlan as string[]).map(normalizeText),
    splitRationale: normalizeText(plan.splitRationale as string),
    checklistAcknowledgements: (plan.checklistAcknowledgements as string[]).map(normalizeText)
  };
}

function validatePlanAgainstContract(
  plan: GenerationPlan,
  contract: DiagramGuideContract,
  mode: DiagramGuideModeContract
): void {
  if (plan.estimatedPrimaryItemCount > mode.maxPrimaryItems) {
    throw new Error(`Plan estimates ${plan.estimatedPrimaryItemCount} primary items, exceeding the ${contract.kind}/${mode.qualityProfile} budget of ${mode.maxPrimaryItems}. Split the diagram.`);
  }
  if (plan.estimatedEdgeCount > mode.maxEdges) {
    throw new Error(`Plan estimates ${plan.estimatedEdgeCount} edges, exceeding the budget of ${mode.maxEdges}. Aggregate or split the diagram.`);
  }
  if (plan.structure.length < mode.minStructureItems) {
    throw new Error(`Plan must identify at least ${mode.minStructureItems} structure item(s) for this guide mode.`);
  }
  if (mode.maxStructureItems !== undefined && plan.structure.length > mode.maxStructureItems) {
    throw new Error(`Plan contains ${plan.structure.length} structure items, exceeding the limit of ${mode.maxStructureItems}. Merge equivalent roles or split the diagram.`);
  }
  const acknowledgements = new Set(plan.checklistAcknowledgements);
  const missing = contract.checklist.filter((item) => !acknowledgements.has(item));
  const unknown = [...acknowledgements].filter((item) => !contract.checklist.includes(item));
  if (missing.length > 0 || unknown.length > 0 || acknowledgements.size !== plan.checklistAcknowledgements.length) {
    throw new Error(`Checklist acknowledgement mismatch. Missing: ${missing.join(', ') || 'none'}; unknown or duplicate: ${unknown.join(', ') || (acknowledgements.size !== plan.checklistAcknowledgements.length ? 'duplicate values' : 'none')}.`);
  }
}

export function verifyGenerationPermit(
  token: string,
  expected: {
    scopeFingerprint: string;
    kind: DiagramKind;
    artifactKey: string;
    title?: string;
    operation?: 'create' | 'revise';
    documentId?: string;
  }
): GenerationPermitPayload {
  const permit = verifySignedPayload<GenerationPermitPayload>(token, 'generation-permit');
  if (permit.scopeFingerprint !== expected.scopeFingerprint) throw new Error('Generation permit belongs to a different ChatOS user or project scope.');
  if (permit.kind !== expected.kind) throw new Error(`Generation permit is for ${permit.kind}, not ${expected.kind}.`);
  if (permit.artifactKey !== expected.artifactKey) throw new Error('Generation permit artifactKey does not match this deliverable.');
  if (expected.title !== undefined && permit.title !== normalizeText(expected.title)) throw new Error('Generation permit title does not match this deliverable.');
  if (expected.operation !== undefined && permit.operation !== expected.operation) throw new Error('Generation permit operation does not match this write.');
  if (expected.documentId !== undefined && permit.documentId !== expected.documentId) throw new Error('Generation permit documentId does not match this revision.');
  return permit;
}

export function inspectGenerationContract(
  document: DiagramDocument,
  permit: GenerationPermitPayload
): GenerationContractIssue[] {
  const primaryItems = document.kind === 'sequence'
    ? document.nodes.filter((node) => node.data.shape === 'lifeline').length
    : document.nodes.filter((node) => !['container', 'lane', 'activation', 'fragment', 'text'].includes(node.data.shape)).length;
  const structureItems = document.kind === 'sequence'
    ? document.nodes.filter((node) => node.data.shape === 'lifeline').length
    : document.kind === 'swimlane'
      ? document.nodes.filter((node) => node.data.shape === 'lane').length
      : document.kind === 'architecture' || document.kind === 'topology'
        ? document.nodes.filter((node) => node.data.shape === 'container').length
        : undefined;
  const issues: GenerationContractIssue[] = [];
  if (primaryItems > permit.maxPrimaryItems) {
    issues.push({ code: 'generation_primary_item_budget_exceeded', message: `${primaryItems} primary items exceed the permit budget of ${permit.maxPrimaryItems}; split the diagram.` });
  }
  if (document.edges.length > permit.maxEdges) {
    issues.push({ code: 'generation_edge_budget_exceeded', message: `${document.edges.length} edges exceed the permit budget of ${permit.maxEdges}; aggregate or split the diagram.` });
  }
  if (structureItems !== undefined && structureItems < permit.minStructureItems) {
    issues.push({ code: 'generation_structure_missing', message: `${structureItems} structure items are present, but this guide requires at least ${permit.minStructureItems}.` });
  }
  if (structureItems !== undefined && permit.maxStructureItems !== undefined && structureItems > permit.maxStructureItems) {
    issues.push({ code: 'generation_structure_budget_exceeded', message: `${structureItems} structure items exceed the permit limit of ${permit.maxStructureItems}.` });
  }
  return issues;
}
