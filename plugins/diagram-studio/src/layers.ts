import type { DiagramNode } from './schema.js';

export type NodeLayerAction = 'front' | 'forward' | 'backward' | 'back';

export function reorderNodeLayers(
  nodes: DiagramNode[],
  selectedNodeIds: Iterable<string>,
  action: NodeLayerAction
): DiagramNode[] {
  const selected = new Set(selectedNodeIds);
  if (selected.size === 0) return nodes;
  const scopes = new Map<string, Array<{ node: DiagramNode; index: number }>>();
  nodes.forEach((node, index) => {
    const scope = node.parentId ?? '';
    const items = scopes.get(scope) ?? [];
    items.push({ node, index });
    scopes.set(scope, items);
  });
  const nextZIndex = new Map<string, number>();
  let changed = false;
  for (const items of scopes.values()) {
    if (!items.some(({ node }) => selected.has(node.id))) continue;
    const ordered = items.sort((left, right) => effectiveNodeZIndex(left.node) - effectiveNodeZIndex(right.node) || left.index - right.index);
    const before = ordered.map(({ node }) => node.id).join('\0');
    if (action === 'front') {
      ordered.sort((left, right) => Number(selected.has(left.node.id)) - Number(selected.has(right.node.id)));
    } else if (action === 'back') {
      ordered.sort((left, right) => Number(selected.has(right.node.id)) - Number(selected.has(left.node.id)));
    } else if (action === 'forward') {
      for (let index = ordered.length - 2; index >= 0; index -= 1) {
        if (selected.has(ordered[index].node.id) && !selected.has(ordered[index + 1].node.id)) {
          [ordered[index], ordered[index + 1]] = [ordered[index + 1], ordered[index]];
        }
      }
    } else {
      for (let index = 1; index < ordered.length; index += 1) {
        if (selected.has(ordered[index].node.id) && !selected.has(ordered[index - 1].node.id)) {
          [ordered[index], ordered[index - 1]] = [ordered[index - 1], ordered[index]];
        }
      }
    }
    if (ordered.map(({ node }) => node.id).join('\0') === before) continue;
    changed = true;
    const baseZIndex = Math.min(...ordered.map(({ node }) => effectiveNodeZIndex(node)));
    ordered.forEach(({ node }, index) => nextZIndex.set(node.id, baseZIndex + index));
  }
  if (!changed) return nodes;
  return nodes.map((node) => nextZIndex.has(node.id) ? { ...node, zIndex: nextZIndex.get(node.id) } : node);
}

export function nextNodeZIndex(nodes: DiagramNode[], parentId?: string): number {
  const siblings = nodes.filter((node) => node.parentId === parentId);
  if (siblings.length === 0) return parentId ? 2 : 1;
  return Math.max(...siblings.map(effectiveNodeZIndex)) + 1;
}

function effectiveNodeZIndex(node: DiagramNode): number {
  if (Number.isFinite(node.zIndex)) return node.zIndex!;
  if (node.type === 'laneNode') return 0;
  return node.parentId ? 2 : 1;
}
