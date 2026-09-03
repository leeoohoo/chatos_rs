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

  async listProjects(): Promise<ReturnType<typeof diagramProjectSummary>[]> {
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
    return projects
      .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
      .map(diagramProjectSummary);
  }

  async readProject(projectId: string): Promise<DiagramProject> {
    assertIdentifier(projectId, 'projectId');
    const data = await fs.readFile(this.projectPath(projectId), 'utf8');
    const value: unknown = JSON.parse(data);
    assertDiagramProject(value);
    if (value.projectId !== projectId) throw new Error('Project file identity does not match its name.');
    return value;
  }

  async listInProject(projectId: string): Promise<ReturnType<typeof diagramSummary>[]> {
    const project = await this.readProject(projectId);
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

  async createProject(name: string, description?: string): Promise<DiagramProject> {
    const trimmedName = name.trim();
    if (!trimmedName || trimmedName.length > 240) throw new Error('Project name must contain 1 to 240 characters.');
    const now = new Date().toISOString();
    const project: DiagramProject = {
      schemaVersion: 1,
      projectId: `project-${randomUUID().slice(0, 8)}`,
      name: trimmedName,
      description: description?.trim().slice(0, 4000) || undefined,
      createdAt: now,
      updatedAt: now,
      diagramIds: []
    };
    await this.atomicWriteProject(this.projectPath(project.projectId), project);
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
    sourceProjectId?: string
  ): Promise<{ sourceProject?: DiagramProject; targetProject: DiagramProject }> {
    return this.withLock(async () => {
      await this.read(documentId);
      const target = await this.readProject(targetProjectId);
      if (sourceProjectId === targetProjectId) {
        return { targetProject: target };
      }
      const source = sourceProjectId ? await this.readProject(sourceProjectId) : undefined;
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
    operations: DiagramPatchOperation[]
  ): Promise<DiagramDocument> {
    return this.withLock(async () => {
      const current = await this.read(documentId);
      if (current.revision !== expectedRevision) throw new RevisionConflictError(current.revision);
      const next = applyDiagramPatch(current, operations);
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

function escapeXml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&apos;');
}

function absolutePosition(document: DiagramDocument, nodeId: string): { x: number; y: number } {
  const node = document.nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return { x: 0, y: 0 };
  if (!node.parentId) return node.position;
  const parent = absolutePosition(document, node.parentId);
  return { x: parent.x + node.position.x, y: parent.y + node.position.y };
}

export function renderDiagramSvg(document: DiagramDocument): string {
  const positions = document.nodes.map((node) => {
    const position = absolutePosition(document, node.id);
    const iconOnly = Boolean(node.data.icon && node.data.showLabel === false);
    const unlabeled = node.data.showLabel === false;
    const width = node.width ?? (node.data.shape === 'lifeline' ? 160 : node.data.shape === 'activation' ? 14 : node.data.shape === 'fragment' ? 620 : node.data.shape === 'lane' ? 900 : node.data.shape === 'container' ? 300 : iconOnly ? 58 : node.data.shape === 'text' ? 120 : unlabeled && node.data.shape === 'circle' ? 72 : unlabeled && node.data.shape === 'diamond' ? 96 : unlabeled && node.data.shape === 'cylinder' ? 120 : unlabeled ? 132 : node.data.shape === 'circle' ? 104 : node.data.shape === 'diamond' ? 138 : node.data.shape === 'cylinder' ? 164 : 168);
    const height = node.height ?? (node.data.shape === 'lifeline' ? 560 : node.data.shape === 'activation' ? 120 : node.data.shape === 'fragment' ? 220 : node.data.shape === 'lane' ? 180 : node.data.shape === 'container' ? 180 : iconOnly ? 58 : node.data.shape === 'text' ? 34 : unlabeled && node.data.shape === 'circle' ? 72 : unlabeled && node.data.shape === 'diamond' ? 72 : unlabeled && node.data.shape === 'cylinder' ? 58 : unlabeled ? 56 : node.data.shape === 'circle' ? 104 : node.data.shape === 'diamond' ? 100 : node.data.shape === 'cylinder' ? 82 : 68);
    return { node, x: position.x, y: position.y, width, height };
  });
  const minX = Math.min(0, ...positions.map((item) => item.x)) - 40;
  const minY = Math.min(0, ...positions.map((item) => item.y)) - 40;
  const maxX = Math.max(800, ...positions.map((item) => item.x + item.width)) + 40;
  const maxY = Math.max(600, ...positions.map((item) => item.y + item.height)) + 40;
  const byId = new Map(positions.map((item) => [item.node.id, item]));
  const edgeMarkup = document.edges.map((edge) => {
    const source = byId.get(edge.source);
    const target = byId.get(edge.target);
    if (!source || !target) return '';
    const start = svgHandlePoint(source, edge.sourceHandle);
    const end = svgHandlePoint(target, edge.targetHandle);
    const { x: x1, y: y1 } = start;
    const { x: x2, y: y2 } = end;
    const lineStyle = edge.data?.lineStyle ?? (edge.data?.dashed ? 'dashed' : 'solid');
    const dash = lineStyle === 'dotted' ? ' stroke-dasharray="2 5" stroke-linecap="round"' : lineStyle === 'dashed' ? ' stroke-dasharray="8 6"' : '';
    const markerId = document.kind === 'sequence'
      ? lineStyle === 'dashed' ? 'sequence-return-arrow' : 'sequence-call-arrow'
      : 'arrow';
    const markerStart = edge.data?.startMarker === 'arrow' ? ` marker-start="url(#${markerId})"` : '';
    const markerEnd = edge.data?.endMarker === 'none' ? '' : ` marker-end="url(#${markerId})"`;
    const color = edge.data?.color ?? '#738099';
    const strokeWidth = edge.data?.strokeWidth ?? 2;
    const label = edge.label || edge.data?.relation;
    const fontSize = edge.data?.fontSize ?? 13;
    const edgeY = document.kind === 'sequence' ? y1 : (y1 + y2) / 2;
    const labelMarkup = label
      ? `<text x="${(x1 + x2) / 2}" y="${edgeY - 7}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="600" fill="#465267" paint-order="stroke" stroke="#F9FBFE" stroke-width="5" stroke-linejoin="round">${escapeXml(label)}</text>`
      : '';
    const path = edge.type === 'straight' || document.kind === 'sequence'
      ? `M ${x1} ${y1} L ${x2} ${document.kind === 'sequence' ? y1 : y2}`
      : `M ${x1} ${y1} C ${(x1 + x2) / 2} ${y1}, ${(x1 + x2) / 2} ${y2}, ${x2} ${y2}`;
    return `<g><path d="${path}" fill="none" stroke="${color}" stroke-width="${strokeWidth}"${dash}${markerStart}${markerEnd}/>${labelMarkup}</g>`;
  }).join('');
  const nodeMarkup = positions.map(({ node, x, y, width, height }) => {
    const fill = node.data.fillColor ?? '#F7F9FC';
    const borderStyle = node.data.borderStyle ?? 'solid';
    const stroke = borderStyle === 'none' ? 'none' : node.data.borderColor ?? node.data.color ?? '#4E7CC7';
    const strokeWidth = node.data.borderWidth ?? 1;
    const textColor = node.data.textColor ?? '#1D2430';
    const fontSize = node.data.fontSize ?? (node.data.shape === 'text' ? 16 : 14);
    const fontWeight = node.data.fontWeight ?? (node.data.shape === 'text' ? 500 : 650);
    const borderDash = borderStyle === 'dotted' ? ' stroke-dasharray="2 4" stroke-linecap="round"' : borderStyle === 'dashed' ? ' stroke-dasharray="8 6"' : '';
    if (node.data.shape === 'lifeline') {
      return `<g><line x1="${x + width / 2}" y1="${y + 60}" x2="${x + width / 2}" y2="${y + height}" stroke="${stroke}" stroke-width="1.5" stroke-dasharray="7 6"/><rect x="${x}" y="${y}" width="${width}" height="60" rx="7" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"/><text x="${x + width / 2}" y="${y + 36}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'activation') {
      return `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="2" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"/>`;
    }
    if (node.data.shape === 'fragment') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" fill="none" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"${borderDash}/><path d="M ${x} ${y + 28} H ${x + 92} L ${x + 105} ${y} H ${x}" fill="${fill}" stroke="${stroke}" stroke-width="1"/><text x="${x + 9}" y="${y + 19}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="13" font-weight="650" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'lane') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="14" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/><text x="${x + 18}" y="${y + 30}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="14" font-weight="600" fill="#445066">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'container') {
      return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="12" fill="${fill}" stroke="${stroke}" stroke-width="${Math.max(1.5, strokeWidth)}"${borderDash}/><rect x="${x + 12}" y="${y + 9}" width="${Math.min(width - 24, Math.max(90, node.data.label.length * 14 + 18))}" height="25" rx="6" fill="#F9FBFE"/><text x="${x + 20}" y="${y + 27}" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="13" font-weight="650" fill="${textColor}">${escapeXml(node.data.label)}</text></g>`;
    }
    if (node.data.shape === 'text') {
      return `<text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.35}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="#1D2430">${escapeXml(node.data.label)}</text>`;
    }
    if (node.data.shape === 'diamond') {
      const points = `${x + width / 2},${y} ${x + width},${y + height / 2} ${x + width / 2},${y + height} ${x},${y + height / 2}`;
      const label = node.data.showLabel === false ? '' : `<text x="${x + width / 2}" y="${y + height / 2 + fontSize * 0.35}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text>`;
      return `<g><polygon points="${points}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/>${label}</g>`;
    }
    const radius = node.data.shape === 'circle' ? Math.min(width, height) / 2 : 14;
    const label = node.data.showLabel === false ? '' : `<text x="${x + width / 2}" y="${y + height / 2 + (node.data.subtitle ? -1 : fontSize * 0.35)}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="${fontSize}" font-weight="${fontWeight}" fill="${textColor}">${escapeXml(node.data.label)}</text>${node.data.subtitle ? `<text x="${x + width / 2}" y="${y + height / 2 + 18}" text-anchor="middle" font-family="-apple-system,BlinkMacSystemFont,sans-serif" font-size="11.5" fill="${textColor}" opacity=".72">${escapeXml(node.data.subtitle)}</text>` : ''}`;
    return `<g><rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"${borderDash}/>${label}</g>`;
  }).join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${minX} ${minY} ${maxX - minX} ${maxY - minY}" width="${maxX - minX}" height="${maxY - minY}"><defs><marker id="arrow" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto-start-reverse"><path d="M0,0 L8,4 L0,8 z" fill="context-stroke"/></marker><marker id="sequence-call-arrow" markerWidth="7" markerHeight="7" refX="6.5" refY="3.5" orient="auto-start-reverse"><path d="M0,0 L7,3.5 L0,7 z" fill="context-stroke"/></marker><marker id="sequence-return-arrow" markerWidth="7" markerHeight="7" refX="6.5" refY="3.5" orient="auto-start-reverse"><path d="M0.5,0.5 L6.5,3.5 L0.5,6.5" fill="none" stroke="context-stroke" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></marker></defs><rect x="${minX}" y="${minY}" width="${maxX - minX}" height="${maxY - minY}" fill="#F9FBFE"/>${edgeMarkup}${nodeMarkup}</svg>`;
}

function svgHandlePoint(
  item: { node: DiagramNode; x: number; y: number; width: number; height: number },
  handleId?: string
): { x: number; y: number } {
  if (item.node.data.shape === 'lifeline') {
    const slot = parseSequenceSlot(handleId);
    return { x: item.x + item.width / 2, y: item.y + item.height * (slot === undefined ? 50 : sequenceSlotPercentage(slot)) / 100 };
  }
  if (item.node.data.shape === 'activation') {
    const handle = parseSequenceActivationHandle(handleId);
    if (handle) {
      return {
        x: handle.side === 'left' ? item.x : item.x + item.width,
        y: item.y + item.height * sequenceActivationSlotPercentage(handle.slot, handle.version) / 100
      };
    }
  }
  switch (handleId) {
    case 'left': return { x: item.x, y: item.y + item.height / 2 };
    case 'right': return { x: item.x + item.width, y: item.y + item.height / 2 };
    case 'top': return { x: item.x + item.width / 2, y: item.y };
    case 'bottom': return { x: item.x + item.width / 2, y: item.y + item.height };
    default: return { x: item.x + item.width / 2, y: item.y + item.height / 2 };
  }
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
