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
import { GenerationCandidateStore } from './v2/generation-candidate-store.js';
import { GenerationPlanRevisionConflictError, GenerationPlanStore } from './v2/generation-plan-store.js';
import { GenerationSoftProtectionStore } from './v2/generation-soft-protection-store.js';
import { GenerationVisualArtifactStore } from './v2/generation-visual-artifact-store.js';
import { GenerationVisualService, type ToolImagePayload } from './v2/generation-visual-service.js';
import { ChromiumSceneImageRenderer } from './v2/headless-scene-renderer.js';
import { AnnotationAiService } from './v2/annotation-ai-service.js';
import { ProgressiveGenerationService } from './v2/progressive-generation-service.js';
import { executeSceneEditorCommand, type SceneEditorCommand } from './v2/scene-editor-command.js';
import { jsonEncodedValueSchema, jsonScalarValueSchema, stringLiteralSchema } from './json-schema.js';
import { SceneQueryIndex, type SceneQuery } from './v2/scene-query.js';
import { createSceneNodeBase, indexSceneDocument, type SceneDocument, type SceneNode, type SceneNodeType } from './v2/scene-schema.js';
import { SceneDocumentStore, SceneRevisionConflictError } from './v2/scene-store.js';
import type { CreateGenerationStepInput, GenerationArtifact, GenerationDesignIntent } from './v2/generation-plan-schema.js';
import type { SceneTransactionOperation } from './v2/scene-transaction.js';
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
const SERVER_VERSION = '3.0.23';
const store = new WebDesignDocumentStore();
await store.initialize();
const scopeKey = runtimeScopeFingerprint(store.rootDirectory);
const defaultProject = await store.ensureScopedProject(
  scopeKey,
  process.env.CHATOS_CONTEXT_SCOPE === 'project' && process.env.CHATOS_PROJECT_ID
    ? process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 网站项目'
    : '公共网站设计',
  { consolidateDefaultProjects: process.env.CHATOS_CONTEXT_SCOPE === 'project' }
);
const generationRepositories = {
  plans: new GenerationPlanStore(store.rootDirectory),
  scenes: new SceneDocumentStore(store.rootDirectory),
  candidates: new GenerationCandidateStore(store.rootDirectory),
  protections: new GenerationSoftProtectionStore(store.rootDirectory),
  visualArtifacts: new GenerationVisualArtifactStore(store.rootDirectory)
};

async function assertGenerationDocumentInScope(documentId: string): Promise<{ name: string }> {
  const document = await store.readInScope(documentId, scopeKey);
  return { name: document.title };
}

function progressiveGenerationService(): ProgressiveGenerationService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Progressive website generation requires a ChatOS project context with a host-injected projectId.');
  const visuals = generationVisualService();
  return new ProgressiveGenerationService({
    projectId,
    repositories: generationRepositories,
    assertDocumentInScope: assertGenerationDocumentInScope,
    verifyCandidate: (input) => visuals.verifyCandidate(input),
    captureVisualInputs: async ({ documentId, pageId, viewportWidths }) => {
      const captures = await Promise.all(viewportWidths.map((viewportWidth) => visuals.capturePage(documentId, pageId, viewportWidth)));
      return captures.flatMap((capture) => (capture.artifacts as GenerationArtifact[])
        .filter((artifact) => artifact.kind === 'page-snapshot' || artifact.kind === 'visual-grounding'));
    },
    loadArtifactImages: (scope, artifacts) => visuals.loadArtifactImages(scope.documentId, artifacts)
  });
}

function generationVisualService(): GenerationVisualService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Visual website inspection requires a ChatOS project context with a host-injected projectId.');
  return new GenerationVisualService({
    projectId,
    scenes: generationRepositories.scenes,
    artifacts: generationRepositories.visualArtifacts,
    renderer: new ChromiumSceneImageRenderer(),
    assertDocumentInScope: async (documentId) => { await assertGenerationDocumentInScope(documentId); }
  });
}

function annotationAiService(): AnnotationAiService {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Annotation AI tasks require a ChatOS project context with a host-injected projectId.');
  return new AnnotationAiService({
    projectId,
    scenes: generationRepositories.scenes,
    visuals: generationVisualService(),
    assertDocumentInScope: async (documentId) => { await assertGenerationDocumentInScope(documentId); }
  });
}

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
      properties: { op: stringLiteralSchema('set_title'), title: { type: 'string', minLength: 1, maxLength: 240 } },
      required: ['op', 'title'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('set_description'), description: { type: 'string', maxLength: 4000 } },
      required: ['op', 'description'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('set_viewport'),
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
        op: stringLiteralSchema('set_breakpoint'),
        device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] },
        width: { type: 'number', minimum: 1 },
        height: { type: 'number', minimum: 1 }
      },
      required: ['op', 'device', 'width', 'height'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('upsert_page'), page: { type: 'object', description: 'Complete page object with id, name, and slash-prefixed slug.' } },
      required: ['op', 'page'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('remove_page'), pageId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'pageId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('upsert_asset'), asset: { type: 'object', description: 'Complete image asset object matching the document schema.' } },
      required: ['op', 'asset'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('remove_asset'), assetId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'assetId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('set_tokens'), tokens: { type: 'object', description: 'Complete color, radii, and typography token groups.' } },
      required: ['op', 'tokens'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('upsert_symbol'), symbol: { type: 'object', description: 'Complete reusable symbol object matching the document schema.' } },
      required: ['op', 'symbol'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('remove_symbol'), symbolId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'symbolId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('upsert_component'), component: webDesignComponentSchema },
      required: ['op', 'component'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('remove_component'), componentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['op', 'componentId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('set_parent'),
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
        op: stringLiteralSchema('set_layout'),
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
            op: stringLiteralSchema(op),
            componentId: { type: 'string', minLength: 1, maxLength: 128 },
            x: { type: 'number' },
            y: { type: 'number' },
            device: { type: 'string', enum: ['desktop', 'tablet', 'mobile'] }
          }
        : {
            op: stringLiteralSchema(op),
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
        op: stringLiteralSchema('update_component'),
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
        op: stringLiteralSchema('add_annotation'),
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        annotation: { type: 'object', description: 'Complete annotation with stable id, author, text, status, and timestamps.' }
      },
      required: ['op', 'componentId', 'annotation'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('resolve_annotation'),
        componentId: { type: 'string', minLength: 1, maxLength: 128 },
        annotationId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['op', 'componentId', 'annotationId'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('add_request'), request: { type: 'object', description: 'Complete AI work request with stable id, prompt, status, timestamps, and optional pageId or componentId.' } },
      required: ['op', 'request'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('resolve_request'),
        requestId: { type: 'string', minLength: 1, maxLength: 128 },
        resolution: { type: 'string', maxLength: 4000 }
      },
      required: ['op', 'requestId'],
      additionalProperties: false
    }
  ]
} as const;

const generationDesignIntentSchema = {
  type: 'object',
  description: 'Visual design intent is mandatory. Interaction intents are optional and cannot replace art direction or composition decisions.',
  properties: {
    artDirection: { type: 'string', minLength: 1, maxLength: 4000 },
    compositionIntent: { type: 'string', minLength: 1, maxLength: 4000 },
    typographyIntent: { type: 'string', minLength: 1, maxLength: 4000 },
    imageStrategy: { type: 'string', minLength: 1, maxLength: 4000 },
    contentHierarchy: { type: 'array', minItems: 1, maxItems: 40, items: { type: 'string', minLength: 1, maxLength: 1000 } },
    designAcceptanceCriteria: { type: 'array', minItems: 2, maxItems: 40, items: { type: 'string', minLength: 1, maxLength: 1000 } },
    interactionIntents: { type: 'array', maxItems: 40, items: { type: 'string', minLength: 1, maxLength: 1000 } }
  },
  required: ['artDirection', 'compositionIntent', 'typographyIntent', 'imageStrategy', 'contentHierarchy', 'designAcceptanceCriteria', 'interactionIntents'],
  additionalProperties: false
} as const;

const generationStepSchema = {
  type: 'object',
  properties: {
    stepId: { type: 'string', minLength: 1, maxLength: 160 },
    title: { type: 'string', minLength: 1, maxLength: 240 },
    kind: { type: 'string', enum: ['structure', 'section', 'visual', 'design-gate', 'interaction', 'responsive', 'polish', 'handoff'] },
    required: { type: 'boolean', default: true },
    dependsOn: { type: 'array', maxItems: 64, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
    target: {
      type: 'object',
      properties: {
        sectionKey: { type: 'string', minLength: 1, maxLength: 160 },
        nodeIds: { type: 'array', maxItems: 64, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
        viewportWidths: { type: 'array', maxItems: 12, uniqueItems: true, items: { type: 'integer', minimum: 240, maximum: 10000 } }
      },
      additionalProperties: false
    }
  },
  required: ['stepId', 'title', 'kind'],
  additionalProperties: false
} as const;

const generationArtifactSchema = {
  type: 'object',
  properties: {
    artifactId: { type: 'string', minLength: 1, maxLength: 160 },
    kind: { type: 'string', enum: ['candidate-transaction', 'scene-diff', 'layout', 'page-snapshot', 'region-crop', 'visual-grounding', 'visual-diff', 'calibration', 'quality-report'] },
    revision: { type: 'integer', minimum: 0 },
    createdAt: { type: 'string', minLength: 1, maxLength: 100 },
    viewportWidth: { type: 'integer', minimum: 240, maximum: 10000 },
    nodeIds: { type: 'array', maxItems: 256, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
    uri: { type: 'string', minLength: 1, maxLength: 4000 },
    sha256: { type: 'string', pattern: '^[a-f0-9]{64}$' },
    metadata: { type: 'object', additionalProperties: { oneOf: [{ type: 'string' }, { type: 'number' }, { type: 'boolean' }] } }
  },
  required: ['artifactId', 'kind', 'revision', 'createdAt'],
  additionalProperties: false
} as const;

const simpleSceneFrameSchema = {
  type: 'object',
  properties: {
    x: { type: 'number', minimum: -100000, maximum: 100000 },
    y: { type: 'number', minimum: -100000, maximum: 100000 },
    width: { type: 'number', exclusiveMinimum: 0, maximum: 100000 },
    height: { type: 'number', exclusiveMinimum: 0, maximum: 100000 }
  },
  required: ['x', 'y', 'width', 'height'],
  additionalProperties: false
} as const;

const simpleSceneNodeSchema = {
  type: 'object',
  description: 'Preferred safe node input. The plugin supplies every required Scene v2 base field and makes paints visible. Insert parents before children in the same transaction.',
  properties: {
    id: { type: 'string', minLength: 1, maxLength: 160, pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' },
    type: { type: 'string', enum: ['frame', 'group', 'text', 'shape', 'library-instance'] },
    name: { type: 'string', minLength: 1, maxLength: 240 },
    role: { type: 'string', minLength: 1, maxLength: 160 },
    frame: simpleSceneFrameSchema,
    content: { type: 'string', maxLength: 12000, description: 'Required for text. One semantic text item only.' },
    shape: { type: 'string', enum: ['rectangle', 'ellipse', 'line', 'polygon', 'star', 'vector'] },
    layout: {
      type: 'object',
      description: 'Optional layout overrides. Defaults are free/fixed/flow with zero padding and gap.',
      properties: {
        mode: { type: 'string', enum: ['free', 'auto', 'grid'] },
        direction: { type: 'string', enum: ['horizontal', 'vertical'] },
        wrap: { type: 'boolean' },
        padding: { type: 'number', minimum: 0, maximum: 2000 },
        gap: { type: 'number', minimum: 0, maximum: 2000 },
        alignItems: { type: 'string', enum: ['start', 'center', 'end', 'stretch', 'baseline'] },
        justifyContent: { type: 'string', enum: ['start', 'center', 'end', 'between', 'around', 'evenly'] },
        sizingX: { type: 'string', enum: ['fixed', 'hug', 'fill'] },
        sizingY: { type: 'string', enum: ['fixed', 'hug', 'fill'] },
        position: { type: 'string', enum: ['flow', 'absolute'] },
        clipContent: { type: 'boolean' }
      },
      additionalProperties: false
    },
    style: {
      type: 'object',
      description: 'Optional safe visual styling. fill is used for both surfaces and text color; generated paints always include visible:true.',
      properties: {
        fill: { type: 'string', minLength: 1, maxLength: 120 },
        fillOpacity: { type: 'number', minimum: 0, maximum: 1 },
        opacity: { type: 'number', minimum: 0, maximum: 1 },
        radius: { type: 'number', minimum: 0, maximum: 2000 },
        stroke: { type: 'string', minLength: 1, maxLength: 120 },
        strokeWidth: { type: 'number', minimum: 0, maximum: 200 },
        shadowColor: { type: 'string', minLength: 1, maxLength: 120 },
        shadowRadius: { type: 'number', minimum: 0, maximum: 500 },
        shadowOffsetX: { type: 'number', minimum: -1000, maximum: 1000 },
        shadowOffsetY: { type: 'number', minimum: -1000, maximum: 1000 },
        fontFamily: { type: 'string', minLength: 1, maxLength: 300 },
        fontSize: { type: 'number', minimum: 1, maximum: 1000 },
        fontWeight: { type: 'number', minimum: 1, maximum: 1000 },
        lineHeight: { type: 'number', minimum: 0.5, maximum: 4, description: 'Unitless multiplier of fontSize. Use values such as 1.1 for display text or 1.5 for body text.' },
        letterSpacing: { type: 'number', minimum: -100, maximum: 500 },
        textAlign: { type: 'string', enum: ['left', 'center', 'right', 'justify'] }
      },
      additionalProperties: false
    },
    library: { type: 'string', minLength: 1, maxLength: 160, description: 'For library-instance, copy from sceneBindingTemplate.' },
    component: { type: 'string', minLength: 1, maxLength: 160, description: 'For library-instance, copy from sceneBindingTemplate.' },
    variant: { type: 'string', maxLength: 160 },
    properties: { type: 'object', additionalProperties: true },
    slots: { type: 'object', additionalProperties: { type: 'array', maxItems: 0, items: { type: 'object' } } }
  },
  required: ['id', 'type', 'name', 'frame'],
  additionalProperties: false
} as const;

const generationSceneOperationSchema = {
  oneOf: [
    {
      type: 'object',
      description: 'Insert a complete editable hierarchy in one compact operation. tree is recursive: { node: <simple Scene node>, children?: [<tree>...] }. Frame and group nodes may have children; every descendant remains an independent stable Scene node.',
      properties: {
        op: stringLiteralSchema('insert-simple-tree'),
        parentId: { type: 'string', minLength: 1, maxLength: 160 },
        index: { type: 'integer', minimum: 0, maximum: 100000 },
        slot: { type: 'string', minLength: 1, maxLength: 160 },
        tree: {
          type: 'object',
          properties: {
            node: simpleSceneNodeSchema,
            children: {
              type: 'array', maxItems: 512,
              items: { type: 'object', description: 'Recursive tree item with the same {node, children?} shape.' }
            }
          },
          required: ['node'], additionalProperties: false
        }
      },
      required: ['op', 'parentId', 'index', 'tree'], additionalProperties: false
    },
    {
      type: 'object',
      description: 'Preferred insertion operation. Use this for custom frames, groups, text, shapes, and component-contract library instances; defaults prevent incomplete invisible Scene nodes.',
      properties: {
        op: stringLiteralSchema('insert-simple-node'), parentId: { type: 'string', minLength: 1, maxLength: 160 },
        index: { type: 'integer', minimum: 0, maximum: 100000 }, slot: { type: 'string', minLength: 1, maxLength: 160 },
        node: simpleSceneNodeSchema
      },
      required: ['op', 'parentId', 'index', 'node'], additionalProperties: false
    },
    {
      type: 'object',
      description: 'Advanced raw insertion. Prefer insert-simple-node. Raw nodes must include the full Scene v2 base contract, including visible/locked, frame, transform, layout, appearance (fills with visible), variableBindings, annotations, aiPolicy, creator fields, timestamps, and type-specific children/content.',
      properties: {
        op: stringLiteralSchema('insert-node'), parentId: { type: 'string', minLength: 1, maxLength: 160 },
        index: { type: 'integer', minimum: 0, maximum: 100000 }, slot: { type: 'string', minLength: 1, maxLength: 160 },
        node: { type: 'object', description: 'A complete Scene v2 node or detached subtree with stable semantic IDs.' }
      },
      required: ['op', 'parentId', 'index', 'node'], additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('update-node'), nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        patches: {
          type: 'array', minItems: 1, maxItems: 64,
          items: {
            oneOf: [
              {
                type: 'object',
                properties: {
                  path: { type: 'array', minItems: 1, maxItems: 16, items: { type: 'string', minLength: 1, maxLength: 160 } },
                  value: jsonScalarValueSchema
                },
                required: ['path', 'value'], additionalProperties: false
              },
              {
                type: 'object',
                properties: {
                  path: { type: 'array', minItems: 1, maxItems: 16, items: { type: 'string', minLength: 1, maxLength: 160 } },
                  valueJson: jsonEncodedValueSchema
                },
                required: ['path', 'valueJson'], additionalProperties: false
              }
            ]
          }
        }
      },
      required: ['op', 'nodeId', 'patches'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('remove-node'), nodeId: { type: 'string', minLength: 1, maxLength: 160 } },
      required: ['op', 'nodeId'], additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        op: stringLiteralSchema('move-node'), nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        parentId: { type: 'string', minLength: 1, maxLength: 160 }, index: { type: 'integer', minimum: 0, maximum: 100000 },
        slot: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['op', 'nodeId', 'parentId', 'index'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('insert-variable-collection'), index: { type: 'integer', minimum: 0, maximum: 10000 }, collection: { type: 'object' } },
      required: ['op', 'index', 'collection'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { op: stringLiteralSchema('insert-responsive-rule'), index: { type: 'integer', minimum: 0, maximum: 10000 }, rule: { type: 'object' } },
      required: ['op', 'index', 'rule'], additionalProperties: false
    }
  ]
} as const;

const progressiveStepExecutionProperties = {
  documentId: { type: 'string', minLength: 1, maxLength: 128 },
  expectedPlanRevision: { type: 'integer', minimum: 0 },
  attemptId: { type: 'string', minLength: 1, maxLength: 160 },
  idempotencyKey: { type: 'string', minLength: 1, maxLength: 160 },
  transactionId: { type: 'string', minLength: 1, maxLength: 160 },
  operations: { type: 'array', minItems: 1, maxItems: 64, items: generationSceneOperationSchema },
  visualInputs: { type: 'array', minItems: 2, maxItems: 40, items: generationArtifactSchema }
} as const;

const visualRectSchema = {
  type: 'object',
  properties: {
    x: { type: 'number', minimum: -100000, maximum: 100000 },
    y: { type: 'number', minimum: -100000, maximum: 100000 },
    width: { type: 'number', exclusiveMinimum: 0, maximum: 10000 },
    height: { type: 'number', exclusiveMinimum: 0, maximum: 50000 }
  },
  required: ['x', 'y', 'width', 'height'],
  additionalProperties: false
} as const;

const TOOL_DEFINITIONS_BASE = [
  {
    name: 'web_design_get_active_context',
    description: 'Read the immutable ChatOS project scope, active document/page hints, current selection, pending human requests, resumable generation plan, and delivery gate. Follow the returned nextAction before unrelated implementation work or task completion. This tool accepts no projectId.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_plan_site',
    description: 'Create or revise only the semantic artboard inventory and site objective. This planning-only call never creates visible Scene content and is not a deliverable. In the same task run, immediately continue with the returned deliveryGate.requiredNextAction.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        planId: { type: 'string', minLength: 1, maxLength: 160 },
        mode: { type: 'string', enum: ['guided', 'auto-current-page', 'review-sensitive'], default: 'auto-current-page' },
        objective: { type: 'string', minLength: 1, maxLength: 12000 },
        audience: { type: 'array', minItems: 1, maxItems: 40, items: { type: 'string', minLength: 1, maxLength: 1000 } },
        pages: {
          type: 'array', minItems: 1, maxItems: 100,
          items: {
            type: 'object',
            properties: {
              pageId: { type: 'string', minLength: 1, maxLength: 160 },
              name: { type: 'string', minLength: 1, maxLength: 240 },
              purpose: { type: 'string', minLength: 1, maxLength: 4000 }
            },
            required: ['pageId', 'name', 'purpose'], additionalProperties: false
          }
        }
      },
      required: ['documentId', 'objective', 'audience', 'pages'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_plan_page',
    description: 'Plan one semantic artboard only: save its visual direction, content hierarchy, acceptance criteria, and bounded step dependency graph. It does not create visible content; immediately continue with the returned deliveryGate.requiredNextAction.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        design: generationDesignIntentSchema,
        steps: { type: 'array', minItems: 1, maxItems: 64, items: generationStepSchema }
      },
      required: ['documentId', 'expectedPlanRevision', 'pageId', 'design', 'steps'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_get_plan',
    description: 'Read a compact generation-plan summary with stable IDs, current page/step, delivery gate, status counts, and exactly one required next action.',
    inputSchema: {
      type: 'object', properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_start_page',
    description: 'Start exactly one planned artboard and ensure its empty canonical Scene root frame exists. The root alone is still visually empty and cannot satisfy delivery; capture it and run the returned visual Step next.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'expectedPlanRevision', 'pageId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_capture_page',
    description: 'Render one Scene v2 page with Chromium and return the actual PNG plus persistent snapshot, layout, grounding, and calibration artifacts for the exact Scene revision.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'pageId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_capture_region',
    description: 'Render and return a PNG crop from one page using either a stable Scene nodeId or an explicit page-space rectangle. Grounding coordinates are relative to the returned crop.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 },
        nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        rect: visualRectSchema,
        padding: { type: 'number', minimum: 0, maximum: 1000, default: 16 }
      },
      required: ['documentId', 'pageId'],
      anyOf: [{ required: ['nodeId'] }, { required: ['rect'] }],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_prepare_annotation_task',
    description: 'Prepare one open human Scene annotation as a revision-bound AI task and return a fresh PNG crop with stable-node grounding. Use this before editing the annotated target.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        nodeId: { type: 'string', minLength: 1, maxLength: 160 },
        annotationId: { type: 'string', minLength: 1, maxLength: 160 },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 },
        dependencyNodeIds: { type: 'array', maxItems: 64, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
        padding: { type: 'number', minimum: 0, maximum: 1000, default: 24 }
      },
      required: ['documentId', 'nodeId', 'annotationId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_get_visual_grounding',
    description: 'Read the persistent mapping from stable Scene node IDs to rectangles for a previously captured PNG and return that same image for visual grounding.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artifactId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'artifactId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_compare_snapshots',
    description: 'Compare two persisted snapshots of the same page and viewport. Returns before, after, and highlighted Diff PNGs with changed regions and affected stable node IDs.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        beforeArtifactId: { type: 'string', minLength: 1, maxLength: 160 },
        afterArtifactId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'beforeArtifactId', 'afterArtifactId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_inspect_at_point',
    description: 'Inspect a point in a captured PNG and return selectable stable Scene node candidates ordered from the smallest visible hit, including their ancestor path and the source image.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artifactId: { type: 'string', minLength: 1, maxLength: 160 },
        x: { type: 'number', minimum: 0, maximum: 10000 },
        y: { type: 'number', minimum: 0, maximum: 50000 },
        limit: { type: 'integer', minimum: 1, maximum: 50, default: 12 }
      },
      required: ['documentId', 'artifactId', 'x', 'y'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_query_scene',
    description: 'Read a bounded flat set of editable nodes from exactly one artboard chosen from get_active_context.artboardDirectory. Never loads several artboards into model context. Containers never recursively repeat descendants.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artboardId: { type: 'string', minLength: 1, maxLength: 160 },
        query: {
          type: 'object',
          properties: {
            ids: { type: 'array', minItems: 1, maxItems: 256, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
            types: { type: 'array', minItems: 1, maxItems: 10, uniqueItems: true, items: { type: 'string', enum: ['section', 'frame', 'group', 'text', 'shape', 'media', 'library-instance', 'component-main', 'component-set', 'component-instance'] } },
            roles: { type: 'array', minItems: 1, maxItems: 100, uniqueItems: true, items: { type: 'string', minLength: 1, maxLength: 160 } },
            name: {
              type: 'object',
              properties: {
                equals: { type: 'string', minLength: 1, maxLength: 240 },
                contains: { type: 'string', minLength: 1, maxLength: 240 },
                startsWith: { type: 'string', minLength: 1, maxLength: 240 },
                caseSensitive: { type: 'boolean' }
              },
              oneOf: [{ required: ['equals'] }, { required: ['contains'] }, { required: ['startsWith'] }],
              additionalProperties: false
            },
            visible: { type: 'boolean' }, locked: { type: 'boolean' }, aiEditable: { type: 'boolean' },
            hasLockedFields: { type: 'boolean' }, limit: { type: 'integer', minimum: 1, maximum: 256, default: 100 }
          },
          additionalProperties: false
        }
      },
      required: ['documentId', 'artboardId'], additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_edit_scene',
    description: 'Apply one atomic commandJson action inside exactly one artboard chosen from get_active_context.artboardDirectory. The server validates the decoded command and rejects cross-artboard edits. Use for focused revision-safe adjustments, then capture that same artboard.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        artboardId: { type: 'string', minLength: 1, maxLength: 160 },
        transactionId: { type: 'string', minLength: 1, maxLength: 160 },
        expectedRevision: { type: 'integer', minimum: 0 },
        reason: { type: 'string', minLength: 1, maxLength: 240 },
        commandJson: { type: 'string', minLength: 2, maxLength: 262144, description: 'JSON object encoded as text. Load web-design-scene-building for command formats.' }
      },
      required: ['documentId', 'artboardId', 'transactionId', 'expectedRevision', 'commandJson'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_run_next_step',
    description: 'Create and mechanically verify one Candidate Transaction for the next ready step. Supply current-revision page captures; the plugin returns real Candidate and Diff images and waits for explicit visual review before commit.',
    inputSchema: {
      type: 'object', properties: progressiveStepExecutionProperties,
      required: ['documentId', 'expectedPlanRevision', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_retry_step',
    description: 'Retry one explicit failed, rejected, stale, or rolled-back step using fresh current-revision visual inputs. Accepted steps are never replayed.',
    inputSchema: {
      type: 'object',
      properties: { ...progressiveStepExecutionProperties, stepId: { type: 'string', minLength: 1, maxLength: 160 } },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_repair_step',
    description: 'Create one targeted repair Candidate for a failed Step using its recorded visual issue IDs and fresh current-revision visual evidence. It cannot broaden the Step target.',
    inputSchema: {
      type: 'object',
      properties: { ...progressiveStepExecutionProperties, stepId: { type: 'string', minLength: 1, maxLength: 160 } },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'idempotencyKey', 'transactionId', 'operations', 'visualInputs'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_execute_step',
    description: 'Prepare one visible Scene Candidate from operationsJson for the next or named Step. The plugin validates decoded operations, captures every required viewport, chooses first-run/retry/repair behavior, creates IDs, renders Candidate and Diff PNGs, and waits for explicit visual review.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        requestId: { type: 'string', minLength: 1, maxLength: 160 },
        operationsJson: { type: 'string', minLength: 2, maxLength: 262144, description: 'JSON array encoded as text. Load web-design-scene-building for insert-simple-tree and focused operation formats.' }
      },
      required: ['documentId', 'expectedPlanRevision', 'operationsJson'],
      additionalProperties: false
    },
    _meta: { ...policy, 'chatos/toolResultMaxChars': 500_000 }
  },
  {
    name: 'web_design_control_plan',
    description: 'Apply one lifecycle decision to the active Plan. Accept commits only the reviewed Candidate and automatically completes an accepted handoff page; reject/skip/rollback/pause/resume/start-page are revision checked. This tool never generates visual content.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        action: { type: 'string', enum: ['accept', 'reject', 'skip', 'rollback', 'pause', 'resume', 'start-page'] },
        pageId: { type: 'string', minLength: 1, maxLength: 160 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        reason: { type: 'string', minLength: 1, maxLength: 12000 },
        approveSoftProtectionConflicts: { type: 'boolean', default: false },
        viewportWidth: { type: 'integer', minimum: 240, maximum: 10000, default: 1440 }
      },
      required: ['documentId', 'expectedPlanRevision', 'action'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_inspect_step',
    description: 'Inspect one generation attempt with its target, Candidate Diff artifacts, screenshots, quality report, issue IDs, and protected-field conflicts.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 }
      },
      required: ['documentId', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_accept_step',
    description: 'Commit one already validated Candidate Transaction. Soft-protected human fields require an explicit approval flag; no other step is executed.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 },
        attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        approveSoftProtectionConflicts: { type: 'boolean', default: false }
      },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'attemptId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_reject_step',
    description: 'Reject and discard one reviewed Candidate while preserving the formal Scene and all accepted steps.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }, attemptId: { type: 'string', minLength: 1, maxLength: 160 },
        reason: { type: 'string', minLength: 1, maxLength: 12000 }
      },
      required: ['documentId', 'expectedPlanRevision', 'stepId', 'attemptId', 'reason'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_skip_step',
    description: 'Explicitly skip one ready optional Step on the active page. Required design, Design Gate, and handoff steps cannot be skipped.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_rollback_step',
    description: 'Roll back one accepted Step only when its exact transaction is still the latest Scene change. This restores the prior Scene and marks dependent steps stale instead of overwriting later human work.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        stepId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'stepId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_complete_page',
    description: 'Complete only the active artboard after its handoff Step and every required visual Step are accepted. Only completed artboards may be implemented in product source code. Continue the plan before claiming the full requested design scope is complete.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 },
        pageId: { type: 'string', minLength: 1, maxLength: 160 }
      }, required: ['documentId', 'expectedPlanRevision', 'pageId'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_pause_plan',
    description: 'Persistently pause the active page at a safe boundary. An active generating or reviewed step must be resolved first.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 }
      }, required: ['documentId', 'expectedPlanRevision'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_resume_plan',
    description: 'Resume the single paused page without starting a new page or executing a generation step.',
    inputSchema: {
      type: 'object', properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 }, expectedPlanRevision: { type: 'integer', minimum: 0 }
      }, required: ['documentId', 'expectedPlanRevision'], additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_list_documents',
    description: 'List editable website design documents in the current program-injected ChatOS scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'web_design_create_document',
    description: 'Create an empty AI-first design workspace in the current program-injected ChatOS scope. This is not visible design output: immediately plan the site, plan one artboard, start it, and accept a visible Candidate before unrelated implementation work.',
    inputSchema: {
      type: 'object',
      properties: {
        title: { type: 'string', minLength: 1, maxLength: 240 }
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
    description: 'Read a small catalog index. The default summary returns library and asset-kind counts only; request one kind for bounded library, section, template, or theme summaries. Use catalog search next instead of loading all design supply.',
    inputSchema: {
      type: 'object',
      properties: { kind: { type: 'string', enum: ['summary', 'libraries', 'sections', 'templates', 'themes'], default: 'summary' } },
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'web_design_search_catalog',
    description: 'Search one bounded catalog kind by intent or keyword. Component results are compact and require web_design_get_component_contract before insertion; section, template, and theme results are design references, not mandatory art direction.',
    inputSchema: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['components', 'sections', 'templates', 'themes'], default: 'components' },
        query: { type: 'string', maxLength: 200 },
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
    description: 'List pending or all human design requests, including Scene v2 node annotations with stable node/page/revision context and the tool call needed to prepare visual evidence.',
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
  if (name === 'web_design_get_active_context' || name === 'web_design_plan_site' || name === 'web_design_plan_page') return ['web-design-planning'];
  if (name === 'web_design_execute_step' || name === 'web_design_query_scene' || name === 'web_design_edit_scene') return ['web-design-scene-building'];
  if (name === 'web_design_control_plan' || name === 'web_design_capture_page' || name === 'web_design_capture_region'
    || name === 'web_design_compare_snapshots' || name === 'web_design_inspect_at_point'
    || name === 'web_design_prepare_annotation_task') return ['web-design-candidate-review'];
  if (name === 'web_design_replace_document'
    || name === 'web_design_insert_section' || name === 'web_design_apply_page_template') {
    return ['web-design-components', 'web-design-responsive-layout', 'web-design-visual-system'];
  }
  if (name === 'web_design_apply_patch' || name === 'web_design_apply_node_batch') return ['web-design-components', 'web-design-responsive-layout'];
  if (name === 'web_design_get_node') return ['web-design-components'];
  if (name.includes('auto_layout')) return ['web-design-responsive-layout'];
  if (name.includes('catalog') || name.includes('component') || name.includes('symbol')) return ['web-design-components'];
  if (name.includes('export') || name.includes('validate')) return ['web-design-validation-export'];
  return ['web-design-documents'];
}

const SCENE_V3_TOOL_NAMES = new Set([
  'web_design_get_active_context',
  'web_design_plan_site', 'web_design_plan_page',
  'web_design_capture_page', 'web_design_capture_region', 'web_design_prepare_annotation_task',
  'web_design_compare_snapshots', 'web_design_inspect_at_point',
  'web_design_query_scene', 'web_design_edit_scene',
  'web_design_execute_step', 'web_design_control_plan',
  'web_design_list_documents', 'web_design_create_document',
  'web_design_get_catalog', 'web_design_search_catalog', 'web_design_get_component_contract',
  'web_design_list_requests'
]);

const TOOL_DEFINITIONS = TOOL_DEFINITIONS_BASE.filter((tool) => SCENE_V3_TOOL_NAMES.has(tool.name)).map((tool) => ({
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

function decodeStructuredJson(value: unknown, label: string): Record<string, unknown> | unknown[] {
  if (typeof value !== 'string') throw new Error(`${label} must be JSON text.`);
  let decoded: unknown;
  try {
    decoded = JSON.parse(value);
  } catch {
    throw new Error(`${label} must contain valid JSON.`);
  }
  if (!decoded || typeof decoded !== 'object') throw new Error(`${label} must encode an object or array.`);
  return decoded as Record<string, unknown> | unknown[];
}

function simpleSceneNode(value: unknown, operationIndex: number): SceneNode {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`operations[${operationIndex}].node must be an object.`);
  const input = value as Record<string, unknown>;
  const type = String(input.type) as SceneNodeType;
  if (!['frame', 'group', 'text', 'shape', 'library-instance'].includes(type)) throw new Error(`operations[${operationIndex}].node.type is not supported by insert-simple-node.`);
  if (!input.frame || typeof input.frame !== 'object' || Array.isArray(input.frame)) throw new Error(`operations[${operationIndex}].node.frame is required.`);
  const frame = input.frame as Record<string, unknown>;
  const base = createSceneNodeBase(type, String(input.name), {
    x: Number(frame.x), y: Number(frame.y), width: Number(frame.width), height: Number(frame.height)
  }, 'ai');
  base.id = String(input.id);
  if (typeof input.role === 'string') base.role = input.role;

  if (input.layout && typeof input.layout === 'object' && !Array.isArray(input.layout)) {
    const layout = input.layout as Record<string, unknown>;
    if (type === 'group' && layout.mode !== undefined && layout.mode !== 'free') throw new Error(`operations[${operationIndex}] group nodes support only free layout; use frame for auto or grid layout.`);
    if (typeof layout.mode === 'string') base.layout.mode = layout.mode as 'free' | 'auto' | 'grid';
    if (typeof layout.direction === 'string') base.layout.direction = layout.direction as 'horizontal' | 'vertical';
    if (typeof layout.wrap === 'boolean') base.layout.wrap = layout.wrap;
    if (typeof layout.padding === 'number') base.layout.padding = { top: layout.padding, right: layout.padding, bottom: layout.padding, left: layout.padding };
    if (typeof layout.gap === 'number') base.layout.gap = { row: layout.gap, column: layout.gap };
    if (typeof layout.alignItems === 'string') base.layout.alignItems = layout.alignItems as typeof base.layout.alignItems;
    if (typeof layout.justifyContent === 'string') base.layout.justifyContent = layout.justifyContent as typeof base.layout.justifyContent;
    if (typeof layout.sizingX === 'string') base.layout.sizingX = layout.sizingX as typeof base.layout.sizingX;
    if (typeof layout.sizingY === 'string') base.layout.sizingY = layout.sizingY as typeof base.layout.sizingY;
    if (typeof layout.position === 'string') base.layout.position = layout.position as typeof base.layout.position;
    if (typeof layout.clipContent === 'boolean') base.layout.clipContent = layout.clipContent;
  }

  const style = input.style && typeof input.style === 'object' && !Array.isArray(input.style)
    ? input.style as Record<string, unknown>
    : {};
  if (typeof style.opacity === 'number') base.appearance.opacity = style.opacity;
  const fill = typeof style.fill === 'string'
    ? style.fill
    : type === 'text' || type === 'shape' ? '#111111' : undefined;
  if (fill) base.appearance.fills = [{ type: 'solid', visible: true, opacity: typeof style.fillOpacity === 'number' ? style.fillOpacity : 1, color: fill }];
  if (typeof style.radius === 'number') base.appearance.radius = { topLeft: style.radius, topRight: style.radius, bottomRight: style.radius, bottomLeft: style.radius };
  if (typeof style.stroke === 'string') {
    const width = typeof style.strokeWidth === 'number' ? style.strokeWidth : 1;
    base.appearance.strokes = [{
      paint: { type: 'solid', visible: true, opacity: 1, color: style.stroke },
      width: { top: width, right: width, bottom: width, left: width },
      style: 'solid'
    }];
  }
  if (typeof style.shadowColor === 'string' || typeof style.shadowRadius === 'number') {
    base.appearance.effects = [{
      type: 'drop-shadow', visible: true,
      radius: typeof style.shadowRadius === 'number' ? style.shadowRadius : 16,
      color: typeof style.shadowColor === 'string' ? style.shadowColor : '#00000033',
      offset: {
        x: typeof style.shadowOffsetX === 'number' ? style.shadowOffsetX : 0,
        y: typeof style.shadowOffsetY === 'number' ? style.shadowOffsetY : 8
      }
    }];
  }
  if (type === 'text') {
    base.appearance.typography = {
      fontFamily: typeof style.fontFamily === 'string' ? style.fontFamily : 'Inter, system-ui, sans-serif',
      fontSize: typeof style.fontSize === 'number' ? style.fontSize : 16,
      fontWeight: typeof style.fontWeight === 'number' ? style.fontWeight : 400,
      lineHeight: typeof style.lineHeight === 'number' ? style.lineHeight : 1.5,
      letterSpacing: typeof style.letterSpacing === 'number' ? style.letterSpacing : 0,
      textAlign: typeof style.textAlign === 'string' ? style.textAlign as 'left' | 'center' | 'right' | 'justify' : 'left'
    };
    return { ...base, type, content: typeof input.content === 'string' ? input.content : '' };
  }
  if (type === 'shape') return { ...base, type, shape: typeof input.shape === 'string' ? input.shape as 'rectangle' : 'rectangle' };
  if (type === 'library-instance') {
    if (typeof input.library !== 'string' || typeof input.component !== 'string') throw new Error(`operations[${operationIndex}] library-instance must copy library and component from web_design_get_component_contract.`);
    return {
      ...base, type, library: input.library, component: input.component,
      ...(typeof input.variant === 'string' ? { variant: input.variant } : {}),
      ...(typeof input.content === 'string' ? { content: input.content } : {}),
      properties: input.properties && typeof input.properties === 'object' && !Array.isArray(input.properties) ? input.properties as Record<string, unknown> : {},
      slots: input.slots && typeof input.slots === 'object' && !Array.isArray(input.slots) ? input.slots as Record<string, SceneNode[]> : {}
    };
  }
  if (type === 'frame') return { ...base, type, children: [] };
  if (type === 'group') return { ...base, type, children: [] };
  throw new Error(`operations[${operationIndex}].node.type is not supported by insert-simple-node.`);
}

function simpleSceneTree(
  value: unknown,
  operationIndex: number,
  depth = 0,
  counter: { value: number } = { value: 0 }
): SceneNode {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`operations[${operationIndex}].tree must use { node, children? }.`);
  }
  if (depth > 24) throw new Error(`operations[${operationIndex}].tree exceeds 24 nesting levels.`);
  counter.value += 1;
  if (counter.value > 512) throw new Error(`operations[${operationIndex}].tree exceeds 512 editable nodes.`);
  const input = value as Record<string, unknown>;
  const node = simpleSceneNode(input.node, operationIndex);
  const children = input.children === undefined ? [] : input.children;
  if (!Array.isArray(children)) throw new Error(`operations[${operationIndex}].tree.children must be an array.`);
  if (children.length > 0) {
    if (node.type !== 'frame' && node.type !== 'group') {
      throw new Error(`operations[${operationIndex}].tree node ${node.id} cannot contain children.`);
    }
    node.children = children.map((child) => simpleSceneTree(child, operationIndex, depth + 1, counter));
  }
  return node;
}

function normalizeGenerationOperations(value: unknown): SceneTransactionOperation[] {
  if (!Array.isArray(value)) return value as SceneTransactionOperation[];
  return value.map((item, operationIndex) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) return item as SceneTransactionOperation;
    const operation = item as Record<string, unknown>;
    if (operation.op === 'insert-simple-tree') {
      return {
        op: 'insert-node',
        parentId: String(operation.parentId),
        index: Number(operation.index),
        ...(typeof operation.slot === 'string' ? { slot: operation.slot } : {}),
        node: simpleSceneTree(operation.tree, operationIndex)
      } as SceneTransactionOperation;
    }
    if (operation.op === 'insert-simple-node') {
      return {
        op: 'insert-node',
        parentId: String(operation.parentId),
        index: Number(operation.index),
        ...(typeof operation.slot === 'string' ? { slot: operation.slot } : {}),
        node: simpleSceneNode(operation.node, operationIndex)
      } as SceneTransactionOperation;
    }
    if (operation.op !== 'update-node' || !Array.isArray(operation.patches)) return item as SceneTransactionOperation;
    return {
      ...operation,
      patches: operation.patches.map((entry, patchIndex) => {
        if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return entry;
        const patch = entry as Record<string, unknown>;
        if (!Object.hasOwn(patch, 'valueJson')) return patch;
        const { valueJson, ...rest } = patch;
        return {
          ...rest,
          value: decodeStructuredJson(valueJson, `operations[${operationIndex}].patches[${patchIndex}].valueJson`)
        };
      })
    } as SceneTransactionOperation;
  });
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
  let sceneEntries: Array<Record<string, unknown>> = [];
  try {
    const scene = await generationRepositories.scenes.read(documentId);
    const index = indexSceneDocument(scene);
    sceneEntries = [...index.values()].flatMap((entry) => entry.node.annotations
      .filter((annotation) => includeResolved || annotation.status === 'open')
      .map((annotation) => ({
        kind: 'scene-annotation',
        documentId,
        documentTitle: document.title,
        revision: scene.revision,
        pageId: entry.pageId,
        request: {
          id: annotation.id,
          nodeId: entry.node.id,
          instruction: annotation.body,
          status: annotation.status === 'open' ? 'pending' : 'resolved',
          author: annotation.author,
          createdAt: annotation.createdAt,
          ...(annotation.resolvedAt ? { resolvedAt: annotation.resolvedAt } : {})
        },
        target: {
          id: entry.node.id,
          name: entry.node.name,
          type: entry.node.type,
          role: entry.node.role,
          frame: entry.node.frame
        },
        prepareWith: {
          tool: 'web_design_prepare_annotation_task',
          arguments: { documentId, nodeId: entry.node.id, annotationId: annotation.id, viewportWidth: 1440 }
        }
      })));
  } catch (error) {
    if (!isMissingFileError(error)) throw error;
  }
  return sceneEntries;
}

function activeSelectionIds(): string[] {
  const source = process.env.CHATOS_ACTIVE_SELECTION?.trim();
  if (!source) return [];
  try {
    const parsed: unknown = JSON.parse(source);
    if (Array.isArray(parsed) && parsed.every((item) => typeof item === 'string')) return [...new Set(parsed)];
  } catch {
    // Comma-separated IDs are accepted as a small host interoperability fallback.
  }
  return [...new Set(source.split(',').map((item) => item.trim()).filter(Boolean))];
}

async function activeProgressiveContext(): Promise<Record<string, unknown>> {
  const projectId = process.env.CHATOS_PROJECT_ID;
  if (!projectId) throw new Error('Web Design Studio is not running in a ChatOS project context.');
  const documents = await store.listInProject(defaultProject.projectId, scopeKey);
  const injectedDocumentId = process.env.CHATOS_ACTIVE_DOCUMENT_ID?.trim();
  const documentId = injectedDocumentId || (documents.length === 1 ? documents[0].documentId : undefined);
  if (documentId && !documents.some((document) => document.documentId === documentId)) {
    throw new Error('The host-injected active document is outside the current ChatOS scope.');
  }
  const pageId = process.env.CHATOS_ACTIVE_PAGE_ID?.trim() || undefined;
  let pendingRequests: Awaited<ReturnType<typeof requestEntries>> = [];
  let plan: Record<string, unknown> | undefined;
  let artboardDirectory: Array<Record<string, unknown>> = [];
  let resumeReview: Record<string, unknown> | undefined;
  let resumeImages: ToolImagePayload[] = [];
  if (documentId) {
    pendingRequests = await requestEntries(documentId, false);
    try {
      const context = await progressiveGenerationService().getActiveContext(documentId);
      plan = context.plan as Record<string, unknown>;
      resumeReview = context.resumeReview as Record<string, unknown> | undefined;
      resumeImages = Array.isArray(context.__images) ? context.__images as ToolImagePayload[] : [];
    }
    catch (error) { if (!isMissingFileError(error)) throw error; }
    try {
      const scene = await generationRepositories.scenes.read(documentId);
      artboardDirectory = scene.pages.map((page, index) => ({
        artboardId: page.id,
        name: page.name,
        order: index,
        rootNodeIds: page.children.map((node) => node.id)
      }));
    } catch (error) { if (!isMissingFileError(error)) throw error; }
  }
  const plannedActivePageId = plan?.activePage && typeof plan.activePage === 'object'
    ? String((plan.activePage as Record<string, unknown>).pageId ?? '') || undefined
    : undefined;
  const activePageId = pageId ?? plannedActivePageId;
  const requiredNextAction = plan
    ? plan.nextAction
    : documentId
      ? { type: 'plan-site', tool: 'web_design_plan_site', documentId }
      : { type: 'select-or-create-document', tool: 'web_design_list_documents' };
  return {
    scope: { projectId, kind: process.env.CHATOS_CONTEXT_SCOPE ?? 'project' },
    active: { documentId, pageId: activePageId, selectionNodeIds: activeSelectionIds() },
    documents,
    artboardDirectory,
    pendingRequests,
    ...(plan ? { plan } : {}),
    ...(resumeReview ? { resumeReview } : {}),
    nextAction: requiredNextAction,
    deliveryGate: plan?.deliveryGate ?? {
      status: 'blocked',
      code: documentId ? 'NO_SITE_PLAN' : 'NO_DOCUMENT',
      message: documentId
        ? 'The document has no generation plan and no accepted visible Scene work. Plan the site and continue its required next action before editing product UI code or reporting a task outcome.'
        : 'No design document is active. Select or create one and continue until a visible Scene Candidate is accepted before editing product UI code or reporting a task outcome.',
      visibleSceneReady: false,
      projectImplementationAllowed: false,
      taskCompletionAllowed: false,
      acceptedVisibleStepCount: 0,
      completedArtboardCount: 0,
      plannedArtboardCount: 0,
      requiredNextAction
    },
    ...(resumeImages.length > 0 ? { __images: resumeImages } : {})
  };
}

function isMissingFileError(error: unknown): boolean {
  return (error as NodeJS.ErrnoException)?.code === 'ENOENT';
}

function assertSceneCommandInArtboard(scene: SceneDocument, command: SceneEditorCommand, artboardId: string): void {
  if (!scene.pages.some((page) => page.id === artboardId)) throw new Error(`Artboard not found: ${artboardId}`);
  const index = indexSceneDocument(scene);
  const assertNode = (nodeId: string, label = 'node'): void => {
    const entry = index.get(nodeId);
    if (!entry) throw new Error(`${label} not found: ${nodeId}`);
    if (entry.pageId !== artboardId) throw new Error(`${label} ${nodeId} is outside artboard ${artboardId}.`);
  };
  const assertNodes = (nodeIds: string[]): void => nodeIds.forEach((nodeId) => assertNode(nodeId));
  switch (command.type) {
    case 'create-page':
    case 'duplicate-page':
    case 'delete-page':
    case 'set-variable-collections':
      throw new Error(`${command.type} is not a focused artboard edit. Use the artboard plan or directory workflow.`);
    case 'rename-page':
      if (command.pageId !== artboardId) throw new Error(`Page ${command.pageId} is outside artboard ${artboardId}.`);
      return;
    case 'insert-node':
      if (command.parentId !== artboardId) assertNode(command.parentId, 'Insertion parent');
      return;
    case 'move':
    case 'align':
    case 'distribute':
    case 'reorder':
    case 'group':
    case 'frame':
    case 'auto-layout-frame':
    case 'delete-nodes':
      assertNodes(command.nodeIds);
      return;
    case 'ungroup':
      assertNode(command.wrapperId, 'Wrapper');
      return;
    case 'resize':
    case 'set-responsive-override':
    case 'clear-responsive-override':
    case 'add-annotation':
    case 'resolve-annotation':
    case 'reopen-annotation':
    case 'update-node':
      assertNode(command.nodeId);
  }
}

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'web_design_get_active_context':
      return activeProgressiveContext();
    case 'web_design_plan_site':
      return progressiveGenerationService().planSite({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: typeof argumentsValue.expectedPlanRevision === 'number' ? argumentsValue.expectedPlanRevision : undefined,
        planId: typeof argumentsValue.planId === 'string' ? argumentsValue.planId : undefined,
        mode: typeof argumentsValue.mode === 'string' ? argumentsValue.mode as 'guided' | 'auto-current-page' | 'review-sensitive' : undefined,
        objective: String(argumentsValue.objective),
        audience: argumentsValue.audience as string[],
        pages: argumentsValue.pages as Array<{ pageId: string; name: string; purpose: string }>
      });
    case 'web_design_plan_page':
      return progressiveGenerationService().planPage({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: Number(argumentsValue.expectedPlanRevision),
        pageId: String(argumentsValue.pageId),
        design: argumentsValue.design as GenerationDesignIntent,
        steps: argumentsValue.steps as CreateGenerationStepInput[]
      });
    case 'web_design_get_plan':
      return progressiveGenerationService().getPlan(String(argumentsValue.documentId));
    case 'web_design_start_page':
      return progressiveGenerationService().startPage(
        String(argumentsValue.documentId),
        Number(argumentsValue.expectedPlanRevision),
        String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
    case 'web_design_capture_page':
      return generationVisualService().capturePage(
        String(argumentsValue.documentId), String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
    case 'web_design_capture_region':
      return generationVisualService().captureRegion({
        documentId: String(argumentsValue.documentId),
        pageId: String(argumentsValue.pageId),
        viewportWidth: typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440,
        ...(typeof argumentsValue.nodeId === 'string' ? { nodeId: argumentsValue.nodeId } : {}),
        ...(argumentsValue.rect && typeof argumentsValue.rect === 'object' ? { rect: argumentsValue.rect as { x: number; y: number; width: number; height: number } } : {}),
        ...(typeof argumentsValue.padding === 'number' ? { padding: argumentsValue.padding } : {})
      });
    case 'web_design_prepare_annotation_task':
      return annotationAiService().prepare({
        documentId: String(argumentsValue.documentId),
        nodeId: String(argumentsValue.nodeId),
        annotationId: String(argumentsValue.annotationId),
        viewportWidth: typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440,
        ...(Array.isArray(argumentsValue.dependencyNodeIds) ? { dependencyNodeIds: argumentsValue.dependencyNodeIds as string[] } : {}),
        ...(typeof argumentsValue.padding === 'number' ? { padding: argumentsValue.padding } : {})
      });
    case 'web_design_get_visual_grounding':
      return generationVisualService().getVisualGrounding(String(argumentsValue.documentId), String(argumentsValue.artifactId));
    case 'web_design_compare_snapshots':
      return generationVisualService().compareSnapshots(
        String(argumentsValue.documentId), String(argumentsValue.beforeArtifactId), String(argumentsValue.afterArtifactId)
      );
    case 'web_design_inspect_at_point':
      return generationVisualService().inspectAtPoint(
        String(argumentsValue.documentId), String(argumentsValue.artifactId), Number(argumentsValue.x), Number(argumentsValue.y),
        typeof argumentsValue.limit === 'number' ? argumentsValue.limit : 12
      );
    case 'web_design_query_scene': {
      const documentId = String(argumentsValue.documentId);
      const artboardId = String(argumentsValue.artboardId);
      await assertGenerationDocumentInScope(documentId);
      const scene = await generationRepositories.scenes.read(documentId);
      const query = argumentsValue.query && typeof argumentsValue.query === 'object'
        ? structuredClone(argumentsValue.query) as SceneQuery
        : { limit: 100 };
      if (!scene.pages.some((page) => page.id === artboardId)) throw new Error(`Artboard not found: ${artboardId}`);
      query.pageIds = [artboardId];
      if (query.limit === undefined) query.limit = 100;
      const results = new SceneQueryIndex(scene).query(query);
      return {
        scene: {
          documentId: scene.documentId,
          name: scene.name,
          revision: scene.revision,
          activeArtboard: {
            artboardId,
            name: scene.pages.find((page) => page.id === artboardId)?.name,
            rootNodeIds: scene.pages.find((page) => page.id === artboardId)?.children.map((node) => node.id) ?? []
          }
        },
        resultCount: results.length,
        results
      };
    }
    case 'web_design_edit_scene': {
      const documentId = String(argumentsValue.documentId);
      const artboardId = String(argumentsValue.artboardId);
      await assertGenerationDocumentInScope(documentId);
      const currentScene = await generationRepositories.scenes.read(documentId);
      const decodedCommand = decodeStructuredJson(argumentsValue.commandJson, 'commandJson');
      if (Array.isArray(decodedCommand)) throw new Error('commandJson must encode one command object.');
      const request = {
        transactionId: String(argumentsValue.transactionId),
        expectedRevision: Number(argumentsValue.expectedRevision),
        ...(typeof argumentsValue.reason === 'string' ? { reason: argumentsValue.reason } : {}),
        command: decodedCommand
      };
      assertSceneCommandInArtboard(currentScene, decodedCommand as SceneEditorCommand, artboardId);
      const edited = await executeSceneEditorCommand(generationRepositories.scenes, documentId, request, 'ai');
      const changedNodeIds = [...new Set([
        ...edited.summary.insertedNodeIds,
        ...edited.summary.updatedNodeIds,
        ...edited.summary.movedNodeIds,
        ...edited.summary.removedNodeIds
      ])];
      const remaining = changedNodeIds.length > 0
        ? new SceneQueryIndex(edited.document).query({ ids: changedNodeIds, limit: 256 })
        : [];
      const affectedPageIds = [...new Set(remaining.map((entry) => entry.pageId))];
      return {
        scene: { documentId, revision: edited.document.revision },
        commandType: edited.commandType,
        transaction: edited.summary,
        recovered: edited.recovered,
        affectedNodeIds: changedNodeIds,
        affectedPageIds,
        nextRecommendedActions: [
          ...(changedNodeIds.length > 0 ? [{ tool: 'web_design_query_scene', arguments: { documentId, query: { ids: changedNodeIds } } }] : []),
          ...affectedPageIds.slice(0, 4).map((pageId) => ({ tool: 'web_design_capture_page', arguments: { documentId, pageId, viewportWidth: 1440 } }))
        ]
      };
    }
    case 'web_design_execute_step':
      {
      const decodedOperations = decodeStructuredJson(argumentsValue.operationsJson, 'operationsJson');
      if (!Array.isArray(decodedOperations)) throw new Error('operationsJson must encode an operation array.');
      return progressiveGenerationService().executeStep({
        documentId: String(argumentsValue.documentId),
        expectedPlanRevision: Number(argumentsValue.expectedPlanRevision),
        stepId: typeof argumentsValue.stepId === 'string' ? argumentsValue.stepId : undefined,
        requestId: typeof argumentsValue.requestId === 'string' ? argumentsValue.requestId : undefined,
        operations: normalizeGenerationOperations(decodedOperations)
      });
      }
    case 'web_design_control_plan': {
      const service = progressiveGenerationService();
      const documentId = String(argumentsValue.documentId);
      const revision = Number(argumentsValue.expectedPlanRevision);
      const action = String(argumentsValue.action);
      if (action === 'accept') return service.acceptStep(
        documentId, revision, String(argumentsValue.stepId), String(argumentsValue.attemptId),
        argumentsValue.approveSoftProtectionConflicts === true
      );
      if (action === 'reject') return service.rejectStep(
        documentId, revision, String(argumentsValue.stepId), String(argumentsValue.attemptId), String(argumentsValue.reason)
      );
      if (action === 'skip') return service.skipStep(documentId, revision, String(argumentsValue.stepId));
      if (action === 'rollback') return service.rollbackStep(documentId, revision, String(argumentsValue.stepId));
      if (action === 'pause') return service.pause(documentId, revision);
      if (action === 'resume') return service.resume(documentId, revision);
      if (action === 'start-page') return service.startPage(
        documentId, revision, String(argumentsValue.pageId),
        typeof argumentsValue.viewportWidth === 'number' ? argumentsValue.viewportWidth : 1440
      );
      throw new Error(`Unsupported plan control action: ${action}`);
    }
    case 'web_design_inspect_step':
      return progressiveGenerationService().inspectStep(
        String(argumentsValue.documentId), String(argumentsValue.stepId),
        typeof argumentsValue.attemptId === 'string' ? argumentsValue.attemptId : undefined
      );
    case 'web_design_accept_step':
      return progressiveGenerationService().acceptStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId),
        String(argumentsValue.attemptId), argumentsValue.approveSoftProtectionConflicts === true
      );
    case 'web_design_reject_step':
      return progressiveGenerationService().rejectStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId),
        String(argumentsValue.attemptId), String(argumentsValue.reason)
      );
    case 'web_design_skip_step':
      return progressiveGenerationService().skipStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId)
      );
    case 'web_design_rollback_step':
      return progressiveGenerationService().rollbackStep(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.stepId)
      );
    case 'web_design_complete_page':
      return progressiveGenerationService().completePage(
        String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision), String(argumentsValue.pageId)
      );
    case 'web_design_pause_plan':
      return progressiveGenerationService().pause(String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision));
    case 'web_design_resume_plan':
      return progressiveGenerationService().resume(String(argumentsValue.documentId), Number(argumentsValue.expectedPlanRevision));
    case 'web_design_list_documents':
      return { documents: await store.listInProject(defaultProject.projectId, scopeKey) };
    case 'web_design_create_document': {
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const document = await store.createInProject(defaultProject.projectId, title, true);
      return { document: designSummary(document) };
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
    case 'web_design_get_catalog': {
      const kind = typeof argumentsValue.kind === 'string' ? argumentsValue.kind : 'summary';
      const libraries = UI_LIBRARIES.map((library) => ({
          id: library.id,
          name: library.displayName,
          version: library.version,
          categories: library.categories,
          componentCount: library.components.length,
          variantCount: library.components.reduce((total, component) => total + (library.variants[component.id]?.length ?? 1), 0)
        }));
      if (kind === 'libraries') return { kind, libraries };
      if (kind === 'sections') return { kind, sections: WEB_DESIGN_BLOCK_PRESETS };
      if (kind === 'templates') return { kind, templates: WEB_DESIGN_PAGE_TEMPLATES };
      if (kind === 'themes') return {
        kind,
        themes: WEB_DESIGN_THEME_PRESETS.map(({ tokens: _tokens, ...theme }) => theme)
      };
      return {
        kind: 'summary',
        libraries,
        assetKinds: {
          components: UI_LIBRARIES.reduce((count, library) => count + library.components.length, 0),
          sections: WEB_DESIGN_BLOCK_PRESETS.length,
          templates: WEB_DESIGN_PAGE_TEMPLATES.length,
          themes: WEB_DESIGN_THEME_PRESETS.length
        },
        nextAction: { tool: 'web_design_search_catalog', detail: 'Search only the kind needed by the active design Step.' }
      };
    }
    case 'web_design_search_catalog': {
      const kind = typeof argumentsValue.kind === 'string' ? argumentsValue.kind : 'components';
      const query = typeof argumentsValue.query === 'string' ? argumentsValue.query.trim().toLocaleLowerCase() : '';
      const queryTokens = [...new Set(query.split(/[^\p{L}\p{N}]+/u).filter(Boolean))];
      const category = typeof argumentsValue.category === 'string' ? argumentsValue.category.trim().toLocaleLowerCase() : '';
      const limit = typeof argumentsValue.limit === 'number' ? Math.max(1, Math.min(50, Math.trunc(argumentsValue.limit))) : 20;
      const matches = (values: string[]): number => {
        if (!query) return 1;
        const searchable = values.map((value) => value.toLocaleLowerCase());
        if (searchable.some((value) => value.includes(query))) return 100;
        return queryTokens.filter((token) => searchable.some((value) => value.includes(token))).length;
      };
      if (kind === 'sections') {
        const candidates = WEB_DESIGN_BLOCK_PRESETS
          .filter((item) => !category || item.category.toLocaleLowerCase() === category)
          .map((item) => ({ item, score: matches([item.id, item.name, item.category, item.description, ...item.keywords]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => item);
        return { kind, query, count: candidates.length, candidates };
      }
      if (kind === 'templates') {
        const candidates = WEB_DESIGN_PAGE_TEMPLATES
          .filter((item) => !category || item.category.toLocaleLowerCase() === category)
          .map((item) => ({ item, score: matches([item.id, item.name, item.category, item.description, ...item.blocks]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => item);
        return { kind, query, count: candidates.length, candidates };
      }
      if (kind === 'themes') {
        const candidates = WEB_DESIGN_THEME_PRESETS
          .map((item) => ({ item, score: matches([item.id, item.name, item.description]) }))
          .filter(({ score }) => score > 0).sort((left, right) => right.score - left.score)
          .slice(0, limit).map(({ item }) => ({
            id: item.id, name: item.name, description: item.description,
            canvasBackground: item.canvasBackground, preview: item.preview
          }));
        return { kind, query, count: candidates.length, candidates };
      }
      const libraryId = typeof argumentsValue.libraryId === 'string' ? argumentsValue.libraryId : undefined;
      const includeDeprecated = argumentsValue.includeDeprecated === true;
      const candidates = UI_LIBRARIES
        .filter((library) => !libraryId || library.id === libraryId)
        .flatMap((library) => library.components.map((component) => ({ library, component })))
        .filter(({ component }) => includeDeprecated || component.status !== 'deprecated')
        .filter(({ component }) => !category || component.category.toLocaleLowerCase() === category)
        .map(({ library, component }) => ({
          library, component,
          score: matches([component.id, component.label, component.category, component.baseType, ...component.keywords])
        }))
        .filter(({ score }) => score > 0)
        .sort((left, right) => right.score - left.score || left.component.id.localeCompare(right.component.id))
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
          status: component.status ?? 'stable'
        }));
      return { kind: 'components', query, count: candidates.length, candidates };
    }
    case 'web_design_search_components': {
      const query = typeof argumentsValue.query === 'string' ? argumentsValue.query.trim().toLocaleLowerCase() : '';
      const queryTokens = [...new Set(query.split(/[^\p{L}\p{N}]+/u).filter(Boolean))];
      const libraryId = typeof argumentsValue.libraryId === 'string' ? argumentsValue.libraryId : undefined;
      const category = typeof argumentsValue.category === 'string' ? argumentsValue.category.trim().toLocaleLowerCase() : '';
      const includeDeprecated = argumentsValue.includeDeprecated === true;
      const limit = typeof argumentsValue.limit === 'number' ? Math.max(1, Math.min(50, Math.trunc(argumentsValue.limit))) : 20;
      const candidates = UI_LIBRARIES
        .filter((library) => !libraryId || library.id === libraryId)
        .flatMap((library) => library.components.map((component) => ({ library, component })))
        .filter(({ component }) => includeDeprecated || component.status !== 'deprecated')
        .filter(({ component }) => !category || component.category.toLocaleLowerCase() === category)
        .map(({ library, component }) => {
          const searchable = [component.id, component.label, component.category, component.baseType, ...component.keywords]
            .map((value) => value.toLocaleLowerCase());
          const exact = query ? searchable.some((value) => value.includes(query)) : true;
          const tokenMatches = queryTokens.filter((token) => searchable.some((value) => value.includes(token))).length;
          return { library, component, exact, tokenMatches };
        })
        .filter(({ exact, tokenMatches }) => !query || exact || tokenMatches > 0)
        .sort((left, right) => Number(right.exact) - Number(left.exact)
          || right.tokenMatches - left.tokenMatches
          || left.component.id.localeCompare(right.component.id))
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
      const editableSlots = editableSlotsForUiComponent(instance);
      const componentSlug = String(component.props?.componentSlug
        ?? component.id.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase());
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
          sceneBindingTemplate: {
            type: 'library-instance',
            library: library.id,
            component: component.id,
            variant: defaultVariant.id,
            properties: { ...(component.props ?? {}), ...defaultVariant.props, componentSlug },
            content: defaultVariant.content ?? component.content,
            frame: { width: defaultVariant.width ?? component.width, height: defaultVariant.height ?? component.height },
            layout: { position: 'absolute' },
            slots: Object.fromEntries(editableSlots.map((slot) => [slot.id, []]))
          },
          editableSlots
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
      const summaries = await store.listInProject(defaultProject.projectId, scopeKey);
      const requests = (await Promise.all(summaries.map((item) => requestEntries(item.documentId, includeResolved)))).flat();
      return { requests };
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
  const sourceImages = Array.isArray(value.__images) ? value.__images as ToolImagePayload[] : [];
  const { __images: _discardedImages, ...structuredContent } = value;
  return {
    content: [
      { type: 'text' as const, text: JSON.stringify(structuredContent) },
      ...sourceImages.map((image) => ({ type: 'image' as const, data: image.data, mimeType: image.mimeType }))
    ],
    structuredContent,
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
        ...(error instanceof RevisionConflictError || error instanceof GenerationPlanRevisionConflictError || error instanceof SceneRevisionConflictError
          ? { actualRevision: error.actualRevision }
          : {})
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
