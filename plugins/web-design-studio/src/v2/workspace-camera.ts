export interface WorkspacePoint { x: number; y: number; }
export interface WorkspaceRect extends WorkspacePoint { width: number; height: number; }
export interface WorkspaceViewport { width: number; height: number; }
export interface WorkspaceCamera extends WorkspacePoint { zoom: number; }
export interface WorkspaceInsets { top: number; right: number; bottom: number; left: number; }
export type WorkspaceRenderTier = 'anchor' | 'shell' | 'content' | 'runtime';
export interface WorkspaceRenderMargins { shell: number; content: number; runtime: number; }

export const MIN_WORKSPACE_ZOOM = 0.1;
export const MAX_WORKSPACE_ZOOM = 8;
export const DEFAULT_WORKSPACE_RENDER_MARGINS: WorkspaceRenderMargins = { shell: 1200, content: 720, runtime: 480 };

function finite(value: number, label: string): number {
  if (!Number.isFinite(value)) throw new Error(`${label} is invalid.`);
  return value;
}

export function clampWorkspaceZoom(zoom: number): number {
  finite(zoom, 'Workspace zoom');
  return Math.min(MAX_WORKSPACE_ZOOM, Math.max(MIN_WORKSPACE_ZOOM, zoom));
}

export function workspaceZoomFromWheel(currentZoom: number, deltaY: number, sensitivity = 0.002): number {
  finite(deltaY, 'Workspace wheel delta');
  finite(sensitivity, 'Workspace wheel sensitivity');
  if (sensitivity <= 0) throw new Error('Workspace wheel sensitivity must be greater than zero.');
  return clampWorkspaceZoom(clampWorkspaceZoom(currentZoom) * Math.exp(-deltaY * sensitivity));
}

export function normalizeWorkspaceCamera(value: WorkspaceCamera): WorkspaceCamera {
  return { x: finite(value.x, 'Workspace camera x'), y: finite(value.y, 'Workspace camera y'), zoom: clampWorkspaceZoom(value.zoom) };
}

export function panWorkspaceCamera(camera: WorkspaceCamera, screenDelta: WorkspacePoint): WorkspaceCamera {
  return normalizeWorkspaceCamera({ x: camera.x + finite(screenDelta.x, 'Workspace pan x'), y: camera.y + finite(screenDelta.y, 'Workspace pan y'), zoom: camera.zoom });
}

export function workspaceToScreen(camera: WorkspaceCamera, point: WorkspacePoint): WorkspacePoint {
  const normalized = normalizeWorkspaceCamera(camera);
  return { x: normalized.x + finite(point.x, 'Workspace point x') * normalized.zoom, y: normalized.y + finite(point.y, 'Workspace point y') * normalized.zoom };
}

export function screenToWorkspace(camera: WorkspaceCamera, point: WorkspacePoint): WorkspacePoint {
  const normalized = normalizeWorkspaceCamera(camera);
  return { x: (finite(point.x, 'Screen point x') - normalized.x) / normalized.zoom, y: (finite(point.y, 'Screen point y') - normalized.y) / normalized.zoom };
}

export function zoomWorkspaceCameraAt(camera: WorkspaceCamera, zoom: number, anchor: WorkspacePoint): WorkspaceCamera {
  const normalized = normalizeWorkspaceCamera(camera);
  const nextZoom = clampWorkspaceZoom(zoom);
  const workspaceAnchor = screenToWorkspace(normalized, anchor);
  return normalizeWorkspaceCamera({ x: finite(anchor.x, 'Zoom anchor x') - workspaceAnchor.x * nextZoom, y: finite(anchor.y, 'Zoom anchor y') - workspaceAnchor.y * nextZoom, zoom: nextZoom });
}

function validateViewport(viewport: WorkspaceViewport): WorkspaceViewport {
  if (!workspaceViewportReady(viewport)) throw new Error('Workspace viewport is invalid.');
  return viewport;
}

export function workspaceViewportReady(viewport: WorkspaceViewport | undefined): viewport is WorkspaceViewport {
  return Boolean(viewport
    && Number.isFinite(viewport.width)
    && viewport.width > 0
    && Number.isFinite(viewport.height)
    && viewport.height > 0);
}

function validateRect(rect: WorkspaceRect): WorkspaceRect {
  if (![rect.x, rect.y, rect.width, rect.height].every(Number.isFinite) || rect.width <= 0 || rect.height <= 0) throw new Error('Workspace target rectangle is invalid.');
  return rect;
}

export function unionWorkspaceRects(rects: readonly WorkspaceRect[]): WorkspaceRect | undefined {
  if (rects.length === 0) return undefined;
  const valid = rects.map(validateRect);
  const left = Math.min(...valid.map((rect) => rect.x));
  const top = Math.min(...valid.map((rect) => rect.y));
  const right = Math.max(...valid.map((rect) => rect.x + rect.width));
  const bottom = Math.max(...valid.map((rect) => rect.y + rect.height));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function workspaceRectIntersectsViewport(camera: WorkspaceCamera, rect: WorkspaceRect, viewport: WorkspaceViewport, preloadMargin = 0): boolean {
  const normalized = normalizeWorkspaceCamera(camera);
  const target = validateRect(rect);
  const visible = validateViewport(viewport);
  if (!Number.isFinite(preloadMargin) || preloadMargin < 0) throw new Error('Workspace preload margin is invalid.');
  const left = normalized.x + target.x * normalized.zoom;
  const top = normalized.y + target.y * normalized.zoom;
  const right = left + target.width * normalized.zoom;
  const bottom = top + target.height * normalized.zoom;
  return right >= -preloadMargin
    && bottom >= -preloadMargin
    && left <= visible.width + preloadMargin
    && top <= visible.height + preloadMargin;
}

export function workspaceArtboardRenderTier(
  camera: WorkspaceCamera,
  rect: WorkspaceRect,
  viewport: WorkspaceViewport,
  active = false,
  margins: WorkspaceRenderMargins = DEFAULT_WORKSPACE_RENDER_MARGINS
): WorkspaceRenderTier {
  const values = [margins.shell, margins.content, margins.runtime];
  if (!values.every(Number.isFinite) || margins.runtime < 0 || margins.content < margins.runtime || margins.shell < margins.content) {
    throw new Error('Workspace render margins are invalid.');
  }
  if (active) return 'runtime';
  if (workspaceRectIntersectsViewport(camera, rect, viewport, margins.runtime)) return 'runtime';
  if (workspaceRectIntersectsViewport(camera, rect, viewport, margins.content)) return 'content';
  if (workspaceRectIntersectsViewport(camera, rect, viewport, margins.shell)) return 'shell';
  return 'anchor';
}

export function fitWorkspaceRect(rect: WorkspaceRect, viewport: WorkspaceViewport, insets: WorkspaceInsets = { top: 80, right: 80, bottom: 80, left: 80 }, maximumZoom = 1): WorkspaceCamera {
  validateRect(rect); validateViewport(viewport);
  const availableWidth = viewport.width - insets.left - insets.right;
  const availableHeight = viewport.height - insets.top - insets.bottom;
  if (availableWidth <= 0 || availableHeight <= 0) throw new Error('Workspace fit insets leave no visible area.');
  const zoom = clampWorkspaceZoom(Math.min(availableWidth / rect.width, availableHeight / rect.height, maximumZoom));
  return normalizeWorkspaceCamera({ x: insets.left + (availableWidth - rect.width * zoom) / 2 - rect.x * zoom, y: insets.top + (availableHeight - rect.height * zoom) / 2 - rect.y * zoom, zoom });
}

export function fitWorkspaceWidth(rect: WorkspaceRect, viewport: WorkspaceViewport, insets: WorkspaceInsets = { top: 108, right: 48, bottom: 80, left: 48 }, maximumZoom = 1): WorkspaceCamera {
  validateRect(rect); validateViewport(viewport);
  const availableWidth = viewport.width - insets.left - insets.right;
  if (availableWidth <= 0) throw new Error('Workspace fit insets leave no visible width.');
  const zoom = clampWorkspaceZoom(Math.min(availableWidth / rect.width, maximumZoom));
  return normalizeWorkspaceCamera({ x: insets.left + (availableWidth - rect.width * zoom) / 2 - rect.x * zoom, y: insets.top - rect.y * zoom, zoom });
}

export function parseWorkspaceCamera(value: unknown): WorkspaceCamera | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const candidate = value as Partial<WorkspaceCamera>;
  if (typeof candidate.x !== 'number' || typeof candidate.y !== 'number' || typeof candidate.zoom !== 'number') return undefined;
  try { return normalizeWorkspaceCamera(candidate as WorkspaceCamera); } catch { return undefined; }
}
