import type { WorkspaceArtboardPlacement, WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';
import { solveSceneLayout } from '../../src/v2/layout-engine';
import { resolveResponsiveScene } from '../../src/v2/responsive-scene';
import { indexSceneDocument, type SceneDocument } from '../../src/v2/scene-schema';

export interface PrototypeFlowSource {
  componentId: string;
  pageId: string;
  targetPageId: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PrototypeFlowConnection {
  id: string;
  componentId: string;
  sourcePageId: string;
  targetPageId: string;
  targetSurfaceKind: WorkspaceSurfaceKind;
  start: { x: number; y: number };
  end: { x: number; y: number };
  control1: { x: number; y: number };
  control2: { x: number; y: number };
  label: { x: number; y: number };
}

export function scenePrototypeFlowSources(
  scene: SceneDocument,
  artboards: readonly WorkspaceArtboardPlacement[]
): PrototypeFlowSource[] {
  return artboards.flatMap((artboard) => {
    const page = scene.pages.find((candidate) => candidate.id === artboard.pageId);
    const rootNodeId = page?.children[0]?.id;
    if (!rootNodeId) return [];
    try {
      const effective = resolveResponsiveScene(scene, artboard.viewportWidth).document;
      const index = indexSceneDocument(effective);
      const solved = solveSceneLayout(effective, { rootNodeId, viewportWidth: artboard.viewportWidth, viewportHeight: artboard.viewportHeight });
      return [...index.values()].flatMap(({ node, pageId }) => {
        const box = solved.boxes.get(node.id);
        if (pageId !== artboard.pageId || !node.visible || !node.prototypeLink || !box) return [];
        return [{
          componentId: node.id,
          pageId,
          targetPageId: node.prototypeLink.targetPageId,
          x: box.x,
          y: box.y,
          width: box.width,
          height: box.height
        }];
      });
    } catch {
      return [];
    }
  });
}

export function buildPrototypeFlowConnections(
  sources: readonly PrototypeFlowSource[],
  artboards: readonly WorkspaceArtboardPlacement[]
): PrototypeFlowConnection[] {
  const boardsByPage = new Map(artboards.map((artboard) => [artboard.pageId, artboard]));
  return sources.flatMap((source) => {
    const sourceBoard = boardsByPage.get(source.pageId);
    const targetBoard = boardsByPage.get(source.targetPageId);
    if (!sourceBoard || !targetBoard || sourceBoard.artboardId === targetBoard.artboardId) return [];
    const start = {
      x: sourceBoard.x + source.x + source.width,
      y: sourceBoard.y + source.y + source.height / 2
    };
    const end = {
      x: targetBoard.x,
      y: targetBoard.y + Math.min(Math.max(72, targetBoard.viewportHeight * .16), 180)
    };
    const direction = end.x >= start.x ? 1 : -1;
    const distance = Math.max(120, Math.abs(end.x - start.x) * .42);
    const control1 = { x: start.x + distance * direction, y: start.y };
    const control2 = { x: end.x - distance * direction, y: end.y };
    return [{
      id: `${source.componentId}:${source.targetPageId}`,
      componentId: source.componentId,
      sourcePageId: source.pageId,
      targetPageId: source.targetPageId,
      targetSurfaceKind: targetBoard.surfaceKind,
      start,
      end,
      control1,
      control2,
      label: { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2 }
    }];
  });
}

export function prototypeFlowPath(connection: PrototypeFlowConnection): string {
  const { start, end, control1, control2 } = connection;
  return `M ${start.x} ${start.y} C ${control1.x} ${control1.y}, ${control2.x} ${control2.y}, ${end.x} ${end.y}`;
}
