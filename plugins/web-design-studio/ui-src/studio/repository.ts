import type { WebDesignDocument, WebDesignProject, WebDesignProjectSummary } from '../../src/schema';
import { createBlankWebsite, createLandingPage } from '../../src/templates';

export interface DesignSummary {
  documentId: string;
  revision: number;
  title: string;
  componentCount: number;
  pageCount?: number;
  pendingRequestCount: number;
  updatedAt: string;
}

export interface WebDesignRuntimeContext {
  kind: string;
  shared: boolean;
  chatosProjectId?: string;
  chatosProjectName?: string;
  workspaceId?: string;
  defaultProjectId?: string;
}

export interface DesignRepository {
  mode: 'server' | 'local';
  runtimeContext(): Promise<WebDesignRuntimeContext>;
  list(): Promise<DesignSummary[]>;
  listProjects(): Promise<WebDesignProjectSummary[]>;
  readProject(projectId: string): Promise<WebDesignProject>;
  createProject(name: string, description?: string): Promise<WebDesignProject>;
  updateProject(projectId: string, updates: { name?: string; description?: string }): Promise<WebDesignProject>;
  deleteProject(projectId: string, deleteDocuments?: boolean): Promise<void>;
  read(documentId: string): Promise<WebDesignDocument>;
  create(title?: string): Promise<WebDesignDocument>;
  createInProject(projectId: string, title?: string, blank?: boolean): Promise<WebDesignDocument>;
  save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument>;
  remove(documentId: string): Promise<void>;
}

const indexKey = 'chatos.web-design-studio.index.v1';
const documentPrefix = 'chatos.web-design-studio.document.v1.';
const projectIndexKey = 'chatos.web-design-studio.project-index.v1';
const projectPrefix = 'chatos.web-design-studio.project.v1.';

function projectSummary(project: WebDesignProject): WebDesignProjectSummary {
  return { projectId: project.projectId, name: project.name, description: project.description, designCount: project.designIds.length, designIds: [...project.designIds], createdAt: project.createdAt, updatedAt: project.updatedAt };
}

function summary(document: WebDesignDocument): DesignSummary {
  return {
    documentId: document.documentId,
    revision: document.revision,
    title: document.title,
    componentCount: document.components.length,
    pageCount: document.pages?.length ?? 1,
    pendingRequestCount: document.requests.filter((request) => request.status === 'pending').length,
    updatedAt: document.updatedAt
  };
}

class LocalRepository implements DesignRepository {
  readonly mode = 'local' as const;

  async runtimeContext(): Promise<WebDesignRuntimeContext> {
    return { kind: 'device', shared: true };
  }

  async list(): Promise<DesignSummary[]> {
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${documentPrefix}${id}`);
      if (!raw) return [];
      try { return [summary(JSON.parse(raw) as WebDesignDocument)]; }
      catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }

  async listProjects(): Promise<WebDesignProjectSummary[]> {
    await this.ensureLegacyProject();
    const ids = JSON.parse(localStorage.getItem(projectIndexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${projectPrefix}${id}`);
      if (!raw) return [];
      try { return [projectSummary(JSON.parse(raw) as WebDesignProject)]; }
      catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }

  async readProject(projectId: string): Promise<WebDesignProject> {
    const raw = localStorage.getItem(`${projectPrefix}${projectId}`);
    if (!raw) throw new Error('没有找到这个网站项目。');
    return JSON.parse(raw) as WebDesignProject;
  }

  async createProject(name: string, description?: string): Promise<WebDesignProject> {
    const trimmedName = name.trim();
    if (!trimmedName) throw new Error('请填写项目名称。');
    const now = new Date().toISOString();
    const project: WebDesignProject = { schemaVersion: 1, projectId: `project-${crypto.randomUUID().slice(0, 8)}`, name: trimmedName.slice(0, 240), description: description?.trim().slice(0, 4000) || undefined, createdAt: now, updatedAt: now, designIds: [] };
    await this.persistProject(project);
    return project;
  }

  async updateProject(projectId: string, updates: { name?: string; description?: string }): Promise<WebDesignProject> {
    const project = await this.readProject(projectId);
    const name = updates.name === undefined ? project.name : updates.name.trim();
    if (!name) throw new Error('项目名称不能为空。');
    const next = { ...project, name: name.slice(0, 240), description: updates.description === undefined ? project.description : updates.description.trim().slice(0, 4000) || undefined, updatedAt: new Date().toISOString() };
    await this.persistProject(next);
    return next;
  }

  async deleteProject(projectId: string, deleteDocuments = false): Promise<void> {
    const project = await this.readProject(projectId);
    if (deleteDocuments) {
      for (const documentId of project.designIds) {
        localStorage.removeItem(`${documentPrefix}${documentId}`);
      }
      const documentIds = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
      localStorage.setItem(indexKey, JSON.stringify(documentIds.filter((id) => !project.designIds.includes(id))));
    }
    localStorage.removeItem(`${projectPrefix}${projectId}`);
    const projectIds = JSON.parse(localStorage.getItem(projectIndexKey) ?? '[]') as string[];
    localStorage.setItem(projectIndexKey, JSON.stringify(projectIds.filter((id) => id !== projectId)));
  }

  async read(documentId: string): Promise<WebDesignDocument> {
    const raw = localStorage.getItem(`${documentPrefix}${documentId}`);
    if (!raw) throw new Error('没有找到这个网站设计。');
    return JSON.parse(raw) as WebDesignDocument;
  }

  async create(title?: string): Promise<WebDesignDocument> {
    const document = createLandingPage(title?.trim() || undefined);
    const now = new Date().toISOString();
    document.revision = 1;
    document.createdAt = now;
    document.updatedAt = now;
    await this.persist(document);
    return document;
  }

  async createInProject(projectId: string, title?: string, blank = false): Promise<WebDesignDocument> {
    const project = await this.readProject(projectId);
    const document = blank ? createBlankWebsite(title?.trim() || undefined) : createLandingPage(title?.trim() || undefined);
    const now = new Date().toISOString();
    document.revision = 1;
    document.createdAt = now;
    document.updatedAt = now;
    await this.persist(document);
    await this.persistProject({ ...project, designIds: [...project.designIds, document.documentId], updatedAt: now });
    return document;
  }

  async save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument> {
    const current = await this.read(document.documentId);
    if (current.revision !== expectedRevision) throw new Error(`设计已经更新到版本 ${current.revision}，请刷新后再编辑。`);
    const next = structuredClone(document);
    next.revision = expectedRevision + 1;
    next.updatedAt = new Date().toISOString();
    await this.persist(next);
    return next;
  }

  async remove(documentId: string): Promise<void> {
    localStorage.removeItem(`${documentPrefix}${documentId}`);
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    localStorage.setItem(indexKey, JSON.stringify(ids.filter((id) => id !== documentId)));
    const projectIds = JSON.parse(localStorage.getItem(projectIndexKey) ?? '[]') as string[];
    for (const projectId of projectIds) {
      const raw = localStorage.getItem(`${projectPrefix}${projectId}`);
      if (!raw) continue;
      try {
        const project = JSON.parse(raw) as WebDesignProject;
        if (!project.designIds.includes(documentId)) continue;
        await this.persistProject({ ...project, designIds: project.designIds.filter((id) => id !== documentId), updatedAt: new Date().toISOString() });
      } catch {
        // Ignore malformed unrelated projects while deleting a design.
      }
    }
  }

  private async persist(document: WebDesignDocument): Promise<void> {
    localStorage.setItem(`${documentPrefix}${document.documentId}`, JSON.stringify(document));
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    if (!ids.includes(document.documentId)) {
      ids.unshift(document.documentId);
      localStorage.setItem(indexKey, JSON.stringify(ids));
    }
  }

  private async persistProject(project: WebDesignProject): Promise<void> {
    localStorage.setItem(`${projectPrefix}${project.projectId}`, JSON.stringify(project));
    const ids = JSON.parse(localStorage.getItem(projectIndexKey) ?? '[]') as string[];
    if (!ids.includes(project.projectId)) {
      ids.unshift(project.projectId);
      localStorage.setItem(projectIndexKey, JSON.stringify(ids));
    }
  }

  private async ensureLegacyProject(): Promise<void> {
    const projectIds = JSON.parse(localStorage.getItem(projectIndexKey) ?? '[]') as string[];
    const assigned = new Set<string>();
    for (const projectId of projectIds) {
      const raw = localStorage.getItem(`${projectPrefix}${projectId}`);
      if (!raw) continue;
      try { (JSON.parse(raw) as WebDesignProject).designIds.forEach((id) => assigned.add(id)); }
      catch { /* Ignore malformed projects during migration. */ }
    }
    const documentIds = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    const unassigned = documentIds.filter((id) => !assigned.has(id) && localStorage.getItem(`${documentPrefix}${id}`));
    if (unassigned.length === 0) return;
    const project = await this.createProject('现有网站设计', '自动归档升级前已经存在的网站设计。');
    await this.persistProject({ ...project, designIds: unassigned });
  }
}

class ServerRepository implements DesignRepository {
  readonly mode = 'server' as const;

  async runtimeContext(): Promise<WebDesignRuntimeContext> {
    const response = await fetch('/api/context', { cache: 'no-store' });
    if (!response.ok) return { kind: 'device', shared: true };
    return response.json() as Promise<WebDesignRuntimeContext>;
  }

  async list(): Promise<DesignSummary[]> {
    const response = await fetch('/api/documents', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取网站设计列表。');
    return (await response.json() as { items: DesignSummary[] }).items;
  }

  async listProjects(): Promise<WebDesignProjectSummary[]> {
    const response = await fetch('/api/projects', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取网站项目列表。');
    return (await response.json() as { items: WebDesignProjectSummary[] }).items;
  }

  async readProject(projectId: string): Promise<WebDesignProject> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开这个网站项目。');
    return response.json() as Promise<WebDesignProject>;
  }

  async createProject(name: string, description?: string): Promise<WebDesignProject> {
    const response = await fetch('/api/projects', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, description }) });
    if (!response.ok) throw new Error('无法创建网站项目。');
    return response.json() as Promise<WebDesignProject>;
  }

  async updateProject(projectId: string, updates: { name?: string; description?: string }): Promise<WebDesignProject> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}`, { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(updates) });
    if (!response.ok) throw new Error('无法更新网站项目。');
    return response.json() as Promise<WebDesignProject>;
  }

  async deleteProject(projectId: string, deleteDocuments = false): Promise<void> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}?deleteDocuments=${deleteDocuments ? 'true' : 'false'}`, { method: 'DELETE' });
    if (!response.ok) throw new Error('无法删除网站项目。');
  }

  async read(documentId: string): Promise<WebDesignDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开这个网站设计。');
    return response.json() as Promise<WebDesignDocument>;
  }

  async create(title?: string): Promise<WebDesignDocument> {
    const response = await fetch('/api/documents', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: title?.trim() || undefined })
    });
    if (!response.ok) throw new Error('无法创建网站设计。');
    return response.json() as Promise<WebDesignDocument>;
  }

  async createInProject(projectId: string, title?: string, blank = false): Promise<WebDesignDocument> {
    const response = await fetch(`/api/projects/${encodeURIComponent(projectId)}/documents`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ title: title?.trim() || undefined, blank })
    });
    if (!response.ok) throw new Error('无法在项目中创建网站设计。');
    return response.json() as Promise<WebDesignDocument>;
  }

  async save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument> {
    const response = await fetch(`/api/documents/${encodeURIComponent(document.documentId)}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ document, expectedRevision })
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '保存失败。' })) as { error?: string };
      throw new Error(body.error ?? '保存失败。');
    }
    return response.json() as Promise<WebDesignDocument>;
  }

  async remove(documentId: string): Promise<void> {
    const response = await fetch(`/api/documents/${encodeURIComponent(documentId)}`, { method: 'DELETE' });
    if (!response.ok) throw new Error('删除失败。');
  }
}

export async function createRepository(): Promise<DesignRepository> {
  try {
    const response = await fetch('/api/health', { cache: 'no-store', signal: AbortSignal.timeout(900) });
    if (response.ok) return new ServerRepository();
  } catch {
    // Vite and a static plugin preview can use browser-local storage.
  }
  return new LocalRepository();
}
