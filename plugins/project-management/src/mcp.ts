import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { z } from 'zod';
import { zodToJsonSchema } from 'zod-to-json-schema';
import { PlanningStore, commandSchema, DomainError } from './store.js';

const store = new PlanningStore();
const server = new Server(
  { name: 'chatos-project-management', version: '0.2.0' },
  { capabilities: { tools: {} } }
);

const entityId = z.string().min(1).max(128);
const getSchema = z.object({
  kind: z.enum(['requirement', 'work_item', 'document', 'document_version', 'plan', 'execution_intent', 'execution_reference']),
  id: entityId
}).strict();
const scopeSchema = z.object({ kind: z.enum(['requirement', 'work_item']), id: entityId }).strict();
const schemas = {
  planning_read: z.object({}).strict(),
  planning_get: getSchema,
  planning_scope: scopeSchema,
  planning_change: commandSchema
};
const descriptions = {
  planning_read: 'Read the bound project planning index and revision. Project identity comes only from the host context.',
  planning_get: 'Read one requirement, work item, independent versioned document, frozen plan, or opaque Task Runner execution reference.',
  planning_scope: 'Compute hierarchy paths, descendants, prerequisites, dependants and the complete related requirement/work-item/document graph.',
  planning_change: 'Change plugin-owned planning data with CAS and idempotency. No project CRUD, Git operation, task execution or runtime status mutation is available.'
};

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: Object.entries(schemas).map(([name, schema]) => ({
    name,
    description: descriptions[name as keyof typeof descriptions],
    inputSchema: { ...zodToJsonSchema(schema, { $refStrategy: 'none' }), type: 'object' as const },
    annotations: {
      readOnlyHint: name !== 'planning_change',
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/riskLevel': name === 'planning_change' ? 'medium' : 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 30_000,
      'chatos/toolResultMaxChars': 300_000,
      'chatos/skillGate': { allOf: ['project-management'] }
    }
  }))
}));

server.setRequestHandler(CallToolRequestSchema, async request => {
  try {
    const name = request.params.name;
    let result: unknown;
    if (name === 'planning_change') {
      result = store.mutate(request.params.arguments);
    } else if (name === 'planning_scope') {
      const args = scopeSchema.parse(request.params.arguments);
      result = store.scope(args.kind, args.id);
    } else if (name === 'planning_read') {
      schemas.planning_read.parse(request.params.arguments ?? {});
      const state = store.read();
      result = {
        revision: state.revision,
        requirements: state.requirements.map(({ detail, acceptanceCriteria, ...item }) => ({
          ...item,
          childCount: state.requirements.filter(value => value.parentId === item.id).length,
          workItemCount: state.workItems.filter(value => value.requirementId === item.id).length,
          documentCount: state.documentLinks.filter(value => value.requirementId === item.id).length
        })),
        workItems: state.workItems.map(({ detail, acceptanceCriteria, ...item }) => item),
        documents: state.documents.map(item => ({
          ...item,
          requirementIds: state.documentLinks.filter(value => value.documentId === item.id).map(value => value.requirementId)
        })),
        dependencies: state.dependencies,
        plans: state.plans.map(({ id, title, version, status, createdAt }) => ({ id, title, version, status, createdAt })),
        executionIntents: state.executionIntents,
        executionReferences: state.executionReferences
      };
    } else if (name === 'planning_get') {
      const args = getSchema.parse(request.params.arguments);
      const state = store.read();
      switch (args.kind) {
        case 'requirement': result = state.requirements.find(value => value.id === args.id); break;
        case 'work_item': result = state.workItems.find(value => value.id === args.id); break;
        case 'document': result = store.getDocument(args.id); break;
        case 'document_version': result = state.documentVersions.find(value => value.id === args.id); break;
        case 'plan': result = state.plans.find(value => value.id === args.id); break;
        case 'execution_intent': result = state.executionIntents.find(value => value.id === args.id); break;
        case 'execution_reference': result = state.executionReferences.find(value => value.id === args.id); break;
      }
      if (!result) throw new DomainError('not_found', 'Planning item not found in the bound project');
    } else {
      throw new DomainError('unknown_tool', 'Unknown planning tool');
    }
    return { content: [{ type: 'text' as const, text: JSON.stringify(result) }] };
  } catch (error) {
    const text = error instanceof DomainError
      ? `${error.code}: ${error.message}`
      : error instanceof z.ZodError
        ? 'invalid_arguments: Unknown or invalid planning fields'
        : 'storage_error: Local planning operation failed';
    return { isError: true, content: [{ type: 'text' as const, text }] };
  }
});

await server.connect(new StdioServerTransport());
for (const signal of ['SIGTERM', 'SIGINT'] as const) {
  process.on(signal, () => { store.close(); process.exit(0); });
}
