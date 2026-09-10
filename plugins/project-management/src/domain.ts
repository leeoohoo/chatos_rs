import { z } from 'zod';

const id = z.string().min(1).max(128).regex(/^[A-Za-z0-9_-]+$/u);
const title = z.string().trim().min(1).max(240);
const content = z.string().max(200_000);
const nonEmptyContent = content.refine(value => value.trim().length > 0, 'Document content cannot be empty');
const uniqueIds = z.array(id).max(500).refine(value => new Set(value).size === value.length, 'Duplicate identifiers');
const base = {
  requestId: id,
  expectedRevision: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER - 1)
};

export const requirementFields = {
  title,
  detail: content,
  acceptanceCriteria: content,
  parentId: id.nullable(),
  status: z.enum(['draft', 'reviewing', 'approved', 'accepted', 'archived'])
};

export const workItemFields = {
  title,
  detail: content,
  acceptanceCriteria: content,
  requirementId: id,
  status: z.enum(['todo', 'ready', 'accepted', 'archived'])
};

export const documentKinds = ['technical', 'design', 'api', 'decision', 'notes'] as const;
export const documentFormats = ['markdown', 'svg'] as const;
export const documentStatuses = ['draft', 'published', 'archived'] as const;

const documentFields = {
  title,
  kind: z.enum(documentKinds),
  format: z.enum(documentFormats),
  status: z.enum(documentStatuses),
  requirementIds: uniqueIds.refine(value => value.length > 0, 'Link at least one requirement'),
  content: nonEmptyContent
};

export const commandSchema = z.discriminatedUnion('operation', [
  z.object({ ...base, operation: z.literal('requirement.create'), ...requirementFields }).strict(),
  z.object({ ...base, operation: z.literal('requirement.update'), id, ...requirementFields, prerequisiteIds: uniqueIds.optional() }).strict(),
  z.object({ ...base, operation: z.literal('work_item.create'), ...workItemFields }).strict(),
  z.object({ ...base, operation: z.literal('work_item.update'), id, ...workItemFields, prerequisiteIds: uniqueIds.optional() }).strict(),
  z.object({ ...base, operation: z.literal('document.create'), ...documentFields }).strict(),
  z.object({ ...base, operation: z.literal('document.revise'), id, ...documentFields }).strict(),
  z.object({ ...base, operation: z.literal('dependencies.set'), kind: z.enum(['requirement', 'work_item']), id, prerequisiteIds: uniqueIds }).strict(),
  z.object({ ...base, operation: z.literal('plan.create'), title, workItemIds: uniqueIds.refine(value => value.length > 0, 'Select at least one work item') }).strict(),
  z.object({ ...base, operation: z.literal('plan.approve'), id }).strict(),
  z.object({ ...base, operation: z.literal('execution.prepare'), planId: id, title }).strict(),
  z.object({ ...base, operation: z.literal('execution.approve'), id }).strict(),
  z.object({
    ...base,
    operation: z.literal('execution.link_batch'),
    intentId: id,
    batchId: z.string().trim().min(1).max(512),
    revision: z.string().trim().min(1).max(256),
    tasks: z.array(z.object({ clientRef: id, taskId: z.string().trim().min(1).max(512) }).strict())
      .min(1).max(100)
      .refine(value => new Set(value.map(item => item.clientRef)).size === value.length, 'Duplicate client references')
      .refine(value => new Set(value.map(item => item.taskId)).size === value.length, 'Duplicate task references')
  }).strict(),
  z.object({
    ...base,
    operation: z.literal('execution.link'),
    intentId: id,
    referenceType: z.enum(['batch', 'task', 'run']),
    externalId: z.string().trim().min(1).max(512),
    url: z.string().url().max(2_048).nullable(),
    revision: z.string().trim().min(1).max(256).nullable()
  }).strict()
]);

export type Command = z.infer<typeof commandSchema>;
export type Requirement = z.infer<ReturnType<typeof requirementShape>> & { id: string };
function requirementShape() { return z.object(requirementFields); }
export type WorkItem = z.infer<ReturnType<typeof workItemShape>> & { id: string };
function workItemShape() { return z.object(workItemFields); }
export type Dependency = { kind: 'requirement' | 'work_item'; id: string; prerequisiteId: string };

export type PlanningDocument = {
  id: string;
  title: string;
  kind: typeof documentKinds[number];
  format: typeof documentFormats[number];
  status: typeof documentStatuses[number];
  currentVersion: number;
  createdAt: string;
  updatedAt: string;
};

export type DocumentVersion = {
  id: string;
  documentId: string;
  version: number;
  content: string;
  contentSha256: string;
  createdAt: string;
};

export type DocumentRequirementLink = { documentId: string; requirementId: string };
export type FrozenDocument = PlanningDocument & { pinnedVersion: DocumentVersion; requirementIds: string[] };

export type Plan = {
  id: string;
  title: string;
  version: number;
  status: 'draft' | 'approved';
  createdAt: string;
  requirements: Requirement[];
  documents: FrozenDocument[];
  workItems: WorkItem[];
  dependencies: Dependency[];
};

export type ExecutionIntent = {
  id: string;
  planId: string;
  title: string;
  status: 'prepared' | 'approved';
  createdAt: string;
  approvedAt: string | null;
};

/** Opaque Task Runner identity only. Runtime state is deliberately absent. */
export type ExecutionReference = {
  id: string;
  intentId: string;
  provider: 'task-runner';
  referenceType: 'batch' | 'task' | 'run';
  externalId: string;
  url: string | null;
  revision: string | null;
  createdAt: string;
};

export type ScopeGraph = {
  kind: 'requirement' | 'work_item';
  rootId: string;
  ancestors: string[];
  descendants: string[];
  prerequisites: string[];
  dependants: string[];
  requirementIds: string[];
  workItemIds: string[];
  documentIds: string[];
  nodes: { id: string; kind: 'requirement' | 'work_item'; title: string; relation: string }[];
  edges: { from: string; to: string; relation: 'contains' | 'requires' | 'implements' }[];
};

export class DomainError extends Error {
  constructor(public code: string, message: string) { super(message); }
}

export function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new DomainError('invalid_operation', message);
}

export function assertAcyclic(nodes: string[], edges: [string, string][]): void {
  const graph = new Map(nodes.map(nodeId => [nodeId, [] as string[]]));
  for (const [nodeId, prerequisite] of edges) {
    assert(graph.has(nodeId) && graph.has(prerequisite), 'Dependency target is outside the bound scope');
    graph.get(nodeId)!.push(prerequisite);
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  function visit(nodeId: string) {
    assert(!visiting.has(nodeId), 'Dependency or requirement hierarchy contains a cycle');
    if (visited.has(nodeId)) return;
    visiting.add(nodeId);
    for (const next of graph.get(nodeId)!) visit(next);
    visiting.delete(nodeId);
    visited.add(nodeId);
  }
  nodes.forEach(visit);
}
