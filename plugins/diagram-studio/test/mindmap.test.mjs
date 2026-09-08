import assert from 'node:assert/strict';
import test from 'node:test';
import { renderDiagramSvg } from '../dist/document-store.test.mjs';
import { layoutDiagram } from '../dist/layout.test.mjs';
import { analyzeMindMap as inspectMindMap, insertMindMapChild } from '../dist/mindmap.test.mjs';
import { inspectDiagramQuality } from '../dist/quality.test.mjs';
import { detectPlantUmlDiagramKind, diagramToPlantUml, parsePlantUmlMindMap, plantUmlToDiagram } from '../dist/plantuml.test.mjs';

const source = `@startmindmap
title Diagram Studio 能力
* Diagram Studio
** AI 生成
*** 专用 Skill
*** 质量门禁
** 手工编辑
*** 组件库
left side
** 互操作
*** PlantUML
*** JSON
@endmindmap`;

test('PlantUML mindmap parses hierarchy and left-side branches', () => {
  assert.equal(detectPlantUmlDiagramKind(source), 'mindmap');
  const ir = parsePlantUmlMindMap(source);
  assert.equal(ir.nodes.length, 9);
  assert.equal(ir.nodes.filter((node) => node.depth === 0).length, 1);
  assert.equal(ir.nodes.find((node) => node.label === '互操作')?.side, 'left');
  assert.equal(ir.nodes.find((node) => node.label === 'PlantUML')?.side, 'left');
});

test('PlantUML mindmap converts to an editable tree and round-trips exactly with layout metadata', () => {
  const document = plantUmlToDiagram(source, { documentId: 'mindmap-round-trip', kind: 'mindmap' });
  assert.equal(document.kind, 'mindmap');
  assert.equal(document.nodes.filter((node) => node.data.shape === 'mindmap-root').length, 1);
  assert.equal(document.edges.length, document.nodes.length - 1);
  assert.ok(document.edges.every((edge) => edge.type === 'bezier' && edge.data?.endMarker === 'none'));
  const exported = diagramToPlantUml(document);
  assert.match(exported, /^@startmindmap/m);
  assert.match(exported, /^left side$/m);
  const restored = plantUmlToDiagram(exported, { documentId: document.documentId, kind: 'mindmap' });
  assert.deepEqual(restored.nodes, document.nodes);
  assert.deepEqual(restored.edges, document.edges);
});

test('mind-map layout balances primary branches and keeps topics separated', async () => {
  const document = plantUmlToDiagram(source, { documentId: 'mindmap-layout', kind: 'mindmap' });
  const laidOut = await layoutDiagram(document);
  const root = laidOut.nodes.find((node) => node.data.shape === 'mindmap-root');
  assert.ok(root);
  const left = laidOut.nodes.filter((node) => node.data.mindmapSide === 'left');
  const right = laidOut.nodes.filter((node) => node.data.mindmapSide === 'right');
  assert.ok(left.length > 0);
  assert.ok(right.length > 0);
  assert.ok(left.every((node) => node.position.x < root.position.x));
  assert.ok(right.every((node) => node.position.x > root.position.x));
  const report = inspectDiagramQuality(laidOut);
  assert.equal(report.valid, true);
  assert.equal(report.ready, true);
  assert.equal(report.metrics.overlapCount, 0);
  assert.equal(report.metrics.mindmapRootCount, 1);
  const repeated = await layoutDiagram(laidOut);
  assert.deepEqual(repeated.nodes.map((node) => [node.id, node.position]), laidOut.nodes.map((node) => [node.id, node.position]));
});

test('mind-map SVG export preserves the dedicated visual language', async () => {
  const document = await layoutDiagram(plantUmlToDiagram(source, { documentId: 'mindmap-svg', kind: 'mindmap' }));
  const svg = renderDiagramSvg(document);
  assert.match(svg, /data-mindmap-node="root"/);
  assert.match(svg, /data-mindmap-node="topic"/);
  assert.match(svg, /<rect[^>]+rx="18"[^>]+fill="#5D6FCD"/);
  assert.match(svg, /<g data-mindmap-node="topic"><line/);
  assert.match(svg, />Diagram Studio<\/text>/);
  assert.match(svg, />AI 生成<\/text>/);
  assert.doesNotMatch(svg, /<path d="M [^"]+"[^>]+marker-start=/);
  assert.doesNotMatch(svg, /<path d="M [^"]+"[^>]+marker-end=/);
});

test('dropping a branch on blank canvas creates and orders an editable child topic', () => {
  const document = plantUmlToDiagram(source, { documentId: 'mindmap-connect-drop', kind: 'mindmap' });
  const root = document.nodes.find((node) => node.data.shape === 'mindmap-root');
  assert.ok(root);
  const inserted = insertMindMapChild(document, root.id, {
    id: 'mindmap-new-child',
    label: '新增分支',
    side: 'right',
    position: { x: root.position.x + 300, y: -1000 }
  });
  const analysis = inspectMindMap(inserted.document);
  assert.equal(inserted.node.data.label, '新增分支');
  assert.equal(inserted.node.data.mindmapSide, 'right');
  assert.equal(analysis.parentByNode.get(inserted.node.id), root.id);
  assert.equal(analysis.childrenByNode.get(root.id)?.filter((node) => node.data.mindmapSide === 'right')[0]?.id, inserted.node.id);
  assert.ok(inserted.document.edges.some((edge) => edge.source === root.id && edge.target === inserted.node.id && edge.data?.endMarker === 'none'));
  assert.equal(inspectDiagramQuality(inserted.document).ready, true);
});

test('mind-map quality rejects multiple parents and cycles', () => {
  const document = plantUmlToDiagram(source, { documentId: 'mindmap-invalid', kind: 'mindmap' });
  const root = document.nodes.find((node) => node.data.shape === 'mindmap-root');
  const child = document.nodes.find((node) => node.data.label === '专用 Skill');
  const otherParent = document.nodes.find((node) => node.data.label === '手工编辑');
  assert.ok(root && child && otherParent);
  document.edges.push({ id: 'extra-parent', source: otherParent.id, target: child.id, type: 'bezier' });
  document.edges.push({ id: 'cycle', source: child.id, target: root.id, type: 'bezier' });
  const report = inspectDiagramQuality(document);
  const codes = new Set(report.errors.map((issue) => issue.code));
  assert.equal(report.ready, false);
  assert.ok(codes.has('mindmap_multiple_parents'));
  assert.ok(codes.has('mindmap_cycle'));
  assert.ok(codes.has('mindmap_not_a_tree'));
});

test('mind-map quality blocks multiple centers, excessive depth, and overloaded branches', () => {
  const multipleRoots = plantUmlToDiagram(source, { documentId: 'mindmap-multiple-roots', kind: 'mindmap' });
  multipleRoots.nodes.push({
    id: 'second-root', type: 'diagramNode', position: { x: 0, y: 0 }, width: 200, height: 64,
    data: { label: '另一个中心', category: 'mindmap', shape: 'mindmap-root', color: '#5D6FCD' }
  });
  assert.ok(inspectDiagramQuality(multipleRoots).errors.some((issue) => issue.code === 'mindmap_multiple_roots'));

  const deep = plantUmlToDiagram(`@startmindmap
* Root
** L1
*** L2
**** L3
***** L4
****** L5
@endmindmap`, { documentId: 'mindmap-too-deep', kind: 'mindmap' });
  assert.ok(inspectDiagramQuality(deep).warnings.some((issue) => issue.code === 'mindmap_too_deep' && issue.blocking));

  const overloadedLines = Array.from({ length: 9 }, (_, index) => `** Child ${index + 1}`).join('\n');
  const overloaded = plantUmlToDiagram(`@startmindmap\n* Root\n${overloadedLines}\n@endmindmap`, { documentId: 'mindmap-overloaded', kind: 'mindmap' });
  assert.ok(inspectDiagramQuality(overloaded).warnings.some((issue) => issue.code === 'mindmap_branch_overloaded' && issue.blocking));
});
