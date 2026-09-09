import {
  assertSceneDocument,
  createSceneNodeBase,
  isSceneContainer,
  isSceneSlotContainer,
  type SceneCreator,
  type SceneDocument,
  type SceneFrameNode,
  type SceneGroupNode,
  type SceneNode
} from './scene-schema.js';
import type { SceneTransaction, SceneTransactionOperation } from './scene-transaction.js';

type SceneNodeCollection = SceneNode[];

interface SceneEditorNodeLocation {
  node: SceneNode;
  parentId: string;
  collection: SceneNodeCollection;
  index: number;
  slot?: string;
}

export interface WrapSceneNodesInput {
  transactionId: string;
  author: SceneCreator;
  nodeIds: string[];
  wrapperId: string;
  kind: 'group' | 'frame';
  name: string;
  padding?: number | { top: number; right: number; bottom: number; left: number };
  childOrder?: 'source' | 'input';
  reason?: string;
}

export interface UngroupSceneNodeInput {
  transactionId: string;
  author: SceneCreator;
  wrapperId: string;
  reason?: string;
}

export type SceneResizeHandle = 'north' | 'north-east' | 'east' | 'south-east' | 'south' | 'south-west' | 'west' | 'north-west';

export interface MoveSceneNodesInput {
  transactionId: string;
  author: SceneCreator;
  nodeIds: string[];
  deltaX: number;
  deltaY: number;
  reason?: string;
}

export interface ResizeSceneNodeInput {
  transactionId: string;
  author: SceneCreator;
  nodeId: string;
  handle: SceneResizeHandle;
  deltaX: number;
  deltaY: number;
  minimumWidth?: number;
  minimumHeight?: number;
  reason?: string;
}

export interface AutoLayoutSceneFrameInput extends Omit<WrapSceneNodesInput, 'kind'> {
  direction: 'horizontal' | 'vertical';
  gap?: number;
  alignItems?: 'start' | 'center' | 'end' | 'stretch' | 'baseline';
  justifyContent?: 'start' | 'center' | 'end' | 'between' | 'around' | 'evenly';
  sizingX?: 'fixed' | 'hug';
  sizingY?: 'fixed' | 'hug';
}

export type SceneAlignment = 'left' | 'horizontal-center' | 'right' | 'top' | 'vertical-center' | 'bottom';
export type SceneDistribution = 'horizontal' | 'vertical';
export type SceneLayerPlacement = 'front' | 'forward' | 'backward' | 'back';

export interface AlignSceneNodesInput {
  transactionId: string;
  author: SceneCreator;
  nodeIds: string[];
  alignment: SceneAlignment;
  reason?: string;
}

export interface DistributeSceneNodesInput {
  transactionId: string;
  author: SceneCreator;
  nodeIds: string[];
  axis: SceneDistribution;
  reason?: string;
}

export interface ReorderSceneNodesInput {
  transactionId: string;
  author: SceneCreator;
  nodeIds: string[];
  placement: SceneLayerPlacement;
  reason?: string;
}

function childCollections(node: SceneNode): Array<{ collection: SceneNodeCollection; slot?: string }> {
  if (isSceneContainer(node)) return [{ collection: node.children }];
  if (isSceneSlotContainer(node)) return Object.entries(node.slots).map(([slot, collection]) => ({ collection, slot }));
  return [];
}

function findLocation(document: SceneDocument, nodeId: string): SceneEditorNodeLocation | undefined {
  function visit(collection: SceneNodeCollection, parentId: string, slot?: string): SceneEditorNodeLocation | undefined {
    for (const [index, node] of collection.entries()) {
      if (node.id === nodeId) return { node, parentId, collection, index, slot };
      for (const children of childCollections(node)) {
        const result = visit(children.collection, node.id, children.slot);
        if (result) return result;
      }
    }
    return undefined;
  }
  for (const page of document.pages) {
    const result = visit(page.children, page.id);
    if (result) return result;
  }
  return undefined;
}

function paddingFor(input: WrapSceneNodesInput['padding']) {
  if (typeof input === 'number') {
    if (!Number.isFinite(input) || input < 0) throw new Error('Scene wrapper padding is invalid.');
    return { top: input, right: input, bottom: input, left: input };
  }
  const padding = input ?? { top: 0, right: 0, bottom: 0, left: 0 };
  if (Object.values(padding).some((value) => !Number.isFinite(value) || value < 0)) throw new Error('Scene wrapper padding is invalid.');
  return { ...padding };
}

function assertFreeParent(document: SceneDocument, location: SceneEditorNodeLocation): void {
  const page = document.pages.find((candidate) => candidate.id === location.parentId);
  if (page) return;
  const parent = findLocation(document, location.parentId)?.node;
  if (!parent) throw new Error(`Scene wrapper parent not found: ${location.parentId}`);
  if ((!isSceneContainer(parent) && !isSceneSlotContainer(parent)) || parent.layout.mode !== 'free') {
    throw new Error('Scene nodes can only be wrapped inside a page, slot, or free-layout container.');
  }
}

function assertFiniteDelta(value: number, label: string): void {
  if (!Number.isFinite(value)) throw new Error(`${label} is invalid.`);
}

function assertFramePositionEditable(document: SceneDocument, location: SceneEditorNodeLocation): void {
  if (location.node.locked) throw new Error(`Scene node ${location.node.id} is locked.`);
  const page = document.pages.find((candidate) => candidate.id === location.parentId);
  if (page) return;
  const parent = findLocation(document, location.parentId)?.node;
  if (!parent) throw new Error(`Scene transform parent not found: ${location.parentId}`);
  if ((!isSceneContainer(parent) && !isSceneSlotContainer(parent))
    || (parent.layout.mode !== 'free' && location.node.layout.position !== 'absolute')) {
    throw new Error(`Scene node ${location.node.id} is flow-positioned by its parent layout and cannot use free transforms.`);
  }
}

function siblingSelectionLocations(document: SceneDocument, nodeIds: string[], minimumLength: number): SceneEditorNodeLocation[] {
  const locations = rootSelectionLocations(document, nodeIds);
  if (locations.length < minimumLength) throw new Error(`Scene operation requires at least ${minimumLength} selection roots.`);
  const first = locations[0];
  if (locations.some((location) => location.parentId !== first.parentId || location.slot !== first.slot)) {
    throw new Error('Scene operation requires sibling nodes from the same parent and slot.');
  }
  return locations;
}

function rootSelectionLocations(document: SceneDocument, nodeIds: string[]): SceneEditorNodeLocation[] {
  if (nodeIds.length === 0 || new Set(nodeIds).size !== nodeIds.length) throw new Error('Scene selection must contain distinct node ids.');
  const selected = new Set(nodeIds);
  const locations = nodeIds.map((nodeId) => {
    const location = findLocation(document, nodeId);
    if (!location) throw new Error(`Scene node not found: ${nodeId}`);
    return location;
  });
  return locations.filter((location) => {
    let parentId: string | undefined = location.parentId;
    const visited = new Set<string>();
    while (parentId && !visited.has(parentId)) {
      if (selected.has(parentId)) return false;
      visited.add(parentId);
      parentId = findLocation(document, parentId)?.parentId;
    }
    return true;
  });
}

function minimumDimension(value: number | undefined, label: string): number {
  const resolved = value ?? 1;
  if (!Number.isFinite(resolved) || resolved < 0) throw new Error(`${label} is invalid.`);
  return resolved;
}

export function resizeSceneRect(
  frame: { x: number; y: number; width: number; height: number },
  handle: SceneResizeHandle,
  deltaX: number,
  deltaY: number,
  minimumWidth = 1,
  minimumHeight = 1
) {
  assertFiniteDelta(deltaX, 'Scene resize deltaX');
  assertFiniteDelta(deltaY, 'Scene resize deltaY');
  minimumWidth = minimumDimension(minimumWidth, 'Scene minimum width');
  minimumHeight = minimumDimension(minimumHeight, 'Scene minimum height');
  if (![frame.x, frame.y, frame.width, frame.height].every(Number.isFinite) || frame.width < 0 || frame.height < 0) {
    throw new Error('Scene resize frame is invalid.');
  }
  const movesLeft = handle.includes('west');
  const movesRight = handle.includes('east');
  const movesTop = handle.includes('north');
  const movesBottom = handle.includes('south');
  const requestedWidth = frame.width + (movesRight ? deltaX : 0) - (movesLeft ? deltaX : 0);
  const requestedHeight = frame.height + (movesBottom ? deltaY : 0) - (movesTop ? deltaY : 0);
  const width = Math.max(minimumWidth, requestedWidth);
  const height = Math.max(minimumHeight, requestedHeight);
  return {
    x: movesLeft ? frame.x + frame.width - width : frame.x,
    y: movesTop ? frame.y + frame.height - height : frame.y,
    width,
    height
  };
}

function sceneWrapper(
  input: WrapSceneNodesInput,
  frame: { x: number; y: number; width: number; height: number },
  padding: { top: number; right: number; bottom: number; left: number }
): SceneGroupNode | SceneFrameNode {
  const node = {
    ...createSceneNodeBase(input.kind, input.name.trim(), frame, input.author),
    id: input.wrapperId,
    layout: {
      ...createSceneNodeBase(input.kind, 'wrapper-layout', { x: 0, y: 0, width: 1, height: 1 }, input.author).layout,
      padding,
      position: 'absolute' as const
    },
    children: []
  };
  return node as SceneGroupNode | SceneFrameNode;
}

export function createWrapSceneNodesTransaction(document: SceneDocument, input: WrapSceneNodesInput): SceneTransaction {
  assertSceneDocument(document);
  if (input.nodeIds.length < 2 || new Set(input.nodeIds).size !== input.nodeIds.length) {
    throw new Error('Scene wrapping requires at least two distinct nodes.');
  }
  if (!input.name.trim()) throw new Error('Scene wrapper name cannot be empty.');
  if (findLocation(document, input.wrapperId) || document.pages.some((page) => page.id === input.wrapperId) || document.documentId === input.wrapperId) {
    throw new Error(`Duplicate scene id: ${input.wrapperId}`);
  }
  const locations = input.nodeIds.map((nodeId) => {
    const location = findLocation(document, nodeId);
    if (!location) throw new Error(`Scene node not found: ${nodeId}`);
    return location;
  });
  const first = locations[0];
  if (locations.some((location) => location.parentId !== first.parentId || location.slot !== first.slot)) {
    throw new Error('Scene wrapping requires sibling nodes from the same parent and slot.');
  }
  assertFreeParent(document, first);
  const padding = input.kind === 'group' ? { top: 0, right: 0, bottom: 0, left: 0 } : paddingFor(input.padding);
  const left = Math.min(...locations.map((location) => location.node.frame.x));
  const top = Math.min(...locations.map((location) => location.node.frame.y));
  const right = Math.max(...locations.map((location) => location.node.frame.x + location.node.frame.width));
  const bottom = Math.max(...locations.map((location) => location.node.frame.y + location.node.frame.height));
  const wrapperFrame = {
    x: left - padding.left,
    y: top - padding.top,
    width: right - left + padding.left + padding.right,
    height: bottom - top + padding.top + padding.bottom
  };
  const wrapper = sceneWrapper(input, wrapperFrame, padding);
  const ordered = input.childOrder === 'input'
    ? locations
    : [...locations].sort((leftLocation, rightLocation) => leftLocation.index - rightLocation.index);
  const insertionIndex = Math.min(...locations.map((location) => location.index));
  const operations: SceneTransactionOperation[] = [{
    op: 'insert-node',
    parentId: first.parentId,
    slot: first.slot,
    index: insertionIndex,
    node: wrapper
  }];
  for (const [index, location] of ordered.entries()) {
    operations.push({
      op: 'update-node',
      nodeId: location.node.id,
      patches: [
        { path: ['frame', 'x'], value: location.node.frame.x - left },
        { path: ['frame', 'y'], value: location.node.frame.y - top }
      ]
    });
    operations.push({ op: 'move-node', nodeId: location.node.id, parentId: wrapper.id, index });
  }
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Wrap ${ordered.length} nodes in a ${input.kind}.`,
    operations
  };
}

export function createUngroupSceneNodeTransaction(document: SceneDocument, input: UngroupSceneNodeInput): SceneTransaction {
  assertSceneDocument(document);
  const location = findLocation(document, input.wrapperId);
  if (!location) throw new Error(`Scene node not found: ${input.wrapperId}`);
  if ((location.node.type !== 'group' && location.node.type !== 'frame') || location.node.layout.mode !== 'free') {
    throw new Error('Only a free-layout Scene group or frame can be ungrouped.');
  }
  if (location.node.children.length === 0) throw new Error(`Scene wrapper ${input.wrapperId} has no children.`);
  assertFreeParent(document, location);
  const padding = location.node.layout.padding;
  const operations: SceneTransactionOperation[] = [];
  for (const [index, child] of location.node.children.entries()) {
    operations.push({
      op: 'update-node',
      nodeId: child.id,
      patches: [
        { path: ['frame', 'x'], value: location.node.frame.x + padding.left + child.frame.x },
        { path: ['frame', 'y'], value: location.node.frame.y + padding.top + child.frame.y }
      ]
    });
    operations.push({
      op: 'move-node',
      nodeId: child.id,
      parentId: location.parentId,
      slot: location.slot,
      index: location.index + index
    });
  }
  operations.push({ op: 'remove-node', nodeId: location.node.id });
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Ungroup ${location.node.name}.`,
    operations
  };
}

export function createMoveSceneNodesTransaction(document: SceneDocument, input: MoveSceneNodesInput): SceneTransaction {
  assertSceneDocument(document);
  assertFiniteDelta(input.deltaX, 'Scene move deltaX');
  assertFiniteDelta(input.deltaY, 'Scene move deltaY');
  if (input.deltaX === 0 && input.deltaY === 0) throw new Error('Scene move requires a non-zero delta.');
  const locations = rootSelectionLocations(document, input.nodeIds);
  for (const location of locations) assertFramePositionEditable(document, location);
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Move ${locations.length} Scene node${locations.length === 1 ? '' : 's'}.`,
    operations: locations.map((location) => ({
      op: 'update-node',
      nodeId: location.node.id,
      patches: [
        { path: ['frame', 'x'], value: location.node.frame.x + input.deltaX },
        { path: ['frame', 'y'], value: location.node.frame.y + input.deltaY }
      ]
    }))
  };
}

export function createAlignSceneNodesTransaction(document: SceneDocument, input: AlignSceneNodesInput): SceneTransaction {
  assertSceneDocument(document);
  const locations = siblingSelectionLocations(document, input.nodeIds, 2);
  assertFreeParent(document, locations[0]);
  for (const location of locations) assertFramePositionEditable(document, location);
  const left = Math.min(...locations.map((location) => location.node.frame.x));
  const right = Math.max(...locations.map((location) => location.node.frame.x + location.node.frame.width));
  const top = Math.min(...locations.map((location) => location.node.frame.y));
  const bottom = Math.max(...locations.map((location) => location.node.frame.y + location.node.frame.height));
  const operations: SceneTransactionOperation[] = [];
  for (const location of locations) {
    const frame = location.node.frame;
    const nextX = input.alignment === 'left' ? left
      : input.alignment === 'horizontal-center' ? left + (right - left - frame.width) / 2
        : input.alignment === 'right' ? right - frame.width : frame.x;
    const nextY = input.alignment === 'top' ? top
      : input.alignment === 'vertical-center' ? top + (bottom - top - frame.height) / 2
        : input.alignment === 'bottom' ? bottom - frame.height : frame.y;
    const patches: Array<{ path: string[]; value: number }> = [];
    if (nextX !== frame.x) patches.push({ path: ['frame', 'x'], value: nextX });
    if (nextY !== frame.y) patches.push({ path: ['frame', 'y'], value: nextY });
    if (patches.length) operations.push({ op: 'update-node', nodeId: location.node.id, patches });
  }
  if (operations.length === 0) throw new Error('Scene nodes are already aligned.');
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Align ${locations.length} Scene nodes ${input.alignment}.`,
    operations
  };
}

export function createDistributeSceneNodesTransaction(document: SceneDocument, input: DistributeSceneNodesInput): SceneTransaction {
  assertSceneDocument(document);
  const locations = siblingSelectionLocations(document, input.nodeIds, 3);
  assertFreeParent(document, locations[0]);
  for (const location of locations) assertFramePositionEditable(document, location);
  const horizontal = input.axis === 'horizontal';
  const ordered = [...locations].sort((left, right) => horizontal
    ? left.node.frame.x - right.node.frame.x || left.index - right.index
    : left.node.frame.y - right.node.frame.y || left.index - right.index);
  const first = ordered[0].node.frame;
  const last = ordered.at(-1)!.node.frame;
  const start = horizontal ? first.x : first.y;
  const end = horizontal ? last.x + last.width : last.y + last.height;
  const occupied = ordered.reduce((total, location) => total + (horizontal ? location.node.frame.width : location.node.frame.height), 0);
  const gap = (end - start - occupied) / (ordered.length - 1);
  let cursor = start;
  const operations: SceneTransactionOperation[] = [];
  for (const [index, location] of ordered.entries()) {
    const frame = location.node.frame;
    const size = horizontal ? frame.width : frame.height;
    if (index > 0 && index < ordered.length - 1) {
      const current = horizontal ? frame.x : frame.y;
      if (cursor !== current) operations.push({
        op: 'update-node',
        nodeId: location.node.id,
        patches: [{ path: ['frame', horizontal ? 'x' : 'y'], value: cursor }]
      });
    }
    cursor += size + gap;
  }
  if (operations.length === 0) throw new Error('Scene nodes are already evenly distributed.');
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Distribute ${locations.length} Scene nodes ${input.axis}.`,
    operations
  };
}

export function createReorderSceneNodesTransaction(document: SceneDocument, input: ReorderSceneNodesInput): SceneTransaction {
  assertSceneDocument(document);
  const locations = siblingSelectionLocations(document, input.nodeIds, 1);
  for (const location of locations) if (location.node.locked) throw new Error(`Scene node ${location.node.id} is locked.`);
  const selected = new Set(locations.map((location) => location.node.id));
  const sourceOrder = locations[0].collection.map((node) => node.id);
  const selectedInSourceOrder = sourceOrder.filter((id) => selected.has(id));
  const desiredOrder = [...sourceOrder];
  if (input.placement === 'front') {
    desiredOrder.splice(0, desiredOrder.length, ...sourceOrder.filter((id) => !selected.has(id)), ...selectedInSourceOrder);
  } else if (input.placement === 'back') {
    desiredOrder.splice(0, desiredOrder.length, ...selectedInSourceOrder, ...sourceOrder.filter((id) => !selected.has(id)));
  } else if (input.placement === 'forward') {
    for (let index = desiredOrder.length - 2; index >= 0; index -= 1) {
      if (selected.has(desiredOrder[index]) && !selected.has(desiredOrder[index + 1])) {
        [desiredOrder[index], desiredOrder[index + 1]] = [desiredOrder[index + 1], desiredOrder[index]];
      }
    }
  } else {
    for (let index = 1; index < desiredOrder.length; index += 1) {
      if (selected.has(desiredOrder[index]) && !selected.has(desiredOrder[index - 1])) {
        [desiredOrder[index], desiredOrder[index - 1]] = [desiredOrder[index - 1], desiredOrder[index]];
      }
    }
  }
  if (desiredOrder.every((id, index) => id === sourceOrder[index])) throw new Error('Scene layers are already at the requested position.');
  const simulated = [...sourceOrder];
  const operations: SceneTransactionOperation[] = [];
  const move = (nodeId: string, index: number) => {
    const currentIndex = simulated.indexOf(nodeId);
    if (currentIndex === index || currentIndex < 0) return;
    simulated.splice(currentIndex, 1);
    simulated.splice(index, 0, nodeId);
    operations.push({ op: 'move-node', nodeId, parentId: locations[0].parentId, slot: locations[0].slot, index });
  };
  if (input.placement === 'front') {
    for (const nodeId of selectedInSourceOrder) move(nodeId, simulated.length - 1);
  } else if (input.placement === 'back') {
    for (const nodeId of [...selectedInSourceOrder].reverse()) move(nodeId, 0);
  } else if (input.placement === 'forward') {
    for (const nodeId of [...selectedInSourceOrder].reverse()) {
      const index = simulated.indexOf(nodeId);
      if (index < simulated.length - 1 && !selected.has(simulated[index + 1])) move(nodeId, index + 1);
    }
  } else {
    for (const nodeId of selectedInSourceOrder) {
      const index = simulated.indexOf(nodeId);
      if (index > 0 && !selected.has(simulated[index - 1])) move(nodeId, index - 1);
    }
  }
  if (operations.length === 0) throw new Error('Scene layers are already at the requested position.');
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Move ${locations.length} Scene layer${locations.length === 1 ? '' : 's'} ${input.placement}.`,
    operations
  };
}

export function createResizeSceneNodeTransaction(document: SceneDocument, input: ResizeSceneNodeInput): SceneTransaction {
  assertSceneDocument(document);
  const handles = new Set<SceneResizeHandle>(['north', 'north-east', 'east', 'south-east', 'south', 'south-west', 'west', 'north-west']);
  if (!handles.has(input.handle)) throw new Error('Scene resize handle is invalid.');
  const location = findLocation(document, input.nodeId);
  if (!location) throw new Error(`Scene node not found: ${input.nodeId}`);
  assertFramePositionEditable(document, location);
  const next = resizeSceneRect(
    location.node.frame,
    input.handle,
    input.deltaX,
    input.deltaY,
    minimumDimension(input.minimumWidth, 'Scene minimum width'),
    minimumDimension(input.minimumHeight, 'Scene minimum height')
  );
  const horizontal = input.handle.includes('east') || input.handle.includes('west');
  const vertical = input.handle.includes('north') || input.handle.includes('south');
  const patches: Array<{ path: string[]; value: unknown }> = [];
  if (next.x !== location.node.frame.x) patches.push({ path: ['frame', 'x'], value: next.x });
  if (next.y !== location.node.frame.y) patches.push({ path: ['frame', 'y'], value: next.y });
  if (horizontal) {
    patches.push({ path: ['frame', 'width'], value: next.width });
    if (location.node.layout.sizingX !== 'fixed') patches.push({ path: ['layout', 'sizingX'], value: 'fixed' });
  }
  if (vertical) {
    patches.push({ path: ['frame', 'height'], value: next.height });
    if (location.node.layout.sizingY !== 'fixed') patches.push({ path: ['layout', 'sizingY'], value: 'fixed' });
  }
  return {
    transactionId: input.transactionId,
    baseRevision: document.revision,
    author: input.author,
    reason: input.reason ?? `Resize ${location.node.name} from ${input.handle}.`,
    operations: [{ op: 'update-node', nodeId: location.node.id, patches }]
  };
}

function inferredAutoLayoutGap(locations: SceneEditorNodeLocation[], direction: 'horizontal' | 'vertical'): number {
  if (locations.length < 2) return 0;
  const gaps = locations.slice(1).map((location, index) => {
    const previous = locations[index].node.frame;
    const current = location.node.frame;
    return direction === 'horizontal'
      ? current.x - (previous.x + previous.width)
      : current.y - (previous.y + previous.height);
  });
  return Math.max(0, Math.round(gaps.reduce((total, gap) => total + gap, 0) / gaps.length));
}

export function createAutoLayoutSceneFrameTransaction(document: SceneDocument, input: AutoLayoutSceneFrameInput): SceneTransaction {
  assertSceneDocument(document);
  const locations = input.nodeIds.map((nodeId) => {
    const location = findLocation(document, nodeId);
    if (!location) throw new Error(`Scene node not found: ${nodeId}`);
    return location;
  }).sort((left, right) => input.direction === 'horizontal'
    ? left.node.frame.x - right.node.frame.x || left.node.frame.y - right.node.frame.y || left.index - right.index
    : left.node.frame.y - right.node.frame.y || left.node.frame.x - right.node.frame.x || left.index - right.index);
  const gap = input.gap ?? inferredAutoLayoutGap(locations, input.direction);
  if (!Number.isFinite(gap) || gap < 0) throw new Error('Scene auto layout gap is invalid.');
  const transaction = createWrapSceneNodesTransaction(document, {
    ...input,
    kind: 'frame',
    nodeIds: locations.map((location) => location.node.id),
    childOrder: 'input'
  });
  const insert = transaction.operations[0];
  if (insert.op !== 'insert-node' || insert.node.type !== 'frame') throw new Error('Scene auto layout frame could not be created.');
  transaction.reason = input.reason ?? `Create a ${input.direction} Auto Layout frame from ${locations.length} nodes.`;
  transaction.operations.push({
    op: 'update-node',
    nodeId: input.wrapperId,
    patches: [{
      path: ['layout'],
      value: {
        ...insert.node.layout,
        mode: 'auto',
        direction: input.direction,
        wrap: false,
        gap: { row: gap, column: gap },
        alignItems: input.alignItems ?? 'start',
        justifyContent: input.justifyContent ?? 'start',
        sizingX: input.sizingX ?? 'fixed',
        sizingY: input.sizingY ?? 'fixed'
      }
    }]
  });
  for (const location of locations) {
    transaction.operations.push({
      op: 'update-node',
      nodeId: location.node.id,
      patches: [{ path: ['layout', 'position'], value: 'flow' }]
    });
  }
  return transaction;
}
