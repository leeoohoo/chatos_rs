import { componentPageId, pageOutline } from './design-quality.js';
import {
  designSummary,
  pageIdForComponent,
  pagesForDocument,
  type WebDesignDocument,
  type WebDesignPatchOperation
} from './schema.js';
import { type SceneEditorCommand } from './v2/scene-editor-command.js';
import {
  createSceneNodeBase,
  indexSceneDocument,
  type SceneDocument,
  type SceneNode,
  type SceneNodeType
} from './v2/scene-schema.js';
import type { SceneTransactionOperation } from './v2/scene-transaction.js';

export function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

export function decodeStructuredJson(value: unknown, label: string): Record<string, unknown> | unknown[] {
  if (typeof value !== 'string') throw new Error(`${label} must be JSON text.`);
  let decoded: unknown;
  try {
    decoded = JSON.parse(value);
  } catch {
    throw new Error(`${label} must contain valid JSON.`);
  }
  if (!decoded || typeof decoded !== 'object') throw new Error(`${label} must encode an object or array.`);
  return decoded as Record<string, unknown> | unknown[];
}

export function simpleSceneNode(value: unknown, operationIndex: number): SceneNode {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`operations[${operationIndex}].node must be an object.`);
  const input = value as Record<string, unknown>;
  const type = String(input.type) as SceneNodeType;
  if (!['frame', 'group', 'text', 'shape', 'library-instance'].includes(type)) throw new Error(`operations[${operationIndex}].node.type is not supported by insert-simple-node.`);
  if (!input.frame || typeof input.frame !== 'object' || Array.isArray(input.frame)) throw new Error(`operations[${operationIndex}].node.frame is required.`);
  const frame = input.frame as Record<string, unknown>;
  const base = createSceneNodeBase(type, String(input.name), {
    x: Number(frame.x), y: Number(frame.y), width: Number(frame.width), height: Number(frame.height)
  }, 'ai');
  base.id = String(input.id);
  if (typeof input.role === 'string') base.role = input.role;

  if (input.layout && typeof input.layout === 'object' && !Array.isArray(input.layout)) {
    const layout = input.layout as Record<string, unknown>;
    if (type === 'group' && layout.mode !== undefined && layout.mode !== 'free') throw new Error(`operations[${operationIndex}] group nodes support only free layout; use frame for auto or grid layout.`);
    if (typeof layout.mode === 'string') base.layout.mode = layout.mode as 'free' | 'auto' | 'grid';
    if (typeof layout.direction === 'string') base.layout.direction = layout.direction as 'horizontal' | 'vertical';
    if (typeof layout.wrap === 'boolean') base.layout.wrap = layout.wrap;
    if (typeof layout.padding === 'number') base.layout.padding = { top: layout.padding, right: layout.padding, bottom: layout.padding, left: layout.padding };
    if (typeof layout.gap === 'number') base.layout.gap = { row: layout.gap, column: layout.gap };
    if (typeof layout.alignItems === 'string') base.layout.alignItems = layout.alignItems as typeof base.layout.alignItems;
    if (typeof layout.justifyContent === 'string') base.layout.justifyContent = layout.justifyContent as typeof base.layout.justifyContent;
    if (typeof layout.sizingX === 'string') base.layout.sizingX = layout.sizingX as typeof base.layout.sizingX;
    if (typeof layout.sizingY === 'string') base.layout.sizingY = layout.sizingY as typeof base.layout.sizingY;
    if (typeof layout.position === 'string') base.layout.position = layout.position as typeof base.layout.position;
    if (typeof layout.clipContent === 'boolean') base.layout.clipContent = layout.clipContent;
  }

  const style = input.style && typeof input.style === 'object' && !Array.isArray(input.style)
    ? input.style as Record<string, unknown>
    : {};
  if (typeof style.opacity === 'number') base.appearance.opacity = style.opacity;
  const fill = typeof style.fill === 'string'
    ? style.fill
    : type === 'text' || type === 'shape' ? '#111111' : undefined;
  if (fill) base.appearance.fills = [{ type: 'solid', visible: true, opacity: typeof style.fillOpacity === 'number' ? style.fillOpacity : 1, color: fill }];
  if (typeof style.radius === 'number') base.appearance.radius = { topLeft: style.radius, topRight: style.radius, bottomRight: style.radius, bottomLeft: style.radius };
  if (typeof style.stroke === 'string') {
    const width = typeof style.strokeWidth === 'number' ? style.strokeWidth : 1;
    base.appearance.strokes = [{
      paint: { type: 'solid', visible: true, opacity: 1, color: style.stroke },
      width: { top: width, right: width, bottom: width, left: width },
      style: 'solid'
    }];
  }
  if (typeof style.shadowColor === 'string' || typeof style.shadowRadius === 'number') {
    base.appearance.effects = [{
      type: 'drop-shadow', visible: true,
      radius: typeof style.shadowRadius === 'number' ? style.shadowRadius : 16,
      color: typeof style.shadowColor === 'string' ? style.shadowColor : '#00000033',
      offset: {
        x: typeof style.shadowOffsetX === 'number' ? style.shadowOffsetX : 0,
        y: typeof style.shadowOffsetY === 'number' ? style.shadowOffsetY : 8
      }
    }];
  }
  if (type === 'text') {
    base.appearance.typography = {
      fontFamily: typeof style.fontFamily === 'string' ? style.fontFamily : 'Inter, system-ui, sans-serif',
      fontSize: typeof style.fontSize === 'number' ? style.fontSize : 16,
      fontWeight: typeof style.fontWeight === 'number' ? style.fontWeight : 400,
      lineHeight: typeof style.lineHeight === 'number' ? style.lineHeight : 1.5,
      letterSpacing: typeof style.letterSpacing === 'number' ? style.letterSpacing : 0,
      textAlign: typeof style.textAlign === 'string' ? style.textAlign as 'left' | 'center' | 'right' | 'justify' : 'left'
    };
    return { ...base, type, content: typeof input.content === 'string' ? input.content : '' };
  }
  if (type === 'shape') return { ...base, type, shape: typeof input.shape === 'string' ? input.shape as 'rectangle' : 'rectangle' };
  if (type === 'library-instance') {
    if (typeof input.library !== 'string' || typeof input.component !== 'string') throw new Error(`operations[${operationIndex}] library-instance must copy library and component from web_design_get_component_contract.`);
    return {
      ...base, type, library: input.library, component: input.component,
      ...(typeof input.variant === 'string' ? { variant: input.variant } : {}),
      ...(typeof input.content === 'string' ? { content: input.content } : {}),
      properties: input.properties && typeof input.properties === 'object' && !Array.isArray(input.properties) ? input.properties as Record<string, unknown> : {},
      slots: input.slots && typeof input.slots === 'object' && !Array.isArray(input.slots) ? input.slots as Record<string, SceneNode[]> : {}
    };
  }
  if (type === 'frame') return { ...base, type, children: [] };
  if (type === 'group') return { ...base, type, children: [] };
  throw new Error(`operations[${operationIndex}].node.type is not supported by insert-simple-node.`);
}

export function simpleSceneTree(
  value: unknown,
  operationIndex: number,
  depth = 0,
  counter: { value: number } = { value: 0 }
): SceneNode {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`operations[${operationIndex}].tree must use { node, children? }.`);
  }
  if (depth > 24) throw new Error(`operations[${operationIndex}].tree exceeds 24 nesting levels.`);
  counter.value += 1;
  if (counter.value > 512) throw new Error(`operations[${operationIndex}].tree exceeds 512 editable nodes.`);
  const input = value as Record<string, unknown>;
  const node = simpleSceneNode(input.node, operationIndex);
  const children = input.children === undefined ? [] : input.children;
  if (!Array.isArray(children)) throw new Error(`operations[${operationIndex}].tree.children must be an array.`);
  if (children.length > 0) {
    if (node.type !== 'frame' && node.type !== 'group') {
      throw new Error(`operations[${operationIndex}].tree node ${node.id} cannot contain children.`);
    }
    node.children = children.map((child) => simpleSceneTree(child, operationIndex, depth + 1, counter));
  }
  return node;
}

export function normalizeGenerationOperations(value: unknown): SceneTransactionOperation[] {
  if (!Array.isArray(value)) return value as SceneTransactionOperation[];
  return value.map((item, operationIndex) => {
    if (!item || typeof item !== 'object' || Array.isArray(item)) return item as SceneTransactionOperation;
    const operation = item as Record<string, unknown>;
    if (operation.op === 'insert-simple-tree') {
      return {
        op: 'insert-node',
        parentId: String(operation.parentId),
        index: Number(operation.index),
        ...(typeof operation.slot === 'string' ? { slot: operation.slot } : {}),
        node: simpleSceneTree(operation.tree, operationIndex)
      } as SceneTransactionOperation;
    }
    if (operation.op === 'insert-simple-node') {
      return {
        op: 'insert-node',
        parentId: String(operation.parentId),
        index: Number(operation.index),
        ...(typeof operation.slot === 'string' ? { slot: operation.slot } : {}),
        node: simpleSceneNode(operation.node, operationIndex)
      } as SceneTransactionOperation;
    }
    if (operation.op !== 'update-node' || !Array.isArray(operation.patches)) return item as SceneTransactionOperation;
    return {
      ...operation,
      patches: operation.patches.map((entry, patchIndex) => {
        if (!entry || typeof entry !== 'object' || Array.isArray(entry)) return entry;
        const patch = entry as Record<string, unknown>;
        if (!Object.hasOwn(patch, 'valueJson')) return patch;
        const { valueJson, ...rest } = patch;
        return {
          ...rest,
          value: decodeStructuredJson(valueJson, `operations[${operationIndex}].patches[${patchIndex}].valueJson`)
        };
      })
    } as SceneTransactionOperation;
  });
}

export function changedComponentIds(operations: WebDesignPatchOperation[]): string[] {
  return [...new Set(operations.flatMap((operation) => {
    if (operation.op === 'upsert_component') return [operation.component.id];
    if ('componentId' in operation && typeof operation.componentId === 'string') return [operation.componentId];
    return [];
  }))];
}

export function compactMutationResult(
  document: WebDesignDocument,
  options: { pageId?: string; changedIds?: string[]; regionName?: string } = {}
) {
  const changedIds = options.changedIds ?? [];
  return {
    document: designSummary(document),
    ...(options.pageId ? { page: pageOutline(document, options.pageId) } : {}),
    ...(options.regionName ? { regionName: options.regionName } : {}),
    changedComponentCount: changedIds.length,
    changedComponentIds: changedIds.slice(0, 64),
    ...(changedIds.length > 64 ? { changedComponentIdsTruncated: true } : {}),
    nextRecommendedActions: options.pageId ? [
      { tool: 'web_design_get_page', arguments: { documentId: document.documentId, pageId: options.pageId } },
      { tool: 'web_design_validate', arguments: { documentId: document.documentId, pageId: options.pageId, mode: 'draft' } }
    ] : [
      { tool: 'web_design_get_document_outline', arguments: { documentId: document.documentId } }
    ]
  };
}

export function assertFocusedOperations(document: WebDesignDocument, operations: WebDesignPatchOperation[]): string | undefined {
  const serializedBytes = Buffer.byteLength(JSON.stringify(operations), 'utf8');
  if (serializedBytes > 65_536) {
    throw new Error(`Patch is ${serializedBytes} bytes. Split it into logical regions smaller than 65536 bytes.`);
  }
  const upserts = operations.filter((operation) => operation.op === 'upsert_component');
  if (upserts.length > 24) throw new Error('A focused patch can insert at most 24 components. Split the page into logical regions.');

  const pageIds = new Set<string>();
  for (const operation of operations) {
    if (operation.op === 'upsert_component') {
      pageIds.add(operation.component.pageId ?? pagesForDocument(document)[0].id);
      continue;
    }
    if ('componentId' in operation && typeof operation.componentId === 'string') {
      const pageId = componentPageId(document, operation.componentId);
      if (pageId) pageIds.add(pageId);
    }
  }
  if (pageIds.size > 1) throw new Error('A focused component patch may touch only one page. Split operations by page.');
  return [...pageIds][0];
}

export function changedIdsBetween(before: WebDesignDocument, after: WebDesignDocument, pageId?: string): string[] {
  const beforeById = new Map(before.components.map((component) => [component.id, JSON.stringify(component)]));
  return after.components
    .filter((component) => !pageId || pageIdForComponent(after, component) === pageId)
    .filter((component) => beforeById.get(component.id) !== JSON.stringify(component))
    .map((component) => component.id);
}


export function isMissingFileError(error: unknown): boolean {
  return (error as NodeJS.ErrnoException)?.code === 'ENOENT';
}

export function assertSceneCommandInArtboard(scene: SceneDocument, command: SceneEditorCommand, artboardId: string): void {
  if (!scene.pages.some((page) => page.id === artboardId)) throw new Error(`Artboard not found: ${artboardId}`);
  const index = indexSceneDocument(scene);
  const assertNode = (nodeId: string, label = 'node'): void => {
    const entry = index.get(nodeId);
    if (!entry) throw new Error(`${label} not found: ${nodeId}`);
    if (entry.pageId !== artboardId) throw new Error(`${label} ${nodeId} is outside artboard ${artboardId}.`);
  };
  const assertNodes = (nodeIds: string[]): void => nodeIds.forEach((nodeId) => assertNode(nodeId));
  switch (command.type) {
    case 'create-page':
    case 'duplicate-page':
    case 'delete-page':
    case 'set-variable-collections':
      throw new Error(`${command.type} is not a focused artboard edit. Use the artboard plan or directory workflow.`);
    case 'rename-page':
      if (command.pageId !== artboardId) throw new Error(`Page ${command.pageId} is outside artboard ${artboardId}.`);
      return;
    case 'insert-node':
      if (command.parentId !== artboardId) assertNode(command.parentId, 'Insertion parent');
      return;
    case 'move':
    case 'align':
    case 'distribute':
    case 'reorder':
    case 'group':
    case 'frame':
    case 'auto-layout-frame':
    case 'delete-nodes':
      assertNodes(command.nodeIds);
      return;
    case 'ungroup':
      assertNode(command.wrapperId, 'Wrapper');
      return;
    case 'resize':
    case 'set-responsive-override':
    case 'clear-responsive-override':
    case 'add-annotation':
    case 'resolve-annotation':
    case 'reopen-annotation':
    case 'update-node':
      assertNode(command.nodeId);
  }
}
