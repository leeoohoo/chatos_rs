import assert from 'node:assert/strict';
import test from 'node:test';
import { createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { SceneQueryIndex, querySceneDocument } from '../dist/v2-scene-query.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

function queryFixture() {
  const document = nestedWebsite();
  const heading = document.pages[0].children[0].children[0].children[0].children[0];
  heading.annotations = [{
    id: 'annotation-heading-copy',
    author: 'human',
    body: 'Keep this wording, but review contrast',
    status: 'open',
    createdAt: '2026-09-07T10:00:00.000Z'
  }];
  heading.variableBindings = {
    'appearance.fills.0.color': 'variable-surface',
    'appearance.typography.fontSize': 'variable-type-display'
  };
  document.variableCollections[0].variables.push({
    id: 'variable-type-display',
    name: 'Display size',
    type: 'number',
    valuesByMode: { 'mode-light': 64, 'mode-dark': 64 }
  });

  const library = {
    ...createSceneNodeBase('library-instance', 'Primary action', { x: 0, y: 0, width: 180, height: 48 }, 'integration:antd'),
    id: 'library-primary-action',
    role: 'primary-action',
    library: 'antd',
    component: 'Button',
    variant: 'primary',
    properties: { danger: false },
    slots: {
      label: [{
        ...createSceneNodeBase('text', 'Button label', { x: 0, y: 0, width: 120, height: 24 }, 'human'),
        id: 'text-button-label',
        content: 'Get started'
      }]
    }
  };
  const instance = {
    ...createSceneNodeBase('component-instance', 'Logo instance', { x: 0, y: 0, width: 120, height: 32 }, 'ai'),
    id: 'component-instance-logo',
    mainComponentId: 'component-main-logo',
    overrides: { '/properties/brand': 'Acme' },
    slots: {}
  };
  const main = {
    ...createSceneNodeBase('component-main', 'Logo component', { x: 1700, y: 0, width: 120, height: 32 }, 'human'),
    id: 'component-main-logo',
    propertyDefinitions: { brand: { type: 'text', defaultValue: 'Brand' } },
    children: [{
      ...createSceneNodeBase('shape', 'Logo mark', { x: 0, y: 0, width: 32, height: 32 }, 'human'),
      id: 'shape-logo-mark',
      shape: 'ellipse'
    }]
  };
  document.pages[0].children[0].children[0].children.push(library, instance);
  document.pages[0].children.push(main);
  return document;
}

test('scene query combines id, type, role, name, page, and provenance filters', () => {
  const document = queryFixture();
  const result = querySceneDocument(document, {
    ids: ['text-hero-heading'],
    pageIds: ['page-home'],
    types: ['text'],
    roles: ['hero-heading'],
    name: { contains: 'HERO' },
    createdBy: ['ai'],
    updatedBy: ['ai']
  });
  assert.deepEqual(result.map((entry) => entry.nodeId), ['text-hero-heading']);
  assert.equal(result[0].parentId, 'group-hero-copy');
  assert.equal(result[0].depth, 3);
  assert.deepEqual(result[0].ancestors.map((ancestor) => ancestor.id), ['section-responsive', 'frame-desktop', 'group-hero-copy']);
});

test('ancestor query distinguishes any ancestor from the direct parent and preserves slot context', () => {
  const document = queryFixture();
  assert.deepEqual(querySceneDocument(document, {
    types: ['text'],
    ancestor: { types: ['frame'] }
  }).map((entry) => entry.nodeId), ['text-hero-heading', 'text-button-label']);
  assert.deepEqual(querySceneDocument(document, {
    ids: ['text-button-label'],
    ancestor: { roles: ['primary-action'], direct: true }
  }).map((entry) => ({ id: entry.nodeId, slot: entry.slot })), [{ id: 'text-button-label', slot: 'label' }]);
  assert.equal(querySceneDocument(document, {
    ids: ['text-hero-heading'],
    ancestor: { types: ['frame'], direct: true }
  }).length, 0);
});

test('AI protection and annotation queries expose human review boundaries', () => {
  const document = queryFixture();
  const protectedNodes = querySceneDocument(document, {
    aiEditable: true,
    hasLockedFields: true,
    annotation: { statuses: ['open'], authors: ['human'], body: { contains: 'contrast' } }
  });
  assert.deepEqual(protectedNodes.map((entry) => entry.nodeId), ['text-hero-heading']);
  document.pages[0].children[0].children[0].children[0].children[0].locked = true;
  assert.equal(querySceneDocument(document, { ids: ['text-hero-heading'], aiEditable: true }).length, 0);
  assert.equal(querySceneDocument(document, { ids: ['text-hero-heading'], aiEditable: false, locked: true }).length, 1);
});

test('library, component, and variable bindings are independently queryable', () => {
  const document = queryFixture();
  assert.deepEqual(querySceneDocument(document, {
    libraryBinding: { libraries: ['antd'], components: ['Button'], variants: ['primary'] }
  }).map((entry) => entry.nodeId), ['library-primary-action']);
  assert.deepEqual(querySceneDocument(document, {
    componentBinding: { mainComponentIds: ['component-main-logo'] }
  }).map((entry) => entry.nodeId), ['component-instance-logo']);
  assert.deepEqual(querySceneDocument(document, {
    variableBinding: { collectionIds: ['variables-brand'], propertyPaths: ['appearance.fills.0.color'] }
  }).map((entry) => entry.nodeId), ['text-hero-heading']);
  assert.deepEqual(querySceneDocument(document, {
    variableBinding: {
      variableIds: ['variable-surface', 'variable-type-display'],
      propertyPaths: ['appearance.fills.0.color', 'appearance.typography.fontSize'],
      match: 'all'
    }
  }).map((entry) => entry.nodeId), ['text-hero-heading']);
});

test('a reusable query index keeps document order, applies limits, and returns mutation-safe nodes', () => {
  const document = queryFixture();
  const index = new SceneQueryIndex(document);
  assert.equal(index.documentId, 'scene-test');
  assert.equal(index.revision, 0);
  const result = index.query({ types: ['text'], limit: 1 });
  assert.deepEqual(result.map((entry) => entry.nodeId), ['text-hero-heading']);
  result[0].node.name = 'Mutated query result';
  assert.equal(document.pages[0].children[0].children[0].children[0].children[0].name, 'Hero heading');
});

test('scene query rejects ambiguous or accidentally broad malformed filters', () => {
  const document = queryFixture();
  assert.throws(() => querySceneDocument(document, { ids: [] }), /non-empty string list/);
  assert.throws(() => querySceneDocument(document, { name: { equals: 'Hero', contains: 'Hero' } }), /exactly one/);
  assert.throws(() => querySceneDocument(document, { types: ['unknown'] }), /invalid scene node type/);
  assert.throws(() => querySceneDocument(document, { limit: 0 }), /positive safe integer/);
});
