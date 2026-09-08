import assert from 'node:assert/strict';
import test from 'node:test';
import { createBlankSceneDocument, createSceneNodeBase } from '../dist/v2-scene-schema.test.mjs';
import { solveSceneLayout } from '../dist/v2-layout-engine.test.mjs';
import { resolveResponsiveScene } from '../dist/v2-responsive-scene.test.mjs';

function textNode(id, content) {
  const base = createSceneNodeBase('text', content, { x: 0, y: 0, width: 120, height: 40 });
  return { ...base, id, content };
}

function responsiveDocument() {
  const document = createBlankSceneDocument('Responsive rules');
  document.documentId = 'scene-responsive-rules';
  document.pages[0].id = 'page-responsive';
  const first = textNode('text-first', 'First');
  const second = textNode('text-second', 'Second');
  const optional = textNode('text-optional', 'Optional');
  const rootBase = createSceneNodeBase('frame', 'Responsive root', { x: 0, y: 0, width: 1200, height: 300 });
  const root = {
    ...rootBase,
    id: 'frame-responsive-root',
    layout: {
      ...rootBase.layout,
      mode: 'auto', direction: 'horizontal', wrap: false,
      padding: { top: 20, right: 20, bottom: 20, left: 20 },
      gap: { row: 24, column: 32 },
      alignItems: 'start', justifyContent: 'start',
      sizingX: 'fill', sizingY: 'hug'
    },
    children: [first, second, optional]
  };
  document.pages[0].children = [root];
  return document;
}

test('responsive layout rules reflow the same nodes instead of creating device copies', () => {
  const document = responsiveDocument();
  document.responsiveRules = [{
    id: 'responsive-stacked', name: 'Stack below tablet', maxWidth: 700,
    variableModes: {},
    nodeOverrides: [{ nodeId: 'frame-responsive-root', layout: { direction: 'vertical' } }]
  }];
  const before = JSON.stringify(document);
  const desktop = solveSceneLayout(document, { rootNodeId: 'frame-responsive-root', viewportWidth: 1200 });
  const mobile = solveSceneLayout(document, { rootNodeId: 'frame-responsive-root', viewportWidth: 390 });
  assert.equal(desktop.boxes.get('text-second').y, 20);
  assert.ok(mobile.boxes.get('text-second').y > mobile.boxes.get('text-first').y);
  assert.deepEqual(desktop.activeResponsiveRuleIds, []);
  assert.deepEqual(mobile.activeResponsiveRuleIds, ['responsive-stacked']);
  assert.equal(desktop.boxes.size, mobile.boxes.size);
  assert.equal(JSON.stringify(document), before);
});

test('responsive rules can hide and reorder existing children without deleting them', () => {
  const document = responsiveDocument();
  document.responsiveRules = [{
    id: 'responsive-mobile-nav', name: 'Mobile navigation', maxWidth: 600,
    variableModes: {},
    nodeOverrides: [
      { nodeId: 'text-optional', visible: false },
      { nodeId: 'frame-responsive-root', childOrder: ['text-second', 'text-first', 'text-optional'] }
    ]
  }];
  const desktop = solveSceneLayout(document, { rootNodeId: 'frame-responsive-root', viewportWidth: 1000 });
  const mobile = solveSceneLayout(document, { rootNodeId: 'frame-responsive-root', viewportWidth: 500 });
  assert.ok(desktop.boxes.has('text-optional'));
  assert.equal(mobile.boxes.has('text-optional'), false);
  assert.ok(mobile.boxes.get('text-second').x < mobile.boxes.get('text-first').x);
  const effective = resolveResponsiveScene(document, 500).document;
  assert.deepEqual(effective.pages[0].children[0].children.map((node) => node.id), ['text-second', 'text-first', 'text-optional']);
  assert.equal(effective.pages[0].children[0].children[2].visible, false);
});

test('responsive Variable Modes materialize bound layout values before solving', () => {
  const document = responsiveDocument();
  const root = document.pages[0].children[0];
  document.variableCollections = [{
    id: 'variables-density', name: 'Density',
    modes: [{ id: 'density-comfortable', name: 'Comfortable' }, { id: 'density-compact', name: 'Compact' }],
    variables: [{
      id: 'variable-horizontal-gap', name: 'Horizontal gap', type: 'number',
      valuesByMode: { 'density-comfortable': 32, 'density-compact': 8 }
    }]
  }];
  root.variableBindings = { 'layout.gap.column': 'variable-horizontal-gap' };
  document.responsiveRules = [{
    id: 'responsive-compact-density', name: 'Compact density', maxWidth: 600,
    variableModes: { 'variables-density': 'density-compact' }, nodeOverrides: []
  }];
  const desktop = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 1000 });
  const mobile = solveSceneLayout(document, { rootNodeId: root.id, viewportWidth: 500 });
  assert.equal(desktop.variableModes['variables-density'], 'density-comfortable');
  assert.equal(mobile.variableModes['variables-density'], 'density-compact');
  assert.equal(desktop.boxes.get('text-second').x - desktop.boxes.get('text-first').x, 152);
  assert.equal(mobile.boxes.get('text-second').x - mobile.boxes.get('text-first').x, 128);
});

test('matching responsive rules cascade in document order', () => {
  const document = responsiveDocument();
  document.responsiveRules = [
    {
      id: 'responsive-tablet', name: 'Tablet stack', maxWidth: 900,
      variableModes: {}, nodeOverrides: [{ nodeId: 'frame-responsive-root', layout: { direction: 'vertical' } }]
    },
    {
      id: 'responsive-phone', name: 'Phone spacing', maxWidth: 500,
      variableModes: {}, nodeOverrides: [{ nodeId: 'frame-responsive-root', layout: { gap: { row: 8, column: 32 } } }]
    }
  ];
  const tablet = resolveResponsiveScene(document, 700);
  assert.deepEqual(tablet.activeRuleIds, ['responsive-tablet']);
  assert.equal(tablet.document.pages[0].children[0].layout.direction, 'vertical');
  assert.equal(tablet.document.pages[0].children[0].layout.gap.row, 24);
  const phone = resolveResponsiveScene(document, 390);
  assert.deepEqual(phone.activeRuleIds, ['responsive-tablet', 'responsive-phone']);
  assert.equal(phone.document.pages[0].children[0].layout.direction, 'vertical');
  assert.equal(phone.document.pages[0].children[0].layout.gap.row, 8);
});
