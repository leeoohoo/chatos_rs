import assert from 'node:assert/strict';
import test from 'node:test';
import {
  assertSceneDocument,
  createBlankSceneDocument,
  createSceneNodeBase,
  indexSceneDocument,
  isSceneContainer
} from '../dist/v2-scene-schema.test.mjs';
import { nestedWebsite } from './helpers/v2-scene-fixture.mjs';

test('schema v2 stores a recursive Page, Section, Frame, Group, and content scene graph', () => {
  const document = nestedWebsite();
  assertSceneDocument(document);
  const index = indexSceneDocument(document);
  assert.equal(index.size, 4);
  assert.equal(index.get('section-responsive').parentId, 'page-home');
  assert.equal(index.get('frame-desktop').parentId, 'section-responsive');
  assert.equal(index.get('group-hero-copy').parentId, 'frame-desktop');
  assert.equal(index.get('text-hero-heading').parentId, 'group-hero-copy');
  assert.deepEqual(index.get('text-hero-heading').path, [0, 0, 0, 0, 0]);
  assert.equal(isSceneContainer(index.get('frame-desktop').node), true);
  assert.deepEqual(index.get('text-hero-heading').node.aiPolicy.lockedFields, ['content']);
});

test('schema v2 survives JSON persistence without moving or flattening nodes', () => {
  const document = nestedWebsite();
  const serialized = JSON.stringify(document);
  const reopened = JSON.parse(serialized);
  assertSceneDocument(reopened);
  assert.equal(JSON.stringify(reopened), serialized);
  assert.deepEqual(indexSceneDocument(reopened).get('text-hero-heading').node.frame, { x: 0, y: 0, width: 560, height: 120 });
});

test('schema v2 rejects duplicate ids across nested branches', () => {
  const document = nestedWebsite();
  const duplicate = {
    ...createSceneNodeBase('text', 'Duplicate', { x: 0, y: 160, width: 100, height: 40 }),
    id: 'text-hero-heading',
    content: 'duplicate'
  };
  document.pages[0].children[0].children[0].children.push(duplicate);
  assert.throws(() => assertSceneDocument(document), /Duplicate scene id: text-hero-heading/);
});

test('Group and Section remain organizational containers instead of auto-layout frames', () => {
  const document = nestedWebsite();
  document.pages[0].children[0].children[0].children[0].layout = {
    ...document.pages[0].children[0].children[0].children[0].layout,
    mode: 'auto',
    direction: 'horizontal'
  };
  assert.throws(() => assertSceneDocument(document), /group group-hero-copy cannot own auto or grid layout/);
});

test('component sets can contain main components but not arbitrary frame nodes', () => {
  const document = createBlankSceneDocument('Components');
  const componentSet = {
    ...createSceneNodeBase('component-set', 'Button set', { x: 0, y: 0, width: 400, height: 200 }),
    id: 'component-set-button',
    variantProperties: ['size', 'tone'],
    children: [{
      ...createSceneNodeBase('frame', 'Invalid variant', { x: 0, y: 0, width: 120, height: 44 }),
      id: 'frame-invalid-variant',
      children: []
    }]
  };
  document.pages[0].children = [componentSet];
  assert.throws(() => assertSceneDocument(document), /can contain only main components/);
});

test('layout bounds and variable modes are validated before persistence', () => {
  const document = nestedWebsite();
  const frame = document.pages[0].children[0].children[0];
  frame.layout.minWidth = 900;
  frame.layout.maxWidth = 600;
  assert.throws(() => assertSceneDocument(document), /width bounds are invalid/);

  frame.layout.maxWidth = 1200;
  document.variableCollections[0].variables[0].valuesByMode['missing-mode'] = '#FF0000';
  assert.throws(() => assertSceneDocument(document), /references an unknown mode/);
});

test('grid tracks and child placement are validated as design data', () => {
  const document = nestedWebsite();
  const frame = document.pages[0].children[0].children[0];
  frame.layout = {
    ...frame.layout,
    mode: 'grid',
    direction: undefined,
    grid: { columns: ['repeat(auto-fit, minmax(220px, 1fr))'], rows: [], autoFlow: 'dense' }
  };
  frame.children[0].layout.gridPlacement = { columnStart: 1, columnSpan: 2 };
  assertSceneDocument(document);
  frame.layout.grid.columns = ['repeat(auto-fit, 1fr)'];
  assert.throws(() => assertSceneDocument(document), /measurable minimum/);
  frame.layout.grid.columns = ['1fr'];
  frame.children[0].layout.gridPlacement.columnSpan = 0;
  assert.throws(() => assertSceneDocument(document), /gridPlacement.columnSpan is invalid/);
});

test('responsive constraints reject unsupported horizontal and vertical policies', () => {
  const document = nestedWebsite();
  const heading = document.pages[0].children[0].children[0].children[0].children[0];
  heading.layout.constraints = { horizontal: 'stretch', vertical: 'bottom' };
  assertSceneDocument(document);
  heading.layout.constraints.horizontal = 'both';
  assert.throws(() => assertSceneDocument(document), /constraints.horizontal is invalid/);
});

test('responsive rules validate ranges, node references, child order, and Variable Modes', () => {
  const document = nestedWebsite();
  document.responsiveRules = [{
    id: 'responsive-mobile', name: 'Mobile', maxWidth: 600,
    variableModes: { 'variables-brand': 'mode-dark' },
    nodeOverrides: [{ nodeId: 'frame-desktop', layout: { direction: 'horizontal' } }]
  }];
  assertSceneDocument(document);

  const badRange = structuredClone(document);
  badRange.responsiveRules[0].minWidth = 600;
  assert.throws(() => assertSceneDocument(badRange), /width range is invalid/);
  const badMode = structuredClone(document);
  badMode.responsiveRules[0].variableModes['variables-brand'] = 'missing-mode';
  assert.throws(() => assertSceneDocument(badMode), /references unknown mode/);
  const badNode = structuredClone(document);
  badNode.responsiveRules[0].nodeOverrides[0].nodeId = 'node-missing';
  assert.throws(() => assertSceneDocument(badNode), /references unknown node/);
  const badOrder = structuredClone(document);
  badOrder.responsiveRules[0].nodeOverrides = [{ nodeId: 'frame-desktop', childOrder: [] }];
  assert.throws(() => assertSceneDocument(badOrder), /child order .* is invalid/);
});

test('image and video media require valid intrinsic dimensions and aspect policy', () => {
  const document = nestedWebsite();
  const frame = document.pages[0].children[0].children[0];
  const base = createSceneNodeBase('media', 'Image', { x: 0, y: 0, width: 800, height: 450 });
  frame.children.push({
    ...base,
    id: 'media-image',
    mediaType: 'image',
    assetId: 'asset-image',
    intrinsicSize: { width: 1600, height: 900 },
    preserveAspectRatio: true
  });
  assertSceneDocument(document);
  frame.children.at(-1).intrinsicSize.width = 0;
  assert.throws(() => assertSceneDocument(document), /intrinsicSize.width is invalid/);
  delete frame.children.at(-1).intrinsicSize;
  assert.throws(() => assertSceneDocument(document), /needs an intrinsic size/);
});
