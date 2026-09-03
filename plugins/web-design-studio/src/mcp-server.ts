import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { RevisionConflictError, WebDesignDocumentStore } from './document-store.js';
import { breakpointFor, resolveComponent } from './editor-model.js';
import { exportDocumentHtmlFiles } from './html-exporter.js';
import { exportReactComponent } from './react-exporter.js';
import { exportVueComponent } from './vue-exporter.js';
import {
  assertWebDesignDocument,
  designSummary,
  type WebDesignDocument,
  type WebDesignPatchOperation
} from './schema.js';

const SERVER_NAME = 'chatos-web-design-studio';
const SERVER_VERSION = '0.8.0';
const store = new WebDesignDocumentStore();

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 100_000
};

const TOOL_DEFINITIONS = [
  {
    name: 'web_design_list_documents',
    description: 'List editable website design documents in the current ChatOS runtime scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_create_document',
    description: 'Create an editable website design from the built-in landing page template.',
    inputSchema: {
      type: 'object',
      properties: { title: { type: 'string', minLength: 1, maxLength: 240 } },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_document',
    description: 'Read a complete website design document including stable component IDs, annotations, and AI requests.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_replace_document',
    description: 'Replace a complete website design using optimistic revision control. Prefer focused patches.',
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
    name: 'web_design_apply_patch',
    description: 'Apply focused operations to components, annotations, requests, and the viewport without replacing unrelated user edits.',
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
                enum: [
                  'set_title', 'set_description', 'set_viewport', 'set_breakpoint', 'upsert_page', 'remove_page', 'upsert_asset', 'remove_asset',
                  'set_tokens', 'upsert_symbol', 'remove_symbol',
                  'upsert_component', 'remove_component', 'set_parent', 'set_layout',
                  'move_component', 'resize_component', 'update_component', 'add_annotation',
                  'resolve_annotation', 'add_request', 'resolve_request'
                ]
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
    name: 'web_design_auto_layout',
    description: 'Apply a container\'s Flex row, Flex column, or Grid layout to its direct children for one responsive device.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        containerId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] }
      },
      required: ['documentId', 'expectedRevision', 'containerId', 'device'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_sync_symbol_instances',
    description: 'Synchronize every instance of one reusable component while preserving instance-level content, style, or frame overrides.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        symbolId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'expectedRevision', 'symbolId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_update_symbol_from_instance',
    description: 'Update a reusable component definition from one selected instance, then synchronize its other instances.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        componentId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'expectedRevision', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_export_html',
    description: 'Export one page or every page as standalone HTML using the selected responsive device layout.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_export_react',
    description: 'Export the complete multi-page design as a single React JSX component with client-side route navigation.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_export_vue',
    description: 'Export the complete multi-page design as a single Vue SFC with client-side route navigation.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'], default: 'desktop' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_list_requests',
    description: 'List pending or all component-level AI design requests, optionally for one design document.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        includeResolved: { type: 'boolean', default: false }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_resolve_request',
    description: 'Mark a component-level AI request resolved after applying the requested design change.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        resolution: { type: 'string', maxLength: 4000 }
      },
      required: ['documentId', 'expectedRevision', 'requestId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_validate',
    description: 'Validate the design document and report out-of-bounds components, open annotations, and pending AI requests.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
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

async function requestEntries(documentId: string, includeResolved: boolean) {
  const document = await store.read(documentId);
  return document.requests
    .filter((request) => includeResolved || request.status === 'pending')
    .map((request) => ({
      documentId,
      documentTitle: document.title,
      revision: document.revision,
      request,
      component: request.componentId
        ? document.components.find((component) => component.id === request.componentId)
        : undefined
    }));
}

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'web_design_list_documents':
      return { scope: runtimeScope(), documents: await store.list() };
    case 'web_design_create_document': {
      const document = await store.create(typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined);
      return { scope: runtimeScope(), document: designSummary(document) };
    }
    case 'web_design_get_document':
      return { document: await store.read(String(argumentsValue.documentId)) };
    case 'web_design_replace_document': {
      const document: unknown = argumentsValue.document;
      assertWebDesignDocument(document);
      return { document: await store.replace(document as WebDesignDocument, Number(argumentsValue.expectedRevision)) };
    }
    case 'web_design_apply_patch':
      return {
        document: await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          argumentsValue.operations as WebDesignPatchOperation[]
        )
      };
    case 'web_design_auto_layout':
      return {
        document: await store.autoLayout(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.containerId),
          String(argumentsValue.device) as 'desktop' | 'tablet' | 'mobile'
        )
      };
    case 'web_design_sync_symbol_instances':
      return {
        document: await store.syncSymbolInstances(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.symbolId)
        )
      };
    case 'web_design_update_symbol_from_instance':
      return {
        document: await store.updateSymbolFromInstance(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.componentId)
        )
      };
    case 'web_design_export_html': {
      const document = await store.read(String(argumentsValue.documentId));
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      if (typeof argumentsValue.pageId === 'string') {
        const pageId = argumentsValue.pageId;
        const file = exportDocumentHtmlFiles(document, device).find((candidate) => candidate.pageId === pageId);
        if (!file) throw new Error(`Page not found: ${pageId}`);
        return { files: [file] };
      }
      return { files: exportDocumentHtmlFiles(document, device) };
    }
    case 'web_design_export_react': {
      const document = await store.read(String(argumentsValue.documentId));
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      return { files: [exportReactComponent(document, device)] };
    }
    case 'web_design_export_vue': {
      const document = await store.read(String(argumentsValue.documentId));
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      return { files: [exportVueComponent(document, device)] };
    }
    case 'web_design_list_requests': {
      const includeResolved = argumentsValue.includeResolved === true;
      if (typeof argumentsValue.documentId === 'string') {
        return { requests: await requestEntries(argumentsValue.documentId, includeResolved) };
      }
      const summaries = await store.list();
      const requests = (await Promise.all(summaries.map((item) => requestEntries(item.documentId, includeResolved)))).flat();
      return { scope: runtimeScope(), requests };
    }
    case 'web_design_resolve_request':
      return {
        document: await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          [{
            op: 'resolve_request',
            requestId: String(argumentsValue.requestId),
            resolution: typeof argumentsValue.resolution === 'string' ? argumentsValue.resolution : undefined
          }]
        )
      };
    case 'web_design_validate': {
      const document = await store.read(String(argumentsValue.documentId));
      const outOfBounds = (['desktop', 'tablet', 'mobile'] as const).flatMap((device) => {
        const target = breakpointFor(document, device);
        return document.components.flatMap((component) => {
          const frame = resolveComponent(component, device);
          return !frame.hidden && (frame.x < 0 || frame.y < 0 || frame.x + frame.width > target.width || frame.y + frame.height > target.height)
            ? [{ device, componentId: component.id }]
            : [];
        });
      });
      const openAnnotations = document.components.flatMap((component) => component.annotations
        .filter((annotation) => annotation.status === 'open')
        .map((annotation) => ({ componentId: component.id, annotation })));
      const byId = new Map(document.components.map((component) => [component.id, component]));
      const childrenOutsideContainers = (['desktop', 'tablet', 'mobile'] as const).flatMap((device) => document.components.flatMap((component) => {
        if (!component.parentId) return [];
        const parent = byId.get(component.parentId);
        if (!parent) return [];
        const childFrame = resolveComponent(component, device);
        const parentFrame = resolveComponent(parent, device);
        return !childFrame.hidden && !parentFrame.hidden && (
          childFrame.x < parentFrame.x || childFrame.y < parentFrame.y
          || childFrame.x + childFrame.width > parentFrame.x + parentFrame.width
          || childFrame.y + childFrame.height > parentFrame.y + parentFrame.height
        ) ? [{ device, componentId: component.id, parentId: parent.id }] : [];
      }));
      const emptyLayoutContainers = document.components
        .filter((component) => component.layout?.mode !== undefined && component.layout.mode !== 'free'
          && !document.components.some((candidate) => candidate.parentId === component.id))
        .map((component) => ({ componentId: component.id, mode: component.layout!.mode }));
      const unusualParents = document.components.flatMap((component) => {
        if (!component.parentId) return [];
        const parent = byId.get(component.parentId);
        return parent && !['section', 'card'].includes(parent.type)
          ? [{ componentId: component.id, parentId: parent.id, parentType: parent.type }]
          : [];
      });
      return {
        valid: outOfBounds.length === 0 && childrenOutsideContainers.length === 0,
        document: designSummary(document),
        warnings: [
          ...(outOfBounds.length ? [{ code: 'out_of_bounds', items: outOfBounds }] : []),
          ...(childrenOutsideContainers.length ? [{ code: 'children_outside_container', items: childrenOutsideContainers }] : []),
          ...(emptyLayoutContainers.length ? [{ code: 'empty_layout_containers', items: emptyLayoutContainers }] : []),
          ...(unusualParents.length ? [{ code: 'unusual_parent_type', items: unusualParents }] : []),
          ...(openAnnotations.length ? [{ code: 'open_annotations', items: openAnnotations }] : []),
          ...(document.requests.some((request) => request.status === 'pending') ? [{ code: 'pending_ai_requests' }] : [])
        ]
      };
    }
    default:
      throw new Error(`Unknown Web Design Studio tool: ${name}`);
  }
}

function result(value: Record<string, unknown>, isError = false) {
  return {
    content: [{ type: 'text' as const, text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
    isError
  };
}

async function runMcp(): Promise<void> {
  await store.initialize();
  const server = new Server({ name: SERVER_NAME, version: SERVER_VERSION }, { capabilities: { tools: {} } });
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
  process.stderr.write('Usage: chatos-web-design-studio mcp\n');
  process.exitCode = 2;
}

await main().catch((error) => {
  process.stderr.write(`Web Design Studio failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
