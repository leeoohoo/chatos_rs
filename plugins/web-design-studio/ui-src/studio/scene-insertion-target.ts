import { editableSlotsForLibraryContract } from '../../src/library-slots';
import { resolveResponsiveScene } from '../../src/v2/responsive-scene';
import { indexSceneDocument, isSceneContainer, type SceneDocument, type SceneLibraryInstanceNode, type SceneNode } from '../../src/v2/scene-schema';
import { solveSceneLayout } from '../../src/v2/layout-engine';

export interface SceneInsertionFocus {
  nodeId: string;
  slot?: string;
}

export interface SceneInsertionTarget extends SceneInsertionFocus {
  index: number;
  x: number;
  y: number;
}

export function editableSlotsForSceneLibraryNode(node: SceneLibraryInstanceNode) {
  return editableSlotsForLibraryContract({
    width: node.frame.width,
    height: node.frame.height,
    library: {
      name: node.library,
      component: node.component,
      variant: node.variant,
      props: node.properties
    }
  });
}

function childCount(node: SceneNode, slot?: string): number {
  if (isSceneContainer(node)) return node.children.length;
  if (node.type === 'library-instance' && slot) return node.slots[slot]?.length ?? 0;
  return 0;
}

function focusForNode(node: SceneNode, preferredSlot?: string): SceneInsertionFocus | undefined {
  if (isSceneContainer(node)) return { nodeId: node.id };
  if (node.type !== 'library-instance') return undefined;
  const slots = editableSlotsForSceneLibraryNode(node);
  const slot = slots.some((candidate) => candidate.id === preferredSlot) ? preferredSlot : slots[0]?.id;
  return slot ? { nodeId: node.id, slot } : undefined;
}

export function resolveSceneInsertionTarget(input: {
  document: SceneDocument;
  pageId: string;
  viewportWidth: number;
  point: { x: number; y: number };
  preferred?: SceneInsertionFocus;
}): SceneInsertionTarget {
  const effective = resolveResponsiveScene(input.document, input.viewportWidth).document;
  const effectiveIndex = indexSceneDocument(effective);
  const sourceIndex = indexSceneDocument(input.document);
  const page = effective.pages.find((candidate) => candidate.id === input.pageId);
  const root = page?.children[0];
  if (!root) throw new Error('当前画板没有可插入内容的 Scene 根节点。');
  const solved = solveSceneLayout(effective, { rootNodeId: root.id, viewportWidth: input.viewportWidth });

  let focus: SceneInsertionFocus | undefined;
  if (input.preferred) {
    const preferredNode = sourceIndex.get(input.preferred.nodeId)?.node;
    if (preferredNode) focus = focusForNode(preferredNode, input.preferred.slot);
  }
  if (!focus) {
    const candidates = [...effectiveIndex.values()]
      .filter((entry) => entry.pageId === input.pageId)
      .flatMap((entry) => {
        const box = solved.boxes.get(entry.node.id);
        const target = focusForNode(entry.node);
        if (!box || !target || input.point.x < box.x || input.point.x > box.x + box.width || input.point.y < box.y || input.point.y > box.y + box.height) return [];
        return [{ entry, box, target }];
      })
      .sort((left, right) => right.entry.path.length - left.entry.path.length);
    focus = candidates[0]?.target ?? { nodeId: root.id };
  }

  const sourceNode = sourceIndex.get(focus.nodeId)?.node;
  const box = solved.boxes.get(focus.nodeId);
  if (!sourceNode || !box) throw new Error('没有找到可用的 Scene 内容容器。');
  const padding = sourceNode.layout.padding;
  return {
    ...focus,
    index: childCount(sourceNode, focus.slot),
    x: Math.max(0, Math.round(input.point.x - box.x - padding.left)),
    y: Math.max(0, Math.round(input.point.y - box.y - padding.top))
  };
}
