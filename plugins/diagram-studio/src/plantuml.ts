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

const layoutPrefix = "' @diagram-studio-layout ";
const maximumSourceLength = 2 * 1024 * 1024;
const participantKeywords = new Set(['participant', 'actor', 'boundary', 'control', 'entity', 'database', 'collections', 'queue']);
const fragmentKeywords = new Set(['alt', 'opt', 'loop', 'par', 'break', 'critical', 'group']);
const componentKeywords = new Set(['component', 'interface', 'database', 'queue', 'collections', 'package']);
const componentDetectionKeywords = new Set(['component', 'interface', 'package']);
const deploymentKeywords = new Set(['node', 'cloud', 'artifact', 'storage', 'device', 'folder', 'frame']);
const structuralKeywords = new Set([...componentKeywords, ...deploymentKeywords, 'actor', 'rectangle']);

export interface PlantUmlSequenceParticipant {
  alias: string;
  label: string;
  type: string;
}

export interface PlantUmlSequenceMessage {
  source: string;
  target: string;
  label: string;
  dashed: boolean;
}

export interface PlantUmlSequenceFragment {
  kind: string;
  label: string;
  startMessage: number;
  endMessage: number;
}

export interface PlantUmlSequenceIr {
  title?: string;
  participants: PlantUmlSequenceParticipant[];
  messages: PlantUmlSequenceMessage[];
  activations: Array<{ alias: string; startMessage: number; endMessage: number }>;
  fragments: PlantUmlSequenceFragment[];
  opaqueBlocks: string[];
}

export interface PlantUmlImportOptions {
  documentId?: string;
  title?: string;
  revision?: number;
  createdAt?: string;
  updatedAt?: string;
  kind?: DiagramKind;
}

interface LayoutPayload {
  version: 1;
  semanticHash: string;
  kind?: DiagramKind;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  viewport: DiagramDocument['viewport'];
}

interface SequenceMessageGeometry extends PlantUmlSequenceMessage {
  y: number;
  edge: DiagramEdge;
}

export function diagramToPlantUml(document: DiagramDocument): string {
  switch (document.kind) {
    case 'sequence': return sequenceDiagramToPlantUml(document);
    case 'flowchart':
    case 'swimlane': return activityDiagramToPlantUml(document);
    case 'architecture': return structuralDiagramToPlantUml(document, 'component');
    case 'topology': return structuralDiagramToPlantUml(document, 'deployment');
  }
}

function sequenceDiagramToPlantUml(document: DiagramDocument): string {
  const lifelines = document.nodes
    .filter((node) => node.data.shape === 'lifeline')
    .sort((left, right) => absolutePosition(document.nodes, left).x - absolutePosition(document.nodes, right).x);
  const aliases = uniqueAliases(lifelines);
  const lines = ['@startuml', `title ${singleLine(document.title)}`, 'hide footbox'];
  for (const lifeline of lifelines) {
    const keyword = participantKeyword(lifeline);
    lines.push(`${keyword} "${escapeQuoted(lifeline.data.label)}" as ${aliases.get(lifeline.id)}`);
  }
  lines.push('');

  const messages = document.edges
    .flatMap((edge): SequenceMessageGeometry[] => {
      const sourceParticipant = owningLifeline(document.nodes, edge.source);
      const targetParticipant = owningLifeline(document.nodes, edge.target);
      if (!sourceParticipant || !targetParticipant) return [];
      const source = aliases.get(sourceParticipant.id);
      const target = aliases.get(targetParticipant.id);
      if (!source || !target) return [];
      return [{
        source,
        target,
        label: edge.label ?? edge.data?.relation ?? '',
        dashed: edge.data?.lineStyle === 'dashed' || edge.data?.dashed === true,
        y: edgeEndpointY(document.nodes, edge),
        edge
      }];
    })
    .sort((left, right) => left.y - right.y);

  const events: Array<{ y: number; priority: number; line: string }> = [];
  for (const message of messages) {
    const arrow = message.dashed ? '-->' : '->';
    events.push({
      y: message.y,
      priority: 10,
      line: `${message.source} ${arrow} ${message.target}${message.label ? `: ${singleLine(message.label)}` : ''}`
    });
  }

  for (const activation of document.nodes.filter((node) => node.data.shape === 'activation')) {
    const owner = owningLifeline(document.nodes, activation.id);
    const alias = owner && aliases.get(owner.id);
    if (!alias) continue;
    const position = absolutePosition(document.nodes, activation);
    events.push({ y: position.y, priority: 20, line: `activate ${alias}` });
    events.push({ y: position.y + (activation.height ?? 120), priority: 30, line: `deactivate ${alias}` });
  }

  for (const fragment of document.nodes.filter((node) => node.data.shape === 'fragment')) {
    const position = absolutePosition(document.nodes, fragment);
    const kind = fragmentKind(fragment);
    const label = fragment.data.label.replace(new RegExp(`^${kind}\\s*`, 'i'), '').trim();
    events.push({ y: position.y, priority: 0, line: `${kind}${label ? ` ${singleLine(label)}` : ''}` });
    events.push({ y: position.y + (fragment.height ?? 220), priority: 40, line: 'end' });
  }

  for (const event of events.sort((left, right) => left.y - right.y || left.priority - right.priority)) {
    lines.push(event.line);
  }
  if (document.notation?.opaqueBlocks?.length) {
    lines.push('', "' PlantUML statements preserved by Diagram Studio");
    lines.push(...document.notation.opaqueBlocks.filter(isSafeOpaqueLine));
  }
  lines.push('@enduml');

  const semanticText = lines.join('\n');
  const layout: LayoutPayload = {
    version: 1,
    semanticHash: stableHash(semanticText),
    kind: document.kind,
    nodes: structuredClone(document.nodes),
    edges: structuredClone(document.edges),
    viewport: { ...document.viewport }
  };
  const encoded = encodeBase64Url(JSON.stringify(layout));
  const metadataLines = chunk(encoded, 180).map((part, index, parts) => `${layoutPrefix}${index + 1}/${parts.length} ${part}`);
  return [...lines.slice(0, -1), ...metadataLines, '@enduml', ''].join('\n');
}

export function parsePlantUmlSequence(source: string): PlantUmlSequenceIr {
  const { semanticText } = extractLayout(source);
  const lines = semanticText.split(/\r?\n/);
  if (!lines.some((line) => line.trim().toLowerCase().startsWith('@startuml'))) {
    throw new Error('PlantUML 文件缺少 @startuml。');
  }
  if (!lines.some((line) => line.trim().toLowerCase() === '@enduml')) {
    throw new Error('PlantUML 文件缺少 @enduml。');
  }

  const participants: PlantUmlSequenceParticipant[] = [];
  const participantByAlias = new Map<string, PlantUmlSequenceParticipant>();
  const messages: PlantUmlSequenceMessage[] = [];
  const activations: PlantUmlSequenceIr['activations'] = [];
  const activationStarts = new Map<string, number[]>();
  const fragments: PlantUmlSequenceFragment[] = [];
  const fragmentStack: Array<Omit<PlantUmlSequenceFragment, 'endMessage'>> = [];
  const opaqueBlocks: string[] = [];
  let title: string | undefined;

  const ensureParticipant = (aliasValue: string) => {
    const alias = unquote(aliasValue.trim());
    let participant = participantByAlias.get(alias);
    if (!participant) {
      participant = { alias, label: alias, type: 'participant' };
      participantByAlias.set(alias, participant);
      participants.push(participant);
    }
    return participant;
  };

  for (const originalLine of lines) {
    const line = originalLine.trim();
    const lower = line.toLowerCase();
    if (!line || lower.startsWith('@startuml') || lower === '@enduml' || lower === 'hide footbox') continue;
    if (line.startsWith("'")) continue;
    if (lower.startsWith('title ')) {
      title = unquote(line.slice(6).trim());
      continue;
    }

    const firstWord = lower.split(/\s+/, 1)[0];
    if (participantKeywords.has(firstWord)) {
      const parsed = parseParticipant(line.slice(firstWord.length).trim(), firstWord);
      const existing = participantByAlias.get(parsed.alias);
      if (existing) Object.assign(existing, parsed);
      else {
        participantByAlias.set(parsed.alias, parsed);
        participants.push(parsed);
      }
      continue;
    }

    const message = parseMessage(line);
    if (message) {
      ensureParticipant(message.source);
      ensureParticipant(message.target);
      messages.push(message);
      continue;
    }

    const activationMatch = line.match(/^(activate|deactivate|destroy)\s+(.+)$/i);
    if (activationMatch) {
      const alias = unquote(activationMatch[2].trim());
      ensureParticipant(alias);
      if (activationMatch[1].toLowerCase() === 'activate') {
        const starts = activationStarts.get(alias) ?? [];
        starts.push(Math.max(0, messages.length - 1));
        activationStarts.set(alias, starts);
      } else {
        const starts = activationStarts.get(alias);
        const startMessage = starts?.pop();
        if (startMessage !== undefined) {
          activations.push({ alias, startMessage, endMessage: Math.max(startMessage, messages.length - 1) });
        }
      }
      continue;
    }

    if (fragmentKeywords.has(firstWord)) {
      fragmentStack.push({ kind: firstWord, label: line.slice(firstWord.length).trim(), startMessage: messages.length });
      continue;
    }
    if (firstWord === 'else') {
      opaqueBlocks.push(originalLine);
      continue;
    }
    if (firstWord === 'end' && fragmentStack.length) {
      const fragment = fragmentStack.pop()!;
      fragments.push({ ...fragment, endMessage: Math.max(fragment.startMessage, messages.length - 1) });
      continue;
    }

    opaqueBlocks.push(originalLine);
  }

  for (const [alias, starts] of activationStarts) {
    for (const startMessage of starts) {
      activations.push({ alias, startMessage, endMessage: Math.max(startMessage, messages.length - 1) });
    }
  }
  while (fragmentStack.length) {
    const fragment = fragmentStack.pop()!;
    fragments.push({ ...fragment, endMessage: Math.max(fragment.startMessage, messages.length - 1) });
  }
  return { title, participants, messages, activations, fragments, opaqueBlocks };
}

export function plantUmlToDiagram(source: string, options: PlantUmlImportOptions = {}): DiagramDocument {
  const extracted = extractLayout(source);
  const metadataKind = extracted.layout?.kind;
  const kind = options.kind
    ?? metadataKind
    ?? detectPlantUmlDiagramKind(source);
  if (kind === 'sequence') return plantUmlSequenceToDiagram(source, { ...options, kind });
  if (kind === 'flowchart' || kind === 'swimlane') return plantUmlActivityToDiagram(source, { ...options, kind });
  return plantUmlStructuralToDiagram(source, { ...options, kind });
}

function plantUmlSequenceToDiagram(source: string, options: PlantUmlImportOptions = {}): DiagramDocument {
  if (source.length > maximumSourceLength) throw new Error('PlantUML 文件超过 2 MiB。');
  const extracted = extractLayout(source);
  const ir = parsePlantUmlSequence(source);
  const now = options.updatedAt ?? new Date().toISOString();
  const identity = {
    schemaVersion: 1 as const,
    documentId: options.documentId ?? `sequence-${crypto.randomUUID().slice(0, 8)}`,
    revision: options.revision ?? 0,
    kind: 'sequence' as const,
    title: singleLine(ir.title || options.title || '导入的时序图').slice(0, 240),
    createdAt: options.createdAt ?? now,
    updatedAt: now
  };

  if (extracted.layout?.semanticHash === stableHash(extracted.semanticText)) {
    const restored: DiagramDocument = {
      ...identity,
      nodes: structuredClone(extracted.layout.nodes),
      edges: structuredClone(extracted.layout.edges),
      viewport: { ...extracted.layout.viewport },
      notation: {
        format: 'plantuml',
        dialect: 'sequence',
        source,
        opaqueBlocks: ir.opaqueBlocks,
        lastSyncedRevision: identity.revision
      }
    };
    assertDiagramDocument(restored);
    return restored;
  }

  const lifelineHeight = Math.max(500, 270 + Math.max(0, ir.messages.length - 1) * 58);
  const participantNodes = ir.participants.map((participant, index): DiagramNode => {
    const appearance = participantAppearance(participant.type);
    return {
      id: safeIdentifier('participant', participant.alias, index),
      type: 'diagramNode',
      position: { x: 40 + index * 240, y: 30 },
      width: 160,
      height: lifelineHeight,
      zIndex: index,
      data: {
        label: participant.label || participant.alias,
        category: appearance.category,
        shape: 'lifeline',
        icon: appearance.icon,
        showLabel: true,
        color: appearance.color,
        plantUmlId: participant.alias,
        plantUmlType: participant.type
      }
    };
  });
  const participantByAlias = new Map(ir.participants.map((participant, index) => [participant.alias, participantNodes[index]]));

  const activationRanges = [...ir.activations];
  if (activationRanges.length === 0) {
    ir.messages.forEach((message, index) => {
      if (message.dashed) return;
      const responseIndex = ir.messages.findIndex((candidate, candidateIndex) => candidateIndex > index && candidate.dashed && candidate.source === message.target);
      activationRanges.push({ alias: message.target, startMessage: index, endMessage: responseIndex >= 0 ? responseIndex : index });
    });
  }
  const activationNodes = activationRanges.flatMap((activation, index): DiagramNode[] => {
    const owner = participantByAlias.get(activation.alias);
    if (!owner) return [];
    const startY = messageY(activation.startMessage);
    const endY = messageY(activation.endMessage);
    const appearance = participantAppearance(String(owner.data.plantUmlType ?? 'participant'));
    return [{
      id: safeIdentifier('activation', `${activation.alias}-${index}`, index),
      type: 'diagramNode',
      parentId: owner.id,
      extent: 'parent',
      position: { x: 73, y: startY - owner.position.y },
      width: 14,
      height: Math.max(72, endY - startY + 30),
      zIndex: 20 + index,
      data: {
        label: `${owner.data.label}激活`,
        category: 'process',
        shape: 'activation',
        showLabel: false,
        color: appearance.color,
        fillColor: appearance.fill,
        sequenceOwnerId: owner.id,
        plantUmlId: `activation-${activation.alias}-${index}`,
        plantUmlType: 'activation'
      }
    }];
  });

  const allNodes = [...participantNodes, ...activationNodes];
  const edges = ir.messages.flatMap((message, index): DiagramEdge[] => {
    const sourceParticipant = participantByAlias.get(message.source);
    const targetParticipant = participantByAlias.get(message.target);
    if (!sourceParticipant || !targetParticipant) return [];
    const y = messageY(index);
    const sourceActivation = activeActivation(allNodes, sourceParticipant.id, y);
    const targetActivation = activeActivation(allNodes, targetParticipant.id, y);
    const sourceNode = sourceActivation ?? sourceParticipant;
    const targetNode = targetActivation ?? targetParticipant;
    const goesRight = targetParticipant.position.x > sourceParticipant.position.x;
    return [{
      id: safeIdentifier('message', String(index + 1), index),
      source: sourceNode.id,
      target: targetNode.id,
      sourceHandle: sequenceHandleForY(allNodes, sourceNode, y, goesRight ? 'right' : 'left'),
      targetHandle: sequenceHandleForY(allNodes, targetNode, y, goesRight ? 'left' : 'right'),
      label: message.label,
      type: 'straight',
      data: {
        relation: message.label,
        dashed: message.dashed,
        lineStyle: message.dashed ? 'dashed' : 'solid',
        startMarker: 'none',
        endMarker: 'arrow',
        strokeWidth: 1.4,
        color: '#77839A',
        plantUmlId: `message-${index + 1}`
      }
    }];
  });

  const fragmentNodes = ir.fragments.map((fragment, index): DiagramNode => {
    const startY = messageY(fragment.startMessage) - 28;
    const endY = messageY(fragment.endMessage) + 42;
    return {
      id: safeIdentifier('fragment', String(index + 1), index),
      type: 'diagramNode',
      position: { x: 20, y: startY },
      width: Math.max(620, participantNodes.length * 240 - 40),
      height: Math.max(120, endY - startY),
      zIndex: 0,
      data: {
        label: `${fragment.kind}${fragment.label ? ` ${fragment.label}` : ''}`,
        category: 'process',
        shape: 'fragment',
        showLabel: true,
        color: '#667085',
        fillColor: '#FFFFFF',
        plantUmlId: `fragment-${index + 1}`,
        plantUmlType: fragment.kind
      }
    };
  });

  const document: DiagramDocument = {
    ...identity,
    nodes: [...fragmentNodes, ...allNodes],
    edges,
    viewport: { x: 0, y: 0, zoom: 1 },
    notation: {
      format: 'plantuml',
      dialect: 'sequence',
      source,
      opaqueBlocks: ir.opaqueBlocks,
      lastSyncedRevision: identity.revision
    }
  };
  assertDiagramDocument(document);
  return document;
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

type ActivityTail = { id: string; label?: string };

export function detectPlantUmlDiagramKind(source: string): DiagramKind {
  const { semanticText, layout } = extractLayout(source);
  if (layout?.kind) return layout.kind;
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
    return {
      id: idByAlias.get(node.alias)!,
      type: isContainer ? 'laneNode' : 'diagramNode',
      parentId,
      extent: parentId ? 'parent' : undefined,
      position: { x: 0, y: 0 },
      width: isContainer ? 280 : 200,
      height: isContainer ? 180 : 88,
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

function addLayoutMetadata(lines: string[], document: DiagramDocument): string {
  const semanticText = lines.join('\n');
  const layout: LayoutPayload = { version: 1, semanticHash: stableHash(semanticText), kind: document.kind, nodes: structuredClone(document.nodes), edges: structuredClone(document.edges), viewport: { ...document.viewport } };
  const encoded = encodeBase64Url(JSON.stringify(layout));
  const metadataLines = chunk(encoded, 180).map((part, index, parts) => `${layoutPrefix}${index + 1}/${parts.length} ${part}`);
  return [...lines.slice(0, -1), ...metadataLines, '@enduml', ''].join('\n');
}

function parseStructuralDeclaration(line: string): PlantUmlStructuralNode | undefined {
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

function parseStructuralEdge(line: string): PlantUmlStructuralEdge | undefined {
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

function normalizeStructuralEndpoint(value: string): string {
  const trimmed = value.trim();
  if (trimmed.startsWith('[') && trimmed.endsWith(']')) return trimmed.slice(1, -1).replaceAll('\\n', '\n').trim();
  return unquote(trimmed);
}

function structuralKeyword(node: DiagramNode, dialect: 'component' | 'deployment'): string {
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

function structuralAppearance(type: string, kind: 'architecture' | 'topology'): {
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

function layoutStructuralNodes(
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
    const childAliases = new Set(children.map((child) => child.alias));
    const childEdges = edges.filter((edge) => childAliases.has(edge.source) && childAliases.has(edge.target));
    const childRanks = structuralRanks(children, childEdges);
    const ranks = [...new Set(children.map((child) => childRanks.get(child.alias) ?? 0))].sort((left, right) => left - right);
    let x = 28;
    let contentBottom = 58;
    for (const rank of ranks) {
      const column = children.filter((child) => (childRanks.get(child.alias) ?? 0) === rank);
      const columnWidth = Math.max(200, ...column.map((child) => visualByAlias.get(child.alias)?.width ?? 200));
      let y = 58;
      for (const child of column) {
        const visualChild = visualByAlias.get(child.alias);
        if (!visualChild) continue;
        visualChild.parentId = idByAlias.get(container.alias);
        visualChild.extent = 'parent';
        visualChild.position = { x, y };
        y += (visualChild.height ?? 88) + 34;
        contentBottom = Math.max(contentBottom, y);
      }
      x += columnWidth + 42;
    }
    visualContainer.width = Math.max(280, x - 12);
    visualContainer.height = Math.max(150, contentBottom + 2);
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
      y += (visualNode.height ?? 88) + 68;
    }
    rankX += columnWidth + 110;
  }
}

function structuralRanks(nodes: PlantUmlStructuralNode[], edges: PlantUmlStructuralEdge[]): Map<string, number> {
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
  return ranks;
}

function activityRanks(nodes: PlantUmlActivityNode[], edges: PlantUmlActivityEdge[]): Map<string, number> {
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

function genericEdgeHandles(nodes: DiagramNode[], source: DiagramNode, target: DiagramNode): { source: string; target: string } {
  const sourcePosition = absolutePosition(nodes, source);
  const targetPosition = absolutePosition(nodes, target);
  const sourceCenter = { x: sourcePosition.x + (source.width ?? 190) / 2, y: sourcePosition.y + (source.height ?? 82) / 2 };
  const targetCenter = { x: targetPosition.x + (target.width ?? 190) / 2, y: targetPosition.y + (target.height ?? 82) / 2 };
  if (Math.abs(targetCenter.y - sourceCenter.y) >= Math.abs(targetCenter.x - sourceCenter.x)) {
    return targetCenter.y >= sourceCenter.y ? { source: 'bottom', target: 'top' } : { source: 'top', target: 'bottom' };
  }
  return targetCenter.x >= sourceCenter.x ? { source: 'right', target: 'left' } : { source: 'left', target: 'right' };
}

function nearestCommonDescendant(sourceA: string, sourceB: string, outgoing: Map<string, DiagramEdge[]>): string | undefined {
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

function uniqueActivityTails(tails: ActivityTail[]): ActivityTail[] {
  const seen = new Set<string>();
  return tails.filter((tail) => {
    const key = `${tail.id}\u0000${tail.label ?? ''}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function activityText(value: string): string {
  return singleLine(value).replaceAll(';', '；').replaceAll('|', '｜').replaceAll('(', '（').replaceAll(')', '）') || '处理步骤';
}

function laneFill(index: number): string {
  return ['#EEF4FC', '#F3F0FB', '#EEF7F1', '#FFF4EA', '#F8EEF3'][index % 5];
}

function parseParticipant(value: string, type: string): PlantUmlSequenceParticipant {
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

function parseMessage(line: string): PlantUmlSequenceMessage | undefined {
  const match = line.match(/^("[^"]+"|[A-Za-z_][A-Za-z0-9_.-]*)\s*(-->>|->>|-->|->|<<--|<<-|<--|<-)\s*("[^"]+"|[A-Za-z_][A-Za-z0-9_.-]*)\s*(?::\s*(.*))?$/);
  if (!match) return undefined;
  const left = unquote(match[1]);
  const right = unquote(match[3]);
  const reverse = match[2].startsWith('<');
  return {
    source: reverse ? right : left,
    target: reverse ? left : right,
    label: (match[4] ?? '').replaceAll('\\n', '\n'),
    dashed: match[2].includes('--')
  };
}

function participantAppearance(type: string): { category: DiagramNodeCategory; icon: DiagramNodeIcon; color: string; fill: string } {
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

function participantKeyword(node: DiagramNode): string {
  const explicit = String(node.data.plantUmlType ?? '').toLowerCase();
  if (participantKeywords.has(explicit)) return explicit;
  if (node.data.icon === 'user') return 'actor';
  if (node.data.icon === 'database' || node.data.category === 'database') return 'database';
  if (node.data.icon === 'queue' || node.data.category === 'queue') return 'queue';
  return 'participant';
}

function fragmentKind(node: DiagramNode): string {
  const explicit = String(node.data.plantUmlType ?? '').toLowerCase();
  if (fragmentKeywords.has(explicit)) return explicit;
  const inferred = node.data.label.trim().split(/\s+/, 1)[0].toLowerCase();
  return fragmentKeywords.has(inferred) ? inferred : 'group';
}

function uniqueAliases(nodes: DiagramNode[]): Map<string, string> {
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

function owningLifeline(nodes: DiagramNode[], nodeId: string): DiagramNode | undefined {
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

function edgeEndpointY(nodes: DiagramNode[], edge: DiagramEdge): number {
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

function absolutePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absolutePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
}

function activeActivation(nodes: DiagramNode[], ownerId: string, y: number): DiagramNode | undefined {
  return nodes
    .filter((node) => node.data.shape === 'activation' && node.data.sequenceOwnerId === ownerId)
    .filter((node) => {
      const position = absolutePosition(nodes, node);
      return y >= position.y && y <= position.y + (node.height ?? 120);
    })
    .sort((left, right) => (left.height ?? 120) - (right.height ?? 120))[0];
}

function sequenceHandleForY(nodes: DiagramNode[], node: DiagramNode, y: number, side: SequenceActivationSide): string {
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

function messageY(index: number): number {
  return 160 + Math.max(0, index) * 58;
}

function extractLayout(source: string): { semanticText: string; layout?: LayoutPayload } {
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

function safeIdentifier(prefix: string, value: string, index: number): string {
  const normalized = value.normalize('NFKD').replace(/[^A-Za-z0-9_-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 80);
  return `${prefix}-${normalized || index + 1}`.slice(0, 128);
}

function sanitizeAlias(value: string): string {
  const normalized = value.normalize('NFKD').replace(/[^A-Za-z0-9_]+/g, '_').replace(/^_+|_+$/g, '');
  const withPrefix = /^[A-Za-z_]/.test(normalized) ? normalized : `participant_${normalized}`;
  return (withPrefix || 'participant').slice(0, 96);
}

function singleLine(value: string): string {
  return value.replace(/\r?\n/g, '\\n').trim();
}

function escapeQuoted(value: string): string {
  return singleLine(value).replaceAll('\\', '\\\\').replaceAll('"', '\\"');
}

function unescapeQuoted(value: string): string {
  return value.replaceAll('\\"', '"').replaceAll('\\\\', '\\');
}

function unquote(value: string): string {
  const trimmed = value.trim();
  return trimmed.startsWith('"') && trimmed.endsWith('"') ? unescapeQuoted(trimmed.slice(1, -1)) : trimmed;
}

function stableHash(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, '0');
}

function encodeBase64Url(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = '';
  for (let index = 0; index < bytes.length; index += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(index, index + 0x8000));
  }
  return btoa(binary).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/g, '');
}

function decodeBase64Url(value: string): string {
  const base64 = value.replaceAll('-', '+').replaceAll('_', '/').padEnd(Math.ceil(value.length / 4) * 4, '=');
  const binary = atob(base64);
  const bytes = Uint8Array.from(binary, (character) => character.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

function chunk(value: string, size: number): string[] {
  const result: string[] = [];
  for (let index = 0; index < value.length; index += size) result.push(value.slice(index, index + size));
  return result;
}

function isSafeOpaqueLine(value: string): boolean {
  const lower = value.trim().toLowerCase();
  return Boolean(lower) && !lower.startsWith('@startuml') && lower !== '@enduml' && !lower.startsWith(layoutPrefix.toLowerCase());
}
