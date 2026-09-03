import type { DiagramDocument, DiagramKind, DiagramProject, DiagramProjectSummary } from '../../src/schema';
import { createBlankDiagram, createTemplate } from '../../src/templates';

export interface DiagramSummary {
  documentId: string;
  revision: number;
  kind: DiagramKind;
  title: string;
  nodeCount: number;
  edgeCount: number;
  updatedAt: string;
}

export interface DiagramRuntimeContext {
  kind: string;
  shared: boolean;
  chatosProjectId?: string;
  chatosProjectName?: string;
  workspaceId?: string;
  defaultProjectId?: string;
}

interface Repository {
  mode: 'server' | 'local';
  runtimeContext(): Promise<DiagramRuntimeContext>;
  list(): Promise<DiagramSummary[]>;
  listProjects(): Promise<DiagramProjectSummary[]>;
  readProject(projectId: string): Promise<DiagramProject>;
  createProject(name: string): Promise<DiagramProject>;
  read(documentId: string): Promise<DiagramDocument>;
  create(kind: DiagramKind, title?: string): Promise<DiagramDocument>;
  createInProject(projectId: string, kind: DiagramKind, title?: string): Promise<DiagramDocument>;
  save(document: DiagramDocument, expectedRevision: number): Promise<DiagramDocument>;
  autoLayout(documentId: string, expectedRevision: number): Promise<DiagramDocument>;
  remove(documentId: string): Promise<void>;
}

const localIndexKey = 'chatos.diagram-studio.index.v1';
const localDocumentPrefix = 'chatos.diagram-studio.document.v1.';
const localProjectIndexKey = 'chatos.diagram-studio.project-index.v1';
const localProjectPrefix = 'chatos.diagram-studio.project.v1.';

function projectSummary(project: DiagramProject): DiagramProjectSummary {
  return { projectId: project.projectId, name: project.name, description: project.description, diagramCount: project.diagramIds.length, diagramIds: [...project.diagramIds], createdAt: project.createdAt, updatedAt: project.updatedAt };
}

function summary(document: DiagramDocument): DiagramSummary {
  return {
    documentId: document.documentId,
    revision: document.revision,
    kind: document.kind,
    title: document.title,
    nodeCount: document.nodes.length,
    edgeCount: document.edges.length,
    updatedAt: document.updatedAt
  };
}

class LocalRepository implements Repository {
  readonly mode = 'local' as const;

  async runtimeContext(): Promise<DiagramRuntimeContext> {
    return { kind: 'device', shared: true };
  }

  async list(): Promise<DiagramSummary[]> {
    const ids = JSON.parse(localStorage.getItem(localIndexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${localDocumentPrefix}${id}`);
      if (!raw) return [];
      try { return [summary(JSON.parse(raw) as DiagramDocument)]; }
      catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }

  async listProjects(): Promise<DiagramProjectSummary[]> {
    const ids = JSON.parse(localStorage.getItem(localProjectIndexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${localProjectPrefix}${id}`);
      if (!raw) return [];
      try { return [projectSummary(JSON.parse(raw) as DiagramProject)]; }
      catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }

  async readProject(projectId: string): Promise<DiagramProject> {
    const raw = localStorage.getItem(`${localProjectPrefix}${projectId}`);
    if (!raw) throw new Error('没有找到这个项目。');
    return JSON.parse(raw) as DiagramProject;
  }

  async createProject(name: string): Promise<DiagramProject> {
    const now = new Date().toISOString();
    const project: DiagramProject = { schemaVersion: 1, projectId: `project-${crypto.randomUUID().slice(0, 8)}`, name: name.trim(), createdAt: now, updatedAt: now, diagramIds: [] };
    await this.persistProject(project);
    return project;
  }

  async read(documentId: string): Promise<DiagramDocument> {
    const raw = localStorage.getItem(`${localDocumentPrefix}${documentId}`);
    if (!raw) throw new Error('没有找到这个图。');
    return JSON.parse(raw) as DiagramDocument;
  }

  async create(kind: DiagramKind, title?: string): Promise<DiagramDocument> {
    const document = createTemplate(kind);
    if (title?.trim()) document.title = title.trim().slice(0, 240);
    const now = new Date().toISOString();
    document.revision = 1;
    document.createdAt = now;
    document.updatedAt = now;
    await this.persist(document);
    return document;
  }

  async createInProject(projectId: string, kind: DiagramKind, title?: string): Promise<DiagramDocument> {
    const project = await this.readProject(projectId);
    const document = createBlankDiagram(kind, title?.trim() || '未命名图形');
    const now = new Date().toISOString();
    document.revision = 1;
    document.createdAt = now;
    document.updatedAt = now;
    await this.persist(document);
    await this.persistProject({ ...project, diagramIds: [...project.diagramIds, document.documentId], updatedAt: new Date().toISOString() });
    return document;
  }

  async save(document: DiagramDocument, expectedRevision: number): Promise<DiagramDocument> {
    const existing = await this.read(document.documentId);
    if (existing.revision !== expectedRevision) throw new Error(`图已经更新到版本 ${existing.revision}，请重新打开。`);
    const next = structuredClone(document);
    next.revision = expectedRevision + 1;
    next.updatedAt = new Date().toISOString();
    await this.persist(next);
    return next;
  }

  async autoLayout(): Promise<DiagramDocument> {
    throw new Error('local-layout');
  }

  async remove(documentId: string): Promise<void> {
    localStorage.removeItem(`${localDocumentPrefix}${documentId}`);
    const ids = JSON.parse(localStorage.getItem(localIndexKey) ?? '[]') as string[];
    localStorage.setItem(localIndexKey, JSON.stringify(ids.filter((id) => id !== documentId)));
    const projectIds = JSON.parse(localStorage.getItem(localProjectIndexKey) ?? '[]') as string[];
    for (const projectId of projectIds) {
      const raw = localStorage.getItem(`${localProjectPrefix}${projectId}`);
      if (!raw) continue;
      try {
        const project = JSON.parse(raw) as DiagramProject;
        if (!project.diagramIds.includes(documentId)) continue;
        await this.persistProject({
          ...project,
          diagramIds: project.diagramIds.filter((id) => id !== documentId),
          updatedAt: new Date().toISOString()
        });
      } catch {
        // Ignore malformed unrelated projects while deleting a document.
      }
    }
  }

  private async persist(document: DiagramDocument): Promise<void> {
    localStorage.setItem(`${localDocumentPrefix}${document.documentId}`, JSON.stringify(document));
    const ids = JSON.parse(localStorage.getItem(localIndexKey) ?? '[]') as string[];
    if (!ids.includes(document.documentId)) {
      ids.unshift(document.documentId);
      localStorage.setItem(localIndexKey, JSON.stringify(ids));
    }
  }

  private async persistProject(project: DiagramProject): Promise<void> {
    localStorage.setItem(`${localProjectPrefix}${project.projectId}`, JSON.stringify(project));
    const ids = JSON.parse(localStorage.getItem(localProjectIndexKey) ?? '[]') as string[];
    if (!ids.includes(project.projectId)) {
      ids.unshift(project.projectId);
      localStorage.setItem(localProjectIndexKey, JSON.stringify(ids));
    }
  }
}

class ServerRepository implements Repository {
  readonly mode = 'server' as const;

  async runtimeContext(): Promise<DiagramRuntimeContext> {
    const response = await fetch('/api/context', { cache: 'no-store' });
    if (!response.ok) return { kind: 'device', shared: true };
    return response.json() as Promise<DiagramRuntimeContext>;
  }

  async list(): Promise<DiagramSummary[]> {
    const response = await fetch('/api/documents', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取图形列表。');
    return (await response.json() as { items: DiagramSummary[] }).items;
  }

  async listProjects(): Promise<DiagramProjectSummary[]> {
    const response = await fetch('/api/projects', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取项目列表。');
    return (await response.json() as { items: DiagramProjectSummary[] }).items;
  }

  async readProject(projectId: string): Promise<DiagramProject> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开这个项目。');
    return response.json() as Promise<DiagramProject>;
  }

  async createProject(name: string): Promise<DiagramProject> {
    const response = await fetch('/api/projects', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name }) });
    if (!response.ok) throw new Error('无法创建项目。');
    return response.json() as Promise<DiagramProject>;
  }

  async read(documentId: string): Promise<DiagramDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开这个图。');
    return response.json() as Promise<DiagramDocument>;
  }

  async create(kind: DiagramKind, title?: string): Promise<DiagramDocument> {
    const response = await fetch('/api/documents', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ kind, title: title?.trim() || undefined })
    });
    if (!response.ok) throw new Error('无法创建新图。');
    return response.json() as Promise<DiagramDocument>;
  }

  async createInProject(projectId: string, kind: DiagramKind, title?: string): Promise<DiagramDocument> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}/documents`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ kind, title: title?.trim() || undefined, blank: true })
    });
    if (!response.ok) throw new Error('无法在项目中创建图形。');
    return response.json() as Promise<DiagramDocument>;
  }

  async save(document: DiagramDocument, expectedRevision: number): Promise<DiagramDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(document.documentId)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ document, expectedRevision })
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '保存失败。' })) as { error?: string };
      throw new Error(body.error ?? '保存失败。');
    }
    return response.json() as Promise<DiagramDocument>;
  }

  async autoLayout(documentId: string, expectedRevision: number): Promise<DiagramDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}/layout`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ expectedRevision })
    });
    if (!response.ok) throw new Error('自动布局失败。');
    return response.json() as Promise<DiagramDocument>;
  }

  async remove(documentId: string): Promise<void> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { method: 'DELETE' });
    if (!response.ok) throw new Error('删除失败。');
  }
}

export async function createRepository(): Promise<Repository> {
  try {
    const response = await fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(900) });
    if (response.ok) return new ServerRepository();
  } catch {
    // Vite development and an embedded plugin host can use local browser storage.
  }
  return new LocalRepository();
}
