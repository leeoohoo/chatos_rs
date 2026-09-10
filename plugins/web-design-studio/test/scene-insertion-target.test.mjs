import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveSceneInsertionTarget } from '../dist/scene-insertion-target.test.mjs';

const now = '2026-09-09T00:00:00.000Z';
const appearance = { opacity: 1, blendMode: 'normal', fills: [], strokes: [], effects: [], radius: { topLeft: 0, topRight: 0, bottomRight: 0, bottomLeft: 0 } };
const layout = { mode: 'free', sizingX: 'fixed', sizingY: 'fixed', position: 'absolute', padding: { top: 0, right: 0, bottom: 0, left: 0 }, gap: { row: 0, column: 0 }, clipContent: false };
const base = (id, name, frame) => ({ id, name, frame, layout: structuredClone(layout), appearance: structuredClone(appearance), transform: { rotation: 0, scaleX: 1, scaleY: 1, skewX: 0, skewY: 0 }, visible: true, locked: false, variableBindings: {}, aiPolicy: { editable: true, lockedFields: [] }, annotations: [], createdBy: 'human', updatedBy: 'human', createdAt: now, updatedAt: now });

function fixture() {
  const card = { ...base('card', 'Card', { x: 100, y: 80, width: 420, height: 280 }), type: 'library-instance', library: 'antd', component: 'Card', variant: 'default', properties: { componentSlug: 'card' }, content: 'Card', slots: {} };
  const root = { ...base('root', 'Root', { x: 0, y: 0, width: 1200, height: 900 }), type: 'frame', role: 'page-root', children: [card] };
  return { schemaVersion: 2, documentId: 'scene:test', revision: 0, name: 'Test', pages: [{ id: 'page:home', name: 'Home', children: [root] }], variableCollections: [], responsiveRules: [], createdAt: now, updatedAt: now };
}

test('uses the deepest component contract slot and local coordinates', () => {
  const target = resolveSceneInsertionTarget({ document: fixture(), pageId: 'page:home', viewportWidth: 1200, point: { x: 180, y: 150 } });
  assert.deepEqual(target, { nodeId: 'card', slot: 'content', index: 0, x: 80, y: 70 });
});

test('honors an explicit content focus even when dropping outside its bounds', () => {
  const target = resolveSceneInsertionTarget({ document: fixture(), pageId: 'page:home', viewportWidth: 1200, point: { x: 40, y: 30 }, preferred: { nodeId: 'card', slot: 'content' } });
  assert.deepEqual(target, { nodeId: 'card', slot: 'content', index: 0, x: 0, y: 0 });
});
