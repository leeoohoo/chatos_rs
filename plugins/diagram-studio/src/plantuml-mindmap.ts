import { assertDiagramDocument, type DiagramDocument, type DiagramNode } from './schema.js';
import { createMindMapEdge, layoutMindMap } from './mindmap.js';
import {
  addLayoutMetadata,
  extractLayout,
  isSafeOpaqueLine,
  maximumSourceLength,
  safeIdentifier,
  singleLine,
  stableHash,
  unquote
} from './plantuml-support.js';
import type { PlantUmlImportOptions } from './plantuml.js';

export interface PlantUmlMindMapNode {
  id: string;
  label: string;
  depth: number;
  parentId?: string;
  side?: 'left' | 'right';
  order: number;
}

export interface PlantUmlMindMapIr {
  title?: string;
  nodes: PlantUmlMindMapNode[];
  opaqueBlocks: string[];
}

export function parsePlantUmlMindMap(source: string): PlantUmlMindMapIr {
  const { semanticText } = extractLayout(source);
  const lines = semanticText.split(/\r?\n/);
  if (!lines.some((line) => line.trim().toLowerCase().startsWith('@startmindmap'))) throw new Error('PlantUML 思维导图缺少 @startmindmap。');
  if (!lines.some((line) => line.trim().toLowerCase() === '@endmindmap')) throw new Error('PlantUML 思维导图缺少 @endmindmap。');
  const nodes: PlantUmlMindMapNode[] = [];
  const opaqueBlocks: string[] = [];
  const stack: PlantUmlMindMapNode[] = [];
  let title: string | undefined;
  let side: 'left' | 'right' = 'right';
  for (const originalLine of lines) {
    const line = originalLine.trim();
    const lower = line.toLowerCase();
    if (!line || lower.startsWith('@startmindmap') || lower === '@endmindmap' || line.startsWith("'")) continue;
    if (lower.startsWith('title ')) {
      title = unquote(line.slice(6).trim());
      continue;
    }
    if (lower === 'left side') {
      side = 'left';
      continue;
    }
    if (lower === 'right side') {
      side = 'right';
      continue;
    }
    const topic = line.match(/^([*+-]+)\s*(?:\[[^\]]*\]\s*)?(.*)$/);
    if (!topic) {
      opaqueBlocks.push(originalLine);
      continue;
    }
    const depth = topic[1].length - 1;
    const label = topic[2].replace(/^_+|_+$/g, '').replaceAll('\\n', '\n').trim() || '未命名主题';
    const parent = depth > 0 ? stack[depth - 1] : undefined;
    const node: PlantUmlMindMapNode = {
      id: safeIdentifier('mindmap', String(nodes.length + 1), nodes.length),
      label,
      depth,
      parentId: parent?.id,
      side: depth === 0 ? undefined : parent?.side ?? side,
      order: nodes.length
    };
    nodes.push(node);
    stack[depth] = node;
    stack.length = depth + 1;
  }
  return { title, nodes, opaqueBlocks };
}

export function mindMapDiagramToPlantUml(document: DiagramDocument): string {
  const nodes = document.nodes.filter((node) => node.data.shape === 'mindmap-root' || node.data.shape === 'mindmap-topic');
  const root = nodes.find((node) => node.data.shape === 'mindmap-root' && !document.edges.some((edge) => edge.target === node.id))
    ?? nodes.find((node) => !document.edges.some((edge) => edge.target === node.id));
  if (!root) return addLayoutMetadata(['@startmindmap', `title ${singleLine(document.title)}`, '@endmindmap'], document);
  const children = new Map<string, DiagramNode[]>();
  for (const edge of document.edges) {
    const child = nodes.find((node) => node.id === edge.target);
    if (!child || !nodes.some((node) => node.id === edge.source)) continue;
    children.set(edge.source, [...(children.get(edge.source) ?? []), child]);
  }
  for (const items of children.values()) items.sort((left, right) => (left.data.mindmapOrder ?? 0) - (right.data.mindmapOrder ?? 0) || left.position.y - right.position.y);
  const lines = ['@startmindmap', `title ${singleLine(document.title)}`, `* ${singleLine(root.data.label)}`];
  const emit = (node: DiagramNode, depth: number, path: Set<string>) => {
    if (path.has(node.id)) return;
    lines.push(`${'*'.repeat(depth + 1)} ${singleLine(node.data.label)}`);
    const nextPath = new Set(path).add(node.id);
    for (const child of children.get(node.id) ?? []) emit(child, depth + 1, nextPath);
  };
  const rootChildren = children.get(root.id) ?? [];
  const right = rootChildren.filter((node) => node.data.mindmapSide !== 'left');
  const left = rootChildren.filter((node) => node.data.mindmapSide === 'left');
  for (const child of right) emit(child, 1, new Set([root.id]));
  if (left.length > 0) {
    lines.push('left side');
    for (const child of left) emit(child, 1, new Set([root.id]));
  }
  if (document.notation?.opaqueBlocks?.length) lines.push(...document.notation.opaqueBlocks.filter(isSafeOpaqueLine));
  lines.push('@endmindmap');
  return addLayoutMetadata(lines, document);
}

export function plantUmlMindMapToDiagram(source: string, options: PlantUmlImportOptions): DiagramDocument {
  if (source.length > maximumSourceLength) throw new Error('PlantUML 文件超过 2 MiB。');
  const extracted = extractLayout(source);
  const ir = parsePlantUmlMindMap(source);
  const now = options.updatedAt ?? new Date().toISOString();
  const identity = {
    schemaVersion: 1 as const,
    documentId: options.documentId ?? `mindmap-${crypto.randomUUID().slice(0, 8)}`,
    revision: options.revision ?? 0,
    kind: 'mindmap' as const,
    title: singleLine(ir.title || options.title || '导入的思维导图').slice(0, 240),
    createdAt: options.createdAt ?? now,
    updatedAt: now
  };
  if (extracted.layout?.semanticHash === stableHash(extracted.semanticText) && (!extracted.layout.kind || extracted.layout.kind === 'mindmap')) {
    const restored: DiagramDocument = {
      ...identity,
      nodes: structuredClone(extracted.layout.nodes),
      edges: structuredClone(extracted.layout.edges),
      viewport: { ...extracted.layout.viewport },
      notation: { format: 'plantuml', dialect: 'mindmap', source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
    };
    assertDiagramDocument(restored);
    return restored;
  }
  const visualNodes = ir.nodes.map((node, index): DiagramNode => ({
    id: node.id,
    type: 'diagramNode',
    position: { x: 0, y: 0 },
    width: node.depth === 0 ? 200 : 150,
    height: node.depth === 0 ? 64 : 46,
    zIndex: index + 1,
    data: {
      label: node.label,
      category: 'mindmap',
      shape: node.depth === 0 ? 'mindmap-root' : 'mindmap-topic',
      showLabel: true,
      color: mindMapColor(node.depth, node.side),
      fillColor: node.depth === 0 ? '#5D6FCD' : 'transparent',
      borderColor: mindMapColor(node.depth, node.side),
      ...(node.depth === 0 ? { textColor: '#FFFFFF' } : {}),
      fontSize: node.depth === 0 ? 17 : node.depth === 1 ? 15 : 13,
      fontWeight: node.depth <= 1 ? 680 : 580,
      ...(node.side ? { mindmapSide: node.side } : {}),
      mindmapOrder: node.order,
      plantUmlId: node.id,
      plantUmlType: 'mindmap-topic'
    }
  }));
  const edges = ir.nodes.flatMap((node, index) => node.parentId ? [createMindMapEdge(node.parentId, node.id, visualNodes, `mindmap-edge-${index}`)] : []);
  const document = layoutMindMap({
    ...identity,
    nodes: visualNodes,
    edges,
    viewport: { x: 0, y: 0, zoom: 1 },
    notation: { format: 'plantuml', dialect: 'mindmap', source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
  });
  assertDiagramDocument(document);
  return document;
}

export function mindMapColor(depth: number, side?: 'left' | 'right'): string {
  if (depth === 0) return '#5D6FCD';
  if (depth === 1) return side === 'left' ? '#7967D8' : '#4E7CC7';
  return side === 'left' ? '#B9658D' : '#4B9B72';
}
