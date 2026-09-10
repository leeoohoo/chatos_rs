import { createHash } from 'node:crypto';
import path from 'node:path';
import { AtomicJsonDirectory } from '../storage/atomic-json-directory.js';
import type { WebDesignSurfaceKind } from '../schema.js';
import { normalizeWorkspaceCamera, type WorkspaceCamera } from './workspace-camera.js';

export type WorkspaceSurfaceKind = WebDesignSurfaceKind;

export interface WorkspaceArtboardPlacement {
  artboardId: string;
  pageId: string;
  surfaceKind: WorkspaceSurfaceKind;
  viewportWidth: number;
  viewportHeight: number;
  x: number;
  y: number;
}

export interface WorkspacePlacementDocument {
  schemaVersion: 2;
  documentId: string;
  revision: number;
  camera: WorkspaceCamera;
  artboards: WorkspaceArtboardPlacement[];
  createdAt: string;
  updatedAt: string;
}

function normalizeArtboard(value: WorkspaceArtboardPlacement): WorkspaceArtboardPlacement {
  assertIdentifier(value.artboardId, 'Workspace artboardId');
  assertIdentifier(value.pageId, 'Workspace pageId');
  if (!(['page', 'modal', 'drawer', 'popover', 'menu', 'state'] as const).includes(value.surfaceKind)) {
    throw new Error(`Workspace artboard ${value.artboardId} surface kind is invalid.`);
  }
  if (![value.viewportWidth, value.viewportHeight].every((item) => Number.isFinite(item) && item > 0)
    || ![value.x, value.y].every(Number.isFinite)) throw new Error(`Workspace artboard ${value.artboardId} geometry is invalid.`);
  return {
    artboardId: value.artboardId,
    pageId: value.pageId,
    surfaceKind: value.surfaceKind,
    viewportWidth: value.viewportWidth,
    viewportHeight: value.viewportHeight,
    x: value.x,
    y: value.y
  };
}

export function normalizeWorkspaceArtboards(value: readonly WorkspaceArtboardPlacement[]): WorkspaceArtboardPlacement[] {
  if (!Array.isArray(value)) throw new Error('Workspace artboards are invalid.');
  const ids = new Set<string>();
  return value.map((candidate) => {
    const artboard = normalizeArtboard(candidate);
    if (ids.has(artboard.artboardId)) throw new Error(`Duplicate workspace artboard: ${artboard.artboardId}`);
    ids.add(artboard.artboardId);
    return artboard;
  });
}

export function parseWorkspaceArtboards(value: unknown): WorkspaceArtboardPlacement[] | undefined {
  if (!Array.isArray(value)) return undefined;
  try { return normalizeWorkspaceArtboards(value as WorkspaceArtboardPlacement[]); }
  catch { return undefined; }
}

function fileName(scopeKey: string, documentId: string): string {
  const digest = createHash('sha256').update(`${scopeKey}\0${documentId}`).digest('hex');
  return `workspace-placement-${digest}.json`;
}

function assertIdentifier(value: string, label: string): void {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/.test(value)) throw new Error(`${label} is invalid.`);
}

export function assertWorkspacePlacement(value: WorkspacePlacementDocument): void {
  if (!value || value.schemaVersion !== 2) throw new Error('Workspace placement is invalid.');
  assertIdentifier(value.documentId, 'Workspace documentId');
  if (!Number.isSafeInteger(value.revision) || value.revision < 1) throw new Error('Workspace placement revision is invalid.');
  normalizeWorkspaceCamera(value.camera);
  normalizeWorkspaceArtboards(value.artboards);
  if (!Number.isFinite(Date.parse(value.createdAt)) || !Number.isFinite(Date.parse(value.updatedAt))) throw new Error('Workspace placement timestamps are invalid.');
}

function initial(documentId: string, camera: WorkspaceCamera, timestamp: string): WorkspacePlacementDocument {
  const value: WorkspacePlacementDocument = { schemaVersion: 2, documentId, revision: 1, camera: normalizeWorkspaceCamera(camera), artboards: [], createdAt: timestamp, updatedAt: timestamp };
  assertWorkspacePlacement(value);
  return value;
}

export class WorkspacePlacementStore {
  readonly files: AtomicJsonDirectory;

  constructor(rootDirectory: string) {
    this.files = new AtomicJsonDirectory(path.join(rootDirectory, 'workspace-placements-v4'));
  }

  async readOrCreate(scopeKey: string, documentId: string, camera: WorkspaceCamera = { x: 0, y: 0, zoom: 1 }): Promise<WorkspacePlacementDocument> {
    assertIdentifier(scopeKey, 'Workspace scope');
    assertIdentifier(documentId, 'Workspace documentId');
    return this.files.withLock(async () => {
      const name = fileName(scopeKey, documentId);
      try {
        const current = await this.files.read<WorkspacePlacementDocument>(name);
        assertWorkspacePlacement(current);
        if (current.documentId !== documentId) throw new Error('Workspace placement identity mismatch.');
        return structuredClone(current);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
      const created = initial(documentId, camera, new Date().toISOString());
      await this.files.write(name, created);
      return structuredClone(created);
    });
  }

  async updateCamera(scopeKey: string, documentId: string, camera: WorkspaceCamera): Promise<WorkspacePlacementDocument> {
    assertIdentifier(scopeKey, 'Workspace scope');
    assertIdentifier(documentId, 'Workspace documentId');
    const normalized = normalizeWorkspaceCamera(camera);
    return this.files.withLock(async () => {
      const name = fileName(scopeKey, documentId);
      let current: WorkspacePlacementDocument;
      try { current = await this.files.read<WorkspacePlacementDocument>(name); }
      catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        current = initial(documentId, normalized, new Date().toISOString());
      }
      assertWorkspacePlacement(current);
      if (current.documentId !== documentId) throw new Error('Workspace placement identity mismatch.');
      const next: WorkspacePlacementDocument = {
        ...structuredClone(current), revision: current.revision + 1, camera: normalized, updatedAt: new Date().toISOString()
      };
      assertWorkspacePlacement(next);
      await this.files.write(name, next);
      return structuredClone(next);
    });
  }


  async updateArtboards(scopeKey: string, documentId: string, artboards: readonly WorkspaceArtboardPlacement[]): Promise<WorkspacePlacementDocument> {
    assertIdentifier(scopeKey, 'Workspace scope');
    assertIdentifier(documentId, 'Workspace documentId');
    const normalized = normalizeWorkspaceArtboards(artboards);
    return this.files.withLock(async () => {
      const name = fileName(scopeKey, documentId);
      let current: WorkspacePlacementDocument;
      try { current = await this.files.read<WorkspacePlacementDocument>(name); }
      catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
        current = initial(documentId, { x: 0, y: 0, zoom: 1 }, new Date().toISOString());
      }
      assertWorkspacePlacement(current);
      if (current.documentId !== documentId) throw new Error('Workspace placement identity mismatch.');
      const next: WorkspacePlacementDocument = {
        ...structuredClone(current), revision: current.revision + 1, artboards: normalized, updatedAt: new Date().toISOString()
      };
      assertWorkspacePlacement(next);
      await this.files.write(name, next);
      return structuredClone(next);
    });
  }
}
