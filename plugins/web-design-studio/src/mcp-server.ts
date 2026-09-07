import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { RevisionConflictError, WebDesignDocumentStore } from './document-store.js';
import { exportDocumentHtmlFiles } from './html-exporter.js';
import { exportReactComponent } from './react-exporter.js';
import { exportVueComponent } from './vue-exporter.js';
import { editableSlotsForUiComponent } from './library-slots.js';
import { createComponentFromUiLibrary, UI_LIBRARIES } from './ui-libraries.js';
import { WEB_DESIGN_THEME_PRESETS } from './design-themes.js';
import { WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES } from './component-library.js';
import { runtimeScopeFingerprint } from './runtime-scope.js';
import {
  assertHandoffQuality,
  componentPageId,
  pageOutline,
  validateWebDesignDocument
} from './design-quality.js';
import {
  assertWebDesignDocument,
  designSummary,
  pageIdForComponent,
  pagesForDocument,
  type WebDesignDocument,
  type WebDesignComponent,
  type WebDesignPatchOperation
} from './schema.js';

const SERVER_NAME = 'chatos-web-design-studio';
const SERVER_VERSION = '0.11.0';
const store = new WebDesignDocumentStore();
await store.initialize();
await store.ensureLegacyProject();
const scopeKey = runtimeScopeFingerprint(store.rootDirectory);
const defaultProject = await store.ensureScopedProject(
  scopeKey,
  process.env.CHATOS_CONTEXT_SCOPE === 'project' && process.env.CHATOS_PROJECT_ID
    ? process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 网站项目'
    : '公共网站设计'
);

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 100_000
};

const componentStyleSchema = {
  type: 'object',
  properties: {
    background: { type: 'string', maxLength: 300 },
    color: { type: 'string', maxLength: 300 },
    borderColor: { type: 'string', maxLength: 300 },
    borderWidth: { type: 'number', minimum: 0, maximum: 40 },
    borderStyle: { type: 'string', enum: ['solid', 'dashed', 'dotted', 'double', 'none'] },
    borderRadius: { type: 'number', minimum: 0, maximum: 999 },
    padding: { type: 'number', minimum: 0, maximum: 2000 },
    fontSize: { type: 'number', minimum: 6, maximum: 240 },
    fontWeight: { type: 'number', minimum: 100, maximum: 1000 },
    textAlign: { type: 'string', enum: ['left', 'center', 'right'] },
    lineHeight: { type: 'number', minimum: 0.5, maximum: 5 },
    letterSpacing: { type: 'number', minimum: -20, maximum: 100 },
    opacity: { type: 'number', minimum: 0, maximum: 1 },
    shadow: { type: 'string', maxLength: 300 },
    overflow: { type: 'string', enum: ['visible', 'hidden', 'auto', 'scroll'] },
    objectFit: { type: 'string', enum: ['cover', 'contain', 'fill', 'none', 'scale-down'] },
    objectPosition: { type: 'string', maxLength: 300 },
    customCss: { type: 'object', maxProperties: 80, additionalProperties: { oneOf: [{ type: 'string', maxLength: 1000 }, { type: 'number' }] } }
  },
  additionalProperties: true
} as const;

const componentFrameProperties = {
  x: { type: 'number', minimum: -100000, maximum: 100000 },
  y: { type: 'number', minimum: -100000, maximum: 100000 },
  width: { type: 'number', minimum: 16, maximum: 100000 },
  height: { type: 'number', minimum: 16, maximum: 100000 }
} as const;

const webDesignComponentSchema = {
  type: 'object',
  description: 'One editable visual node. A text node represents one semantic text item only; never encode a whole page, navigation, card collection, table, or form in one text content string.',
  properties: {
    id: { type: 'string', minLength: 1, maxLength: 128, pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
    type: { type: 'string', enum: ['section', 'text', 'heading', 'button', 'link', 'image', 'icon', 'logo', 'card', 'input', 'textarea', 'select', 'checkbox', 'switch', 'divider', 'badge', 'avatar', 'list', 'table', 'video'] },
    name: { type: 'string', minLength: 1, maxLength: 240, description: 'Semantic layer name such as Header, Primary navigation, Pricing card, or Submit button.' },
    pageId: { type: 'string', minLength: 1, maxLength: 128 },
    parentId: { type: 'string', minLength: 1, maxLength: 128 },
    slot: { type: 'string', minLength: 1, maxLength: 128 },
    symbolId: { type: 'string', minLength: 1, maxLength: 128 },
    symbolInstanceId: { type: 'string', minLength: 1, maxLength: 128 },
    symbolComponentId: { type: 'string', minLength: 1, maxLength: 128 },
    symbolOverrides: { type: 'array', maxItems: 3, uniqueItems: true, items: { type: 'string', enum: ['content', 'style', 'frame'] } },
    interaction: {
      type: 'object',
      properties: { type: { type: 'string', enum: ['page', 'url'] }, target: { type: 'string', minLength: 1, maxLength: 2000 } },
      required: ['type', 'target'],
      additionalProperties: false
    },
    library: {
      type: 'object',
      description: 'Copy this binding exactly from web_design_get_component_contract; do not invent names, variants, props, or slots.',
      properties: {
        name: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        version: { type: 'string', minLength: 1, maxLength: 80 },
        component: { type: 'string', minLength: 1, maxLength: 160 },
        variant: { type: 'string', maxLength: 160 },
        props: { type: 'object', additionalProperties: true }
      },
      required: ['name', 'version', 'component', 'props'],
      additionalProperties: false
    },
    ...componentFrameProperties,
    zIndex: { type: 'number', minimum: -10000, maximum: 10000 },
    content: { type: 'string', maxLength: 12000, description: 'One semantic text value or media reference. Do not use whitespace/newlines to draw UI.' },
    style: componentStyleSchema,
    states: { type: 'object', properties: { hover: componentStyleSchema, active: componentStyleSchema, focus: componentStyleSchema }, additionalProperties: false },
    locked: { type: 'boolean' },
    hidden: { type: 'boolean' },
    layout: {
      type: 'object',
      description: 'Figma-like layout intent for a container. Prefer flex/grid over manually positioning a repeated group.',
      properties: {
        mode: { type: 'string', enum: ['free', 'flex-row', 'flex-column', 'grid'] },
        gap: { type: 'number', minimum: 0, maximum: 2000 },
        padding: { type: 'number', minimum: 0, maximum: 2000 },
        columns: { type: 'integer', minimum: 1, maximum: 24 },
        align: { type: 'string', enum: ['start', 'center', 'end', 'stretch'] },
        justify: { type: 'string', enum: ['start', 'center', 'end', 'space-between', 'space-around'] },
        wrap: { type: 'boolean' }
      },
      required: ['mode', 'gap', 'padding'],
      additionalProperties: false
    },
    responsive: {
      type: 'object',
      properties: {
        tablet: { type: 'object', properties: { ...componentFrameProperties, hidden: { type: 'boolean' }, style: componentStyleSchema }, required: ['x', 'y', 'width', 'height'], additionalProperties: false },
        mobile: { type: 'object', properties: { ...componentFrameProperties, hidden: { type: 'boolean' }, style: componentStyleSchema }, required: ['x', 'y', 'width', 'height'], additionalProperties: false }
      },
      additionalProperties: false
    },
    constraints: { type: 'object', additionalProperties: true },
    annotations: {
      type: 'array',
      maxItems: 100,
      items: {
        type: 'object',
        properties: {
          id: { type: 'string', minLength: 1, maxLength: 128 },
          text: { type: 'string', minLength: 1, maxLength: 12000 },
          status: { type: 'string', enum: ['open', 'resolved'] },
          createdAt: { type: 'string' },
          resolvedAt: { type: 'string' }
        },
        required: ['id', 'text', 'status', 'createdAt'],
        additionalProperties: false
      }
    }
  },
  required: ['id', 'type', 'name', 'pageId', 'x', 'y', 'width', 'height', 'zIndex', 'content', 'style', 'annotations'],
  additionalProperties: false
} as const;

const patchOperationSchema = {
  oneOf: [
    {
      type: 'object',
      properties: { op: { const: 'set_title' }, title: { type: 'string', minLength: 1, maxLength: 240 } },
      required: ['op', 'title'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'set_description' }, description: { type: 'string', maxLength: 4000 } },
      required: ['op', 'description'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'set_viewport' },
        viewport: {
          type: 'object',
          properties: {
            width: { type: 'number', minimum: 1 },
            height: { type: 'number', minimum: 1 },
            background: { type: 'string', minLength: 1, maxLength: 200 }
          },
          required: ['width', 'height', 'background'],
          additionalProperties: false
        }
      },
      required: ['op', 'viewport'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'set_breakpoint' },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] },
        width: { type: 'number', minimum: 1 },
        height: { type: 'number', minimum: 1 }
      },
      required: ['op', 'device', 'width', 'height'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'upsert_page' }, page: { type: 'object', description: 'Complete page object with id, name, and slash-prefixed slug.' } },
      required: ['op', 'page'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'remove_page' }, pageId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'pageId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'upsert_asset' }, asset: { type: 'object', description: 'Complete image asset object matching the document schema.' } },
      required: ['op', 'asset'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'remove_asset' }, assetId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'assetId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'set_tokens' }, tokens: { type: 'object', description: 'Complete color, radii, and typography token groups.' } },
      required: ['op', 'tokens'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'upsert_symbol' }, symbol: { type: 'object', description: 'Complete reusable symbol object matching the document schema.' } },
      required: ['op', 'symbol'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'remove_symbol' }, symbolId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'symbolId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'upsert_component' }, component: webDesignComponentSchema },
      required: ['op', 'component'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'remove_component' }, componentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'componentId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'set_parent' },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        parentId: { type: 'string', minLength: 1, maxLength: 128 },
        slot: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['op', 'componentId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'set_layout' },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        layout: { type: 'object', description: 'Container layout with mode free, flex-row, flex-column, or grid and its supported alignment, gap, padding, wrapping, or grid fields.' }
      },
      required: ['op', 'componentId', 'layout'],
      additionalProperties: false
    },
    ...(['move_component', 'resize_component'] as const).map((op) => ({
      type: 'object',
      properties: op === 'move_component'
        ? {
            op: { const: op },
            componentId: { type: 'string', minLength: 1, maxLength: 128 },
            x: { type: 'number' },
            y: { type: 'number' },
            device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] }
          }
        : {
            op: { const: op },
            componentId: { type: 'string', minLength: 1, maxLength: 128 },
            width: { type: 'number', minimum: 1 },
            height: { type: 'number', minimum: 1 },
            device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] }
          },
      required: op === 'move_component' ? ['op', 'componentId', 'x', 'y'] : ['op', 'componentId', 'width', 'height'],
      additionalProperties: false
    })),
    {
      type: 'object',
      properties: {
        op: { const: 'update_component' },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] },
        changes: { type: 'object', minProperties: 1, description: 'Only changed component fields: name, content, zIndex, style, states, locked, hidden, symbolOverrides, constraints, or interaction.' }
      },
      required: ['op', 'componentId', 'changes'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'add_annotation' },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        annotation: { type: 'object', description: 'Complete annotation with stable id, author, text, status, and timestamps.' }
      },
      required: ['op', 'componentId', 'annotation'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'resolve_annotation' },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        annotationId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['op', 'componentId', 'annotationId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: { const: 'add_request' }, request: { type: 'object', description: 'Complete AI work request with stable id, prompt, status, timestamps, and optional pageId or componentId.' } },
      required: ['op', 'request'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: { const: 'resolve_request' },
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        resolution: { type: 'string', maxLength: 4000 }
      },
      required: ['op', 'requestId'],
      additionalProperties: false
    }
  ]
} as const;

const TOOL_DEFINITIONS_BASE = [
  {
    name: 'web_design_list_documents',
    description: 'List editable website design documents, optionally limited to one Web Design Studio project.',
    inputSchema: { type: 'object', properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 } }, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_list_projects',
    description: 'List Web Design Studio projects in the current ChatOS runtime scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_create_project',
    description: 'Create a project that can contain multiple separately named website designs.',
    inputSchema: { type: 'object', properties: { name: { type: 'string', minLength: 1, maxLength: 240 }, description: { type: 'string', maxLength: 4000 } }, required: ['name'], additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_get_project',
    description: 'Read a website project and the summaries of designs assigned to it.',
    inputSchema: { type: 'object', properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 } }, required: ['projectId'], additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_update_project',
    description: 'Rename a website project or update its description.',
    inputSchema: { type: 'object', properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 }, name: { type: 'string', minLength: 1, maxLength: 240 }, description: { type: 'string', maxLength: 4000 } }, required: ['projectId'], additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_delete_project',
    description: 'Delete a website project, optionally deleting all website designs assigned to it.',
    inputSchema: { type: 'object', properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 }, deleteDocuments: { type: 'boolean', default: false } }, required: ['projectId'], additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_move_document',
    description: 'Attach or move one website design into another Web Design Studio project.',
    inputSchema: { type: 'object', properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 }, targetProjectId: { type: 'string', minLength: 1, maxLength: 128 }, sourceProjectId: { type: 'string', minLength: 1, maxLength: 128 } }, required: ['documentId', 'targetProjectId'], additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_create_document',
    description: 'Create an editable website design, optionally inside a project and optionally as a blank canvas.',
    inputSchema: {
      type: 'object',
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 240 },
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        blank: { type: 'boolean', default: false }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_document',
    description: 'Read the complete website design document. This is a recovery/global-inspection tool and can be large. For normal AI work, read web_design_get_document_outline first, then web_design_get_page for only the page being edited.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_document_outline',
    description: 'Read sparse Figma-like document metadata: pages, root layers, type distribution, component-system usage, and quality signals. Call this before opening or editing a design; it intentionally omits component content and full node payloads.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_page',
    description: 'Read one editable page and its node tree only. Use this after web_design_get_document_outline and work one page at a time instead of loading the complete document. For a large page or focused edit, use web_design_get_node on the relevant semantic container.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'pageId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_node',
    description: 'Read one component/Frame and a bounded descendant subtree, plus its ancestor path. Use this Figma-like focused context for component-by-component inspection and revision without loading unrelated page nodes.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        depth: { type: 'integer', minimum: 0, maximum: 8, default: 4 }
      },
      required: ['documentId', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_catalog',
    description: 'Read the compact Web Design Studio catalog: available design systems, categories, component counts, production sections, page templates, and visual themes. Use search next instead of loading every component and variant.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_search_components',
    description: 'Search a bounded component catalog by intent, label, category, or component name. Returns compact candidates only; call web_design_get_component_contract for the chosen component before inserting it.',
    inputSchema: {
      type: 'object',
      properties: {
        query: { type: 'string', maxLength: 200, description: 'Optional user intent or component keyword, for example pricing card, 表格, navigation, or upload.' },
        libraryId: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        category: { type: 'string', maxLength: 120 },
        includeDeprecated: { type: 'boolean', default: false },
        limit: { type: 'integer', minimum: 1, maximum: 50, default: 20 }
      },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_component_contract',
    description: 'Read one selected component contract with its supported variants, exact library binding template, default content and size, and editable slots. Do not invent library, component, variant, prop, or slot names.',
    inputSchema: {
      type: 'object',
      properties: {
        libraryId: { type: 'string', enum: UI_LIBRARIES.map((library) => library.id) },
        componentId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['libraryId', 'componentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_insert_section',
    description: 'Insert one production-ready responsive page section at the end of a page while preserving all existing components.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        sectionId: { type: 'string', enum: WEB_DESIGN_BLOCK_PRESETS.map((section) => section.id) }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'sectionId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_apply_page_template',
    description: 'Replace one page with a complete editable responsive page template while preserving every other page.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        templateId: { type: 'string', enum: WEB_DESIGN_PAGE_TEMPLATES.map((template) => template.id) }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'templateId'],
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
    description: 'Apply a small focused edit without replacing unrelated work. A call may touch components on only one page, contain at most 48 operations and 24 component insertions, and stay below 64 KB. Build one logical region at a time. If a call is too large, split it; never fall back to a large text node that simulates UI.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 48,
          items: patchOperationSchema
        }
      },
      required: ['documentId', 'expectedRevision', 'operations'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_apply_node_batch',
    description: 'Insert or replace up to 24 editable nodes in one page and one logical region, using optimistic revision control. This is the preferred construction tool after reading the page. Every visual object must be a separate node; never encode a complete interface in text content.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        regionName: { type: 'string', minLength: 1, maxLength: 120, description: 'Logical chunk such as Header, Sidebar navigation, Hero, Pricing grid, or Checkout form.' },
        components: { type: 'array', minItems: 1, maxItems: 24, items: webDesignComponentSchema }
      },
      required: ['documentId', 'expectedRevision', 'pageId', 'regionName', 'components'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_auto_layout',
    description: 'Apply a container\'s Flex row, Flex column, or Grid layout to its direct children for one responsive device, including justify distribution and Flex row wrapping.',
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
    description: 'Validate structure and visual-delivery quality. Use draft mode after each page region, then handoff mode before completion. Handoff rejects empty/underbuilt pages, whole-page text mockups, long text used as UI, overflow, and invalid containment.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 128 },
        mode: { type: 'string', enum: ['draft', 'handoff'], default: 'handoff' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  }
] as const;

function webDesignToolSkills(name: string): string[] {
  if (name === 'web_design_replace_document'
    || name === 'web_design_insert_section' || name === 'web_design_apply_page_template') {
    return ['web-design-components', 'web-design-responsive-layout', 'web-design-visual-system'];
  }
  if (name === 'web_design_apply_patch' || name === 'web_design_apply_node_batch') return ['web-design-components', 'web-design-responsive-layout'];
  if (name === 'web_design_get_node') return ['web-design-components'];
  if (name.includes('auto_layout')) return ['web-design-responsive-layout'];
  if (name.includes('catalog') || name.includes('component') || name.includes('symbol')) return ['web-design-components'];
  if (name.includes('export') || name.includes('validate')) return ['web-design-validation-export'];
  return ['web-design-projects'];
}

const TOOL_DEFINITIONS = TOOL_DEFINITIONS_BASE.map((tool) => ({
  ...tool,
  _meta: {
    ...tool._meta,
    'chatos/skillGate': {
      allOf: ['web-design-studio', ...webDesignToolSkills(tool.name)]
    }
  }
}));

function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

function runtimeScope(): Record<string, unknown> {
  const kind = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
  return {
    kind,
    isolated: true,
    hasProjectContext: kind === 'project',
    ...(process.env.CHATOS_PROJECT_NAME ? { projectName: process.env.CHATOS_PROJECT_NAME } : {})
  };
}

function changedComponentIds(operations: WebDesignPatchOperation[]): string[] {
  return [...new Set(operations.flatMap((operation) => {
    if (operation.op === 'upsert_component') return [operation.component.id];
    if ('componentId' in operation && typeof operation.componentId === 'string') return [operation.componentId];
    return [];
  }))];
}

function compactMutationResult(
  document: WebDesignDocument,
  options: { pageId?: string; changedIds?: string[]; regionName?: string } = {}
) {
  const changedIds = options.changedIds ?? [];
  return {
    document: designSummary(document),
    ...(options.pageId ? { page: pageOutline(document, options.pageId) } : {}),
    ...(options.regionName ? { regionName: options.regionName } : {}),
    changedComponentCount: changedIds.length,
    changedComponentIds: changedIds.slice(0, 64),
    ...(changedIds.length > 64 ? { changedComponentIdsTruncated: true } : {}),
    nextRecommendedActions: options.pageId ? [
      { tool: 'web_design_get_page', arguments: { documentId: document.documentId, pageId: options.pageId } },
      { tool: 'web_design_validate', arguments: { documentId: document.documentId, pageId: options.pageId, mode: 'draft' } }
    ] : [
      { tool: 'web_design_get_document_outline', arguments: { documentId: document.documentId } }
    ]
  };
}

function assertFocusedOperations(document: WebDesignDocument, operations: WebDesignPatchOperation[]): string | undefined {
  const serializedBytes = Buffer.byteLength(JSON.stringify(operations), 'utf8');
  if (serializedBytes > 65_536) {
    throw new Error(`Patch is ${serializedBytes} bytes. Split it into logical regions smaller than 65536 bytes.`);
  }
  const upserts = operations.filter((operation) => operation.op === 'upsert_component');
  if (upserts.length > 24) throw new Error('A focused patch can insert at most 24 components. Split the page into logical regions.');

  const pageIds = new Set<string>();
  for (const operation of operations) {
    if (operation.op === 'upsert_component') {
      pageIds.add(operation.component.pageId ?? pagesForDocument(document)[0].id);
      continue;
    }
    if ('componentId' in operation && typeof operation.componentId === 'string') {
      const pageId = componentPageId(document, operation.componentId);
      if (pageId) pageIds.add(pageId);
    }
  }
  if (pageIds.size > 1) throw new Error('A focused component patch may touch only one page. Split operations by page.');
  return [...pageIds][0];
}

function changedIdsBetween(before: WebDesignDocument, after: WebDesignDocument, pageId?: string): string[] {
  const beforeById = new Map(before.components.map((component) => [component.id, JSON.stringify(component)]));
  return after.components
    .filter((component) => !pageId || pageIdForComponent(after, component) === pageId)
    .filter((component) => beforeById.get(component.id) !== JSON.stringify(component))
    .map((component) => component.id);
}

async function requestEntries(documentId: string, includeResolved: boolean) {
  const document = await store.readInScope(documentId, scopeKey);
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
      return { scope: runtimeScope(), documents: typeof argumentsValue.projectId === 'string' ? await store.listInProject(argumentsValue.projectId, scopeKey) : await store.listInScope(scopeKey) };
    case 'web_design_list_projects':
      return { scope: runtimeScope(), projects: await store.listProjects(scopeKey) };
    case 'web_design_create_project':
      return { scope: runtimeScope(), project: await store.createProject(String(argumentsValue.name), typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined, scopeKey) };
    case 'web_design_get_project': {
      const projectId = String(argumentsValue.projectId);
      return { scope: runtimeScope(), project: await store.readProjectInScope(projectId, scopeKey), documents: await store.listInProject(projectId, scopeKey) };
    }
    case 'web_design_update_project':
      await store.readProjectInScope(String(argumentsValue.projectId), scopeKey);
      return { project: await store.updateProject(String(argumentsValue.projectId), { name: typeof argumentsValue.name === 'string' ? argumentsValue.name : undefined, description: typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined }) };
    case 'web_design_delete_project':
      await store.readProjectInScope(String(argumentsValue.projectId), scopeKey);
      await store.deleteProject(String(argumentsValue.projectId), argumentsValue.deleteDocuments === true);
      return { deleted: true, projectId: String(argumentsValue.projectId) };
    case 'web_design_move_document':
      return store.moveDocument(String(argumentsValue.documentId), String(argumentsValue.targetProjectId), typeof argumentsValue.sourceProjectId === 'string' ? argumentsValue.sourceProjectId : undefined, scopeKey);
    case 'web_design_create_document': {
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const document = typeof argumentsValue.projectId === 'string'
        ? (await store.readProjectInScope(argumentsValue.projectId, scopeKey), await store.createInProject(argumentsValue.projectId, title, argumentsValue.blank === true))
        : await store.createInProject(defaultProject.projectId, title, argumentsValue.blank === true);
      return { scope: runtimeScope(), document: designSummary(document) };
    }
    case 'web_design_get_document':
      return { document: await store.readInScope(String(argumentsValue.documentId), scopeKey) };
    case 'web_design_get_document_outline': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      return {
        document: designSummary(document),
        pages: pagesForDocument(document).map((page) => pageOutline(document, page.id)),
        symbolCount: document.symbols?.length ?? 0,
        assetCount: document.assets?.length ?? 0
      };
    }
    case 'web_design_get_page': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const page = pagesForDocument(document).find((candidate) => candidate.id === pageId);
      if (!page) throw new Error(`Page not found: ${pageId}`);
      const components = document.components
        .filter((component) => pageIdForComponent(document, component) === pageId)
        .sort((left, right) => left.zIndex - right.zIndex);
      const componentIds = new Set(components.map((component) => component.id));
      return {
        document: designSummary(document),
        page,
        viewport: document.viewport,
        breakpoints: document.breakpoints,
        tokens: document.tokens,
        rootIds: components.filter((component) => !component.parentId).map((component) => component.id),
        components,
        requests: document.requests.filter((request) => !request.componentId || componentIds.has(request.componentId)),
        quality: pageOutline(document, pageId).quality
      };
    }
    case 'web_design_get_node': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.componentId);
      const root = document.components.find((component) => component.id === componentId);
      if (!root) throw new Error(`Component not found: ${componentId}`);
      const depth = typeof argumentsValue.depth === 'number' ? Math.max(0, Math.min(8, Math.trunc(argumentsValue.depth))) : 4;
      const byParent = new Map<string, WebDesignComponent[]>();
      for (const component of document.components) {
        if (!component.parentId) continue;
        const children = byParent.get(component.parentId) ?? [];
        children.push(component);
        byParent.set(component.parentId, children);
      }
      const descendants: Array<{ component: WebDesignComponent; depth: number }> = [];
      let frontier = [{ component: root, depth: 0 }];
      while (frontier.length > 0) {
        const current = frontier.shift()!;
        descendants.push(current);
        if (current.depth >= depth) continue;
        frontier.push(...(byParent.get(current.component.id) ?? [])
          .sort((left, right) => left.zIndex - right.zIndex)
          .map((component) => ({ component, depth: current.depth + 1 })));
      }
      const byId = new Map(document.components.map((component) => [component.id, component]));
      const ancestors: Array<{ id: string; name: string; type: string }> = [];
      let parentId = root.parentId;
      while (parentId) {
        const parent = byId.get(parentId);
        if (!parent) break;
        ancestors.unshift({ id: parent.id, name: parent.name, type: parent.type });
        parentId = parent.parentId;
      }
      return {
        document: designSummary(document),
        pageId: pageIdForComponent(document, root),
        ancestorPath: ancestors,
        rootId: root.id,
        requestedDepth: depth,
        truncated: descendants.some(({ component, depth: componentDepth }) => componentDepth === depth && (byParent.get(component.id)?.length ?? 0) > 0),
        nodes: descendants
      };
    }
    case 'web_design_get_catalog':
      return {
        libraries: UI_LIBRARIES.map((library) => ({
          id: library.id,
          name: library.displayName,
          version: library.version,
          license: library.license,
          sourceUrl: library.sourceUrl,
          licenseUrl: library.licenseUrl,
          categories: library.categories,
          componentCount: library.components.length,
          variantCount: library.components.reduce((total, component) => total + (library.variants[component.id]?.length ?? 1), 0)
        })),
        sections: WEB_DESIGN_BLOCK_PRESETS,
        pageTemplates: WEB_DESIGN_PAGE_TEMPLATES,
        themes: WEB_DESIGN_THEME_PRESETS
      };
    case 'web_design_search_components': {
      const query = typeof argumentsValue.query === 'string' ? argumentsValue.query.trim().toLocaleLowerCase() : '';
      const libraryId = typeof argumentsValue.libraryId === 'string' ? argumentsValue.libraryId : undefined;
      const category = typeof argumentsValue.category === 'string' ? argumentsValue.category.trim().toLocaleLowerCase() : '';
      const includeDeprecated = argumentsValue.includeDeprecated === true;
      const limit = typeof argumentsValue.limit === 'number' ? Math.max(1, Math.min(50, Math.trunc(argumentsValue.limit))) : 20;
      const candidates = UI_LIBRARIES
        .filter((library) => !libraryId || library.id === libraryId)
        .flatMap((library) => library.components.map((component) => ({ library, component })))
        .filter(({ component }) => includeDeprecated || component.status !== 'deprecated')
        .filter(({ component }) => !category || component.category.toLocaleLowerCase() === category)
        .filter(({ component }) => {
          if (!query) return true;
          return [component.id, component.label, component.category, component.baseType, ...component.keywords]
            .some((value) => value.toLocaleLowerCase().includes(query));
        })
        .slice(0, limit)
        .map(({ library, component }) => ({
          libraryId: library.id,
          libraryName: library.displayName,
          libraryVersion: library.version,
          componentId: component.id,
          label: component.label,
          category: component.category,
          baseType: component.baseType,
          defaultSize: { width: component.width, height: component.height },
          variantCount: library.variants[component.id]?.length ?? 1,
          keywords: component.keywords,
          status: component.status ?? 'stable',
          docsUrl: component.docsUrl
        }));
      return { query, count: candidates.length, candidates };
    }
    case 'web_design_get_component_contract': {
      const library = UI_LIBRARIES.find((candidate) => candidate.id === argumentsValue.libraryId);
      if (!library) throw new Error(`UI library not found: ${String(argumentsValue.libraryId)}`);
      const component = library.components.find((candidate) => candidate.id === argumentsValue.componentId);
      if (!component) throw new Error(`${library.displayName} component not found: ${String(argumentsValue.componentId)}`);
      const variants = library.variants[component.id] ?? [{ id: 'default', label: '默认款式', props: {} }];
      const defaultVariant = variants[0];
      const instance = createComponentFromUiLibrary(library.id, component.id, 0, 0);
      return {
        library: {
          id: library.id,
          name: library.displayName,
          version: library.version,
          license: library.license,
          sourceUrl: library.sourceUrl,
          licenseUrl: library.licenseUrl
        },
        component: {
          ...component,
          variants,
          bindingTemplate: {
            name: library.id,
            version: library.version,
            component: component.id,
            variant: defaultVariant.id,
            props: { ...(component.props ?? {}), ...defaultVariant.props }
          },
          editableSlots: editableSlotsForUiComponent(instance)
        }
      };
    }
    case 'web_design_insert_section': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const document = await store.insertSection(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          pageId,
          String(argumentsValue.sectionId) as (typeof WEB_DESIGN_BLOCK_PRESETS)[number]['id']
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_apply_page_template': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      const document = await store.applyPageTemplate(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          pageId,
          String(argumentsValue.templateId) as (typeof WEB_DESIGN_PAGE_TEMPLATES)[number]['id']
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_replace_document': {
      const document: unknown = argumentsValue.document;
      assertWebDesignDocument(document);
      await store.readInScope((document as WebDesignDocument).documentId, scopeKey);
      const saved = await store.replace(document as WebDesignDocument, Number(argumentsValue.expectedRevision));
      return compactMutationResult(saved, { changedIds: saved.components.map((component) => component.id) });
    }
    case 'web_design_apply_patch': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const operations = argumentsValue.operations as WebDesignPatchOperation[];
      const pageId = assertFocusedOperations(current, operations);
      const document = await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          operations
        );
      return compactMutationResult(document, { pageId, changedIds: changedComponentIds(operations) });
    }
    case 'web_design_apply_node_batch': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const pageId = String(argumentsValue.pageId);
      if (!pagesForDocument(current).some((page) => page.id === pageId)) throw new Error(`Page not found: ${pageId}`);
      const components = argumentsValue.components as WebDesignComponent[];
      if (components.some((component) => component.pageId !== pageId)) {
        throw new Error('Every component in a node batch must use the requested pageId.');
      }
      const serializedBytes = Buffer.byteLength(JSON.stringify(components), 'utf8');
      if (serializedBytes > 65_536) throw new Error(`Node batch is ${serializedBytes} bytes. Split the logical region into smaller batches.`);
      const operations: WebDesignPatchOperation[] = components.map((component) => ({ op: 'upsert_component', component }));
      const document = await store.patch(String(argumentsValue.documentId), Number(argumentsValue.expectedRevision), operations);
      return compactMutationResult(document, {
        pageId,
        regionName: String(argumentsValue.regionName),
        changedIds: components.map((component) => component.id)
      });
    }
    case 'web_design_auto_layout': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.containerId);
      const pageId = componentPageId(current, componentId);
      const document = await store.autoLayout(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          componentId,
          String(argumentsValue.device) as 'desktop' | 'tablet' | 'mobile'
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document, pageId) });
    }
    case 'web_design_sync_symbol_instances': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const document = await store.syncSymbolInstances(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.symbolId)
        );
      return compactMutationResult(document, { changedIds: changedIdsBetween(current, document) });
    }
    case 'web_design_update_symbol_from_instance': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const componentId = String(argumentsValue.componentId);
      const pageId = componentPageId(current, componentId);
      const document = await store.updateSymbolFromInstance(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          componentId
        );
      return compactMutationResult(document, { pageId, changedIds: changedIdsBetween(current, document) });
    }
    case 'web_design_export_html': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      if (typeof argumentsValue.pageId === 'string') {
        const pageId = argumentsValue.pageId;
        assertHandoffQuality(document, pageId);
        const file = exportDocumentHtmlFiles(document, device).find((candidate) => candidate.pageId === pageId);
        if (!file) throw new Error(`Page not found: ${pageId}`);
        return { files: [file] };
      }
      assertHandoffQuality(document);
      return { files: exportDocumentHtmlFiles(document, device) };
    }
    case 'web_design_export_react': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      assertHandoffQuality(document);
      return { files: [exportReactComponent(document, device)] };
    }
    case 'web_design_export_vue': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const device = (typeof argumentsValue.device === 'string' ? argumentsValue.device : 'desktop') as 'desktop' | 'tablet' | 'mobile';
      assertHandoffQuality(document);
      return { files: [exportVueComponent(document, device)] };
    }
    case 'web_design_list_requests': {
      const includeResolved = argumentsValue.includeResolved === true;
      if (typeof argumentsValue.documentId === 'string') {
        return { requests: await requestEntries(argumentsValue.documentId, includeResolved) };
      }
      const summaries = await store.listInScope(scopeKey);
      const requests = (await Promise.all(summaries.map((item) => requestEntries(item.documentId, includeResolved)))).flat();
      return { scope: runtimeScope(), requests };
    }
    case 'web_design_resolve_request': {
      const current = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const requestId = String(argumentsValue.requestId);
      const request = current.requests.find((candidate) => candidate.id === requestId);
      const pageId = request?.componentId ? componentPageId(current, request.componentId) : undefined;
      const document = await store.patch(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          [{
            op: 'resolve_request',
            requestId,
            resolution: typeof argumentsValue.resolution === 'string' ? argumentsValue.resolution : undefined
          }]
        );
      return compactMutationResult(document, { pageId, changedIds: request?.componentId ? [request.componentId] : [] });
    }
    case 'web_design_validate': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      return validateWebDesignDocument(document, {
        pageId: typeof argumentsValue.pageId === 'string' ? argumentsValue.pageId : undefined,
        mode: argumentsValue.mode === 'draft' ? 'draft' : 'handoff'
      });
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
  await store.ensureLegacyProject();
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
