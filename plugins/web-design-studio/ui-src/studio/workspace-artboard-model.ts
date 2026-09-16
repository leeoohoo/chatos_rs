import { breakpointFor } from '../../src/editor-model';
import { pagesForDocument, type WebDesignDevice, type WebDesignDocument } from '../../src/schema';
import { matchViewportPreset, viewportDimensions } from '../../src/viewport-presets';
import type { SceneDocument } from '../../src/v2/scene-schema';
import type { WorkspaceArtboardPlacement, WorkspaceSurfaceKind } from '../../src/v2/workspace-placement-store';

type WorkspacePageSource = { id: string; surfaceKind: WorkspaceSurfaceKind };

export function updateWorkspaceArtboardById(
  artboards: readonly WorkspaceArtboardPlacement[],
  artboardId: string,
  changes: Partial<Pick<WorkspaceArtboardPlacement, 'viewportWidth' | 'viewportHeight' | 'surfaceKind' | 'x' | 'y'>>
): WorkspaceArtboardPlacement[] {
  return artboards.map((artboard) => artboard.artboardId === artboardId
    ? { ...artboard, ...changes, x: 0, y: 0 }
    : { ...artboard, x: 0, y: 0 });
}

export function workspaceViewportHeight(document: WebDesignDocument, device: WebDesignDevice): number {
  const responsive = breakpointFor(document, device);
  if (responsive.preview?.viewportHeight) return responsive.preview.viewportHeight;
  const preset = matchViewportPreset(device, responsive.width);
  return preset
    ? viewportDimensions(preset.preset, preset.orientation).height
    : Math.min(responsive.height, device === 'desktop' ? 1080 : device === 'tablet' ? 1024 : 844);
}

function workspacePageSources(document: WebDesignDocument, scene?: SceneDocument): WorkspacePageSource[] {
  return scene
    ? scene.pages.map((page) => ({ id: page.id, surfaceKind: 'page' }))
    : pagesForDocument(document).map((page) => ({ id: page.id, surfaceKind: page.surfaceKind ?? 'page' }));
}

function closestDevice(document: WebDesignDocument, width: number): WebDesignDevice {
  return (['desktop', 'tablet', 'mobile'] as const).reduce((closest, candidate) => (
    Math.abs(breakpointFor(document, candidate).width - width)
      < Math.abs(breakpointFor(document, closest).width - width) ? candidate : closest
  ), 'desktop' as WebDesignDevice);
}

function initialScenePageWidth(scene: SceneDocument | undefined, pageId: string): number | undefined {
  const root = scene?.pages.find((page) => page.id === pageId)?.children[0];
  if (!root) return undefined;
  return Number.isFinite(root.frame.width) && root.frame.width > 0 ? root.frame.width : undefined;
}

function viewportForPage(
  document: WebDesignDocument,
  scene: SceneDocument | undefined,
  page: WorkspacePageSource,
  current?: WorkspaceArtboardPlacement
): { width: number; height: number } {
  const desktop = breakpointFor(document, 'desktop');
  // Scene geometry may provide a useful initial width, but it never owns the
  // artboard viewport. Once a user or AI has chosen an artboard size, preserve
  // it independently instead of forcing it back from the root node.
  const sceneWidth = initialScenePageWidth(scene, page.id);
  const width = current?.viewportWidth ?? sceneWidth ?? desktop.width;
  const device = closestDevice(document, width);
  const preset = matchViewportPreset(device, width);
  const defaultHeight = preset
    ? viewportDimensions(preset.preset, preset.orientation).height
    : workspaceViewportHeight(document, device);
  return {
    width,
    height: current?.viewportHeight ?? defaultHeight
  };
}

export function initialWorkspaceArtboards(
  document: WebDesignDocument,
  scene?: SceneDocument
): WorkspaceArtboardPlacement[] {
  return workspacePageSources(document, scene).map((page) => {
    const viewport = viewportForPage(document, scene, page);
    const artboard: WorkspaceArtboardPlacement = {
      artboardId: `artboard-${page.id}`,
      pageId: page.id,
      surfaceKind: page.surfaceKind,
      viewportWidth: viewport.width,
      viewportHeight: viewport.height,
      x: 0,
      y: 0
    };
    return artboard;
  });
}

export function reconcileWorkspaceArtboards(
  document: WebDesignDocument,
  stored: readonly WorkspaceArtboardPlacement[],
  scene?: SceneDocument
): WorkspaceArtboardPlacement[] {
  const pages = workspacePageSources(document, scene);
  const pageById = new Map(pages.map((page) => [page.id, page]));
  const seenPages = new Set<string>();
  const valid = stored.filter((artboard) => {
    if (!pageById.has(artboard.pageId) || seenPages.has(artboard.pageId)) return false;
    seenPages.add(artboard.pageId);
    return true;
  }).map((artboard) => ({ ...artboard }));

  // The collection is a directory, not a world-space layout. Only the active
  // entry is mounted, so every persisted legacy coordinate is normalized.
  for (const artboard of valid) {
    artboard.x = 0;
    artboard.y = 0;
  }

  for (const page of pages) {
    if (seenPages.has(page.id)) continue;
    const viewport = viewportForPage(document, scene, page);
    valid.push({
      artboardId: `artboard-${page.id}`,
      pageId: page.id,
      surfaceKind: page.surfaceKind,
      viewportWidth: viewport.width,
      viewportHeight: viewport.height,
      x: 0,
      y: 0
    });
  }
  return valid;
}
