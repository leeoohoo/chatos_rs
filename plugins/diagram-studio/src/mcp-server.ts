import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import {
  DiagramDocumentStore,
  RevisionConflictError,
  writeExportArtifact
} from './document-store.js';
import {
  assertDiagramDocument,
  diagramSummary,
  type DiagramKind,
  type DiagramPatchOperation
} from './schema.js';
import { plantUmlToDiagram } from './plantuml.js';

const SERVER_NAME = 'chatos-diagram-studio';
const SERVER_VERSION = '0.1.6';
const store = new DiagramDocumentStore();

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 80_000
};

const TOOL_DEFINITIONS = [
  {
    name: 'diagram_list_documents',
    description: 'List diagram documents in the current ChatOS scope, optionally limited to one Diagram Studio project.',
    inputSchema: {
      type: 'object',
      properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 } },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_list_projects',
    description: 'List Diagram Studio projects in the current user and ChatOS project or shared scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'diagram_create_project',
    description: 'Create a Diagram Studio project inside the current isolated ChatOS scope.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', minLength: 1, maxLength: 240 },
        description: { type: 'string', maxLength: 4000 }
      },
      required: ['name'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_get_project',
    description: 'Read a Diagram Studio project and its diagram summaries.',
    inputSchema: {
      type: 'object',
      properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_update_project',
    description: 'Rename or update the description of a Diagram Studio project.',
    inputSchema: {
      type: 'object',
      properties: {
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        name: { type: 'string', minLength: 1, maxLength: 240 },
        description: { type: 'string', maxLength: 4000 }
      },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_delete_project',
    description: 'Delete a Diagram Studio project, optionally deleting all diagrams assigned to it.',
    inputSchema: {
      type: 'object',
      properties: {
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        deleteDocuments: { type: 'boolean', default: false }
      },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_create_document',
    description: 'Create a diagram from a built-in template. idempotencyKey deduplicates one retried tool call; artifactKey with upsert updates one logical deliverable. Different keys or create_new may intentionally create similar diagrams.',
    inputSchema: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'] },
        title: { type: 'string', minLength: 1, maxLength: 240 },
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        blank: { type: 'boolean', default: false },
        artifactKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        idempotencyKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        mode: { type: 'string', enum: ['upsert', 'create_new'], default: 'upsert' }
      },
      required: ['kind'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_import_plantuml',
    description: 'Create or update an editable Diagram Studio diagram from PlantUML. idempotencyKey deduplicates one retried call; artifactKey with upsert identifies the intended deliverable without preventing intentional copies.',
    inputSchema: {
      type: 'object',
      properties: {
        source: { type: 'string', minLength: 1, maxLength: 2_097_152 },
        title: { type: 'string', minLength: 1, maxLength: 240 },
        kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'] },
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        artifactKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        idempotencyKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        mode: { type: 'string', enum: ['upsert', 'create_new'], default: 'upsert' }
      },
      required: ['source'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_move_document',
    description: 'Attach or move a diagram into another Diagram Studio project in the current ChatOS scope.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        targetProjectId: { type: 'string', minLength: 1, maxLength: 128 },
        sourceProjectId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'targetProjectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_delete_document',
    description: 'Delete a diagram document and remove it from projects in the current ChatOS scope.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_get_document',
    description: 'Read the complete structured diagram document, including stable node and edge IDs.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_replace_document',
    description: 'Replace a complete diagram using optimistic revision control. Prefer diagram_apply_patch for focused changes.',
    inputSchema: {
      type: 'object',
      properties: {
        expectedRevision: { type: 'integer', minimum: 0 },
        document: { type: 'object' }
      },
      required: ['expectedRevision', 'document'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_apply_patch',
    description: 'Apply focused title, node, edge, position, and viewport operations without replacing unrelated diagram content.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 1000,
          items: {
            type: 'object',
            properties: {
              op: {
                type: 'string',
                enum: ['set_title', 'set_description', 'upsert_node', 'remove_node', 'move_node', 'upsert_edge', 'remove_edge', 'set_viewport']
              }
            },
            required: ['op'],
            additionalProperties: true
          }
        }
      },
      required: ['documentId', 'expectedRevision', 'operations'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_auto_layout',
    description: 'Automatically arrange a diagram while preserving node IDs and semantic edges.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        direction: { type: 'string', enum: ['RIGHT', 'DOWN'] }
      },
      required: ['documentId', 'expectedRevision'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_validate',
    description: 'Validate node IDs, edge references, isolated nodes, and basic diagram quality.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_export',
    description: 'Export a diagram as managed structured JSON, SVG, or PlantUML source.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        format: { type: 'string', enum: ['json', 'svg', 'plantuml'] }
      },
      required: ['documentId', 'format'],
      additionalProperties: false
    },
    _meta: {
      ...policy,
      'chatos/requiredPermissions': ['artifact.create']
    }
  }
] as const;

function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

function runtimeScope(): Record<string, unknown> {
  const kind = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
  return {
    kind,
    shared: kind === 'device',
    ...(process.env.CHATOS_PROJECT_ID ? { chatosProjectId: process.env.CHATOS_PROJECT_ID } : {}),
    ...(process.env.CHATOS_PROJECT_NAME ? { chatosProjectName: process.env.CHATOS_PROJECT_NAME } : {}),
    ...(process.env.CHATOS_WORKSPACE_ID ? { workspaceId: process.env.CHATOS_WORKSPACE_ID } : {})
  };
}

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'diagram_list_documents':
      return {
        documents: typeof argumentsValue.projectId === 'string'
          ? await store.listInProject(argumentsValue.projectId)
          : await store.list()
      };
    case 'diagram_list_projects':
      return {
        scope: runtimeScope(),
        projects: await store.listProjects()
      };
    case 'diagram_create_project': {
      const project = await store.createProject(
        String(argumentsValue.name),
        typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined
      );
      return { scope: runtimeScope(), project };
    }
    case 'diagram_get_project': {
      const projectId = String(argumentsValue.projectId);
      return {
        scope: runtimeScope(),
        project: await store.readProject(projectId),
        documents: await store.listInProject(projectId)
      };
    }
    case 'diagram_update_project': {
      const project = await store.updateProject(String(argumentsValue.projectId), {
        name: typeof argumentsValue.name === 'string' ? argumentsValue.name : undefined,
        description: typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined
      });
      return { project };
    }
    case 'diagram_delete_project':
      await store.deleteProject(
        String(argumentsValue.projectId),
        argumentsValue.deleteDocuments === true
      );
      return { deleted: true, projectId: String(argumentsValue.projectId) };
    case 'diagram_create_document': {
      const kind = argumentsValue.kind as DiagramKind;
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const artifactKey = typeof argumentsValue.artifactKey === 'string' ? argumentsValue.artifactKey : undefined;
      const idempotencyKey = typeof argumentsValue.idempotencyKey === 'string' ? argumentsValue.idempotencyKey : undefined;
      const useUpsert = artifactKey && argumentsValue.mode !== 'create_new';
      if (useUpsert) {
        const write = typeof argumentsValue.projectId === 'string'
          ? await store.createOrGetInProject(argumentsValue.projectId, kind, title, argumentsValue.blank === true, artifactKey, idempotencyKey)
          : await store.createOrGet(kind, title, argumentsValue.blank === true, artifactKey, idempotencyKey);
        return { document: diagramSummary(write.document), created: write.created, reused: write.reused };
      }
      if (idempotencyKey) {
        const write = typeof argumentsValue.projectId === 'string'
          ? await store.createNewInProjectIdempotent(argumentsValue.projectId, kind, title, argumentsValue.blank === true, idempotencyKey)
          : await store.createNewIdempotent(kind, title, argumentsValue.blank === true, idempotencyKey);
        return { document: diagramSummary(write.document), created: write.created, reused: write.reused };
      }
      const document = typeof argumentsValue.projectId === 'string'
        ? await store.createInProject(argumentsValue.projectId, kind, title, argumentsValue.blank === true)
        : await store.create(kind, title, argumentsValue.blank === true);
      return { document: diagramSummary(document), created: true, reused: false };
    }
    case 'diagram_import_plantuml': {
      const requestedKind = typeof argumentsValue.kind === 'string'
        && ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'].includes(argumentsValue.kind)
        ? argumentsValue.kind as DiagramKind
        : undefined;
      const document = plantUmlToDiagram(String(argumentsValue.source), {
        title: typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined,
        kind: requestedKind
      });
      const artifactKey = typeof argumentsValue.artifactKey === 'string' ? argumentsValue.artifactKey : undefined;
      const idempotencyKey = typeof argumentsValue.idempotencyKey === 'string' ? argumentsValue.idempotencyKey : undefined;
      const useUpsert = artifactKey && argumentsValue.mode !== 'create_new';
      if (useUpsert) {
        const write = typeof argumentsValue.projectId === 'string'
          ? await store.upsertInProject(argumentsValue.projectId, document, artifactKey, idempotencyKey)
          : await store.upsert(document, artifactKey, idempotencyKey);
        return { document: write.document, created: write.created, reused: write.reused };
      }
      if (idempotencyKey) {
        const write = typeof argumentsValue.projectId === 'string'
          ? await store.writeNewInProjectIdempotent(argumentsValue.projectId, document, idempotencyKey)
          : await store.writeNewIdempotent(document, idempotencyKey);
        return { document: write.document, created: write.created, reused: write.reused };
      }
      const saved = typeof argumentsValue.projectId === 'string'
        ? await store.writeNewInProject(argumentsValue.projectId, document)
        : await store.writeNew(document);
      return { document: saved, created: true, reused: false };
    }
    case 'diagram_move_document': {
      const moved = await store.moveDocument(
        String(argumentsValue.documentId),
        String(argumentsValue.targetProjectId),
        typeof argumentsValue.sourceProjectId === 'string' ? argumentsValue.sourceProjectId : undefined
      );
      return moved;
    }
    case 'diagram_delete_document':
      await store.remove(String(argumentsValue.documentId));
      return { deleted: true, documentId: String(argumentsValue.documentId) };
    case 'diagram_get_document':
      return { document: await store.read(String(argumentsValue.documentId)) };
    case 'diagram_replace_document': {
      const document: unknown = argumentsValue.document;
      assertDiagramDocument(document);
      const saved = await store.replace(document, Number(argumentsValue.expectedRevision));
      return { document: saved };
    }
    case 'diagram_apply_patch': {
      const saved = await store.patch(
        String(argumentsValue.documentId),
        Number(argumentsValue.expectedRevision),
        argumentsValue.operations as DiagramPatchOperation[]
      );
      return { document: saved };
    }
    case 'diagram_auto_layout': {
      const direction = argumentsValue.direction === 'DOWN' ? 'DOWN' : argumentsValue.direction === 'RIGHT' ? 'RIGHT' : undefined;
      const saved = await store.autoLayout(
        String(argumentsValue.documentId),
        Number(argumentsValue.expectedRevision),
        direction
      );
      return { document: saved };
    }
    case 'diagram_validate': {
      const document = await store.read(String(argumentsValue.documentId));
      assertDiagramDocument(document);
      const connected = new Set(document.edges.flatMap((edge) => [edge.source, edge.target]));
      const isolatedNodeIds = document.nodes
        .filter((node) => node.type !== 'laneNode' && !connected.has(node.id))
        .map((node) => node.id);
      const nodesWithoutSourceReferences = document.nodes
        .filter((node) => node.type !== 'laneNode' && (node.data.sourceReferences?.length ?? 0) === 0)
        .map((node) => node.id);
      const architectureComponents = document.kind === 'architecture'
        ? document.nodes.filter((node) => node.data.shape !== 'container')
        : [];
      const architectureContainers = document.kind === 'architecture'
        ? document.nodes.filter((node) => node.data.shape === 'container')
        : [];
      return {
        valid: true,
        document: diagramSummary(document),
        warnings: [
          ...(isolatedNodeIds.length > 0 ? [{ code: 'isolated_nodes', nodeIds: isolatedNodeIds }] : []),
          ...(nodesWithoutSourceReferences.length > 0 ? [{ code: 'missing_source_references', nodeIds: nodesWithoutSourceReferences }] : []),
          ...(architectureComponents.length >= 8 && architectureContainers.length === 0
            ? [{ code: 'flat_architecture', message: 'Architecture has many components but no package or system boundaries.' }]
            : []),
          ...(architectureComponents.length > 20
            ? [{ code: 'architecture_too_dense', message: 'Create an overview and split implementation detail into separate diagrams.' }]
            : [])
        ]
      };
    }
    case 'diagram_export': {
      const document = await store.read(String(argumentsValue.documentId));
      const format = argumentsValue.format === 'svg' ? 'svg' : argumentsValue.format === 'plantuml' ? 'plantuml' : 'json';
      return await writeExportArtifact(document, format);
    }
    default:
      throw new Error(`Unknown Diagram Studio tool: ${name}`);
  }
}

function result(value: Record<string, unknown>, isError = false) {
  const response: Record<string, unknown> = {
    content: [{ type: 'text', text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
    isError
  };
  if (!isError && typeof value.relativePath === 'string') {
    response._meta = {
      'chatos/artifacts': [{
        producer_artifact_id: `diagram_${value.sha256}`,
        relative_path: value.relativePath,
        display_name: value.relativePath,
        media_type: value.mimeType,
        size_bytes: value.size,
        sha256: value.sha256
      }]
    };
  }
  return response;
}

async function runMcp(): Promise<void> {
  await store.initialize();
  const server = new Server(
    { name: SERVER_NAME, version: SERVER_VERSION },
    { capabilities: { tools: {} } }
  );
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [...TOOL_DEFINITIONS] }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      return result(await callTool(request.params.name, request.params.arguments ?? {}));
    } catch (error) {
      return result({
        error: error instanceof Error ? error.message : String(error),
        ...(error instanceof RevisionConflictError ? { actualRevision: error.actualRevision } : {})
      }, true);
    }
  });
  await server.connect(new StdioServerTransport());
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === '--version' || command === '-v') {
    process.stdout.write(`${SERVER_VERSION}\n`);
    return;
  }
  if (command === 'mcp') {
    await runMcp();
    return;
  }
  process.stderr.write([
    'Usage: chatos-diagram-studio mcp',
    '       chatos-diagram-studio --version',
    '',
    'Run `npm run studio` from the plugin directory to open the standalone visual workbench.'
  ].join('\n') + '\n');
  process.exitCode = 2;
}

await main().catch((error) => {
  process.stderr.write(`Diagram Studio failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
