import { solveSceneLayout } from '../../src/v2/layout-engine';
import { resolveResponsiveScene } from '../../src/v2/responsive-scene';
import { indexSceneDocument, type SceneDocument } from '../../src/v2/scene-schema';

export interface SceneArtboardContentBounds {
  width: number;
  height: number;
  nodeCount: number;
}

/**
 * Measure the editable surface from every solved node, not only the page root.
 * Absolute nodes deliberately stay out of auto-layout flow, but they still own
 * visible space on an artboard and therefore must grow its outer boundary.
 */
export function sceneArtboardContentBounds(
  scene: SceneDocument,
  pageId: string,
  viewportWidth: number,
  fallbackHeight = 1
): SceneArtboardContentBounds {
  const page = scene.pages.find((candidate) => candidate.id === pageId);
  const rootNodeId = page?.children[0]?.id;
  if (!rootNodeId) return { width: viewportWidth, height: Math.max(1, fallbackHeight), nodeCount: 0 };

  try {
    const effective = resolveResponsiveScene(scene, viewportWidth).document;
    const solved = solveSceneLayout(effective, { rootNodeId, viewportWidth });
    const index = indexSceneDocument(effective);
    let right = viewportWidth;
    let bottom = 0;
    let nodeCount = 0;

    for (const box of solved.boxes.values()) {
      const entry = index.get(box.nodeId);
      if (!entry || entry.pageId !== pageId || !entry.node.visible) continue;
      const { padding } = entry.node.layout;
      const paintedWidth = Math.max(box.width, padding.left + box.contentWidth + padding.right);
      const paintedHeight = Math.max(box.height, padding.top + box.contentHeight + padding.bottom);
      right = Math.max(right, box.x + paintedWidth);
      bottom = Math.max(bottom, box.y + paintedHeight);
      nodeCount += 1;
    }

    return {
      width: Math.max(1, Math.ceil(right)),
      // viewportHeight is a minimum visible editing surface, not a fixed
      // canvas height. Content can always extend it, but a short/full-screen
      // composition must not collapse below the selected artboard minimum.
      height: Math.max(1, Math.ceil(fallbackHeight), Math.ceil(bottom)),
      nodeCount
    };
  } catch {
    return { width: viewportWidth, height: Math.max(1, fallbackHeight), nodeCount: 0 };
  }
}

export function sceneArtboardContentHeight(
  scene: SceneDocument,
  pageId: string,
  viewportWidth: number,
  fallbackHeight = 1
): number {
  return sceneArtboardContentBounds(scene, pageId, viewportWidth, fallbackHeight).height;
}
