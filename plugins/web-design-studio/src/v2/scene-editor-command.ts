import { createHash } from 'node:crypto';
import {
  createAlignSceneNodesTransaction,
  createAutoLayoutSceneFrameTransaction,
  createDistributeSceneNodesTransaction,
  createMoveSceneNodesTransaction,
  createReorderSceneNodesTransaction,
  createResizeSceneNodeTransaction,
  createUngroupSceneNodeTransaction,
  createWrapSceneNodesTransaction,
  type SceneAlignment,
  type SceneDistribution,
  type SceneLayerPlacement,
  type SceneResizeHandle
} from './scene-editor-transaction.js';
import {
  createSceneNodeBase,
  indexSceneDocument,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneCreator,
  type SceneDocument,
  type SceneFrameNode,
  type SceneNode,
  type ScenePage,
  type ScenePrototypeLink,
  type SceneResponsiveNodeOverride,
  type SceneVariableCollection
} from './scene-schema.js';
import { SceneDocumentStore, SceneRevisionConflictError } from './scene-store.js';
import type { SceneTransaction, SceneTransactionSummary } from './scene-transaction.js';

type ScenePadding = number | { top: number; right: number; bottom: number; left: number };

export interface SceneEditorNodePatch {
  path: string[];
  value: unknown;
}

export type SceneEditorCommand =
  | { type: 'create-page'; pageId: string; name: string; rootNodeId: string; width: number; height: number }
  | { type: 'duplicate-page'; pageId: string; newPageId: string; name: string }
  | { type: 'delete-page'; pageId: string }
  | { type: 'rename-page'; pageId: string; name: string }
  | { type: 'insert-node'; parentId: string; index: number; slot?: string; node: SceneNode }
  | { type: 'set-variable-collections'; collections: SceneVariableCollection[] }
  | { type: 'move'; nodeIds: string[]; deltaX: number; deltaY: number }
  | { type: 'align'; nodeIds: string[]; alignment: SceneAlignment }
  | { type: 'distribute'; nodeIds: string[]; axis: SceneDistribution }
  | { type: 'reorder'; nodeIds: string[]; placement: SceneLayerPlacement }
  | {
      type: 'resize';
      nodeId: string;
      handle: SceneResizeHandle;
      deltaX: number;
      deltaY: number;
      minimumWidth?: number;
      minimumHeight?: number;
    }
  | { type: 'group'; nodeIds: string[]; wrapperId: string; name: string }
  | { type: 'frame'; nodeIds: string[]; wrapperId: string; name: string; padding?: ScenePadding }
  | {
      type: 'auto-layout-frame';
      nodeIds: string[];
      wrapperId: string;
      name: string;
      direction: 'horizontal' | 'vertical';
      padding?: ScenePadding;
      gap?: number;
      alignItems?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
      justifyContent?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
      sizingX?: 'fixed' | 'hug';
      sizingY?: 'fixed' | 'hug';
    }
  | { type: 'ungroup'; wrapperId: string }
  | { type: 'delete-nodes'; nodeIds: string[] }
  | {
      type: 'set-responsive-override';
      ruleId: string;
      ruleName: string;
      minWidth?: number;
      maxWidth?: number;
      nodeId: string;
      override: Omit<SceneResponsiveNodeOverride, 'nodeId'>;
    }
  | { type: 'clear-responsive-override'; ruleId: string; nodeId: string }
  | { type: 'add-annotation'; nodeId: string; annotationId: string; body: string }
  | { type: 'resolve-annotation'; nodeId: string; annotationId: string }
  | { type: 'reopen-annotation'; nodeId: string; annotationId: string }
  | { type: 'update-node'; nodeId: string; patches: SceneEditorNodePatch[] };

export interface SceneEditorCommandRequest {
  transactionId: string;
  expectedRevision: number;
  reason?: string;
  command: SceneEditorCommand;
}

export interface SceneEditorCommandResult {
  document: SceneDocument;
  summary: SceneTransactionSummary;
  commandType: SceneEditorCommand['type'];
  recovered: boolean;
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;
const resizeHandles = new Set<SceneResizeHandle>([
  'north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west', 'north-west'
]);

function objectValue(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label} must be an object.`);
  return value as Record<string, unknown>;
}

function exactKeys(value: Record<string, unknown>, allowed: readonly string[], label: string): void {
  const unknown = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unknown.length > 0) throw new Error(`${label} contains unsupported fields: ${unknown.join(', ')}.`);
}

function identifier(value: unknown, label: string): string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
  return value;
}

function finiteNumber(value: unknown, label: string, minimum = -100_000, maximum = 100_000): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < minimum || value > maximum) {
    throw new Error(`${label} is invalid.`);
  }
  return value;
}

function optionalFiniteNumber(value: unknown, label: string, minimum: number, maximum: number): number | undefined {
  return value === undefined ? undefined : finiteNumber(value, label, minimum, maximum);
}

function nodeIds(value: unknown, label: string, minimumLength = 1): string[] {
  if (!Array.isArray(value) || value.length < minimumLength || value.length > 256) throw new Error(`${label} is invalid.`);
  const result = value.map((item, index) => identifier(item, `${label}[${index}]`));
  if (new Set(result).size !== result.length) throw new Error(`${label} must contain distinct node ids.`);
  return result;
}

function nonEmptyName(value: unknown, label: string): string {
  if (typeof value !== 'string' || !value.trim() || value.length > 240) throw new Error(`${label} is invalid.`);
  return value.trim();
}

function annotationBody(value: unknown, label: string): string {
  if (typeof value !== 'string' || !value.trim() || value.length > 4000) throw new Error(`${label} is invalid.`);
  return value.trim();
}

function plainObject(value: unknown, label: string): Record<string, unknown> {
  const source = objectValue(value, label);
  for (const key of Object.keys(source)) {
    if (!key || ['__proto__', 'prototype', 'constructor'].includes(key)) throw new Error(`${label} contains an invalid property.`);
  }
  return structuredClone(source);
}

function padding(value: unknown, label: string): ScenePadding | undefined {
  if (value === undefined) return undefined;
  if (typeof value === 'number') return finiteNumber(value, label, 0, 10_000);
  const source = objectValue(value, label);
  exactKeys(source, ['top', 'right', 'bottom', 'left'], label);
  return {
    top: finiteNumber(source.top, `${label}.top`, 0, 10_000),
    right: finiteNumber(source.right, `${label}.right`, 0, 10_000),
    bottom: finiteNumber(source.bottom, `${label}.bottom`, 0, 10_000),
    left: finiteNumber(source.left, `${label}.left`, 0, 10_000)
  };
}

function optionalEnum<T extends string>(value: unknown, values: readonly T[], label: string): T | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== 'string' || !values.includes(value as T)) throw new Error(`${label} is invalid.`);
  return value as T;
}

const editableNodePaths = new Map<string, (value: unknown, label: string) => unknown>([
  ['name', (value, label) => nonEmptyName(value, label)],
  ['visible', (value, label) => {
    if (typeof value !== 'boolean') throw new Error(`${label} is invalid.`);
    return value;
  }],
  ['locked', (value, label) => {
    if (typeof value !== 'boolean') throw new Error(`${label} is invalid.`);
    return value;
  }],
  ['content', (value, label) => {
    if (typeof value !== 'string' || value.length > 100_000) throw new Error(`${label} is invalid.`);
    return value;
  }],
  ['library', (value, label) => nonEmptyName(value, label)],
  ['component', (value, label) => nonEmptyName(value, label)],
  ['variant', (value, label) => {
    if (typeof value !== 'string' || value.length > 240) throw new Error(`${label} is invalid.`);
    return value;
  }],
  ['properties', (value, label) => plainObject(value, label)],
  ['prototypeLink', (value, label): ScenePrototypeLink | null => {
    if (value === null) return null;
    const source = objectValue(value, label);
    exactKeys(source, ['trigger', 'action', 'targetPageId'], label);
    if (source.trigger !== 'click') throw new Error(`${label}.trigger is invalid.`);
    const action = optionalEnum(source.action, ['navigate', 'overlay'] as const, `${label}.action`);
    if (!action) throw new Error(`${label}.action is invalid.`);
    return { trigger: 'click', action, targetPageId: identifier(source.targetPageId, `${label}.targetPageId`) };
  }],
  ['frame.x', (value, label) => finiteNumber(value, label)],
  ['frame.y', (value, label) => finiteNumber(value, label)],
  ['frame.width', (value, label) => finiteNumber(value, label, 0, 100_000)],
  ['frame.height', (value, label) => finiteNumber(value, label, 0, 100_000)],
  ['layout.mode', (value, label) => optionalEnum(value, ['free', 'auto', 'grid'] as const, label)!],
  ['layout.direction', (value, label) => optionalEnum(value, ['horizontal', 'vertical'] as const, label)!],
  ['layout.wrap', (value, label) => {
    if (typeof value !== 'boolean') throw new Error(`${label} is invalid.`);
    return value;
  }],
  ['layout.sizingX', (value, label) => optionalEnum(value, ['fixed', 'hug', 'fill'] as const, label)!],
  ['layout.sizingY', (value, label) => optionalEnum(value, ['fixed', 'hug', 'fill'] as const, label)!],
  ['layout.position', (value, label) => optionalEnum(value, ['flow', 'absolute'] as const, label)!],
  ['layout.clipContent', (value, label) => {
    if (typeof value !== 'boolean') throw new Error(`${label} is invalid.`);
    return value;
  }],
  ...(['top', 'right', 'bottom', 'left'] as const).map((side) => [
    `layout.padding.${side}`,
    (value: unknown, label: string) => finiteNumber(value, label, 0, 10_000)
  ] as const),
  ...(['row', 'column'] as const).map((axis) => [
    `layout.gap.${axis}`,
    (value: unknown, label: string) => finiteNumber(value, label, 0, 10_000)
  ] as const)
]);

function nodePatches(value: unknown, label: string): SceneEditorNodePatch[] {
  if (!Array.isArray(value) || value.length < 1 || value.length > 32) throw new Error(`${label} is invalid.`);
  const seen = new Set<string>();
  return value.map((item, index) => {
    const patch = objectValue(item, `${label}[${index}]`);
    exactKeys(patch, ['path', 'value'], `${label}[${index}]`);
    if (!Array.isArray(patch.path) || patch.path.length < 1 || patch.path.length > 3
      || patch.path.some((segment) => typeof segment !== 'string')) throw new Error(`${label}[${index}].path is invalid.`);
    const path = patch.path as string[];
    const key = path.join('.');
    const validate = editableNodePaths.get(key);
    if (!validate) throw new Error(`${label}[${index}].path is not editable.`);
    if (seen.has(key)) throw new Error(`${label} contains duplicate path ${key}.`);
    seen.add(key);
    return { path: [...path], value: validate(patch.value, `${label}[${index}].value`) };
  });
}

function responsiveOverride(value: unknown, label: string): Omit<SceneResponsiveNodeOverride, 'nodeId'> {
  const source = objectValue(value, label);
  exactKeys(source, ['visible', 'layout', 'childOrder'], label);
  const result: Omit<SceneResponsiveNodeOverride, 'nodeId'> = {};
  if (source.visible !== undefined) {
    if (typeof source.visible !== 'boolean') throw new Error(`${label}.visible is invalid.`);
    result.visible = source.visible;
  }
  if (source.layout !== undefined) result.layout = plainObject(source.layout, `${label}.layout`);
  if (source.childOrder !== undefined) result.childOrder = nodeIds(source.childOrder, `${label}.childOrder`, 1);
  if (result.visible === undefined && result.layout === undefined && result.childOrder === undefined) throw new Error(`${label} has no effect.`);
  return result;
}

function commandRequestSha256(request: SceneEditorCommandRequest): string {
  return createHash('sha256').update(JSON.stringify({
    expectedRevision: request.expectedRevision,
    reason: request.reason ?? null,
    command: request.command
  })).digest('hex');
}

function cloneScenePageForDuplicate(
  source: ScenePage,
  newPageId: string,
  name: string,
  author: SceneCreator
): { page: ScenePage; idMap: Map<string, string> } {
  const page = structuredClone(source);
  const idMap = new Map<string, string>();
  const visit = (node: SceneNode, action: (node: SceneNode) => void) => {
    action(node);
    if (isSceneContainer(node)) node.children.forEach((child) => visit(child, action));
    if (isSceneSlotContainer(node)) Object.values(node.slots).flat().forEach((child) => visit(child, action));
  };
  for (const root of page.children) visit(root, (node) => {
    const digest = createHash('sha256').update(`${newPageId}:${node.id}`).digest('hex').slice(0, 24);
    idMap.set(node.id, `${node.type}:${digest}`);
  });
  const timestamp = new Date().toISOString();
  for (const root of page.children) visit(root, (node) => {
    const sourceId = node.id;
    node.id = idMap.get(sourceId)!;
    node.annotations = [];
    node.createdBy = author;
    node.updatedBy = author;
    node.createdAt = timestamp;
    node.updatedAt = timestamp;
    if (node.type === 'component-instance' && idMap.has(node.mainComponentId)) node.mainComponentId = idMap.get(node.mainComponentId)!;
    if (node.prototypeLink?.targetPageId === source.id) node.prototypeLink.targetPageId = newPageId;
  });
  page.id = newPageId;
  page.name = name;
  return { page, idMap };
}

export function parseSceneEditorCommand(value: unknown): SceneEditorCommandRequest {
  const source = objectValue(value, 'Scene editor request');
  exactKeys(source, ['transactionId', 'expectedRevision', 'reason', 'command'], 'Scene editor request');
  const transactionId = identifier(source.transactionId, 'transactionId');
  if (!Number.isSafeInteger(source.expectedRevision) || (source.expectedRevision as number) < 0) {
    throw new Error('expectedRevision is invalid.');
  }
  const reason = source.reason === undefined ? undefined : nonEmptyName(source.reason, 'reason');
  const commandSource = objectValue(source.command, 'Scene editor command');
  const type = commandSource.type;
  if (typeof type !== 'string') throw new Error('Scene editor command type is invalid.');

  let command: SceneEditorCommand;
  if (type === 'create-page') {
    exactKeys(commandSource, ['type', 'pageId', 'name', 'rootNodeId', 'width', 'height'], 'Create page command');
    command = {
      type,
      pageId: identifier(commandSource.pageId, 'command.pageId'),
      name: nonEmptyName(commandSource.name, 'command.name'),
      rootNodeId: identifier(commandSource.rootNodeId, 'command.rootNodeId'),
      width: finiteNumber(commandSource.width, 'command.width', 240, 10_000),
      height: finiteNumber(commandSource.height, 'command.height', 240, 50_000)
    };
  } else if (type === 'duplicate-page') {
    exactKeys(commandSource, ['type', 'pageId', 'newPageId', 'name'], 'Duplicate page command');
    command = {
      type,
      pageId: identifier(commandSource.pageId, 'command.pageId'),
      newPageId: identifier(commandSource.newPageId, 'command.newPageId'),
      name: nonEmptyName(commandSource.name, 'command.name')
    };
  } else if (type === 'delete-page') {
    exactKeys(commandSource, ['type', 'pageId'], 'Delete page command');
    command = { type, pageId: identifier(commandSource.pageId, 'command.pageId') };
  } else if (type === 'rename-page') {
    exactKeys(commandSource, ['type', 'pageId', 'name'], 'Rename page command');
    command = {
      type,
      pageId: identifier(commandSource.pageId, 'command.pageId'),
      name: nonEmptyName(commandSource.name, 'command.name')
    };
  } else if (type === 'insert-node') {
    exactKeys(commandSource, ['type', 'parentId', 'index', 'slot', 'node'], 'Insert node command');
    if (!Number.isSafeInteger(commandSource.index) || (commandSource.index as number) < 0 || (commandSource.index as number) > 10_000) {
      throw new Error('command.index is invalid.');
    }
    const node = objectValue(commandSource.node, 'command.node') as unknown as SceneNode;
    command = {
      type,
      parentId: identifier(commandSource.parentId, 'command.parentId'),
      index: commandSource.index as number,
      slot: commandSource.slot === undefined ? undefined : nonEmptyName(commandSource.slot, 'command.slot'),
      node: structuredClone(node)
    };
  } else if (type === 'set-variable-collections') {
    exactKeys(commandSource, ['type', 'collections'], 'Set variable collections command');
    if (!Array.isArray(commandSource.collections) || commandSource.collections.length > 100) {
      throw new Error('command.collections is invalid.');
    }
    command = {
      type,
      collections: structuredClone(commandSource.collections as SceneVariableCollection[])
    };
  } else if (type === 'move') {
    exactKeys(commandSource, ['type', 'nodeIds', 'deltaX', 'deltaY'], 'Move command');
    command = {
      type,
      nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds'),
      deltaX: finiteNumber(commandSource.deltaX, 'command.deltaX'),
      deltaY: finiteNumber(commandSource.deltaY, 'command.deltaY')
    };
  } else if (type === 'align') {
    exactKeys(commandSource, ['type', 'nodeIds', 'alignment'], 'Align command');
    const alignment = optionalEnum(commandSource.alignment, ['left', 'horizontal-center', 'right', 'top', 'vertical-center', 'bottom'] as const, 'command.alignment');
    if (!alignment) throw new Error('command.alignment is invalid.');
    command = { type, nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds', 2), alignment };
  } else if (type === 'distribute') {
    exactKeys(commandSource, ['type', 'nodeIds', 'axis'], 'Distribute command');
    const axis = optionalEnum(commandSource.axis, ['horizontal', 'vertical'] as const, 'command.axis');
    if (!axis) throw new Error('command.axis is invalid.');
    command = { type, nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds', 3), axis };
  } else if (type === 'reorder') {
    exactKeys(commandSource, ['type', 'nodeIds', 'placement'], 'Reorder command');
    const placement = optionalEnum(commandSource.placement, ['front', 'forward', 'backward', 'back'] as const, 'command.placement');
    if (!placement) throw new Error('command.placement is invalid.');
    command = { type, nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds'), placement };
  } else if (type === 'resize') {
    exactKeys(commandSource, ['type', 'nodeId', 'handle', 'deltaX', 'deltaY', 'minimumWidth', 'minimumHeight'], 'Resize command');
    if (typeof commandSource.handle !== 'string' || !resizeHandles.has(commandSource.handle as SceneResizeHandle)) {
      throw new Error('command.handle is invalid.');
    }
    const handle = commandSource.handle as SceneResizeHandle;
    const deltaX = finiteNumber(commandSource.deltaX, 'command.deltaX');
    const deltaY = finiteNumber(commandSource.deltaY, 'command.deltaY');
    const changesHorizontalAxis = (handle.includes('east') || handle.includes('west')) && deltaX !== 0;
    const changesVerticalAxis = (handle.includes('north') || handle.includes('south')) && deltaY !== 0;
    if (!changesHorizontalAxis && !changesVerticalAxis) {
      throw new Error('Resize command requires a non-zero delta on an active handle axis.');
    }
    command = {
      type,
      nodeId: identifier(commandSource.nodeId, 'command.nodeId'),
      handle,
      deltaX,
      deltaY,
      minimumWidth: optionalFiniteNumber(commandSource.minimumWidth, 'command.minimumWidth', 0, 100_000),
      minimumHeight: optionalFiniteNumber(commandSource.minimumHeight, 'command.minimumHeight', 0, 100_000)
    };
  } else if (type === 'group') {
    exactKeys(commandSource, ['type', 'nodeIds', 'wrapperId', 'name'], 'Group command');
    command = {
      type,
      nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds', 2),
      wrapperId: identifier(commandSource.wrapperId, 'command.wrapperId'),
      name: nonEmptyName(commandSource.name, 'command.name')
    };
  } else if (type === 'frame') {
    exactKeys(commandSource, ['type', 'nodeIds', 'wrapperId', 'name', 'padding'], 'Frame command');
    command = {
      type,
      nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds', 2),
      wrapperId: identifier(commandSource.wrapperId, 'command.wrapperId'),
      name: nonEmptyName(commandSource.name, 'command.name'),
      padding: padding(commandSource.padding, 'command.padding')
    };
  } else if (type === 'auto-layout-frame') {
    exactKeys(commandSource, [
      'type', 'nodeIds', 'wrapperId', 'name', 'direction', 'padding', 'gap', 'alignItems',
      'justifyContent', 'sizingX', 'sizingY'
    ], 'Auto Layout Frame command');
    const direction = optionalEnum(commandSource.direction, ['horizontal', 'vertical'] as const, 'command.direction');
    if (!direction) throw new Error('command.direction is invalid.');
    command = {
      type,
      nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds', 2),
      wrapperId: identifier(commandSource.wrapperId, 'command.wrapperId'),
      name: nonEmptyName(commandSource.name, 'command.name'),
      direction,
      padding: padding(commandSource.padding, 'command.padding'),
      gap: optionalFiniteNumber(commandSource.gap, 'command.gap', 0, 10_000),
      alignItems: optionalEnum(commandSource.alignItems, ['start', 'center', 'end', 'stretch', 'baseline'] as const, 'command.alignItems'),
      justifyContent: optionalEnum(commandSource.justifyContent, ['start', 'center', 'end', 'between', 'around', 'evenly'] as const, 'command.justifyContent'),
      sizingX: optionalEnum(commandSource.sizingX, ['fixed', 'hug'] as const, 'command.sizingX'),
      sizingY: optionalEnum(commandSource.sizingY, ['fixed', 'hug'] as const, 'command.sizingY')
    };
  } else if (type === 'ungroup') {
    exactKeys(commandSource, ['type', 'wrapperId'], 'Ungroup command');
    command = { type, wrapperId: identifier(commandSource.wrapperId, 'command.wrapperId') };
  } else if (type === 'update-node') {
    exactKeys(commandSource, ['type', 'nodeId', 'patches'], 'Update node command');
    command = {
      type,
      nodeId: identifier(commandSource.nodeId, 'command.nodeId'),
      patches: nodePatches(commandSource.patches, 'command.patches')
    };
  } else if (type === 'set-responsive-override') {
    exactKeys(commandSource, ['type', 'ruleId', 'ruleName', 'minWidth', 'maxWidth', 'nodeId', 'override'], 'Set responsive override command');
    command = {
      type,
      ruleId: identifier(commandSource.ruleId, 'command.ruleId'),
      ruleName: nonEmptyName(commandSource.ruleName, 'command.ruleName'),
      minWidth: optionalFiniteNumber(commandSource.minWidth, 'command.minWidth', 0, 10_000),
      maxWidth: optionalFiniteNumber(commandSource.maxWidth, 'command.maxWidth', 0, 10_000),
      nodeId: identifier(commandSource.nodeId, 'command.nodeId'),
      override: responsiveOverride(commandSource.override, 'command.override')
    };
  } else if (type === 'clear-responsive-override') {
    exactKeys(commandSource, ['type', 'ruleId', 'nodeId'], 'Clear responsive override command');
    command = { type, ruleId: identifier(commandSource.ruleId, 'command.ruleId'), nodeId: identifier(commandSource.nodeId, 'command.nodeId') };
  } else if (type === 'delete-nodes') {
    exactKeys(commandSource, ['type', 'nodeIds'], 'Delete nodes command');
    command = { type, nodeIds: nodeIds(commandSource.nodeIds, 'command.nodeIds') };
  } else if (type === 'add-annotation') {
    exactKeys(commandSource, ['type', 'nodeId', 'annotationId', 'body'], 'Add annotation command');
    command = {
      type,
      nodeId: identifier(commandSource.nodeId, 'command.nodeId'),
      annotationId: identifier(commandSource.annotationId, 'command.annotationId'),
      body: annotationBody(commandSource.body, 'command.body')
    };
  } else if (type === 'resolve-annotation' || type === 'reopen-annotation') {
    exactKeys(commandSource, ['type', 'nodeId', 'annotationId'], `${type === 'resolve-annotation' ? 'Resolve' : 'Reopen'} annotation command`);
    command = {
      type,
      nodeId: identifier(commandSource.nodeId, 'command.nodeId'),
      annotationId: identifier(commandSource.annotationId, 'command.annotationId')
    };
  } else {
    throw new Error(`Unsupported Scene editor command: ${type}.`);
  }

  return { transactionId, expectedRevision: source.expectedRevision as number, reason, command };
}

export function createSceneEditorCommandTransaction(
  document: SceneDocument,
  request: SceneEditorCommandRequest,
  author: SceneCreator
): SceneTransaction {
  const common = { transactionId: request.transactionId, author, reason: request.reason };
  const command = request.command;
  let transaction: SceneTransaction;
  if (command.type === 'create-page') {
    if (author !== 'human') throw new Error('AI must use the progressive page-start workflow instead of creating pages through the editor command.');
    if (document.pages.some((page) => page.id === command.pageId) || indexSceneDocument(document).has(command.pageId)) {
      throw new Error(`Duplicate scene id: ${command.pageId}`);
    }
    if (document.pages.some((page) => page.id === command.rootNodeId) || indexSceneDocument(document).has(command.rootNodeId) || command.rootNodeId === command.pageId) {
      throw new Error(`Duplicate scene id: ${command.rootNodeId}`);
    }
    const root: SceneFrameNode = {
      ...createSceneNodeBase('frame', `${command.name} Root`, { x: 0, y: 0, width: command.width, height: command.height }, author),
      type: 'frame',
      id: command.rootNodeId,
      role: 'page-root',
      layout: {
        mode: 'auto',
        direction: 'vertical',
        wrap: false,
        padding: { top: 0, right: 0, bottom: 0, left: 0 },
        gap: { row: 0, column: 0 },
        alignItems: 'stretch',
        justifyContent: 'start',
        sizingX: 'fixed',
        sizingY: 'hug',
        minHeight: command.height,
        position: 'flow',
        clipContent: false
      },
      children: []
    };
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Create Scene page ${command.name}.`,
      operations: [{ op: 'insert-page', index: document.pages.length, page: { id: command.pageId, name: command.name, children: [root] } }]
    };
  } else if (command.type === 'duplicate-page') {
    if (author !== 'human') throw new Error('AI must use the progressive page-start workflow instead of duplicating pages through the editor command.');
    const source = document.pages.find((page) => page.id === command.pageId);
    if (!source) throw new Error(`Scene page not found: ${command.pageId}`);
    if (document.pages.some((page) => page.id === command.newPageId) || indexSceneDocument(document).has(command.newPageId)) {
      throw new Error(`Duplicate scene id: ${command.newPageId}`);
    }
    const duplicate = cloneScenePageForDuplicate(source, command.newPageId, command.name, author);
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Duplicate Scene page ${source.name}.`,
      operations: [
        { op: 'insert-page', index: document.pages.indexOf(source) + 1, page: duplicate.page },
        ...document.responsiveRules.flatMap((rule) => {
          const clonedOverrides = rule.nodeOverrides.flatMap((override) => {
            const nodeId = duplicate.idMap.get(override.nodeId);
            if (!nodeId) return [];
            return [{
              ...structuredClone(override),
              nodeId,
              ...(override.childOrder ? { childOrder: override.childOrder.map((id) => duplicate.idMap.get(id) ?? id) } : {})
            }];
          });
          return clonedOverrides.length ? [{
            op: 'set-responsive-node-overrides' as const,
            ruleId: rule.id,
            nodeOverrides: [...structuredClone(rule.nodeOverrides), ...clonedOverrides]
          }] : [];
        })
      ]
    };
  } else if (command.type === 'delete-page') {
    if (author !== 'human') throw new Error('AI cannot delete Scene pages through the editor command.');
    if (!document.pages.some((page) => page.id === command.pageId)) throw new Error(`Scene page not found: ${command.pageId}`);
    if (document.pages.length <= 1) throw new Error('A Scene document must keep at least one page.');
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Delete Scene page ${command.pageId}.`,
      operations: [{ op: 'remove-page', pageId: command.pageId }]
    };
  } else if (command.type === 'rename-page') {
    if (!document.pages.some((page) => page.id === command.pageId)) throw new Error(`Scene page not found: ${command.pageId}`);
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Rename Scene page ${command.pageId}.`,
      operations: [{ op: 'rename-page', pageId: command.pageId, name: command.name }]
    };
  } else if (command.type === 'insert-node') {
    if (author !== 'human') throw new Error('AI must insert Scene nodes through the bounded progressive generation workflow.');
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Insert Scene node ${command.node.name}.`,
      operations: [{ op: 'insert-node', parentId: command.parentId, index: command.index, slot: command.slot, node: command.node }]
    };
  } else if (command.type === 'set-variable-collections') {
    if (author !== 'human') throw new Error('AI must edit variables through bounded progressive generation transactions.');
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? 'Update Scene variable collections.',
      operations: [{ op: 'set-variable-collections', collections: command.collections }]
    };
  } else if (command.type === 'move') transaction = createMoveSceneNodesTransaction(document, { ...common, ...command });
  else if (command.type === 'align') transaction = createAlignSceneNodesTransaction(document, { ...common, ...command });
  else if (command.type === 'distribute') transaction = createDistributeSceneNodesTransaction(document, { ...common, ...command });
  else if (command.type === 'reorder') transaction = createReorderSceneNodesTransaction(document, { ...common, ...command });
  else if (command.type === 'resize') transaction = createResizeSceneNodeTransaction(document, { ...common, ...command });
  else if (command.type === 'group') {
    transaction = createWrapSceneNodesTransaction(document, { ...common, ...command, kind: 'group' });
  } else if (command.type === 'frame') {
    transaction = createWrapSceneNodesTransaction(document, { ...common, ...command, kind: 'frame' });
  } else if (command.type === 'auto-layout-frame') {
    transaction = createAutoLayoutSceneFrameTransaction(document, { ...common, ...command });
  } else if (command.type === 'update-node') {
    const node = indexSceneDocument(document).get(command.nodeId)?.node;
    if (!node) throw new Error(`Scene node not found: ${command.nodeId}`);
    const libraryOnlyRoots = new Set(['library', 'component', 'variant', 'properties']);
    if (command.patches.some((patch) => patch.path[0] === 'content') && node.type !== 'text' && node.type !== 'library-instance') {
      throw new Error('Only Scene text and library instance nodes support content editing.');
    }
    if (command.patches.some((patch) => libraryOnlyRoots.has(patch.path[0])) && node.type !== 'library-instance') {
      throw new Error('Only Scene library instance nodes support library binding edits.');
    }
    for (const patch of command.patches) {
      if (patch.path[0] !== 'prototypeLink' || patch.value === null) continue;
      const link = patch.value as ScenePrototypeLink;
      if (!document.pages.some((page) => page.id === link.targetPageId)) {
        throw new Error(`Scene prototype target page not found: ${link.targetPageId}`);
      }
    }
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Update Scene node ${node.name}.`,
      operations: [{ op: 'update-node', nodeId: command.nodeId, patches: command.patches.map((patch) => structuredClone(patch)) }]
    };
  } else if (command.type === 'set-responsive-override') {
    if (author !== 'human') throw new Error('AI must edit responsive rules through a bounded responsive generation step.');
    if (!indexSceneDocument(document).has(command.nodeId)) throw new Error(`Scene node not found: ${command.nodeId}`);
    if (command.minWidth !== undefined && command.maxWidth !== undefined && command.minWidth >= command.maxWidth) {
      throw new Error('Responsive rule width range is invalid.');
    }
    const currentRule = document.responsiveRules.find((rule) => rule.id === command.ruleId);
    const nextOverride = { nodeId: command.nodeId, ...structuredClone(command.override) };
    const nextOverrides = currentRule
      ? [...currentRule.nodeOverrides.filter((override) => override.nodeId !== command.nodeId), nextOverride]
      : [nextOverride];
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Update responsive layout for ${command.nodeId}.`,
      operations: currentRule
        ? [{ op: 'set-responsive-node-overrides', ruleId: currentRule.id, nodeOverrides: nextOverrides }]
        : [{ op: 'insert-responsive-rule', index: document.responsiveRules.length, rule: {
          id: command.ruleId,
          name: command.ruleName,
          minWidth: command.minWidth,
          maxWidth: command.maxWidth,
          variableModes: {},
          nodeOverrides: nextOverrides
        } }]
    };
  } else if (command.type === 'clear-responsive-override') {
    if (author !== 'human') throw new Error('AI must edit responsive rules through a bounded responsive generation step.');
    const currentRule = document.responsiveRules.find((rule) => rule.id === command.ruleId);
    if (!currentRule) throw new Error(`Scene responsive rule not found: ${command.ruleId}`);
    const nextOverrides = currentRule.nodeOverrides.filter((override) => override.nodeId !== command.nodeId);
    if (nextOverrides.length === currentRule.nodeOverrides.length) throw new Error(`Scene responsive override not found: ${command.nodeId}`);
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Clear responsive layout for ${command.nodeId}.`,
      operations: nextOverrides.length === 0 && Object.keys(currentRule.variableModes).length === 0
        ? [{ op: 'remove-responsive-rule', ruleId: currentRule.id }]
        : [{ op: 'set-responsive-node-overrides', ruleId: currentRule.id, nodeOverrides: nextOverrides }]
    };
  } else if (command.type === 'delete-nodes') {
    const index = indexSceneDocument(document);
    const selected = new Set(command.nodeIds);
    for (const nodeId of command.nodeIds) if (!index.has(nodeId)) throw new Error(`Scene node not found: ${nodeId}`);
    const rootIds = command.nodeIds.filter((nodeId) => {
      let parentId = index.get(nodeId)?.parentId;
      while (parentId && index.has(parentId)) {
        if (selected.has(parentId)) return false;
        parentId = index.get(parentId)?.parentId;
      }
      return true;
    });
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Delete ${rootIds.length} Scene node${rootIds.length === 1 ? '' : 's'}.`,
      operations: rootIds.map((nodeId) => ({ op: 'remove-node', nodeId }))
    };
  } else if (command.type === 'add-annotation') {
    if (author !== 'human') throw new Error('Only a human editor can add Scene review annotations.');
    const node = indexSceneDocument(document).get(command.nodeId)?.node;
    if (!node) throw new Error(`Scene node not found: ${command.nodeId}`);
    if (node.annotations.some((annotation) => annotation.id === command.annotationId)) {
      throw new Error(`Scene annotation id is already used: ${command.annotationId}`);
    }
    const createdAt = new Date().toISOString();
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `Add a human annotation to Scene node ${node.name}.`,
      operations: [{
        op: 'update-node',
        nodeId: command.nodeId,
        patches: [{
          path: ['annotations'],
          value: [...structuredClone(node.annotations), {
            id: command.annotationId,
            author,
            body: command.body,
            status: 'open',
            createdAt
          }]
        }]
      }]
    };
  } else if (command.type === 'resolve-annotation' || command.type === 'reopen-annotation') {
    if (author !== 'human') throw new Error('AI cannot directly change Scene annotation status.');
    const node = indexSceneDocument(document).get(command.nodeId)?.node;
    if (!node) throw new Error(`Scene node not found: ${command.nodeId}`);
    const annotationIndex = node.annotations.findIndex((annotation) => annotation.id === command.annotationId);
    if (annotationIndex < 0) throw new Error(`Scene annotation not found: ${command.annotationId}`);
    const annotations = structuredClone(node.annotations);
    const annotation = annotations[annotationIndex];
    const nextStatus = command.type === 'resolve-annotation' ? 'resolved' : 'open';
    if (annotation.status === nextStatus) throw new Error(`Scene annotation ${command.annotationId} is already ${nextStatus}.`);
    annotations[annotationIndex] = command.type === 'resolve-annotation'
      ? { ...annotation, status: 'resolved', resolvedAt: new Date().toISOString() }
      : { id: annotation.id, author: annotation.author, body: annotation.body, status: 'open', createdAt: annotation.createdAt };
    transaction = {
      transactionId: request.transactionId,
      baseRevision: document.revision,
      author,
      reason: request.reason ?? `${command.type === 'resolve-annotation' ? 'Resolve' : 'Reopen'} Scene annotation ${command.annotationId}.`,
      operations: [{ op: 'update-node', nodeId: command.nodeId, patches: [{ path: ['annotations'], value: annotations }] }]
    };
  } else {
    transaction = createUngroupSceneNodeTransaction(document, { ...common, ...command });
  }
  transaction.metadata = { source: 'scene-editor-command', requestSha256: commandRequestSha256(request) };
  return transaction;
}

export async function executeSceneEditorCommand(
  store: SceneDocumentStore,
  documentId: string,
  value: unknown,
  author: SceneCreator
): Promise<SceneEditorCommandResult> {
  const request = parseSceneEditorCommand(value);
  const applied = await store.findAppliedTransaction(documentId, request.transactionId);
  if (applied) {
    if (applied.transaction.author !== author
      || applied.transaction.baseRevision !== request.expectedRevision
      || applied.transaction.metadata?.source !== 'scene-editor-command'
      || applied.transaction.metadata.requestSha256 !== commandRequestSha256(request)) {
      throw new Error(`Scene transaction id is already used: ${request.transactionId}`);
    }
    return {
      document: await store.read(documentId),
      summary: applied.summary,
      commandType: request.command.type,
      recovered: true
    };
  }
  const document = await store.read(documentId);
  if (document.revision !== request.expectedRevision) throw new SceneRevisionConflictError(document.revision);
  const transaction = createSceneEditorCommandTransaction(document, request, author);
  const result = await store.apply(documentId, transaction);
  return { ...result, commandType: request.command.type, recovered: false };
}

const sceneIdSchema = { type: 'string', minLength: 1, maxLength: 160, pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$' } as const;
const sceneNodeIdsSchema = { type: 'array', minItems: 1, maxItems: 256, uniqueItems: true, items: sceneIdSchema } as const;
const sceneDeltaSchema = { type: 'number', minimum: -100000, maximum: 100000 } as const;
const scenePaddingSchema = {
  oneOf: [
    { type: 'number', minimum: 0, maximum: 10000 },
    {
      type: 'object',
      properties: {
        top: { type: 'number', minimum: 0, maximum: 10000 },
        right: { type: 'number', minimum: 0, maximum: 10000 },
        bottom: { type: 'number', minimum: 0, maximum: 10000 },
        left: { type: 'number', minimum: 0, maximum: 10000 }
      },
      required: ['top', 'right', 'bottom', 'left'],
      additionalProperties: false
    }
  ]
} as const;

export const sceneEditorCommandRequestSchema = {
  type: 'object',
  properties: {
    transactionId: sceneIdSchema,
    expectedRevision: { type: 'integer', minimum: 0 },
    reason: { type: 'string', minLength: 1, maxLength: 240 },
    command: {
      oneOf: [
        {
          type: 'object',
          properties: { type: { const: 'move' }, nodeIds: sceneNodeIdsSchema, deltaX: sceneDeltaSchema, deltaY: sceneDeltaSchema },
          required: ['type', 'nodeIds', 'deltaX', 'deltaY'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'align' }, nodeIds: { ...sceneNodeIdsSchema, minItems: 2 },
            alignment: { type: 'string', enum: ['left', 'horizontal-center', 'right', 'top', 'vertical-center', 'bottom'] }
          },
          required: ['type', 'nodeIds', 'alignment'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'distribute' }, nodeIds: { ...sceneNodeIdsSchema, minItems: 3 },
            axis: { type: 'string', enum: ['horizontal', 'vertical'] }
          },
          required: ['type', 'nodeIds', 'axis'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'reorder' }, nodeIds: sceneNodeIdsSchema,
            placement: { type: 'string', enum: ['front', 'forward', 'backward', 'back'] }
          },
          required: ['type', 'nodeIds', 'placement'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'resize' }, nodeId: sceneIdSchema,
            handle: { type: 'string', enum: [...resizeHandles] },
            deltaX: sceneDeltaSchema, deltaY: sceneDeltaSchema,
            minimumWidth: { type: 'number', minimum: 0, maximum: 100000 },
            minimumHeight: { type: 'number', minimum: 0, maximum: 100000 }
          },
          required: ['type', 'nodeId', 'handle', 'deltaX', 'deltaY'], additionalProperties: false
        },
        {
          type: 'object',
          properties: { type: { const: 'group' }, nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema, name: { type: 'string', minLength: 1, maxLength: 240 } },
          required: ['type', 'nodeIds', 'wrapperId', 'name'], additionalProperties: false
        },
        {
          type: 'object',
          properties: { type: { const: 'frame' }, nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema, name: { type: 'string', minLength: 1, maxLength: 240 }, padding: scenePaddingSchema },
          required: ['type', 'nodeIds', 'wrapperId', 'name'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'auto-layout-frame' }, nodeIds: { ...sceneNodeIdsSchema, minItems: 2 }, wrapperId: sceneIdSchema,
            name: { type: 'string', minLength: 1, maxLength: 240 }, direction: { type: 'string', enum: ['horizontal', 'vertical'] },
            padding: scenePaddingSchema, gap: { type: 'number', minimum: 0, maximum: 10000 },
            alignItems: { type: 'string', enum: ['start', 'center', 'end', 'stretch', 'baseline'] },
            justifyContent: { type: 'string', enum: ['start', 'center', 'end', 'between', 'around', 'evenly'] },
            sizingX: { type: 'string', enum: ['fixed', 'hug'] }, sizingY: { type: 'string', enum: ['fixed', 'hug'] }
          },
          required: ['type', 'nodeIds', 'wrapperId', 'name', 'direction'], additionalProperties: false
        },
        {
          type: 'object', properties: { type: { const: 'ungroup' }, wrapperId: sceneIdSchema },
          required: ['type', 'wrapperId'], additionalProperties: false
        },
        {
          type: 'object',
          properties: {
            type: { const: 'update-node' },
            nodeId: sceneIdSchema,
            patches: {
              type: 'array', minItems: 1, maxItems: 32,
              items: {
                type: 'object',
                properties: {
                  path: {
                    enum: [...editableNodePaths.keys()].map((path) => path.split('.'))
                  },
                  value: {}
                },
                required: ['path', 'value'], additionalProperties: false
              }
            }
          },
          required: ['type', 'nodeId', 'patches'], additionalProperties: false
        },
        {
          type: 'object', properties: { type: { const: 'delete-nodes' }, nodeIds: sceneNodeIdsSchema },
          required: ['type', 'nodeIds'], additionalProperties: false
        }
      ]
    }
  },
  required: ['transactionId', 'expectedRevision', 'command'],
  additionalProperties: false
} as const;
