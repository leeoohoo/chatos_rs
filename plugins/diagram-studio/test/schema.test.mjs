import assert from 'node:assert/strict';
import test from 'node:test';
import { createTemplate } from '../dist/test-helpers.mjs';

test('all built-in templates contain valid connected diagrams', () => {
  for (const kind of ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence']) {
    const document = createTemplate(kind);
    assert.equal(document.kind, kind);
    assert.ok(document.nodes.length >= 5);
    assert.ok(document.edges.length >= 4);
    const ids = new Set(document.nodes.map((node) => node.id));
    for (const edge of document.edges) {
      assert.ok(ids.has(edge.source));
      assert.ok(ids.has(edge.target));
    }
  }
});

test('sequence template uses lifelines, activations, and horizontal message semantics instead of swimlanes', () => {
  const document = createTemplate('sequence');
  assert.equal(document.nodes.some((node) => node.data.shape === 'lane'), false);
  assert.equal(document.nodes.filter((node) => node.data.shape === 'lifeline').length, 4);
  assert.ok(document.nodes.some((node) => node.data.shape === 'activation'));
  assert.ok(document.edges.every((edge) => edge.type === 'straight'));
  assert.ok(document.edges.every((edge) => edge.sourceHandle?.startsWith('slot-') && edge.targetHandle?.startsWith('slot-')));
  assert.ok(document.edges.some((edge) => edge.data?.lineStyle === 'dashed'));
});
