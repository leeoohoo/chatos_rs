import {
  assertDiagramDocument,
  type DiagramDocument,
  type DiagramEdge,
  type DiagramKind,
  type DiagramNode,
  type DiagramNodeCategory,
  type DiagramNodeIcon
} from './schema.js';
import {
  parseSequenceActivationHandle,
  parseSequenceSlot,
  sequenceActivationHandleId,
  sequenceActivationSlotCount,
  sequenceActivationSlotPercentage,
  sequenceLifelineSlotCount,
  sequenceSlotPercentage,
  type SequenceActivationSide
} from './sequence.js';
import { createMindMapEdge, layoutMindMap } from './mindmap.js';
import {
  activityRanks,
  activityText,
  activeActivation,
  addLayoutMetadata,
  absolutePosition,
  chunk,
  componentDetectionKeywords,
  deploymentKeywords,
  edgeEndpointY,
  encodeBase64Url,
  escapeQuoted,
  extractLayout,
  fragmentKind,
  fragmentKeywords,
  genericEdgeHandles,
  inferMissingSequenceActivations,
  isSafeOpaqueLine,
  laneFill,
  layoutStructuralNodes,
  layoutPrefix,
  maximumSourceLength,
  messageY,
  nearestCommonDescendant,
  normalizeStructuralEndpoint,
  owningLifeline,
  parseMessage,
  parseParticipant,
  parseStructuralDeclaration,
  parseStructuralEdge,
  participantAppearance,
  participantKeyword,
  participantKeywords,
  safeIdentifier,
  sanitizeAlias,
  sequenceHandleForY,
  singleLine,
  stableHash,
  structuralAppearance,
  structuralKeyword,
  uniqueActivityTails,
  uniqueAliases,
  unquote,
  type ActivityTail,
  type LayoutPayload
} from './plantuml-support.js';


export {
  parsePlantUmlSequence,
  type PlantUmlSequenceFragment,
  type PlantUmlSequenceIr,
  type PlantUmlSequenceMessage,
  type PlantUmlSequenceParticipant
} from './plantuml-sequence.js';
import {
  plantUmlSequenceToDiagram,
  sequenceDiagramToPlantUml
} from './plantuml-sequence.js';
export {
  parsePlantUmlMindMap,
  type PlantUmlMindMapIr,
  type PlantUmlMindMapNode
} from './plantuml-mindmap.js';
import {
  mindMapDiagramToPlantUml,
  plantUmlMindMapToDiagram
} from './plantuml-mindmap.js';

export interface PlantUmlImportOptions {
  documentId?: string;
  title?: string;
  revision?: number;
  createdAt?: string;
  updatedAt?: string;
  kind?: DiagramKind;
}

export function hasEmbeddedDiagramLayout(source: string): boolean {
  return source.replaceAll('\r\n', '\n').split('\n').some((line) => line.startsWith(layoutPrefix));
}

export function diagramToPlantUml(document: DiagramDocument): string {
  switch (document.kind) {
    case 'sequence': return sequenceDiagramToPlantUml(document);
    case 'flowchart':
    case 'swimlane': return activityDiagramToPlantUml(document);
    case 'architecture': return structuralDiagramToPlantUml(document, 'component');
    case 'topology': return structuralDiagramToPlantUml(document, 'deployment');
    case 'mindmap': return mindMapDiagramToPlantUml(document);
  }
}

export function plantUmlToDiagram(source: string, options: PlantUmlImportOptions = {}): DiagramDocument {
  const extracted = extractLayout(source);
  const metadataKind = extracted.layout?.kind;
  const kind = options.kind
    ?? metadataKind
    ?? detectPlantUmlDiagramKind(source);
  if (kind === 'sequence') return plantUmlSequenceToDiagram(source, { ...options, kind });
  if (kind === 'mindmap') return plantUmlMindMapToDiagram(source, { ...options, kind });
  if (kind === 'flowchart' || kind === 'swimlane') return plantUmlActivityToDiagram(source, { ...options, kind });
  return plantUmlStructuralToDiagram(source, { ...options, kind });
}

export interface PlantUmlActivityLane {
  id: string;
  label: string;
}

export interface PlantUmlActivityNode {
  id: string;
  label: string;
  type: 'start' | 'activity' | 'decision' | 'stop';
  laneId?: string;
}

export interface PlantUmlActivityEdge {
  source: string;
  target: string;
  label?: string;
}

export interface PlantUmlActivityIr {
  title?: string;
  lanes: PlantUmlActivityLane[];
  nodes: PlantUmlActivityNode[];
  edges: PlantUmlActivityEdge[];
  opaqueBlocks: string[];
}

export interface PlantUmlStructuralNode {
  alias: string;
  label: string;
  type: string;
  parentAlias?: string;
  container?: boolean;
}

export interface PlantUmlStructuralEdge {
  source: string;
  target: string;
  label?: string;
  dashed: boolean;
  directed: boolean;
}

export interface PlantUmlStructuralIr {
  title?: string;
  nodes: PlantUmlStructuralNode[];
  edges: PlantUmlStructuralEdge[];
  opaqueBlocks: string[];
}

export function detectPlantUmlDiagramKind(source: string): DiagramKind {
  const { semanticText, layout } = extractLayout(source);
  if (layout?.kind) return layout.kind;
  if (/^\s*@startmindmap\b/im.test(semanticText)) return 'mindmap';
  const lines = semanticText.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  if (lines.some((line) => /^\|(?:#[^|]+\|)?[^|]+\|$/.test(line) || /^partition\s+/i.test(line))) {
    return 'swimlane';
  }
  const strongSequenceDeclaration = lines.some((line) => /^(participant|boundary|control|entity)\b/i.test(line));
  const sequenceStatement = lines.some((line) => /^(activate|deactivate|hide\s+footbox|alt\b|opt\b|loop\b|par\b|break\b|critical\b|group\b)/i.test(line));
  if (strongSequenceDeclaration || sequenceStatement) return 'sequence';
  if (lines.some((line) => /^(start|stop|end|kill|detach|:\s*.*;|if\s*\(|else\b|endif\b)/i.test(line))) return 'flowchart';
  if (lines.some((line) => componentDetectionKeywords.has(line.toLowerCase().split(/\s+/, 1)[0]) || /^\[[^\]]+\](?:\s+as\s+\w+)?/i.test(line))) return 'architecture';
  if (lines.some((line) => deploymentKeywords.has(line.toLowerCase().split(/\s+/, 1)[0]))) return 'topology';
  const sequenceDeclaration = lines.some((line) => /^(participant|actor|boundary|control|entity)\b/i.test(line));
  if (sequenceDeclaration || sequenceStatement || lines.some((line) => Boolean(parseMessage(line)))) return 'sequence';
  if (lines.some((line) => Boolean(parseStructuralEdge(line)))) return 'architecture';
  return 'flowchart';
}

export function parsePlantUmlActivity(source: string): PlantUmlActivityIr {
  const { semanticText } = extractLayout(source);
  const lines = semanticText.split(/\r?\n/);
  if (!lines.some((line) => line.trim().toLowerCase().startsWith('@startuml'))) throw new Error('PlantUML 文件缺少 @startuml。');
  if (!lines.some((line) => line.trim().toLowerCase() === '@enduml')) throw new Error('PlantUML 文件缺少 @enduml。');

  const lanes: PlantUmlActivityLane[] = [];
  const laneByLabel = new Map<string, PlantUmlActivityLane>();
  const nodes: PlantUmlActivityNode[] = [];
  const edges: PlantUmlActivityEdge[] = [];
  const opaqueBlocks: string[] = [];
  const decisionStack: Array<{ decisionId: string; thenTails?: ActivityTail[]; elseLabel?: string; hasElse: boolean }> = [];
  const partitionStack: Array<string | undefined> = [];
  let currentLaneId: string | undefined;
  let currentTails: ActivityTail[] = [];
  let title: string | undefined;
  let nodeCounter = 0;

  const ensureLane = (labelValue: string) => {
    const label = unquote(labelValue.trim()) || '未命名泳道';
    let lane = laneByLabel.get(label);
    if (!lane) {
      lane = { id: safeIdentifier('lane', label, lanes.length), label };
      lanes.push(lane);
      laneByLabel.set(label, lane);
    }
    return lane;
  };
  const appendNode = (type: PlantUmlActivityNode['type'], label: string) => {
    const node: PlantUmlActivityNode = {
      id: safeIdentifier(type, String(++nodeCounter), nodeCounter),
      label,
      type,
      laneId: currentLaneId
    };
    nodes.push(node);
    for (const tail of currentTails) edges.push({ source: tail.id, target: node.id, label: tail.label });
    currentTails = [{ id: node.id }];
    return node;
  };

  for (const originalLine of lines) {
    const line = originalLine.trim();
    const lower = line.toLowerCase();
    if (!line || lower.startsWith('@startuml') || lower === '@enduml') continue;
    if (line.startsWith("'")) continue;
    if (lower.startsWith('title ')) {
      title = unquote(line.slice(6).trim());
      continue;
    }

    const laneSwitch = line.match(/^\|(?:#[^|]+\|)?([^|]+)\|$/);
    if (laneSwitch) {
      currentLaneId = ensureLane(laneSwitch[1]).id;
      continue;
    }
    const partition = line.match(/^partition\s+("(?:\\.|[^"])*"|[^\s{]+)\s*\{$/i);
    if (partition) {
      partitionStack.push(currentLaneId);
      currentLaneId = ensureLane(unquote(partition[1])).id;
      continue;
    }
    if (line === '}' && partitionStack.length) {
      currentLaneId = partitionStack.pop();
      continue;
    }

    if (lower === 'start' || lower === '(*)') {
      appendNode('start', '开始');
      continue;
    }
    if (lower === 'stop' || lower === 'end' || lower === 'kill' || lower === 'detach') {
      appendNode('stop', '结束');
      currentTails = [];
      continue;
    }
    const activity = line.match(/^:(.*);$/);
    if (activity) {
      appendNode('activity', activity[1].replaceAll('\\n', '\n').trim() || '处理步骤');
      continue;
    }
    const decision = line.match(/^if\s*\((.*)\)\s*then\s*(?:\((.*)\))?\s*$/i);
    if (decision) {
      const node = appendNode('decision', decision[1].trim() || '条件判断');
      decisionStack.push({ decisionId: node.id, elseLabel: '否', hasElse: false });
      currentTails = [{ id: node.id, label: decision[2]?.trim() || '是' }];
      continue;
    }
    const elseMatch = line.match(/^else(?:\s*\((.*)\))?\s*$/i);
    if (elseMatch && decisionStack.length) {
      const context = decisionStack[decisionStack.length - 1];
      context.thenTails = currentTails;
      context.hasElse = true;
      currentTails = [{ id: context.decisionId, label: elseMatch[1]?.trim() || context.elseLabel || '否' }];
      continue;
    }
    if (lower === 'endif' && decisionStack.length) {
      const context = decisionStack.pop()!;
      currentTails = context.hasElse
        ? uniqueActivityTails([...(context.thenTails ?? []), ...currentTails])
        : uniqueActivityTails([...currentTails, { id: context.decisionId, label: context.elseLabel || '否' }]);
      continue;
    }
    if (/^(skinparam|!theme|scale|header|footer|legend|caption)\b/i.test(line)) {
      opaqueBlocks.push(originalLine);
      continue;
    }
    opaqueBlocks.push(originalLine);
  }

  return { title, lanes, nodes, edges, opaqueBlocks };
}

function activityDiagramToPlantUml(document: DiagramDocument): string {
  const lines = ['@startuml', `title ${singleLine(document.title)}`];
  const nodes = document.nodes.filter((node) => node.data.shape !== 'lane' && node.data.shape !== 'text');
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const outgoing = new Map(nodes.map((node) => [node.id, document.edges.filter((edge) => edge.source === node.id && byId.has(edge.target))]));
  const incomingCount = new Map(nodes.map((node) => [node.id, document.edges.filter((edge) => edge.target === node.id && byId.has(edge.source)).length]));
  const visited = new Set<string>();
  const laneById = new Map(document.nodes.filter((node) => node.data.shape === 'lane').map((lane) => [lane.id, lane]));

  const switchLane = (node: DiagramNode) => {
    if (document.kind !== 'swimlane') return;
    const lane = node.parentId ? laneById.get(node.parentId) : undefined;
    lines.push(`|${activityText(lane?.data.label ?? '未分配')}|`);
  };
  const emitNode = (nodeId: string, stopAt?: string): void => {
    if (nodeId === stopAt || visited.has(nodeId)) return;
    const node = byId.get(nodeId);
    if (!node) return;
    visited.add(nodeId);
    switchLane(node);
    const nextEdges = outgoing.get(node.id) ?? [];
    const isDecision = node.data.shape === 'diamond' || node.data.category === 'decision' || nextEdges.length > 1;
    const isTerminal = node.data.category === 'terminal';
    if (isTerminal && (incomingCount.get(node.id) ?? 0) === 0) {
      lines.push('start');
      if (nextEdges[0]) emitNode(nextEdges[0].target, stopAt);
      return;
    }
    if (isTerminal && nextEdges.length === 0) {
      lines.push('stop');
      return;
    }
    if (isDecision && nextEdges.length >= 2) {
      const primary = nextEdges[0];
      const secondary = nextEdges[1];
      const merge = nearestCommonDescendant(primary.target, secondary.target, outgoing);
      lines.push(`if (${activityText(node.data.label)}) then (${activityText(primary.label ?? primary.data?.relation ?? '是')})`);
      emitNode(primary.target, merge);
      lines.push(`else (${activityText(secondary.label ?? secondary.data?.relation ?? '否')})`);
      emitNode(secondary.target, merge);
      lines.push('endif');
      if (merge) emitNode(merge, stopAt);
      for (const extra of nextEdges.slice(2)) lines.push(`' Additional branch: ${activityText(extra.label ?? extra.target)}`);
      return;
    }
    lines.push(`:${activityText(node.data.label)};`);
    if (nextEdges.length === 0) {
      lines.push('stop');
      return;
    }
    emitNode(nextEdges[0].target, stopAt);
    for (const extra of nextEdges.slice(1)) lines.push(`' Additional edge to ${activityText(extra.target)}`);
  };

  const starts = nodes
    .filter((node) => (incomingCount.get(node.id) ?? 0) === 0)
    .sort((left, right) => absolutePosition(document.nodes, left).y - absolutePosition(document.nodes, right).y || absolutePosition(document.nodes, left).x - absolutePosition(document.nodes, right).x);
  for (const start of starts) {
    if (visited.has(start.id)) continue;
    if (lines[lines.length - 1] !== `title ${singleLine(document.title)}`) lines.push('');
    emitNode(start.id);
  }
  for (const node of nodes) {
    if (visited.has(node.id)) continue;
    lines.push('', 'start');
    emitNode(node.id);
  }
  if (document.notation?.opaqueBlocks?.length) {
    lines.push('', "' PlantUML statements preserved by Diagram Studio", ...document.notation.opaqueBlocks.filter(isSafeOpaqueLine));
  }
  lines.push('@enduml');
  return addLayoutMetadata(lines, document);
}

export function parsePlantUmlStructural(source: string): PlantUmlStructuralIr {
  const { semanticText } = extractLayout(source);
  const lines = semanticText.split(/\r?\n/);
  if (!lines.some((line) => line.trim().toLowerCase().startsWith('@startuml'))) throw new Error('PlantUML 文件缺少 @startuml。');
  if (!lines.some((line) => line.trim().toLowerCase() === '@enduml')) throw new Error('PlantUML 文件缺少 @enduml。');

  const nodes: PlantUmlStructuralNode[] = [];
  const edges: PlantUmlStructuralEdge[] = [];
  const opaqueBlocks: string[] = [];
  const nodeByAlias = new Map<string, PlantUmlStructuralNode>();
  const aliasByReference = new Map<string, string>();
  const groupStack: string[] = [];
  let title: string | undefined;

  const addNode = (node: PlantUmlStructuralNode) => {
    const alias = node.alias || sanitizeAlias(node.label);
    const normalized = { ...node, alias };
    const existing = nodeByAlias.get(alias);
    if (existing) Object.assign(existing, normalized);
    else {
      nodeByAlias.set(alias, normalized);
      nodes.push(normalized);
    }
    aliasByReference.set(alias, alias);
    aliasByReference.set(node.label, alias);
    aliasByReference.set(sanitizeAlias(node.label), alias);
    return alias;
  };
  const ensureEndpoint = (reference: string) => {
    const cleaned = normalizeStructuralEndpoint(reference);
    const known = aliasByReference.get(cleaned) ?? aliasByReference.get(sanitizeAlias(cleaned));
    return known ?? addNode({ alias: sanitizeAlias(cleaned), label: cleaned, type: 'component' });
  };

  for (const originalLine of lines) {
    const line = originalLine.trim();
    const lower = line.toLowerCase();
    if (!line || lower.startsWith('@startuml') || lower === '@enduml') continue;
    if (line.startsWith("'")) continue;
    if (lower.startsWith('title ')) {
      title = unquote(line.slice(6).trim());
      continue;
    }
    if (line === '}') {
      if (groupStack.length > 0) groupStack.pop();
      else opaqueBlocks.push(originalLine);
      continue;
    }
    const declaration = parseStructuralDeclaration(line);
    if (declaration) {
      const opensGroup = /\{\s*$/.test(line);
      const alias = addNode({
        ...declaration,
        parentAlias: groupStack[groupStack.length - 1],
        container: opensGroup
      });
      if (opensGroup) groupStack.push(alias);
      continue;
    }
    const edge = parseStructuralEdge(line);
    if (edge) {
      edges.push({ ...edge, source: ensureEndpoint(edge.source), target: ensureEndpoint(edge.target) });
      continue;
    }
    if (/^(left to right direction|top to bottom direction|skinparam|!theme|scale|header|footer|legend|caption)\b/i.test(line)) {
      opaqueBlocks.push(originalLine);
      continue;
    }
    opaqueBlocks.push(originalLine);
  }
  return { title, nodes, edges, opaqueBlocks };
}

function structuralDiagramToPlantUml(document: DiagramDocument, dialect: 'component' | 'deployment'): string {
  const aliases = uniqueAliases(document.nodes);
  const lines = ['@startuml', `title ${singleLine(document.title)}`, 'left to right direction'];
  const orderedNodes = [...document.nodes].sort((left, right) => {
    const leftPosition = absolutePosition(document.nodes, left);
    const rightPosition = absolutePosition(document.nodes, right);
    return leftPosition.x - rightPosition.x || leftPosition.y - rightPosition.y;
  });
  const childrenByParent = new Map<string | undefined, DiagramNode[]>();
  for (const node of orderedNodes) {
    const siblings = childrenByParent.get(node.parentId) ?? [];
    siblings.push(node);
    childrenByParent.set(node.parentId, siblings);
  }
  const emitNode = (node: DiagramNode, indent = '') => {
    const alias = aliases.get(node.id);
    if (!alias) return;
    const children = childrenByParent.get(node.id) ?? [];
    const keyword = structuralKeyword(node, dialect);
    if (node.data.shape === 'container' || children.length > 0) {
      lines.push(`${indent}${keyword} "${escapeQuoted(node.data.label)}" as ${alias} {`);
      for (const child of children) emitNode(child, `${indent}  `);
      lines.push(`${indent}}`);
      return;
    }
    lines.push(`${indent}${keyword} "${escapeQuoted(node.data.label)}" as ${alias}`);
  };
  for (const node of childrenByParent.get(undefined) ?? []) emitNode(node);
  if (document.edges.length) lines.push('');
  for (const edge of document.edges) {
    const source = aliases.get(edge.source);
    const target = aliases.get(edge.target);
    if (!source || !target) continue;
    const dashed = edge.data?.lineStyle === 'dashed' || edge.data?.dashed === true;
    const directed = edge.data?.endMarker !== 'none';
    const arrow = dashed ? (directed ? '..>' : '..') : (directed ? '-->' : '--');
    const label = edge.label ?? edge.data?.relation;
    lines.push(`${source} ${arrow} ${target}${label ? ` : ${singleLine(label)}` : ''}`);
  }
  if (document.notation?.opaqueBlocks?.length) {
    lines.push('', "' PlantUML statements preserved by Diagram Studio", ...document.notation.opaqueBlocks.filter(isSafeOpaqueLine));
  }
  lines.push('@enduml');
  return addLayoutMetadata(lines, document);
}

function plantUmlStructuralToDiagram(source: string, options: PlantUmlImportOptions): DiagramDocument {
  if (source.length > maximumSourceLength) throw new Error('PlantUML 文件超过 2 MiB。');
  const extracted = extractLayout(source);
  const ir = parsePlantUmlStructural(source);
  const kind: 'architecture' | 'topology' = options.kind === 'topology' ? 'topology' : 'architecture';
  const dialect = kind === 'topology' ? 'deployment' : 'component';
  const now = options.updatedAt ?? new Date().toISOString();
  const identity = {
    schemaVersion: 1 as const,
    documentId: options.documentId ?? `${kind}-${crypto.randomUUID().slice(0, 8)}`,
    revision: options.revision ?? 0,
    kind,
    title: singleLine(ir.title || options.title || (kind === 'topology' ? '导入的拓扑图' : '导入的架构图')).slice(0, 240),
    createdAt: options.createdAt ?? now,
    updatedAt: now
  };
  if (extracted.layout?.semanticHash === stableHash(extracted.semanticText) && (!extracted.layout.kind || extracted.layout.kind === kind)) {
    const restored: DiagramDocument = {
      ...identity,
      nodes: structuredClone(extracted.layout.nodes),
      edges: structuredClone(extracted.layout.edges),
      viewport: { ...extracted.layout.viewport },
      notation: { format: 'plantuml', dialect, source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
    };
    assertDiagramDocument(restored);
    return restored;
  }

  const idByAlias = new Map(ir.nodes.map((node, index) => [node.alias, safeIdentifier('node', node.alias, index)]));
  const visualNodes = ir.nodes.map((node, index): DiagramNode => {
    const appearance = structuralAppearance(node.type, kind);
    const parentId = node.parentAlias ? idByAlias.get(node.parentAlias) : undefined;
    const isContainer = node.container === true;
    const visualLineCount = node.label.split(/\r?\n/).reduce((count, line) => count + Math.max(1, Math.ceil([...line].length / 22)), 0);
    return {
      id: idByAlias.get(node.alias)!,
      type: isContainer ? 'laneNode' : 'diagramNode',
      parentId,
      extent: parentId ? 'parent' : undefined,
      position: { x: 0, y: 0 },
      width: isContainer ? 300 : 220,
      height: isContainer ? 180 : Math.max(92, 52 + visualLineCount * 18),
      zIndex: isContainer ? 0 : 2 + index,
      data: {
        label: node.label,
        category: isContainer ? 'external' : appearance.category,
        shape: isContainer ? 'container' : appearance.shape,
        icon: isContainer ? 'cluster' : appearance.icon,
        showLabel: true,
        color: appearance.color,
        fillColor: isContainer ? 'rgba(125, 135, 151, 0.035)' : 'transparent',
        borderColor: isContainer ? '#9AA4B2' : appearance.color,
        borderStyle: isContainer ? 'dashed' : 'solid',
        plantUmlId: node.alias,
        plantUmlType: node.type
      }
    };
  });
  layoutStructuralNodes(ir.nodes, ir.edges, visualNodes, idByAlias);
  const visualById = new Map(visualNodes.map((node) => [node.id, node]));
  const edges = ir.edges.flatMap((edge, index): DiagramEdge[] => {
    const sourceId = idByAlias.get(edge.source);
    const targetId = idByAlias.get(edge.target);
    const sourceNode = sourceId ? visualById.get(sourceId) : undefined;
    const targetNode = targetId ? visualById.get(targetId) : undefined;
    if (!sourceId || !targetId || !sourceNode || !targetNode) return [];
    const handles = genericEdgeHandles(visualNodes, sourceNode, targetNode);
    return [{
      id: safeIdentifier('structural-edge', String(index + 1), index),
      source: sourceId,
      target: targetId,
      sourceHandle: handles.source,
      targetHandle: handles.target,
      label: edge.label,
      type: 'smoothstep',
      data: {
        relation: edge.label,
        dashed: edge.dashed,
        lineStyle: edge.dashed ? 'dashed' : 'solid',
        startMarker: 'none',
        endMarker: edge.directed ? 'arrow' : 'none',
        strokeWidth: 1.7,
        color: '#77839A',
        plantUmlId: `structural-edge-${index + 1}`
      }
    }];
  });
  const document: DiagramDocument = {
    ...identity,
    nodes: visualNodes,
    edges,
    viewport: { x: 0, y: 0, zoom: 1 },
    notation: { format: 'plantuml', dialect, source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
  };
  assertDiagramDocument(document);
  return document;
}

function plantUmlActivityToDiagram(source: string, options: PlantUmlImportOptions): DiagramDocument {
  if (source.length > maximumSourceLength) throw new Error('PlantUML 文件超过 2 MiB。');
  const extracted = extractLayout(source);
  const ir = parsePlantUmlActivity(source);
  const kind: 'flowchart' | 'swimlane' = options.kind === 'swimlane' || (options.kind === undefined && ir.lanes.length > 0) ? 'swimlane' : 'flowchart';
  const now = options.updatedAt ?? new Date().toISOString();
  const identity = {
    schemaVersion: 1 as const,
    documentId: options.documentId ?? `${kind}-${crypto.randomUUID().slice(0, 8)}`,
    revision: options.revision ?? 0,
    kind,
    title: singleLine(ir.title || options.title || (kind === 'swimlane' ? '导入的泳道图' : '导入的流程图')).slice(0, 240),
    createdAt: options.createdAt ?? now,
    updatedAt: now
  };
  if (extracted.layout?.semanticHash === stableHash(extracted.semanticText) && (!extracted.layout.kind || extracted.layout.kind === kind)) {
    const restored: DiagramDocument = {
      ...identity,
      nodes: structuredClone(extracted.layout.nodes),
      edges: structuredClone(extracted.layout.edges),
      viewport: { ...extracted.layout.viewport },
      notation: { format: 'plantuml', dialect: 'activity', source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
    };
    assertDiagramDocument(restored);
    return restored;
  }

  const rankById = activityRanks(ir.nodes, ir.edges);
  const laneNodes: DiagramNode[] = [];
  const laneOrder = kind === 'swimlane'
    ? (ir.lanes.length ? ir.lanes : [{ id: 'lane-unassigned', label: '未分配' }])
    : [];
  const nodeOrder = new Map(ir.nodes.map((node, index) => [node.id, index]));
  const laneWidth = Math.max(900, ir.nodes.length * 220 + 240);
  laneOrder.forEach((lane, index) => {
    laneNodes.push({
      id: lane.id,
      type: 'laneNode',
      position: { x: 30, y: 30 + index * 210 },
      width: laneWidth,
      height: 180,
      zIndex: 0,
      data: { label: lane.label, category: 'lane', shape: 'lane', showLabel: true, color: '#667085', fillColor: laneFill(index), plantUmlId: lane.id, plantUmlType: 'partition' }
    });
  });
  const defaultLaneId = laneOrder[0]?.id;
  const visualNodes = ir.nodes.map((node, index): DiagramNode => {
    const rank = rankById.get(node.id) ?? index;
    const sameRank = ir.nodes.filter((candidate) => (rankById.get(candidate.id) ?? 0) === rank);
    const rankIndex = sameRank.findIndex((candidate) => candidate.id === node.id);
    const shape = node.type === 'decision' ? 'diamond' : node.type === 'start' || node.type === 'stop' ? 'circle' : 'rectangle';
    const category: DiagramNodeCategory = node.type === 'decision' ? 'decision' : node.type === 'start' || node.type === 'stop' ? 'terminal' : 'process';
    const color = node.type === 'decision' ? '#C98145' : node.type === 'start' || node.type === 'stop' ? '#4B9B72' : '#4E7CC7';
    const parentId = kind === 'swimlane' ? (node.laneId && laneOrder.some((lane) => lane.id === node.laneId) ? node.laneId : defaultLaneId) : undefined;
    return {
      id: node.id,
      type: 'diagramNode',
      parentId,
      extent: parentId ? 'parent' : undefined,
      position: parentId
        ? { x: 120 + (nodeOrder.get(node.id) ?? index) * 220, y: node.type === 'decision' ? 35 : node.type === 'start' || node.type === 'stop' ? 46 : 49 }
        : { x: 390 + (rankIndex - (sameRank.length - 1) / 2) * 260, y: 40 + rank * 150 },
      width: node.type === 'decision' ? 150 : node.type === 'start' || node.type === 'stop' ? 92 : 190,
      height: node.type === 'decision' ? 110 : node.type === 'start' || node.type === 'stop' ? 92 : 82,
      zIndex: 2 + index,
      data: { label: node.label, category, shape, showLabel: true, color, plantUmlId: node.id, plantUmlType: node.type }
    };
  });
  const visualById = new Map(visualNodes.map((node) => [node.id, node]));
  const edges = ir.edges.flatMap((edge, index): DiagramEdge[] => {
    const sourceNode = visualById.get(edge.source);
    const targetNode = visualById.get(edge.target);
    if (!sourceNode || !targetNode) return [];
    const handles = genericEdgeHandles([...laneNodes, ...visualNodes], sourceNode, targetNode);
    return [{
      id: safeIdentifier('activity-edge', String(index + 1), index),
      source: edge.source,
      target: edge.target,
      sourceHandle: handles.source,
      targetHandle: handles.target,
      label: edge.label,
      type: 'smoothstep',
      data: { relation: edge.label, lineStyle: 'solid', startMarker: 'none', endMarker: 'arrow', strokeWidth: 1.7, color: '#77839A', plantUmlId: `activity-edge-${index + 1}` }
    }];
  });
  const document: DiagramDocument = {
    ...identity,
    nodes: [...laneNodes, ...visualNodes],
    edges,
    viewport: { x: 0, y: 0, zoom: 1 },
    notation: { format: 'plantuml', dialect: 'activity', source, opaqueBlocks: ir.opaqueBlocks, lastSyncedRevision: identity.revision }
  };
  assertDiagramDocument(document);
  return document;
}
