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


import type { PlantUmlImportOptions } from './plantuml.js';

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
  async?: boolean;
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

interface SequenceMessageGeometry extends PlantUmlSequenceMessage {
  y: number;
  edge: DiagramEdge;
}

export function sequenceDiagramToPlantUml(document: DiagramDocument): string {
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
        async: edge.data?.plantUmlType === 'async-message',
        y: edgeEndpointY(document.nodes, edge),
        edge
      }];
    })
    .sort((left, right) => left.y - right.y);

  const events: Array<{ y: number; priority: number; line: string }> = [];
  for (const message of messages) {
    const arrow = message.async ? (message.dashed ? '-->>' : '->>') : (message.dashed ? '-->' : '->');
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


export function plantUmlSequenceToDiagram(source: string, options: PlantUmlImportOptions = {}): DiagramDocument {
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

  const activationRanges = inferMissingSequenceActivations(ir.messages, ir.activations);
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
      height: Math.max(44, endY - startY + 30),
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
        plantUmlId: `message-${index + 1}`,
        plantUmlType: message.async ? 'async-message' : 'message'
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
        fillColor: 'transparent',
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
