import { assertExecutionPlan, type ExecutionPlan } from './schema.js';

const identifier = { type: 'string', pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' } as const;
const text = { type: 'string', maxLength: 100_000 } as const;
const stringList = { type: 'array', maxItems: 5_000, items: { type: 'string', maxLength: 10_000 } } as const;
const point = {
  type: 'object', properties: { x: { type: 'number' }, y: { type: 'number' } },
  required: ['x', 'y'], additionalProperties: false
} as const;

export const executionPlanInputSchema = {
  type: 'object',
  description: 'Execution plan business content. Revision and updatedAt are server-managed. Omit statusEvents; executor history is preserved. Omitted positions/viewport preserve the current layout.',
  properties: {
    status: { type: 'string', enum: ['draft', 'review', 'approved'] },
    basedOnDesignRevision: { type: 'integer', minimum: 0 },
    objective: text,
    tasks: {
      type: 'array', maxItems: 2_000,
      items: {
        type: 'object',
        properties: {
          id: identifier, moduleId: identifier, title: { ...text, minLength: 1 },
          description: text, type: { type: 'string', enum: ['task', 'review', 'milestone'] },
          phase: text, dependsOn: stringList,
          status: { type: 'string', enum: ['planned', 'in_progress', 'blocked', 'done', 'cancelled'], description: 'New tasks must be planned. Existing execution status is preserved by the server.' },
          blockedReason: text, requirementIds: stringList, designSectionIds: stringList,
          deliverables: stringList, acceptanceCriteria: stringList, sourceReferences: stringList
        },
        required: ['id', 'moduleId', 'title', 'description', 'type', 'phase', 'dependsOn', 'status', 'requirementIds', 'designSectionIds', 'deliverables', 'acceptanceCriteria', 'sourceReferences'],
        additionalProperties: false
      }
    },
    statusEvents: { type: 'array', maxItems: 0, description: 'Executor-owned. Omit or send an empty array; existing history is never replaced.' },
    positions: { type: 'object', propertyNames: identifier, additionalProperties: point },
    viewport: {
      type: 'object', properties: { ...point.properties, zoom: { type: 'number' } },
      required: ['x', 'y', 'zoom'], additionalProperties: false
    },
    revision: { type: 'integer', minimum: 0, description: 'Optional legacy field; ignored and assigned by the server.' },
    updatedAt: { type: 'string', description: 'Optional legacy field; ignored and assigned by the server.' }
  },
  required: ['status', 'basedOnDesignRevision', 'objective', 'tasks'],
  additionalProperties: false
} as const;

export function normalizeAuthoringPlan(value: unknown, current: ExecutionPlan): ExecutionPlan {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('executionPlan must be an object.');
  const input = value as Record<string, unknown>;
  const plan = {
    ...input,
    revision: current.revision + 1,
    updatedAt: new Date().toISOString(),
    statusEvents: input.statusEvents === undefined ? [] : input.statusEvents,
    positions: input.positions === undefined ? structuredClone(current.positions) : input.positions,
    viewport: input.viewport === undefined ? structuredClone(current.viewport) : input.viewport
  } as ExecutionPlan;
  // Validate before the authoring protections dereference collections or task fields.
  assertExecutionPlan(plan);
  return plan;
}
