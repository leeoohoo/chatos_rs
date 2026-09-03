import { createHash } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import lockfile from 'proper-lockfile';
import {
  applyWebDesignPatch,
  assertIdentifier,
  assertWebDesignDocument,
  designSummary,
  type WebDesignDocument,
  type WebDesignPatchOperation
} from './schema.js';
import { createLandingPage } from './templates.js';
import { autoLayoutContainer, syncSymbolInstances, updateSymbolFromInstance } from './editor-model.js';
import type { WebDesignDevice } from './schema.js';

export class RevisionConflictError extends Error {
  constructor(public readonly actualRevision: number) {
    super(`Web design revision conflict. Current revision is ${actualRevision}.`);
  }
}

export function resolveDataDirectory(): string {
  return path.resolve(
    process.env.WEB_DESIGN_STUDIO_DATA_DIR
      ?? process.env.CHATOS_PLUGIN_DATA_DIR
      ?? path.join(process.cwd(), '.web-design-studio-data')
  );
}

export class WebDesignDocumentStore {
  constructor(public readonly rootDirectory = resolveDataDirectory()) {}

  async initialize(): Promise<void> {
    await fs.mkdir(this.rootDirectory, { recursive: true });
  }

  async list(): Promise<ReturnType<typeof designSummary>[]> {
    await this.initialize();
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    const documents: WebDesignDocument[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.web-design.json')) continue;
      try {
        documents.push(await this.read(entry.name.slice(0, -'.web-design.json'.length)));
      } catch {
        // A malformed document stays visible through a direct read error but does not break listing.
      }
    }
    return documents.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)).map(designSummary);
  }

  async read(documentId: string): Promise<WebDesignDocument> {
    assertIdentifier(documentId, 'documentId');
    const raw = await fs.readFile(this.documentPath(documentId), 'utf8');
    const value: unknown = JSON.parse(raw);
    assertWebDesignDocument(value);
    if (value.documentId !== documentId) throw new Error('Web design file identity does not match its name.');
    return value;
  }

  async create(title?: string): Promise<WebDesignDocument> {
    return this.writeNew(createLandingPage(title?.trim() || undefined));
  }

  async writeNew(document: WebDesignDocument): Promise<WebDesignDocument> {
    await this.initialize();
    assertWebDesignDocument(document);
    const now = new Date().toISOString();
    const next = structuredClone(document);
    next.revision = 1;
    next.createdAt = now;
    next.updatedAt = now;
    const filePath = this.documentPath(next.documentId);
    try {
      await fs.writeFile(filePath, `${JSON.stringify(next, null, 2)}\n`, { encoding: 'utf8', flag: 'wx' });
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'EEXIST') throw new Error(`Document already exists: ${next.documentId}`);
      throw error;
    }
    return next;
  }

  async replace(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument> {
    assertWebDesignDocument(document);
    return this.withLock(document.documentId, async () => {
      const current = await this.read(document.documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = structuredClone(document);
      next.revision = expectedRevision + 1;
      next.createdAt = current.createdAt;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(next.documentId), next);
      return next;
    });
  }

  async patch(documentId: string, expectedRevision: number, operations: WebDesignPatchOperation[]): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = applyWebDesignPatch(current, operations);
      next.revision = expectedRevision + 1;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async autoLayout(
    documentId: string,
    expectedRevision: number,
    containerId: string,
    device: WebDesignDevice
  ): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = autoLayoutContainer(current, containerId, device);
      next.revision = expectedRevision + 1;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async syncSymbolInstances(documentId: string, expectedRevision: number, symbolId: string): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = syncSymbolInstances(current, symbolId);
      next.revision = expectedRevision + 1;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async updateSymbolFromInstance(documentId: string, expectedRevision: number, componentId: string): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = updateSymbolFromInstance(current, componentId);
      next.revision = expectedRevision + 1;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async remove(documentId: string): Promise<void> {
    assertIdentifier(documentId, 'documentId');
    await fs.unlink(this.documentPath(documentId));
  }

  private documentPath(documentId: string): string {
    assertIdentifier(documentId, 'documentId');
    return path.join(this.rootDirectory, `${documentId}.web-design.json`);
  }

  private async withLock<T>(documentId: string, action: () => Promise<T>): Promise<T> {
    await this.initialize();
    const lockTarget = this.documentPath(documentId);
    await fs.access(lockTarget);
    const release = await lockfile.lock(lockTarget, { retries: { retries: 20, factor: 1.2, minTimeout: 10, maxTimeout: 100 } });
    try {
      return await action();
    } finally {
      await release();
    }
  }

  private async atomicWrite(filePath: string, document: WebDesignDocument): Promise<void> {
    const payload = `${JSON.stringify(document, null, 2)}\n`;
    const temporary = `${filePath}.${createHash('sha256').update(payload).digest('hex').slice(0, 12)}.tmp`;
    await fs.writeFile(temporary, payload, 'utf8');
    await fs.rename(temporary, filePath);
  }
}
