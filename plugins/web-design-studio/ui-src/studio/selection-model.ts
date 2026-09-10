export interface EditorSelectionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface EditorSelectableNode {
  id: string;
  name: string;
  type: string;
  parentId?: string;
  zIndex: number;
  locked?: boolean;
  visible: boolean;
  rect: EditorSelectionRect;
}

export interface EditorSelectionCandidate extends EditorSelectableNode {
  depth: number;
}

function nodeDepth(node: EditorSelectableNode, byId: ReadonlyMap<string, EditorSelectableNode>): number {
  let depth = 0;
  let parentId = node.parentId;
  const visited = new Set<string>();
  while (parentId && !visited.has(parentId)) {
    visited.add(parentId);
    const parent = byId.get(parentId);
    if (!parent) break;
    depth += 1;
    parentId = parent.parentId;
  }
  return depth;
}

function containsPoint(rect: EditorSelectionRect, point: { x: number; y: number }): boolean {
  return rect.width >= 0 && rect.height >= 0
    && point.x >= rect.x && point.x <= rect.x + rect.width
    && point.y >= rect.y && point.y <= rect.y + rect.height;
}

function containsRect(container: EditorSelectionRect, target: EditorSelectionRect): boolean {
  return target.x >= container.x
    && target.y >= container.y
    && target.x + target.width <= container.x + container.width
    && target.y + target.height <= container.y + container.height;
}

function intersectsRect(left: EditorSelectionRect, right: EditorSelectionRect): boolean {
  return left.width >= 0 && left.height >= 0 && right.width >= 0 && right.height >= 0
    && left.x < right.x + right.width
    && left.x + left.width > right.x
    && left.y < right.y + right.height
    && left.y + left.height > right.y;
}

export function normalizedSelectionRect(
  start: { x: number; y: number },
  end: { x: number; y: number }
): EditorSelectionRect {
  return {
    x: Math.min(start.x, end.x),
    y: Math.min(start.y, end.y),
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y)
  };
}

export function selectionBounds(rects: readonly EditorSelectionRect[]): EditorSelectionRect | undefined {
  if (rects.length === 0) return undefined;
  const left = Math.min(...rects.map((rect) => rect.x));
  const top = Math.min(...rects.map((rect) => rect.y));
  const right = Math.max(...rects.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...rects.map((rect) => rect.y + rect.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function selectionNodesInRect(
  nodes: readonly EditorSelectableNode[],
  rect: EditorSelectionRect
): EditorSelectableNode[] {
  if (rect.width <= 0 || rect.height <= 0) return [];
  const visible = nodes.filter((node) => node.visible && intersectsRect(node.rect, rect));
  const contained = visible.filter((node) => containsRect(rect, node.rect));
  const matched = contained.length > 0
    ? contained
    : visible.filter((node) => !containsRect(node.rect, rect));
  const matchedIds = new Set(matched.map((node) => node.id));
  return matched.filter((node) => {
    let parentId = node.parentId;
    const visited = new Set<string>();
    while (parentId && !visited.has(parentId)) {
      if (matchedIds.has(parentId)) return false;
      visited.add(parentId);
      parentId = nodes.find((candidate) => candidate.id === parentId)?.parentId;
    }
    return true;
  });
}

export function selectionCandidatesAtPoint(
  nodes: readonly EditorSelectableNode[],
  point: { x: number; y: number }
): EditorSelectionCandidate[] {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  return nodes
    .map((node, sourceIndex) => ({ ...node, depth: nodeDepth(node, byId), sourceIndex }))
    .filter((node) => node.visible && containsPoint(node.rect, point))
    .sort((left, right) => right.zIndex - left.zIndex
      || right.depth - left.depth
      || left.rect.width * left.rect.height - right.rect.width * right.rect.height
      || right.sourceIndex - left.sourceIndex)
    .map(({ sourceIndex: _sourceIndex, ...candidate }) => candidate);
}

export function deepestSelectionChild(nodes: readonly EditorSelectableNode[], selectedId: string): EditorSelectableNode | undefined {
  return nodes
    .filter((node) => node.visible && node.parentId === selectedId)
    .sort((left, right) => right.zIndex - left.zIndex || left.rect.width * left.rect.height - right.rect.width * right.rect.height)[0];
}

export function selectionParent(nodes: readonly EditorSelectableNode[], selectedId: string): EditorSelectableNode | undefined {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const selected = byId.get(selectedId);
  return selected?.parentId ? byId.get(selected.parentId) : undefined;
}
