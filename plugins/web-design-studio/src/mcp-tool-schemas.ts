import { jsonEncodedValueSchema, jsonScalarValueSchema, stringLiteralSchema } from './json-schema.js';
import { UI_LIBRARIES } from './ui-libraries.js';

export const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 100_000
};

export const componentStyleSchema = {
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

export const componentFrameProperties = {
  x: { type: 'number', minimum: -100000, maximum: 100000 },
  y: { type: 'number', minimum: -100000, maximum: 100000 },
  width: { type: 'number', minimum: 16, maximum: 100000 },
  height: { type: 'number', minimum: 16, maximum: 100000 }
} as const;

export const webDesignComponentSchema = {
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

export const patchOperationSchema = {
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

export const generationDesignIntentSchema = {
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

export const generationStepSchema = {
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

export const generationArtifactSchema = {
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

export const simpleSceneFrameSchema = {
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

export const simpleSceneNodeSchema = {
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

export const generationSceneOperationSchema = {
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

export const progressiveStepExecutionProperties = {
  documentId: { type: 'string', minLength: 1, maxLength: 128 },
  expectedPlanRevision: { type: 'integer', minimum: 0 },
  attemptId: { type: 'string', minLength: 1, maxLength: 160 },
  idempotencyKey: { type: 'string', minLength: 1, maxLength: 160 },
  transactionId: { type: 'string', minLength: 1, maxLength: 160 },
  operations: { type: 'array', minItems: 1, maxItems: 64, items: generationSceneOperationSchema },
  visualInputs: { type: 'array', minItems: 2, maxItems: 40, items: generationArtifactSchema }
} as const;

export const visualRectSchema = {
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
