import { jsonEncodedValueSchema, stringLiteralSchema } from '../json-schema.js';

const resizeHandles = new Set(['top', 'right', 'bottom', 'left', 'top-left', 'top-right', 'bottom-right', 'bottom-left']);

const sceneIdSchema = { type: 'string', minLength: 1, maxLength: 160, pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' } as const;
const sceneNodeIdsSchema = { type: 'array', minItems: 1, maxItems: 256, uniqueItems: true, items: sceneIdSchema } as const;
const sceneDeltaSchema = { type: 'number', minimum: -100000, maximum: 100000 } as const;
const scenePaddingSchema = {
  oneOf: [
    { type: 'number', minimum: 0, maximum: 10000 },
    {
      type: 'object',
      properties: {
        top: { type: 'number', minimum: 0, maximum: 10000 },
        right: { type: 'number', minimum: 0, maximum: 10000 },
        bottom: { type: 'number', minimum: 0, maximum: 10000 },
        left: { type: 'number', minimum: 0, maximum: 10000 }
      },
      required: ['top', 'right', 'bottom', 'left'],
      additionalProperties: false
    }
  ]
} as const;

const sceneStringPatchPaths = [
  'name', 'content', 'library', 'component', 'variant', 'layout.mode', 'layout.direction',
  'layout.sizingX', 'layout.sizingY', 'layout.position'
] as const;
const sceneBooleanPatchPaths = ['visible', 'locked', 'layout.wrap', 'layout.clipContent'] as const;
const sceneNumberPatchPaths = [
  'frame.x', 'frame.y', 'frame.width', 'frame.height', 'layout.padding.top', 'layout.padding.right',
  'layout.padding.bottom', 'layout.padding.left', 'layout.gap.row', 'layout.gap.column'
] as const;
const sceneEditorNodePatchInputSchema = {
  oneOf: [
    {
      type: 'object',
      properties: { path: { type: 'string', enum: sceneStringPatchPaths }, value: { type: 'string', maxLength: 100_000 } },
      required: ['path', 'value'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { path: { type: 'string', enum: sceneBooleanPatchPaths }, value: { type: 'boolean' } },
      required: ['path', 'value'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { path: { type: 'string', enum: sceneNumberPatchPaths }, value: sceneDeltaSchema },
      required: ['path', 'value'], additionalProperties: false
    },
    {
      type: 'object',
      properties: { path: stringLiteralSchema('properties'), valueJson: jsonEncodedValueSchema },
      required: ['path', 'valueJson'], additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        path: stringLiteralSchema('prototypeLink'),
        value: {
          anyOf: [
            { type: 'null' },
            {
              type: 'object',
              properties: {
                trigger: stringLiteralSchema('click'),
                action: { type: 'string', enum: ['navigate', 'overlay'] },
                targetPageId: sceneIdSchema
              },
              required: ['trigger', 'action', 'targetPageId'], additionalProperties: false
            }
          ]
        }
      },
      required: ['path', 'value'], additionalProperties: false
    }
  ]
} as const;

export const sceneEditorCommandRequestSchema = {
  type: 'object',
  properties: {
    transactionId: sceneIdSchema,
    expectedRevision: { type: 'integer', minimum: 0 },
    reason: { type: 'string', minLength: 1, maxLength: 240 },
    command: {
      oneOf: [
        {
          type: 'object',
          properties: { type: stringLiteralSchema('move'), nodeIds: sceneNodeIdsSchema, deltaX: sceneDeltaSchema, deltaY: sceneDeltaSchema },
          required: ['type', 'nodeIds', 'deltaX', 'deltaY'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('align'), nodeIds: { ...sceneNodeIdsSchema, minItems: 2 },
            alignment: { type: 'string', enum: ['left', 'horizontal-center', 'right', 'top', 'vertical-center', 'bottom'] }
          },
          required: ['type', 'nodeIds', 'alignment'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('distribute'), nodeIds: { ...sceneNodeIdsSchema, minItems: 3 },
            axis: { type: 'string', enum: ['horizontal', 'vertical'] }
          },
          required: ['type', 'nodeIds', 'axis'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('reorder'), nodeIds: sceneNodeIdsSchema,
            placement: { type: 'string', enum: ['front', 'forward', 'backward', 'back'] }
          },
          required: ['type', 'nodeIds', 'placement'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('resize'), nodeId: sceneIdSchema,
            handle: { type: 'string', enum: [...resizeHandles] },
            deltaX: sceneDeltaSchema, deltaY: sceneDeltaSchema,
            minimumWidth: { type: 'number', minimum: 0, maximum: 100000 },
            minimumHeight: { type: 'number', minimum: 0, maximum: 100000 }
          },
          required: ['type', 'nodeId', 'handle', 'deltaX', 'deltaY'], additionalProperties: false
        },
        {
          type: 'object',
          properties: { type: stringLiteralSchema('group'), nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema, name: { type: 'string', minLength: 1, maxLength: 240 } },
          required: ['type', 'nodeIds', 'wrapperId', 'name'], additionalProperties: false
        },
        {
          type: 'object',
          properties: { type: stringLiteralSchema('frame'), nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema, name: { type: 'string', minLength: 1, maxLength: 240 }, padding: scenePaddingSchema },
          required: ['type', 'nodeIds', 'wrapperId', 'name'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('auto-layout-frame'), nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema,
            name: { type: 'string', minLength: 1, maxLength: 240 }, direction: { type: 'string', enum: ['horizontal', 'vertical'] },
            padding: scenePaddingSchema, gap: { type: 'number', minimum: 0, maximum: 10000 },
            alignItems: { type: 'string', enum: ['start', 'center', 'end', 'stretch', 'baseline'] },
            justifyContent: { type: 'string', enum: ['start', 'center', 'end', 'between', 'around', 'evenly'] },
            sizingX: { type: 'string', enum: ['fixed', 'hug'] }, sizingY: { type: 'string', enum: ['fixed', 'hug'] }
          },
          required: ['type', 'nodeIds', 'wrapperId', 'name', 'direction'], additionalProperties: false
        },
        {
          type: 'object', properties: { type: stringLiteralSchema('ungroup'), wrapperId: sceneIdSchema },
          required: ['type', 'wrapperId'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: stringLiteralSchema('update-node'),
            nodeId: sceneIdSchema,
            patches: {
              type: 'array', minItems: 1, maxItems: 32,
              items: sceneEditorNodePatchInputSchema
            }
          },
          required: ['type', 'nodeId', 'patches'], additionalProperties: false
        },
        {
          type: 'object', properties: { type: stringLiteralSchema('delete-nodes'), nodeIds: sceneNodeIdsSchema },
          required: ['type', 'nodeIds'], additionalProperties: false
        }
      ]
    }
  },
  required: ['transactionId', 'expectedRevision', 'command'],
  additionalProperties: false
} as const;
