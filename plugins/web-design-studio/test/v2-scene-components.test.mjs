import assert from 'node:assert/strict';
import test from 'node:test';
import { assertSceneDocument, createBlankSceneDocument, createSceneNodeBase, indexSceneDocument } from '../dist/v2-scene-schema.test.mjs';
import { applySceneTransaction } from '../dist/v2-scene-transaction.test.mjs';

function componentDocument() {
  const document = createBlankSceneDocument('Components');
  document.documentId = 'scene-components';
  document.pages[0].id = 'page-components';
  const label = {
    ...createSceneNodeBase('text', 'Button label', { x: 16, y: 12, width: 100, height: 20 }),
    id: 'text-button-label',
    content: 'Continue'
  };
  const main = {
    ...createSceneNodeBase('component-main', 'Button main', { x: 0, y: 0, width: 160, height: 44 }),
    id: 'component-main-button',
    propertyDefinitions: {
      label: { type: 'text', defaultValue: 'Continue' },
      disabled: { type: 'boolean', defaultValue: false },
      tone: { type: 'variant', defaultValue: 'primary', options: ['primary', 'secondary'] },
      leading: { type: 'slot', acceptedNodeTypes: ['shape', 'media'], minItems: 0, maxItems: 1 }
    },
    children: [label]
  };
  const instance = {
    ...createSceneNodeBase('component-instance', 'Button instance', { x: 0, y: 100, width: 160, height: 44 }),
    id: 'component-instance-button',
    mainComponentId: main.id,
    overrides: {
      '/properties/label': 'Buy now',
      '/properties/tone': 'secondary',
      '/nodes/text-button-label/content': 'Buy now'
    },
    slots: {
      leading: [{
        ...createSceneNodeBase('shape', 'Cart icon', { x: 0, y: 0, width: 16, height: 16 }),
        id: 'shape-cart-icon',
        shape: 'rectangle'
      }]
    }
  };
  const frame = {
    ...createSceneNodeBase('frame', 'Instances', { x: 0, y: 200, width: 800, height: 600 }),
    id: 'frame-instances',
    children: [instance]
  };
  document.pages[0].children = [main, frame];
  return document;
}

test('component instances reference a real main and expose slot content in the scene graph', () => {
  const document = componentDocument();
  assertSceneDocument(document);
  const index = indexSceneDocument(document);
  assert.equal(index.get('component-instance-button').node.mainComponentId, 'component-main-button');
  assert.equal(index.get('shape-cart-icon').parentId, 'component-instance-button');
});

test('component overrides use explicit JSON Pointer paths and validate property values', () => {
  const malformed = componentDocument();
  indexSceneDocument(malformed).get('component-instance-button').node.overrides = { 'label.content': 'Broken' };
  assert.throws(() => assertSceneDocument(malformed), /must be a JSON Pointer/);

  const unknown = componentDocument();
  indexSceneDocument(unknown).get('component-instance-button').node.overrides = { '/properties/missing': 'Broken' };
  assert.throws(() => assertSceneDocument(unknown), /overrides unknown property missing/);

  const wrongVariant = componentDocument();
  indexSceneDocument(wrongVariant).get('component-instance-button').node.overrides['/properties/tone'] = 'danger';
  assert.throws(() => assertSceneDocument(wrongVariant), /needs a valid variant value/);

  const outside = componentDocument();
  indexSceneDocument(outside).get('component-instance-button').node.overrides = { '/nodes/frame-instances/name': 'Broken' };
  assert.throws(() => assertSceneDocument(outside), /outside its main component/);

  const protectedField = componentDocument();
  indexSceneDocument(protectedField).get('component-instance-button').node.overrides = { '/nodes/text-button-label/id': 'replacement' };
  assert.throws(() => assertSceneDocument(protectedField), /cannot override id/);
});

test('component slots reject unknown names, excess content, and unsupported node types', () => {
  const unknown = componentDocument();
  indexSceneDocument(unknown).get('component-instance-button').node.slots.extra = [];
  assert.throws(() => assertSceneDocument(unknown), /uses unknown slot extra/);

  const excess = componentDocument();
  const instance = indexSceneDocument(excess).get('component-instance-button').node;
  instance.slots.leading.push({ ...structuredClone(instance.slots.leading[0]), id: 'shape-second-icon' });
  assert.throws(() => assertSceneDocument(excess), /allows at most 1 items/);

  const unsupported = componentDocument();
  const unsupportedInstance = indexSceneDocument(unsupported).get('component-instance-button').node;
  unsupportedInstance.slots.leading = [{
    ...createSceneNodeBase('text', 'Invalid slot text', { x: 0, y: 0, width: 80, height: 20 }),
    id: 'text-invalid-slot',
    content: 'No'
  }];
  assert.throws(() => assertSceneDocument(unsupported), /contains an unsupported node type/);
});

test('component property definitions and instance-swap references are validated', () => {
  const invalidBounds = componentDocument();
  indexSceneDocument(invalidBounds).get('component-main-button').node.propertyDefinitions.leading = {
    type: 'slot', minItems: 2, maxItems: 1
  };
  assert.throws(() => assertSceneDocument(invalidBounds), /item bounds are invalid/);

  const invalidSwap = componentDocument();
  indexSceneDocument(invalidSwap).get('component-main-button').node.propertyDefinitions.icon = {
    type: 'instance-swap', defaultValue: 'component-main-missing'
  };
  assert.throws(() => assertSceneDocument(invalidSwap), /needs a valid main component id/);

  const missingMain = componentDocument();
  indexSceneDocument(missingMain).get('component-instance-button').node.mainComponentId = 'component-main-missing';
  assert.throws(() => assertSceneDocument(missingMain), /references unknown main component/);
});

test('transactions can insert into component slots while slot constraints remain atomic', () => {
  const source = componentDocument();
  const instance = indexSceneDocument(source).get('component-instance-button').node;
  instance.slots.leading = [];
  const inserted = applySceneTransaction(source, {
    transactionId: 'transaction-component-slot',
    baseRevision: 0,
    author: 'human',
    operations: [{
      op: 'insert-node',
      parentId: 'component-instance-button',
      slot: 'leading',
      index: 0,
      node: {
        ...createSceneNodeBase('shape', 'New icon', { x: 0, y: 0, width: 16, height: 16 }),
        id: 'shape-new-icon',
        shape: 'star'
      }
    }]
  }).document;
  assert.equal(indexSceneDocument(inserted).get('shape-new-icon').parentId, 'component-instance-button');

  assert.throws(() => applySceneTransaction(inserted, {
    transactionId: 'transaction-component-slot-overflow',
    baseRevision: 1,
    author: 'human',
    operations: [{
      op: 'insert-node',
      parentId: 'component-instance-button',
      slot: 'leading',
      index: 1,
      node: {
        ...createSceneNodeBase('shape', 'Extra icon', { x: 0, y: 0, width: 16, height: 16 }),
        id: 'shape-extra-icon',
        shape: 'star'
      }
    }]
  }), /allows at most 1 items/);
});
