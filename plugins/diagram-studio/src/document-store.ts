import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import lockfile from 'proper-lockfile';
import {
  applyDiagramPatch,
  assertDiagramProject,
  assertDiagramDocument,
  assertIdentifier,
  diagramProjectSummary,
  diagramSummary,
  type DiagramDocument,
  type DiagramKind,
  type DiagramNode,
  type DiagramProject,
  type DiagramPatchOperation
} from './schema.js';
import { createBlankDiagram, createTemplate } from './templates.js';
import { layoutDiagram } from './layout.js';
import { parseSequenceActivationHandle, parseSequenceSlot, sequenceActivationSlotPercentage, sequenceSlotPercentage } from './sequence.js';
import { diagramToPlantUml } from './plantuml.js';
import { renderDiagramSvg } from './diagram-svg.js';

export { renderDiagramSvg } from './diagram-svg.js';

export class RevisionConflictError extends Error {
  constructor(public readonly actualRevision: number) {
    super(`Diagram revision conflict. Current revision is ${actualRevision}.`);
  }
}

export interface DiagramWriteResult {
  document: DiagramDocument;
  created: boolean;
  reused: boolean;
}

export function resolveDataDirectory(): string {
  return path.resolve(
    process.env.DIAGRAM_STUDIO_DATA_DIR
      ?? process.env.CHATOS_PLUGIN_DATA_DIR
      ?? path.join(process.cwd(), '.diagram-studio-data')
  );
}

export class DiagramDocumentStore {
  constructor(public readonly rootDirectory = resolveDataDirectory()) {}

  async initialize(): Promise<void> {
    await fs.mkdir(this.rootDirectory, { recursive: true });
  }

  async list(): Promise<ReturnType<typeof diagramSummary>[]> {
    await this.initialize();
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    const documents: DiagramDocument[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.diagram.json')) continue;
      try {
        documents.push(await this.read(entry.name.slice(0, -'.diagram.json'.length)));
      } catch {
        // Ignore malformed files in list; direct reads still report the problem.
      }
    }
    return documents
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(diagramSummary);
  }

  async listProjects(scopeKey?: string): Promise<ReturnType<typeof diagramProjectSummary>[]> {
    const projects = await this.readAllProjects();
    return projects
      .filter((project) => scopeKey === undefined || project.scopeKey === scopeKey)
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(diagramProjectSummary);
  }

  private async readAllProjects(): Promise<DiagramProject[]> {
    await this.initialize();
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    const projects: DiagramProject[] = [];
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.project.json')) continue;
      try {
        projects.push(await this.readProject(entry.name.slice(0, -'.project.json'.length)));
      } catch {
        // Ignore malformed project files in list; direct reads still report the problem.
      }
    }
    return projects;
  }

  async readProject(projectId: string): Promise<DiagramProject> {
    assertIdentifier(projectId, 'projectId');
    const data = await fs.readFile(this.projectPath(projectId), 'utf8');
    const value: unknown = JSON.parse(data);
    assertDiagramProject(value);
    if (value.projectId !== projectId) throw new Error('Project file identity does not match its name.');
    return value;
  }

  async listInProject(projectId: string, scopeKey?: string): Promise<ReturnType<typeof diagramSummary>[]> {
    const project = scopeKey ? await this.readProjectInScope(projectId, scopeKey) : await this.readProject(projectId);
    const documents = await Promise.all(project.diagramIds.map(async (documentId) => {
      try {
        return await this.read(documentId);
      } catch {
        return undefined;
      }
    }));
    return documents
      .filter((document): document is DiagramDocument => document !== undefined)
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(diagramSummary);
  }

  async createProject(name: string, description?: string, scopeKey?: string): Promise<DiagramProject> {
    return this.withLock(() => this.createProjectUnlocked(name, description, scopeKey));
  }

  private async createProjectUnlocked(name: string, description?: string, scopeKey?: string, isScopeDefault = false): Promise<DiagramProject> {
    const trimmedName = name.trim();
    if (!trimmedName || trimmedName.length > 240) throw new Error('Project name must contain 1 to 240 characters.');
    if (scopeKey !== undefined && !/^[a-f0-9]{64}$/.test(scopeKey)) throw new Error('scopeKey must be a SHA-256 fingerprint.');
    const now = new Date().toISOString();
    const project: DiagramProject = {
      schemaVersion: 1,
      projectId: `project-${randomUUID().slice(0, 8)}`,
      ...(scopeKey ? { scopeKey } : {}),
      ...(isScopeDefault ? { isScopeDefault: true } : {}),
      name: trimmedName,
      description: description?.trim().slice(0, 4000) || undefined,
      createdAt: now,
      updatedAt: now,
      diagramIds: []
    };
    await this.atomicWriteProject(this.projectPath(project.projectId), project);
    return project;
  }

  async ensureScopedProject(scopeKey: string, name: string): Promise<DiagramProject> {
    if (!/^[a-f0-9]{64}$/.test(scopeKey)) throw new Error('scopeKey must be a SHA-256 fingerprint.');
    return this.withLock(async () => {
      const projects = await this.readAllProjects();
      if (projects.length === 0) return this.createProjectUnlocked(name, undefined, scopeKey, true);

      const defaultProjects = projects.filter((project) => project.isScopeDefault === true);
      const primaryCandidates = defaultProjects.length > 0 ? defaultProjects : projects;
      const ordered = [...primaryCandidates].sort((left, right) => {
        const contentDifference = right.diagramIds.length - left.diagramIds.length;
        if (contentDifference !== 0) return contentDifference;
        return left.createdAt.localeCompare(right.createdAt);
      });
      const primary = ordered[0];
      const normalizedAt = new Date().toISOString();
      for (const project of projects) {
        const shouldBeDefault = project.projectId === primary.projectId;
        if (project.scopeKey === scopeKey && project.isScopeDefault === shouldBeDefault) continue;
        await this.atomicWriteProject(this.projectPath(project.projectId), {
          ...project,
          scopeKey,
          ...(shouldBeDefault ? { isScopeDefault: true } : { isScopeDefault: undefined }),
          updatedAt: normalizedAt
        });
      }
      return this.readProject(primary.projectId);
    });
  }

  async readProjectInScope(projectId: string, scopeKey: string): Promise<DiagramProject> {
    const project = await this.readProject(projectId);
    if (project.scopeKey !== scopeKey) throw new Error('Diagram Studio project belongs to a different ChatOS user or project scope.');
    return project;
  }

  async listInScope(scopeKey: string): Promise<ReturnType<typeof diagramSummary>[]> {
    const projects = await this.readAllProjects();
    const documentIds = new Set(projects.filter((project) => project.scopeKey === scopeKey).flatMap((project) => project.diagramIds));
    const documents = await Promise.all([...documentIds].map(async (documentId) => {
      try {
        return await this.read(documentId);
      } catch {
        return undefined;
      }
    }));
    return documents
      .filter((document): document is DiagramDocument => document !== undefined)
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(diagramSummary);
  }

  async readInScope(documentId: string, scopeKey: string): Promise<DiagramDocument> {
    const projects = await this.readAllProjects();
    if (!projects.some((project) => project.scopeKey === scopeKey && project.diagramIds.includes(documentId))) {
      throw new Error('Diagram document belongs to a different ChatOS user or project scope.');
    }
    return this.read(documentId);
  }

  async findProjectForDocumentInScope(documentId: string, scopeKey: string): Promise<DiagramProject> {
    const projects = await this.readAllProjects();
    const project = projects.find((candidate) => candidate.scopeKey === scopeKey && candidate.diagramIds.includes(documentId));
    if (!project) throw new Error('Diagram document belongs to a different ChatOS user or project scope.');
    return project;
  }

  async updateProject(
    projectId: string,
    updates: { name?: string; description?: string }
  ): Promise<DiagramProject> {
    return this.withLock(async () => {
      const current = await this.readProject(projectId);
      const name = updates.name === undefined ? current.name : updates.name.trim();
      if (!name || name.length > 240) throw new Error('Project name must contain 1 to 240 characters.');
      const description = updates.description === undefined
        ? current.description
        : updates.description.trim().slice(0, 4000) || undefined;
      const next: DiagramProject = {
        ...current,
        name,
        description,
        updatedAt: new Date().toISOString()
      };
      await this.atomicWriteProject(this.projectPath(projectId), next);
      return next;
    });
  }

  async deleteProject(projectId: string, deleteDocuments = false): Promise<void> {
    await this.withLock(async () => {
      const project = await this.readProject(projectId);
      if (deleteDocuments) {
        for (const documentId of project.diagramIds) {
          await fs.unlink(this.documentPath(documentId)).catch((error: NodeJS.ErrnoException) => {
            if (error.code !== 'ENOENT') throw error;
          });
        }
      }
      await fs.unlink(this.projectPath(projectId));
    });
  }

  async createInProject(projectId: string, kind: DiagramKind, title?: string, blank = false): Promise<DiagramDocument> {
    const project = await this.readProject(projectId);
    const document = await this.create(kind, title, blank);
    const nextProject: DiagramProject = {
      ...project,
      diagramIds: [...project.diagramIds, document.documentId],
      updatedAt: new Date().toISOString()
    };
    await this.atomicWriteProject(this.projectPath(projectId), nextProject);
    return document;
  }

  async createOrGetInProject(
    projectId: string,
    kind: DiagramKind,
    title: string | undefined,
    blank: boolean,
    artifactKey: string,
    idempotencyKey?: string
  ): Promise<DiagramWriteResult> {
    assertIdentifier(artifactKey, 'artifactKey');
    if (idempotencyKey) assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = idempotencyKey ? await this.readReceipt(projectId, idempotencyKey) : undefined;
      if (retried) return { document: retried, created: false, reused: true };
      const project = await this.readProject(projectId);
      const existing = await this.findByArtifactKey(project.diagramIds, artifactKey);
      if (existing) {
        if (existing.kind !== kind) {
          throw new Error(`Diagram artifactKey ${artifactKey} already belongs to a ${existing.kind} diagram.`);
        }
        if (idempotencyKey) await this.writeReceipt(projectId, idempotencyKey, existing.documentId);
        return { document: existing, created: false, reused: true };
      }
      const document = this.prepareDocument(kind, title, blank, artifactKey);
      const saved = await this.writeNewUnlocked(document);
      await this.atomicWriteProject(this.projectPath(projectId), {
        ...project,
        diagramIds: [...project.diagramIds, saved.documentId],
        updatedAt: new Date().toISOString()
      });
      if (idempotencyKey) await this.writeReceipt(projectId, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async createOrGet(
    kind: DiagramKind,
    title: string | undefined,
    blank: boolean,
    artifactKey: string,
    idempotencyKey?: string
  ): Promise<DiagramWriteResult> {
    assertIdentifier(artifactKey, 'artifactKey');
    if (idempotencyKey) assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = idempotencyKey ? await this.readReceipt(undefined, idempotencyKey) : undefined;
      if (retried) return { document: retried, created: false, reused: true };
      const existing = await this.findByArtifactKey(undefined, artifactKey);
      if (existing) {
        if (existing.kind !== kind) {
          throw new Error(`Diagram artifactKey ${artifactKey} already belongs to a ${existing.kind} diagram.`);
        }
        if (idempotencyKey) await this.writeReceipt(undefined, idempotencyKey, existing.documentId);
        return { document: existing, created: false, reused: true };
      }
      const saved = await this.writeNewUnlocked(this.prepareDocument(kind, title, blank, artifactKey));
      if (idempotencyKey) await this.writeReceipt(undefined, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async writeNewInProject(projectId: string, document: DiagramDocument): Promise<DiagramDocument> {
    const project = await this.readProject(projectId);
    const saved = await this.writeNew(document);
    try {
      const nextProject: DiagramProject = {
        ...project,
        diagramIds: [...project.diagramIds, saved.documentId],
        updatedAt: new Date().toISOString()
      };
      await this.atomicWriteProject(this.projectPath(projectId), nextProject);
      return saved;
    } catch (error) {
      await fs.unlink(this.documentPath(saved.documentId)).catch(() => undefined);
      throw error;
    }
  }

  async upsertInProject(projectId: string, document: DiagramDocument, artifactKey: string, idempotencyKey?: string): Promise<DiagramWriteResult> {
    assertIdentifier(artifactKey, 'artifactKey');
    if (idempotencyKey) assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = idempotencyKey ? await this.readReceipt(projectId, idempotencyKey) : undefined;
      if (retried) return { document: retried, created: false, reused: true };
      const project = await this.readProject(projectId);
      const existing = await this.findByArtifactKey(project.diagramIds, artifactKey);
      if (!existing) {
        const saved = await this.writeNewUnlocked({ ...structuredClone(document), artifactKey });
        await this.atomicWriteProject(this.projectPath(projectId), {
          ...project,
          diagramIds: [...project.diagramIds, saved.documentId],
          updatedAt: new Date().toISOString()
        });
        if (idempotencyKey) await this.writeReceipt(projectId, idempotencyKey, saved.documentId);
        return { document: saved, created: true, reused: false };
      }
      const result = await this.replaceArtifactUnlocked(existing, document, artifactKey);
      if (idempotencyKey) await this.writeReceipt(projectId, idempotencyKey, result.document.documentId);
      return result;
    });
  }

  async upsert(document: DiagramDocument, artifactKey: string, idempotencyKey?: string): Promise<DiagramWriteResult> {
    assertIdentifier(artifactKey, 'artifactKey');
    if (idempotencyKey) assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = idempotencyKey ? await this.readReceipt(undefined, idempotencyKey) : undefined;
      if (retried) return { document: retried, created: false, reused: true };
      const existing = await this.findByArtifactKey(undefined, artifactKey);
      if (!existing) {
        const saved = await this.writeNewUnlocked({ ...structuredClone(document), artifactKey });
        if (idempotencyKey) await this.writeReceipt(undefined, idempotencyKey, saved.documentId);
        return { document: saved, created: true, reused: false };
      }
      const result = await this.replaceArtifactUnlocked(existing, document, artifactKey);
      if (idempotencyKey) await this.writeReceipt(undefined, idempotencyKey, result.document.documentId);
      return result;
    });
  }

  async createNewInProjectIdempotent(
    projectId: string,
    kind: DiagramKind,
    title: string | undefined,
    blank: boolean,
    idempotencyKey: string
  ): Promise<DiagramWriteResult> {
    assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = await this.readReceipt(projectId, idempotencyKey);
      if (retried) return { document: retried, created: false, reused: true };
      const project = await this.readProject(projectId);
      const saved = await this.writeNewUnlocked(this.prepareDocument(kind, title, blank));
      await this.atomicWriteProject(this.projectPath(projectId), {
        ...project,
        diagramIds: [...project.diagramIds, saved.documentId],
        updatedAt: new Date().toISOString()
      });
      await this.writeReceipt(projectId, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async createNewIdempotent(kind: DiagramKind, title: string | undefined, blank: boolean, idempotencyKey: string): Promise<DiagramWriteResult> {
    assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = await this.readReceipt(undefined, idempotencyKey);
      if (retried) return { document: retried, created: false, reused: true };
      const saved = await this.writeNewUnlocked(this.prepareDocument(kind, title, blank));
      await this.writeReceipt(undefined, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async writeNewInProjectIdempotent(projectId: string, document: DiagramDocument, idempotencyKey: string): Promise<DiagramWriteResult> {
    assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = await this.readReceipt(projectId, idempotencyKey);
      if (retried) return { document: retried, created: false, reused: true };
      const project = await this.readProject(projectId);
      const saved = await this.writeNewUnlocked(document);
      await this.atomicWriteProject(this.projectPath(projectId), {
        ...project,
        diagramIds: [...project.diagramIds, saved.documentId],
        updatedAt: new Date().toISOString()
      });
      await this.writeReceipt(projectId, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async writeNewIdempotent(document: DiagramDocument, idempotencyKey: string): Promise<DiagramWriteResult> {
    assertIdentifier(idempotencyKey, 'idempotencyKey');
    return this.withLock(async () => {
      const retried = await this.readReceipt(undefined, idempotencyKey);
      if (retried) return { document: retried, created: false, reused: true };
      const saved = await this.writeNewUnlocked(document);
      await this.writeReceipt(undefined, idempotencyKey, saved.documentId);
      return { document: saved, created: true, reused: false };
    });
  }

  async moveDocument(
    documentId: string,
    targetProjectId: string,
    sourceProjectId?: string,
    scopeKey?: string
  ): Promise<{ sourceProject?: DiagramProject; targetProject: DiagramProject }> {
    return this.withLock(async () => {
      if (scopeKey) await this.readInScope(documentId, scopeKey);
      else await this.read(documentId);
      const target = scopeKey ? await this.readProjectInScope(targetProjectId, scopeKey) : await this.readProject(targetProjectId);
      if (sourceProjectId === targetProjectId) {
        return { targetProject: target };
      }
      const source = sourceProjectId
        ? scopeKey
          ? await this.readProjectInScope(sourceProjectId, scopeKey)
          : await this.readProject(sourceProjectId)
        : scopeKey
          ? (await this.readAllProjects()).find((project) => project.scopeKey === scopeKey && project.diagramIds.includes(documentId))
          : undefined;
      const now = new Date().toISOString();
      const nextTarget: DiagramProject = {
        ...target,
        diagramIds: target.diagramIds.includes(documentId)
          ? target.diagramIds
          : [...target.diagramIds, documentId],
        updatedAt: now
      };
      const nextSource = source ? {
        ...source,
        diagramIds: source.diagramIds.filter((id) => id !== documentId),
        updatedAt: now
      } : undefined;
      if (nextSource) await this.atomicWriteProject(this.projectPath(nextSource.projectId), nextSource);
      await this.atomicWriteProject(this.projectPath(nextTarget.projectId), nextTarget);
      return { sourceProject: nextSource, targetProject: nextTarget };
    });
  }

  async read(documentId: string): Promise<DiagramDocument> {
    assertIdentifier(documentId, 'documentId');
    const data = await fs.readFile(this.documentPath(documentId), 'utf8');
    const value: unknown = JSON.parse(data);
    assertDiagramDocument(value);
    if (value.documentId !== documentId) throw new Error('Diagram file identity does not match its name.');
    return value;
  }

  async create(kind: DiagramKind, title?: string, blank = false): Promise<DiagramDocument> {
    return this.writeNew(this.prepareDocument(kind, title, blank));
  }

  async writeNew(document: DiagramDocument): Promise<DiagramDocument> {
    await this.initialize();
    return this.writeNewUnlocked(document);
  }

  private prepareDocument(kind: DiagramKind, title?: string, blank = false, artifactKey?: string): DiagramDocument {
    const document = blank
      ? createBlankDiagram(kind, title?.trim() || '未命名图形')
      : createTemplate(kind);
    document.documentId = `${kind}-${randomUUID().slice(0, 8)}`;
    document.artifactKey = artifactKey;
    if (title?.trim()) document.title = title.trim().slice(0, 240);
    return document;
  }

  private async writeNewUnlocked(document: DiagramDocument): Promise<DiagramDocument> {
    assertDiagramDocument(document);
    const now = new Date().toISOString();
    const next = structuredClone(document);
    next.revision = 1;
    next.createdAt = now;
    next.updatedAt = now;
    if (next.notation) next.notation.lastSyncedRevision = next.revision;
    const destination = this.documentPath(next.documentId);
    try {
      await fs.access(destination);
      throw new Error(`Diagram already exists: ${next.documentId}`);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
    await this.atomicWrite(destination, next);
    return next;
  }

  private async replaceArtifactUnlocked(
    current: DiagramDocument,
    incoming: DiagramDocument,
    artifactKey: string
  ): Promise<DiagramWriteResult> {
    if (current.kind !== incoming.kind) {
      throw new Error(`Diagram artifactKey ${artifactKey} already belongs to a ${current.kind} diagram.`);
    }
    const candidate: DiagramDocument = {
      ...structuredClone(incoming),
      documentId: current.documentId,
      artifactKey,
      revision: current.revision,
      createdAt: current.createdAt,
      updatedAt: current.updatedAt
    };
    if (sameArtifactContent(current, candidate)) {
      return { document: current, created: false, reused: true };
    }
    candidate.revision = current.revision + 1;
    candidate.updatedAt = new Date().toISOString();
    if (candidate.notation) candidate.notation.lastSyncedRevision = candidate.revision;
    assertDiagramDocument(candidate);
    await this.atomicWrite(this.documentPath(current.documentId), candidate);
    return { document: candidate, created: false, reused: false };
  }

  private async findByArtifactKey(documentIds: string[] | undefined, artifactKey: string): Promise<DiagramDocument | undefined> {
    const allowed = documentIds ? new Set(documentIds) : undefined;
    const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isFile() || !entry.name.endsWith('.diagram.json')) continue;
      const documentId = entry.name.slice(0, -'.diagram.json'.length);
      if (allowed && !allowed.has(documentId)) continue;
      try {
        const document = await this.read(documentId);
        if (document.artifactKey === artifactKey) return document;
      } catch {
        // Ignore malformed unrelated files while resolving a stable artifact.
      }
    }
    return undefined;
  }

  async replace(document: DiagramDocument, expectedRevision: number): Promise<DiagramDocument> {
    assertDiagramDocument(document);
    return this.withLock(async () => {
      const current = await this.read(document.documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = structuredClone(document);
      next.revision = current.revision + 1;
      next.createdAt = current.createdAt;
      next.updatedAt = new Date().toISOString();
      assertDiagramDocument(next);
      await this.atomicWrite(this.documentPath(next.documentId), next);
      return next;
    });
  }

  async patch(
    documentId: string,
    expectedRevision: number,
    operations: DiagramPatchOperation[],
    generationProvenance?: DiagramDocument['generationProvenance']
  ): Promise<DiagramDocument> {
    return this.withLock(async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = applyDiagramPatch(current, operations);
      if (generationProvenance) next.generationProvenance = structuredClone(generationProvenance);
      next.revision = current.revision + 1;
      next.updatedAt = new Date().toISOString();
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async autoLayout(
    documentId: string,
    expectedRevision: number,
    direction?: 'RIGHT' | 'DOWN'
  ): Promise<DiagramDocument> {
    return this.withLock(async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = await layoutDiagram(current, direction);
      next.revision = current.revision + 1;
      next.updatedAt = new Date().toISOString();
      await this.atomicWrite(this.documentPath(documentId), next);
      return next;
    });
  }

  async remove(documentId: string): Promise<void> {
    assertIdentifier(documentId, 'documentId');
    await this.withLock(async () => {
      await fs.unlink(this.documentPath(documentId));
      const entries = await fs.readdir(this.rootDirectory, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isFile() || !entry.name.endsWith('.project.json')) continue;
        const projectId = entry.name.slice(0, -'.project.json'.length);
        const project = await this.readProject(projectId);
        if (!project.diagramIds.includes(documentId)) continue;
        await this.atomicWriteProject(this.projectPath(projectId), {
          ...project,
          diagramIds: project.diagramIds.filter((id) => id !== documentId),
          updatedAt: new Date().toISOString()
        });
      }
    });
  }

  private receiptPath(projectId: string | undefined, idempotencyKey: string): string {
    const scope = projectId ?? 'global';
    const digest = createHash('sha256').update(`${scope}\u0000${idempotencyKey}`).digest('hex');
    return path.join(this.rootDirectory, `${digest}.idempotency.json`);
  }

  private async readReceipt(projectId: string | undefined, idempotencyKey: string): Promise<DiagramDocument | undefined> {
    try {
      const body = await fs.readFile(this.receiptPath(projectId, idempotencyKey), 'utf8');
      const receipt = JSON.parse(body) as { projectId?: string; idempotencyKey?: string; documentId?: string };
      if (receipt.projectId !== projectId || receipt.idempotencyKey !== idempotencyKey || typeof receipt.documentId !== 'string') return undefined;
      return await this.read(receipt.documentId);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') return undefined;
      return undefined;
    }
  }

  private async writeReceipt(projectId: string | undefined, idempotencyKey: string, documentId: string): Promise<void> {
    const destination = this.receiptPath(projectId, idempotencyKey);
    const body = `${JSON.stringify({ projectId, idempotencyKey, documentId }, null, 2)}\n`;
    const temporary = `${destination}.${process.pid}.${randomUUID()}.tmp`;
    await fs.writeFile(temporary, body, { encoding: 'utf8', mode: 0o600 });
    await fs.rename(temporary, destination);
  }

  private documentPath(documentId: string): string {
    assertIdentifier(documentId, 'documentId');
    return path.join(this.rootDirectory, `${documentId}.diagram.json`);
  }

  private projectPath(projectId: string): string {
    assertIdentifier(projectId, 'projectId');
    return path.join(this.rootDirectory, `${projectId}.project.json`);
  }

  private async withLock<T>(operation: () => Promise<T>): Promise<T> {
    await this.initialize();
    const release = await lockfile.lock(this.rootDirectory, {
      realpath: false,
      retries: { retries: 10, minTimeout: 20, maxTimeout: 200 }
    });
    try {
      return await operation();
    } finally {
      await release();
    }
  }

  private async atomicWrite(destination: string, document: DiagramDocument): Promise<void> {
    const body = `${JSON.stringify(document, null, 2)}\n`;
    if (Buffer.byteLength(body) > 8 * 1024 * 1024) throw new Error('Diagram document exceeds 8 MiB.');
    const temporary = `${destination}.${process.pid}.${randomUUID()}.tmp`;
    await fs.writeFile(temporary, body, { encoding: 'utf8', mode: 0o600 });
    await fs.rename(temporary, destination);
  }

  private async atomicWriteProject(destination: string, project: DiagramProject): Promise<void> {
    assertDiagramProject(project);
    const body = `${JSON.stringify(project, null, 2)}\n`;
    const temporary = `${destination}.${process.pid}.${randomUUID()}.tmp`;
    await fs.writeFile(temporary, body, { encoding: 'utf8', mode: 0o600 });
    await fs.rename(temporary, destination);
  }
}

function sameArtifactContent(left: DiagramDocument, right: DiagramDocument): boolean {
  const notation = (document: DiagramDocument) => document.notation ? {
    format: document.notation.format,
    dialect: document.notation.dialect,
    source: document.notation.source,
    opaqueBlocks: document.notation.opaqueBlocks
  } : undefined;
  const comparable = (document: DiagramDocument) => ({
    kind: document.kind,
    title: document.title,
    description: document.description,
    nodes: document.nodes,
    edges: document.edges,
    viewport: document.viewport,
    notation: notation(document),
    metadata: document.metadata
  });
  return JSON.stringify(comparable(left)) === JSON.stringify(comparable(right));
}

export async function writeExportArtifact(
  document: DiagramDocument,
  format: 'json' | 'svg' | 'plantuml'
): Promise<{ relativePath: string; mimeType: string; size: number; sha256: string }> {
  const artifactDirectory = path.resolve(
    process.env.CHATOS_PLUGIN_ARTIFACT_DIR
      ?? process.env.DIAGRAM_STUDIO_EXPORT_DIR
      ?? path.join(process.cwd(), 'exports')
  );
  await fs.mkdir(artifactDirectory, { recursive: true });
  const safeTitle = document.title.replace(/[^a-zA-Z0-9\u4e00-\u9fff_-]+/g, '-').replace(/^-+|-+$/g, '') || document.documentId;
  const extension = format === 'json' ? 'diagram.json' : format === 'plantuml' ? 'puml' : 'svg';
  const relativePath = `${safeTitle}-${document.documentId}.${extension}`;
  const body = format === 'json'
    ? `${JSON.stringify(document, null, 2)}\n`
    : format === 'plantuml'
      ? diagramToPlantUml(document)
      : renderDiagramSvg(document);
  const bytes = Buffer.from(body, 'utf8');
  await fs.writeFile(path.join(artifactDirectory, relativePath), bytes, { mode: 0o600 });
  return {
    relativePath,
    mimeType: format === 'json' ? 'application/vnd.chatos.diagram+json' : format === 'plantuml' ? 'text/vnd.plantuml' : 'image/svg+xml',
    size: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex')
  };
}
