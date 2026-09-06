import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { RevisionConflictError, WebDesignDocumentStore } from './document-store.js';
import { breakpointFor, resolveComponent } from './editor-model.js';
import { exportDocumentHtmlFiles } from './html-exporter.js';
import { exportReactComponent } from './react-exporter.js';
import { exportVueComponent } from './vue-exporter.js';
import { editableSlotsForUiComponent, isUiContentContainer } from './library-slots.js';
import { createComponentFromUiLibrary, UI_LIBRARIES } from './ui-libraries.js';
import { WEB_DESIGN_THEME_PRESETS } from './design-themes.js';
import { createBlankWebsite } from './templates.js';
import { WEB_DESIGN_BLOCK_PRESETS, WEB_DESIGN_PAGE_TEMPLATES } from './component-library.js';
import {
  assertWebDesignDocument,
  designSummary,
  type WebDesignDocument,
  type WebDesignPatchOperation
} from './schema.js';

const SERVER_NAME = 'chatos-web-design-studio';
const SERVER_VERSION = '0.9.0';
const store = new WebDesignDocumentStore();

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 100_000
};

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
    name: 'web_design_get_component_library',
    description: 'Read independently grouped design-library components, including Ant Design, Chakra UI, shadcn/ui and licensed creative libraries, plus variants, editable slots, sample data, insertion sizes, production sections, page templates, and visual themes. Children placed in a slot use parentId plus slot.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 300_000 }
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
    description: 'Apply focused operations without replacing unrelated edits. Component style supports gradients, fill/stroke, effects, transforms, typography, and media fit; component states supports hover/active/focus style overrides; per-device constraints support min/max size and aspect-ratio locking.',
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

const webDesignSkillEvidence = {
  type: 'array',
  minItems: 1,
  maxItems: 8,
  items: { type: 'string', minLength: 1 },
  description: 'Platform-issued activation evidence for Web Design Studio and the specialist workflow required by this tool. ChatOS validates and removes it before local execution.'
} as const;

function webDesignToolSkills(name: string): string[] {
  if (name === 'web_design_replace_document'
    || name === 'web_design_insert_section' || name === 'web_design_apply_page_template') {
    return ['web-design-components', 'web-design-responsive-layout', 'web-design-visual-system'];
  }
  if (name === 'web_design_apply_patch') return ['web-design-components'];
  if (name.includes('auto_layout')) return ['web-design-responsive-layout'];
  if (name.includes('component_library') || name.includes('symbol')) return ['web-design-components'];
  if (name.includes('export') || name.includes('validate')) return ['web-design-validation-export'];
  return ['web-design-projects'];
}

const TOOL_DEFINITIONS = TOOL_DEFINITIONS_BASE.map((tool) => ({
  ...tool,
  inputSchema: {
    ...tool.inputSchema,
    properties: {
      ...tool.inputSchema.properties,
      skillEvidence: webDesignSkillEvidence
    },
    required: [...('required' in tool.inputSchema ? tool.inputSchema.required : []), 'skillEvidence']
  },
  _meta: {
    ...tool._meta,
    'chatos/skillGate': {
      evidenceArgument: 'skillEvidence',
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
      return { scope: runtimeScope(), documents: typeof argumentsValue.projectId === 'string' ? await store.listInProject(argumentsValue.projectId) : await store.list() };
    case 'web_design_list_projects':
      return { scope: runtimeScope(), projects: await store.listProjects() };
    case 'web_design_create_project':
      return { scope: runtimeScope(), project: await store.createProject(String(argumentsValue.name), typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined) };
    case 'web_design_get_project': {
      const projectId = String(argumentsValue.projectId);
      return { scope: runtimeScope(), project: await store.readProject(projectId), documents: await store.listInProject(projectId) };
    }
    case 'web_design_update_project':
      return { project: await store.updateProject(String(argumentsValue.projectId), { name: typeof argumentsValue.name === 'string' ? argumentsValue.name : undefined, description: typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined }) };
    case 'web_design_delete_project':
      await store.deleteProject(String(argumentsValue.projectId), argumentsValue.deleteDocuments === true);
      return { deleted: true, projectId: String(argumentsValue.projectId) };
    case 'web_design_move_document':
      return store.moveDocument(String(argumentsValue.documentId), String(argumentsValue.targetProjectId), typeof argumentsValue.sourceProjectId === 'string' ? argumentsValue.sourceProjectId : undefined);
    case 'web_design_create_document': {
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const document = typeof argumentsValue.projectId === 'string'
        ? await store.createInProject(argumentsValue.projectId, title, argumentsValue.blank === true)
        : argumentsValue.blank === true
          ? await store.writeNew(createBlankWebsite(title))
          : await store.create(title);
      return { scope: runtimeScope(), document: designSummary(document) };
    }
    case 'web_design_get_document':
      return { document: await store.read(String(argumentsValue.documentId)) };
    case 'web_design_get_component_library':
      return {
        libraries: UI_LIBRARIES.map((library) => ({
          id: library.id,
          name: library.displayName,
          version: library.version,
          license: library.license,
          sourceUrl: library.sourceUrl,
          licenseUrl: library.licenseUrl,
          categories: library.categories,
          components: library.components.map((component) => ({
            ...component,
            variants: library.variants[component.id] ?? [{ id: 'default', label: '默认款式', props: {} }],
            editableSlots: editableSlotsForUiComponent(createComponentFromUiLibrary(library.id, component.id, 0, 0))
          }))
        })),
        sections: WEB_DESIGN_BLOCK_PRESETS,
        pageTemplates: WEB_DESIGN_PAGE_TEMPLATES,
        themes: WEB_DESIGN_THEME_PRESETS
      };
    case 'web_design_insert_section':
      return {
        document: await store.insertSection(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.pageId),
          String(argumentsValue.sectionId) as (typeof WEB_DESIGN_BLOCK_PRESETS)[number]['id']
        )
      };
    case 'web_design_apply_page_template':
      return {
        document: await store.applyPageTemplate(
          String(argumentsValue.documentId),
          Number(argumentsValue.expectedRevision),
          String(argumentsValue.pageId),
          String(argumentsValue.templateId) as (typeof WEB_DESIGN_PAGE_TEMPLATES)[number]['id']
        )
      };
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
        if (component.slot && isUiContentContainer(parent)) return [];
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
        return parent && !['section', 'card'].includes(parent.type) && !isUiContentContainer(parent)
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
