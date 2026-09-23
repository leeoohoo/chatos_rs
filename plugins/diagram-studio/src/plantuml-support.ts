import type {
  DiagramDocument,
  DiagramEdge,
  DiagramKind,
  DiagramNode,
  DiagramNodeCategory,
  DiagramNodeIcon
} from './schema.js';
import {
  parseSequenceActivationHandle,
  parseSequenceSlot,
  sequenceActivationSlotPercentage,
  sequenceActivationHandleId,
  sequenceActivationSlotCount,
  sequenceLifelineSlotCount,
  sequenceSlotPercentage,
  type SequenceActivationSide
} from './sequence.js';
import type {
  PlantUmlActivityEdge,
  PlantUmlActivityNode,
  PlantUmlImportOptions,
  PlantUmlSequenceMessage,
  PlantUmlSequenceIr,
  PlantUmlSequenceParticipant,
  PlantUmlStructuralEdge,
  PlantUmlStructuralNode
} from './plantuml.js';

export interface LayoutPayload {
  version: 1;
  semanticHash: string;
  kind?: DiagramKind;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  viewport: DiagramDocument['viewport'];
}

export type ActivityTail = { id: string; label?: string };

export const layoutPrefix = "' @diagram-studio-layout ";
export const maximumSourceLength = 2 * 1024 * 1024;
export const participantKeywords = new Set(['participant', 'actor', 'boundary', 'control', 'entity', 'database', 'collections', 'queue']);
export const fragmentKeywords = new Set(['alt', 'opt', 'loop', 'par', 'break', 'critical', 'group']);
export const componentKeywords = new Set(['component', 'interface', 'database', 'queue', 'collections', 'package']);
export const componentDetectionKeywords = new Set(['component', 'interface', 'package']);
export const deploymentKeywords = new Set(['node', 'cloud', 'artifact', 'storage', 'device', 'folder', 'frame']);
export const structuralKeywords = new Set([...componentKeywords, ...deploymentKeywords, 'actor', 'rectangle']);

export function addLayoutMetadata(lines: string[], document: DiagramDocument): string {
  const semanticText = lines.join('\n');
  const layout: LayoutPayload = { version: 1, semanticHash: stableHash(semanticText), kind: document.kind, nodes: structuredClone(document.nodes), edges: structuredClone(document.edges), viewport: { ...document.viewport } };
  const encoded = encodeBase64Url(JSON.stringify(layout));
  const metadataLines = chunk(encoded, 180).map((part, index, parts) => `${layoutPrefix}${index + 1}/${parts.length} ${part}`);
  const endMarker = lines[lines.length - 1] ?? '@enduml';
  return [...lines.slice(0, -1), ...metadataLines, endMarker, ''].join('\n');
}

export function parseStructuralDeclaration(line: string): PlantUmlStructuralNode | undefined {
  const cleaned = line.replace(/\s*\{\s*$/, '').trim();
  const bracket = cleaned.match(/^\[((?:\\.|[^\]])+)\](?:\s+as\s+([A-Za-z_][A-Za-z0-9_.-]*))?(?:\s+#[A-Za-z0-9_]+)?$/i);
  if (bracket) {
    const label = bracket[1].replaceAll('\\n', '\n').trim();
    return { alias: bracket[2] ?? sanitizeAlias(label), label, type: 'component' };
  }
  const firstWord = cleaned.toLowerCase().split(/\s+/, 1)[0];
  if (!structuralKeywords.has(firstWord)) return undefined;
  const value = cleaned.slice(firstWord.length).trim();
  if (!value) return undefined;
  const parsed = parseParticipant(value, firstWord);
  return { alias: parsed.alias, label: parsed.label, type: firstWord };
}

export function parseStructuralEdge(line: string): PlantUmlStructuralEdge | undefined {
  const normalizedLine = line.replace(/-(left|right|up|down)-/i, '--');
  const endpoint = '(\\[[^\\]]+\\]|"(?:\\\\.|[^"])*"|[A-Za-z_][A-Za-z0-9_.-]*)';
  const match = normalizedLine.match(new RegExp(`^${endpoint}\\s*(<)?([.=-]+)(>)?\\s*${endpoint}\\s*(?::\\s*(.*))?$`));
  if (!match) return undefined;
  const left = normalizeStructuralEndpoint(match[1]);
  const leftArrow = Boolean(match[2]);
  const connector = match[3];
  const rightArrow = Boolean(match[4]);
  const right = normalizeStructuralEndpoint(match[5]);
  const reverse = leftArrow && !rightArrow;
  return {
    source: reverse ? right : left,
    target: reverse ? left : right,
    label: (match[6] ?? '').replaceAll('\\n', '\n').trim() || undefined,
    dashed: connector.includes('.'),
    directed: leftArrow || rightArrow
  };
}

export function normalizeStructuralEndpoint(value: string): string {
  const trimmed = value.trim();
  if (trimmed.startsWith('[') && trimmed.endsWith(']')) return trimmed.slice(1, -1).replaceAll('\\n', '\n').trim();
  return unquote(trimmed);
}

export function structuralKeyword(node: DiagramNode, dialect: 'component' | 'deployment'): string {
  const explicit = String(node.data.plantUmlType ?? '').toLowerCase();
  if (structuralKeywords.has(explicit)) return explicit;
  if (node.data.icon === 'user') return 'actor';
  if (node.data.icon === 'database' || node.data.category === 'database') return 'database';
  if (node.data.icon === 'queue' || node.data.category === 'queue') return 'queue';
  if (node.data.icon === 'cloud' || node.data.category === 'external') return 'cloud';
  if (dialect === 'deployment') {
    if (node.data.icon === 'storage') return 'storage';
    if (node.data.icon === 'document') return 'artifact';
    return 'node';
  }
  if (node.data.icon === 'api' || node.data.icon === 'network' || node.data.category === 'network') return 'interface';
  return 'component';
}

export function structuralAppearance(type: string, kind: 'architecture' | 'topology'): {
  category: DiagramNodeCategory;
  shape: DiagramNode['data']['shape'];
  icon: DiagramNodeIcon;
  color: string;
  fill: string;
} {
  switch (type.toLowerCase()) {
    case 'actor': return { category: 'client', shape: 'rounded', icon: 'user', color: '#7967D8', fill: '#EEEAFE' };
    case 'interface': return { category: 'network', shape: 'rounded', icon: 'api', color: '#438FA6', fill: '#E7F5F8' };
    case 'database': return { category: 'database', shape: 'rounded', icon: 'database', color: '#4B9B72', fill: '#E8F6ED' };
    case 'queue':
    case 'collections': return { category: 'queue', shape: 'rounded', icon: 'queue', color: '#C98145', fill: '#FFF1E6' };
    case 'cloud': return { category: 'external', shape: 'rounded', icon: 'cloud', color: '#7967D8', fill: '#EEEAFE' };
    case 'artifact': return { category: 'note', shape: 'rounded', icon: 'document', color: '#667085', fill: '#EEF1F5' };
    case 'storage':
    case 'folder': return { category: 'database', shape: 'rounded', icon: 'storage', color: '#4B9B72', fill: '#E8F6ED' };
    case 'device': return { category: 'client', shape: 'rounded', icon: 'terminal', color: '#4E7CC7', fill: '#E8F1FF' };
    case 'node': return { category: 'service', shape: 'rounded', icon: 'server', color: '#4B9B72', fill: '#E8F6ED' };
    case 'package':
    case 'frame': return { category: 'external', shape: 'rounded', icon: 'cluster', color: '#667085', fill: '#EEF1F5' };
    default: return kind === 'topology'
      ? { category: 'service', shape: 'rounded', icon: 'server', color: '#4B9B72', fill: '#E8F6ED' }
      : { category: 'service', shape: 'rounded', icon: 'server', color: '#4E7CC7', fill: '#E8F1FF' };
  }
}

export function layoutStructuralNodes(
  nodes: PlantUmlStructuralNode[],
  edges: PlantUmlStructuralEdge[],
  visualNodes: DiagramNode[],
  idByAlias: Map<string, string>
): void {
  const semanticByAlias = new Map(nodes.map((node) => [node.alias, node]));
  const visualByAlias = new Map(nodes.map((node, index) => [node.alias, visualNodes[index]]));
  const childrenByParent = new Map<string, PlantUmlStructuralNode[]>();
  for (const node of nodes) {
    if (!node.parentAlias) continue;
    const children = childrenByParent.get(node.parentAlias) ?? [];
    children.push(node);
    childrenByParent.set(node.parentAlias, children);
  }

  const depth = (node: PlantUmlStructuralNode) => {
    let result = 0;
    let current = node;
    const seen = new Set<string>();
    while (current.parentAlias && !seen.has(current.parentAlias)) {
      seen.add(current.parentAlias);
      result += 1;
      const parent = semanticByAlias.get(current.parentAlias);
      if (!parent) break;
      current = parent;
    }
    return result;
  };

  const containers = nodes
    .filter((node) => node.container)
    .sort((left, right) => depth(right) - depth(left));
  for (const container of containers) {
    const children = childrenByParent.get(container.alias) ?? [];
    const visualContainer = visualByAlias.get(container.alias);
    if (!visualContainer) continue;
    if (children.length === 0) {
      visualContainer.width = 280;
      visualContainer.height = 150;
      continue;
    }
    const directChild = (alias: string): string | undefined => {
      let current = semanticByAlias.get(alias);
      const seen = new Set<string>();
      while (current?.parentAlias && !seen.has(current.alias)) {
        seen.add(current.alias);
        if (current.parentAlias === container.alias) return current.alias;
        current = semanticByAlias.get(current.parentAlias);
      }
      return undefined;
    };
    const seenChildEdges = new Set<string>();
    const childEdges = edges.flatMap((edge): PlantUmlStructuralEdge[] => {
      const source = directChild(edge.source);
      const target = directChild(edge.target);
      if (!source || !target || source === target) return [];
      const key = `${source}\u0000${target}`;
      if (seenChildEdges.has(key)) return [];
      seenChildEdges.add(key);
      return [{ ...edge, source, target }];
    });
    const childRanks = structuralRanks(children, childEdges);
    const ranks = [...new Set(children.map((child) => childRanks.get(child.alias) ?? 0))].sort((left, right) => left - right);
    let x = 34;
    let y = 70;
    let rowHeight = 0;
    let contentRight = 34;
    let contentBottom = 70;
    const maximumRowWidth = 1540;
    for (const rank of ranks) {
      const column = children.filter((child) => (childRanks.get(child.alias) ?? 0) === rank);
      const columnWidth = Math.max(200, ...column.map((child) => visualByAlias.get(child.alias)?.width ?? 200));
      const columnHeight = column.reduce((height, child, index) => height + (visualByAlias.get(child.alias)?.height ?? 88) + (index > 0 ? 58 : 0), 0);
      if (x > 34 && x + columnWidth > maximumRowWidth) {
        x = 34;
        y += rowHeight + 90;
        rowHeight = 0;
      }
      let childY = y;
      for (const child of column) {
        const visualChild = visualByAlias.get(child.alias);
        if (!visualChild) continue;
        visualChild.parentId = idByAlias.get(container.alias);
        visualChild.extent = 'parent';
        visualChild.position = { x, y: childY };
        childY += (visualChild.height ?? 88) + 58;
      }
      rowHeight = Math.max(rowHeight, columnHeight);
      contentRight = Math.max(contentRight, x + columnWidth);
      contentBottom = Math.max(contentBottom, y + columnHeight);
      x += columnWidth + 76;
    }
    visualContainer.width = Math.max(300, contentRight + 34);
    visualContainer.height = Math.max(170, contentBottom + 34);
  }

  const topAlias = (alias: string): string => {
    let current = semanticByAlias.get(alias);
    const seen = new Set<string>();
    while (current?.parentAlias && !seen.has(current.parentAlias)) {
      seen.add(current.parentAlias);
      const parent = semanticByAlias.get(current.parentAlias);
      if (!parent) break;
      current = parent;
    }
    return current?.alias ?? alias;
  };
  const topNodes = nodes.filter((node) => !node.parentAlias);
  const topEdges: PlantUmlStructuralEdge[] = [];
  const seenTopEdges = new Set<string>();
  for (const edge of edges) {
    const source = topAlias(edge.source);
    const target = topAlias(edge.target);
    if (source === target) continue;
    const key = `${source}\u0000${target}`;
    if (seenTopEdges.has(key)) continue;
    seenTopEdges.add(key);
    topEdges.push({ ...edge, source, target });
  }
  const topRanks = structuralRanks(topNodes, topEdges);
  const rankValues = [...new Set(topNodes.map((node) => topRanks.get(node.alias) ?? 0))].sort((left, right) => left - right);
  let rankX = 60;
  for (const rank of rankValues) {
    const column = topNodes.filter((node) => (topRanks.get(node.alias) ?? 0) === rank);
    const columnWidth = Math.max(200, ...column.map((node) => visualByAlias.get(node.alias)?.width ?? 200));
    let y = 60;
    for (const node of column) {
      const visualNode = visualByAlias.get(node.alias);
      if (!visualNode) continue;
      visualNode.position = { x: rankX, y };
      y += (visualNode.height ?? 88) + 110;
    }
    rankX += columnWidth + 160;
  }
}

export function structuralRanks(nodes: PlantUmlStructuralNode[], edges: PlantUmlStructuralEdge[]): Map<string, number> {
  const ranks = new Map(nodes.map((node) => [node.alias, 0]));
  const incoming = new Map(nodes.map((node) => [node.alias, 0]));
  const outgoing = new Map(nodes.map((node) => [node.alias, [] as string[]]));
  for (const edge of edges) {
    if (!incoming.has(edge.target) || !outgoing.has(edge.source)) continue;
    incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
    outgoing.get(edge.source)!.push(edge.target);
  }
  const queue = nodes.filter((node) => (incoming.get(node.alias) ?? 0) === 0).map((node) => node.alias);
  const visited = new Set<string>();
  while (queue.length) {
    const current = queue.shift()!;
    if (visited.has(current)) continue;
    visited.add(current);
    for (const target of outgoing.get(current) ?? []) {
      ranks.set(target, Math.max(ranks.get(target) ?? 0, (ranks.get(current) ?? 0) + 1));
      incoming.set(target, Math.max(0, (incoming.get(target) ?? 0) - 1));
      if ((incoming.get(target) ?? 0) === 0) queue.push(target);
    }
  }
  for (const node of nodes) {
    if (visited.has(node.alias)) continue;
    const cycleQueue = [node.alias];
    visited.add(node.alias);
    while (cycleQueue.length) {
      const current = cycleQueue.shift()!;
      for (const target of outgoing.get(current) ?? []) {
        if (visited.has(target)) continue;
        ranks.set(target, Math.max(ranks.get(target) ?? 0, (ranks.get(current) ?? 0) + 1));
        visited.add(target);
        cycleQueue.push(target);
      }
    }
  }
  return ranks;
}

export function activityRanks(nodes: PlantUmlActivityNode[], edges: PlantUmlActivityEdge[]): Map<string, number> {
  const ranks = new Map(nodes.map((node) => [node.id, 0]));
  for (let pass = 0; pass < nodes.length; pass += 1) {
    let changed = false;
    for (const edge of edges) {
      const next = Math.min(nodes.length, (ranks.get(edge.source) ?? 0) + 1);
      if (next > (ranks.get(edge.target) ?? 0)) {
        ranks.set(edge.target, next);
        changed = true;
      }
    }
    if (!changed) break;
  }
  return ranks;
}

export function genericEdgeHandles(nodes: DiagramNode[], source: DiagramNode, target: DiagramNode): { source: string; target: string } {
  const sourcePosition = absolutePosition(nodes, source);
  const targetPosition = absolutePosition(nodes, target);
  const sourceCenter = { x: sourcePosition.x + (source.width ?? 190) / 2, y: sourcePosition.y + (source.height ?? 82) / 2 };
  const targetCenter = { x: targetPosition.x + (target.width ?? 190) / 2, y: targetPosition.y + (target.height ?? 82) / 2 };
  if (Math.abs(targetCenter.y - sourceCenter.y) >= Math.abs(targetCenter.x - sourceCenter.x)) {
    return targetCenter.y >= sourceCenter.y ? { source: 'bottom', target: 'top' } : { source: 'top', target: 'bottom' };
  }
  return targetCenter.x >= sourceCenter.x ? { source: 'right', target: 'left' } : { source: 'left', target: 'right' };
}

export function nearestCommonDescendant(sourceA: string, sourceB: string, outgoing: Map<string, DiagramEdge[]>): string | undefined {
  const distances = (start: string) => {
    const result = new Map<string, number>();
    const queue: Array<{ id: string; distance: number }> = [{ id: start, distance: 0 }];
    while (queue.length && result.size < 2000) {
      const current = queue.shift()!;
      if (result.has(current.id)) continue;
      result.set(current.id, current.distance);
      for (const edge of outgoing.get(current.id) ?? []) queue.push({ id: edge.target, distance: current.distance + 1 });
    }
    return result;
  };
  const left = distances(sourceA);
  const right = distances(sourceB);
  return [...left.keys()]
    .filter((id) => right.has(id))
    .sort((a, b) => (left.get(a)! + right.get(a)!) - (left.get(b)! + right.get(b)!))[0];
}

export function uniqueActivityTails(tails: ActivityTail[]): ActivityTail[] {
  const seen = new Set<string>();
  return tails.filter((tail) => {
    const key = `${tail.id}\u0000${tail.label ?? ''}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export function activityText(value: string): string {
  return singleLine(value).replaceAll(';', '；').replaceAll('|', '｜').replaceAll('(', '（').replaceAll(')', '）') || '处理步骤';
}

export function laneFill(index: number): string {
  return ['#EEF4FC', '#F3F0FB', '#EEF7F1', '#FFF4EA', '#F8EEF3'][index % 5];
}

export function parseParticipant(value: string, type: string): PlantUmlSequenceParticipant {
  const cleaned = value.replace(/\s+#[A-Za-z0-9_]+\s*$/, '').replace(/\s+<<[^>]+>>\s*$/, '').trim();
  const quotedFirst = cleaned.match(/^"((?:\\.|[^"])*)"\s+as\s+([A-Za-z_][A-Za-z0-9_.-]*)/i);
  if (quotedFirst) return { alias: quotedFirst[2], label: unescapeQuoted(quotedFirst[1]), type };
  const quotedSecond = cleaned.match(/^([A-Za-z_][A-Za-z0-9_.-]*)\s+as\s+"((?:\\.|[^"])*)"/i);
  if (quotedSecond) return { alias: quotedSecond[1], label: unescapeQuoted(quotedSecond[2]), type };
  const aliased = cleaned.match(/^(.+?)\s+as\s+([A-Za-z_][A-Za-z0-9_.-]*)$/i);
  if (aliased) return { alias: aliased[2], label: unquote(aliased[1]), type };
  const label = unquote(cleaned);
  return { alias: sanitizeAlias(label), label, type };
}

export function parseMessage(line: string): PlantUmlSequenceMessage | undefined {
  const match = line.match(/^("[^"]+"|[A-Za-z_][A-Za-z0-9_.-]*)\s*(-->>|->>|-->|->|<<--|<<-|<--|<-)\s*("[^"]+"|[A-Za-z_][A-Za-z0-9_.-]*)\s*(?::\s*(.*))?$/);
  if (!match) return undefined;
  const left = unquote(match[1]);
  const right = unquote(match[3]);
  const reverse = match[2].startsWith('<');
  return {
    source: reverse ? right : left,
    target: reverse ? left : right,
    label: (match[4] ?? '').replaceAll('\\n', '\n'),
    dashed: match[2].includes('--'),
    async: match[2].includes('>>') || match[2].includes('<<')
  };
}

export function inferMissingSequenceActivations(
  messages: PlantUmlSequenceMessage[],
  explicitActivations: PlantUmlSequenceIr['activations']
): PlantUmlSequenceIr['activations'] {
  const ranges = explicitActivations.map((activation) => ({ ...activation }));

  messages.forEach((message, index) => {
    // Dashed arrows are returns, while open-arrow messages are asynchronous and
    // do not transfer synchronous control to the receiver.
    if (message.dashed || message.async) return;
    if (ranges.some((range) => range.alias === message.target && range.startMessage <= index && range.endMessage >= index)) return;

    const responseIndex = messages.findIndex((candidate, candidateIndex) => (
      candidateIndex > index
      && candidate.dashed
      && !candidate.async
      && candidate.source === message.target
      && candidate.target === message.source
    ));
    let endMessage = responseIndex >= 0 ? responseIndex : index;

    // An activation beginning inside this call owns the rest of that interval.
    // Stop immediately before it instead of rendering overlapping bars.
    const nextRange = ranges
      .filter((range) => range.alias === message.target && range.startMessage > index && range.startMessage <= endMessage)
      .sort((left, right) => left.startMessage - right.startMessage)[0];
    if (nextRange) endMessage = Math.max(index, nextRange.startMessage - 1);

    ranges.push({ alias: message.target, startMessage: index, endMessage });
  });

  return ranges.sort((left, right) => left.startMessage - right.startMessage || left.endMessage - right.endMessage || left.alias.localeCompare(right.alias));
}

export function participantAppearance(type: string): { category: DiagramNodeCategory; icon: DiagramNodeIcon; color: string; fill: string } {
  switch (type.toLowerCase()) {
    case 'actor': return { category: 'client', icon: 'user', color: '#7967D8', fill: '#EEEAFE' };
    case 'database': return { category: 'database', icon: 'database', color: '#4B9B72', fill: '#E8F6ED' };
    case 'queue':
    case 'collections': return { category: 'queue', icon: 'queue', color: '#C98145', fill: '#FFF1E6' };
    case 'boundary': return { category: 'client', icon: 'browser', color: '#4E7CC7', fill: '#E8F1FF' };
    case 'control': return { category: 'service', icon: 'api', color: '#7967D8', fill: '#EEEAFE' };
    case 'entity': return { category: 'service', icon: 'server', color: '#438FA6', fill: '#E7F5F8' };
    default: return { category: 'service', icon: 'server', color: '#4E7CC7', fill: '#E8F1FF' };
  }
}

export function participantKeyword(node: DiagramNode): string {
  const explicit = String(node.data.plantUmlType ?? '').toLowerCase();
  if (participantKeywords.has(explicit)) return explicit;
  if (node.data.icon === 'user') return 'actor';
  if (node.data.icon === 'database' || node.data.category === 'database') return 'database';
  if (node.data.icon === 'queue' || node.data.category === 'queue') return 'queue';
  return 'participant';
}

export function fragmentKind(node: DiagramNode): string {
  const explicit = String(node.data.plantUmlType ?? '').toLowerCase();
  if (fragmentKeywords.has(explicit)) return explicit;
  const inferred = node.data.label.trim().split(/\s+/, 1)[0].toLowerCase();
  return fragmentKeywords.has(inferred) ? inferred : 'group';
}

export function uniqueAliases(nodes: DiagramNode[]): Map<string, string> {
  const result = new Map<string, string>();
  const used = new Set<string>();
  nodes.forEach((node, index) => {
    const preferred = sanitizeAlias(String(node.data.plantUmlId || node.id || `participant_${index + 1}`));
    let alias = preferred;
    let suffix = 2;
    while (used.has(alias)) alias = `${preferred}_${suffix++}`;
    used.add(alias);
    result.set(node.id, alias);
  });
  return result;
}

export function owningLifeline(nodes: DiagramNode[], nodeId: string): DiagramNode | undefined {
  const node = nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return undefined;
  if (node.data.shape === 'lifeline') return node;
  if (node.data.sequenceOwnerId) {
    const owner = nodes.find((candidate) => candidate.id === node.data.sequenceOwnerId && candidate.data.shape === 'lifeline');
    if (owner) return owner;
  }
  if (node.data.shape === 'activation') {
    const position = absolutePosition(nodes, node);
    const centerX = position.x + (node.width ?? 14) / 2;
    return nodes
      .filter((candidate) => candidate.data.shape === 'lifeline')
      .sort((left, right) => Math.abs(absolutePosition(nodes, left).x + (left.width ?? 160) / 2 - centerX) - Math.abs(absolutePosition(nodes, right).x + (right.width ?? 160) / 2 - centerX))[0];
  }
  return undefined;
}

export function edgeEndpointY(nodes: DiagramNode[], edge: DiagramEdge): number {
  const source = nodes.find((node) => node.id === edge.source);
  if (!source) return 0;
  const position = absolutePosition(nodes, source);
  const height = source.height ?? (source.data.shape === 'lifeline' ? 560 : 120);
  if (source.data.shape === 'lifeline') {
    const slot = parseSequenceSlot(edge.sourceHandle);
    return position.y + height * (slot === undefined ? 50 : sequenceSlotPercentage(slot)) / 100;
  }
  if (source.data.shape === 'activation') {
    const handle = parseSequenceActivationHandle(edge.sourceHandle);
    return position.y + height * (handle ? sequenceActivationSlotPercentage(handle.slot, handle.version) : 50) / 100;
  }
  return position.y + height / 2;
}

export function absolutePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absolutePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
}

export function activeActivation(nodes: DiagramNode[], ownerId: string, y: number): DiagramNode | undefined {
  return nodes
    .filter((node) => node.data.shape === 'activation' && node.data.sequenceOwnerId === ownerId)
    .filter((node) => {
      const position = absolutePosition(nodes, node);
      return y >= position.y && y <= position.y + (node.height ?? 120);
    })
    .sort((left, right) => (left.height ?? 120) - (right.height ?? 120))[0];
}

export function sequenceHandleForY(nodes: DiagramNode[], node: DiagramNode, y: number, side: SequenceActivationSide): string {
  const position = absolutePosition(nodes, node);
  const height = node.height ?? (node.data.shape === 'lifeline' ? 560 : 120);
  const percentage = Math.max(0, Math.min(100, (y - position.y) / height * 100));
  if (node.data.shape === 'activation') {
    const slot = Math.round(percentage * (sequenceActivationSlotCount - 1) / 100);
    return sequenceActivationHandleId(side, slot);
  }
  const slot = Math.round((percentage - 12) * (sequenceLifelineSlotCount - 1) / 86);
  return `slot-${Math.max(0, Math.min(sequenceLifelineSlotCount - 1, slot))}`;
}

export function messageY(index: number): number {
  return 160 + Math.max(0, index) * 58;
}

export function extractLayout(source: string): { semanticText: string; layout?: LayoutPayload } {
  if (source.length > maximumSourceLength) throw new Error('PlantUML 文件超过 2 MiB。');
  const chunks: Array<{ index: number; total: number; value: string }> = [];
  const semanticLines: string[] = [];
  for (const line of source.replaceAll('\r\n', '\n').split('\n')) {
    if (line.startsWith(layoutPrefix)) {
      const match = line.slice(layoutPrefix.length).match(/^(\d+)\/(\d+)\s+([A-Za-z0-9_-]+)$/);
      if (match) chunks.push({ index: Number(match[1]), total: Number(match[2]), value: match[3] });
      continue;
    }
    semanticLines.push(line);
  }
  const semanticText = semanticLines.join('\n').trim();
  if (chunks.length === 0) return { semanticText };
  try {
    const total = chunks[0].total;
    if (total !== chunks.length || chunks.some((item) => item.total !== total)) return { semanticText };
    const encoded = chunks.sort((left, right) => left.index - right.index).map((item) => item.value).join('');
    const parsed = JSON.parse(decodeBase64Url(encoded)) as Partial<LayoutPayload>;
    if (parsed.version !== 1 || typeof parsed.semanticHash !== 'string' || !Array.isArray(parsed.nodes) || !Array.isArray(parsed.edges) || !parsed.viewport) {
      return { semanticText };
    }
    return { semanticText, layout: parsed as LayoutPayload };
  } catch {
    return { semanticText };
  }
}

export function safeIdentifier(prefix: string, value: string, index: number): string {
  const normalized = value.normalize('NFKD').replace(/[^A-Za-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 80);
  return `${prefix}-${normalized || index + 1}`.slice(0, 128);
}

export function sanitizeAlias(value: string): string {
  const canonical = value.normalize('NFKD');
  const normalized = canonical.replace(/[^A-Za-z0-9_]+/g, '_').replace(/^_+|_+$/g, '');
  const readable = /^[A-Za-z_]/.test(normalized)
    ? normalized
    : normalized
      ? `node_${normalized}`
      : 'node';
  // PlantUML permits declarations such as `package "客户端" {` without an explicit
  // alias. A purely ASCII sanitizer used to collapse every non-Latin label to the
  // same `participant_` identifier, silently merging otherwise unrelated packages.
  // Keep generated aliases readable, but add a stable suffix whenever transliteration
  // discards non-ASCII content so distinct labels remain distinct across imports.
  const suffix = /[^\x00-\x7F]/.test(canonical) ? `_${stableHash(value)}` : '';
  return `${readable.slice(0, Math.max(1, 96 - suffix.length))}${suffix}`;
}

export function singleLine(value: string): string {
  return value.replace(/\r?\n/g, '\\n').trim();
}

export function escapeQuoted(value: string): string {
  return singleLine(value).replaceAll('\\', '\\\\').replaceAll('"', '\\"');
}

export function unescapeQuoted(value: string): string {
  return value.replaceAll('\\"', '"').replaceAll('\\\\', '\\').replaceAll('\\n', '\n');
}

export function unquote(value: string): string {
  const trimmed = value.trim();
  return trimmed.startsWith('"') && trimmed.endsWith('"') ? unescapeQuoted(trimmed.slice(1, -1)) : trimmed;
}

export function stableHash(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

export function encodeBase64Url(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = '';
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/g, '');
}

export function decodeBase64Url(value: string): string {
  const base64 = value.replaceAll('-', '+').replaceAll('_', '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = atob(base64);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

export function chunk(value: string, size: number): string[] {
  const result: string[] = [];
  for (let index = 0; index < value.length; index += size) result.push(value.slice(index, index + size));
  return result;
}

export function isSafeOpaqueLine(value: string): boolean {
  const lower = value.trim().toLowerCase();
  return Boolean(lower) && !lower.startsWith('@startuml') && lower !== '@enduml' && !lower.startsWith(layoutPrefix.toLowerCase());
}
