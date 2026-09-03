import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import lockfile from 'proper-lockfile';
import {
  applyWebDesignPatch,
  assertIdentifier,
  assertWebDesignDocument,
  assertWebDesignProject,
  designSummary,
  webDesignProjectSummary,
  type WebDesignDocument,
  type WebDesignProject,
  type WebDesignPatchOperation
} from './schema.js';
import { createBlankWebsite, createLandingPage } from './templates.js';
import { autoLayoutContainer, growPageToFitContent, syncSymbolInstances, updateSymbolFromInstance } from './editor-model.js';
import { createBlockPreset, createPageTemplate, type WebDesignBlockPresetId, type WebDesignPageTemplateId } from './component-library.js';
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

  async listProjects(): Promise<ReturnType<typeof webDesignProjectSummary>[]> {
    await this.initialize();
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    const projects: WebDesignProject[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.web-project.json')) continue;
      try {
        projects.push(await this.readProject(entry.name.slice(0, -'.web-project.json'.length)));
      } catch {
        // Malformed project files do not prevent the remaining project list from loading.
      }
    }
    return projects.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)).map(webDesignProjectSummary);
  }

  async readProject(projectId: string): Promise<WebDesignProject> {
    assertIdentifier(projectId, 'projectId');
    const raw = await fs.readFile(this.projectPath(projectId), 'utf8');
    const value: unknown = JSON.parse(raw);
    assertWebDesignProject(value);
    if (value.projectId !== projectId) throw new Error('Web design project identity does not match its file name.');
    return value;
  }

  async listInProject(projectId: string): Promise<ReturnType<typeof designSummary>[]> {
    const project = await this.readProject(projectId);
    const documents = await Promise.all(project.designIds.map(async (documentId) => {
      try { return await this.read(documentId); }
      catch { return undefined; }
    }));
    return documents
      .filter((document): document is WebDesignDocument => document !== undefined)
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(designSummary);
  }

  async createProject(name: string, description?: string): Promise<WebDesignProject> {
    return this.withStoreLock(async () => {
      const project = this.prepareProject(name, description);
      await this.atomicWriteProject(this.projectPath(project.projectId), project);
      return project;
    });
  }

  async updateProject(projectId: string, updates: { name?: string; description?: string }): Promise<WebDesignProject> {
    return this.withStoreLock(async () => {
      const current = await this.readProject(projectId);
      const name = updates.name === undefined ? current.name : updates.name.trim();
      if (!name || name.length > 240) throw new Error('Project name must contain 1 to 240 characters.');
      const next: WebDesignProject = {
        ...current,
        name,
        description: updates.description === undefined ? current.description : updates.description.trim().slice(0, 4000) || undefined,
        updatedAt: new Date().toISOString()
      };
      await this.atomicWriteProject(this.projectPath(projectId), next);
      return next;
    });
  }

  async deleteProject(projectId: string, deleteDocuments = false): Promise<void> {
    await this.withStoreLock(async () => {
      const project = await this.readProject(projectId);
      if (deleteDocuments) {
        for (const documentId of project.designIds) {
          await fs.unlink(this.documentPath(documentId)).catch((error: NodeJS.ErrnoException) => {
            if (error.code !== 'ENOENT') throw error;
          });
        }
      }
      await fs.unlink(this.projectPath(projectId));
    });
  }

  async createInProject(projectId: string, title?: string, blank = false): Promise<WebDesignDocument> {
    return this.withStoreLock(async () => {
      const project = await this.readProject(projectId);
      const document = blank ? createBlankWebsite(title?.trim() || undefined) : createLandingPage(title?.trim() || undefined);
      const saved = await this.writeNewUnlocked(document);
      try {
        await this.atomicWriteProject(this.projectPath(projectId), {
          ...project,
          designIds: [...project.designIds, saved.documentId],
          updatedAt: new Date().toISOString()
        });
        return saved;
      } catch (error) {
        await fs.unlink(this.documentPath(saved.documentId)).catch(() => undefined);
        throw error;
      }
    });
  }

  async moveDocument(documentId: string, targetProjectId: string, sourceProjectId?: string): Promise<{ sourceProject?: WebDesignProject; targetProject: WebDesignProject }> {
    return this.withStoreLock(async () => {
      await this.read(documentId);
      const target = await this.readProject(targetProjectId);
      if (sourceProjectId === targetProjectId) return { targetProject: target };
      const source = sourceProjectId ? await this.readProject(sourceProjectId) : undefined;
      const now = new Date().toISOString();
      const nextTarget = { ...target, designIds: target.designIds.includes(documentId) ? target.designIds : [...target.designIds, documentId], updatedAt: now };
      const nextSource = source ? { ...source, designIds: source.designIds.filter((id) => id !== documentId), updatedAt: now } : undefined;
      if (nextSource) await this.atomicWriteProject(this.projectPath(nextSource.projectId), nextSource);
      await this.atomicWriteProject(this.projectPath(nextTarget.projectId), nextTarget);
      return { sourceProject: nextSource, targetProject: nextTarget };
    });
  }

  async ensureLegacyProject(name = '现有网站设计'): Promise<WebDesignProject | undefined> {
    return this.withStoreLock(async () => {
      const [projects, documents] = await Promise.all([this.listProjects(), this.list()]);
      const assigned = new Set(projects.flatMap((project) => project.designIds));
      const unassigned = documents.map((document) => document.documentId).filter((documentId) => !assigned.has(documentId));
      if (unassigned.length === 0) return undefined;
      const project = this.prepareProject(name, '自动归档升级前已经存在的网站设计。');
      project.designIds = unassigned;
      await this.atomicWriteProject(this.projectPath(project.projectId), project);
      return project;
    });
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
    return this.writeNewUnlocked(document);
  }

  private async writeNewUnlocked(document: WebDesignDocument): Promise<WebDesignDocument> {
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

  async insertSection(
    documentId: string,
    expectedRevision: number,
    pageId: string,
    presetId: WebDesignBlockPresetId
  ): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      if (!(current.pages ?? [{ id: 'home' }]).some((page) => page.id === pageId)) throw new Error(`Page not found: ${pageId}`);
      const block = createBlockPreset(current, pageId, presetId);
      let next: WebDesignDocument = { ...current, components: [...current.components, ...block.components] };
      for (const device of ['desktop', 'tablet', 'mobile'] as const) next = growPageToFitContent(next, pageId, device);
      next.revision = expectedRevision + 1;
      next.updatedAt = new Date().toISOString();
      assertWebDesignDocument(next);
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async applyPageTemplate(
    documentId: string,
    expectedRevision: number,
    pageId: string,
    templateId: WebDesignPageTemplateId
  ): Promise<WebDesignDocument> {
    return this.withLock(documentId, async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      if (!(current.pages ?? [{ id: 'home' }]).some((page) => page.id === pageId)) throw new Error(`Page not found: ${pageId}`);
      const removedIds = new Set(current.components.filter((component) => (component.pageId ?? 'home') === pageId).map((component) => component.id));
      const template = createPageTemplate(current, pageId, templateId);
      let next: WebDesignDocument = {
        ...current,
        components: [...current.components.filter((component) => !removedIds.has(component.id)), ...template.components],
        requests: current.requests.filter((request) => !request.componentId || !removedIds.has(request.componentId))
      };
      for (const device of ['desktop', 'tablet', 'mobile'] as const) next = growPageToFitContent(next, pageId, device);
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
    await this.withStoreLock(async () => {
      await fs.unlink(this.documentPath(documentId));
      const projects = await this.listProjects();
      for (const summary of projects) {
        if (!summary.designIds.includes(documentId)) continue;
        const project = await this.readProject(summary.projectId);
        await this.atomicWriteProject(this.projectPath(project.projectId), {
          ...project,
          designIds: project.designIds.filter((id) => id !== documentId),
          updatedAt: new Date().toISOString()
        });
      }
    });
  }

  private documentPath(documentId: string): string {
    assertIdentifier(documentId, 'documentId');
    return path.join(this.rootDirectory, `${documentId}.web-design.json`);
  }

  private projectPath(projectId: string): string {
    assertIdentifier(projectId, 'projectId');
    return path.join(this.rootDirectory, `${projectId}.web-project.json`);
  }

  private prepareProject(name: string, description?: string): WebDesignProject {
    const trimmedName = name.trim();
    if (!trimmedName || trimmedName.length > 240) throw new Error('Project name must contain 1 to 240 characters.');
    const now = new Date().toISOString();
    return {
      schemaVersion: 1,
      projectId: `project-${randomUUID().slice(0, 8)}`,
      name: trimmedName,
      description: description?.trim().slice(0, 4000) || undefined,
      createdAt: now,
      updatedAt: now,
      designIds: []
    };
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

  private async withStoreLock<T>(action: () => Promise<T>): Promise<T> {
    await this.initialize();
    const release = await lockfile.lock(this.rootDirectory, {
      realpath: false,
      retries: { retries: 20, factor: 1.2, minTimeout: 10, maxTimeout: 100 }
    });
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

  private async atomicWriteProject(filePath: string, project: WebDesignProject): Promise<void> {
    assertWebDesignProject(project);
    const payload = `${JSON.stringify(project, null, 2)}\n`;
    const temporary = `${filePath}.${createHash('sha256').update(payload).digest('hex').slice(0, 12)}.tmp`;
    await fs.writeFile(temporary, payload, 'utf8');
    await fs.rename(temporary, filePath);
  }
}
