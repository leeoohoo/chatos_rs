export const CURRENT_WORKSPACE_VIEWPORT_DEFAULTS_VERSION = 2;

interface MigratableWorkspacePlacement {
  viewportDefaultsVersion?: number;
  revision: number;
  artboards: Array<{
    surfaceKind: string;
    viewportWidth: number;
    viewportHeight: number;
  }>;
  updatedAt: string;
}

export function migrateWorkspaceViewportDefaults<T extends MigratableWorkspacePlacement>(
  value: T
): { value: T; changed: boolean } {
  if ((value.viewportDefaultsVersion ?? 1) >= CURRENT_WORKSPACE_VIEWPORT_DEFAULTS_VERSION) {
    return { value, changed: false };
  }
  const next = {
    ...structuredClone(value),
    viewportDefaultsVersion: CURRENT_WORKSPACE_VIEWPORT_DEFAULTS_VERSION,
    revision: value.revision + 1,
    artboards: value.artboards.map((artboard) => (
      (artboard.surfaceKind === 'page' || artboard.surfaceKind === 'state') && artboard.viewportWidth === 1200
        ? { ...artboard, viewportWidth: 1440, viewportHeight: artboard.viewportHeight === 940 ? 900 : artboard.viewportHeight }
        : artboard
    )),
    updatedAt: new Date().toISOString()
  } as T;
  return { value: next, changed: true };
}
