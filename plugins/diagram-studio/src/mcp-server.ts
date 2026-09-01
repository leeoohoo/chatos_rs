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

const SERVER_NAME = 'chatos-diagram-studio';
const SERVER_VERSION = '0.1.0';
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
    description: 'List the structured diagram documents available in Diagram Studio.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'diagram_create_document',
    description: 'Create a diagram from a built-in architecture, flowchart, swimlane, topology, or sequence template.',
    inputSchema: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'] },
        title: { type: 'string', minLength: 1, maxLength: 240 }
      },
      required: ['kind'],
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
    description: 'Export a diagram as a managed structured JSON or SVG artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        format: { type: 'string', enum: ['json', 'svg'] }
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

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'diagram_list_documents':
      return { documents: await store.list() };
    case 'diagram_create_document': {
      const kind = argumentsValue.kind as DiagramKind;
      const document = await store.create(kind, typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined);
      return { document: diagramSummary(document) };
    }
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
      return {
        valid: true,
        document: diagramSummary(document),
        warnings: [
          ...(isolatedNodeIds.length > 0 ? [{ code: 'isolated_nodes', nodeIds: isolatedNodeIds }] : []),
          ...(nodesWithoutSourceReferences.length > 0 ? [{ code: 'missing_source_references', nodeIds: nodesWithoutSourceReferences }] : [])
        ]
      };
    }
    case 'diagram_export': {
      const document = await store.read(String(argumentsValue.documentId));
      return await writeExportArtifact(document, argumentsValue.format === 'svg' ? 'svg' : 'json');
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
