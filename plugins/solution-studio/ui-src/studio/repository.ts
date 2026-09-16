import type { SolutionWorkspace, SolutionWorkspaceSummary, SourceMode, WorkspaceValidation } from '../../src/schema';

export interface RuntimeContext {
  kind: string;
  isolated: boolean;
  hasProjectContext: boolean;
  sourceModeHint: SourceMode;
  scopeId?: string;
  projectId?: string;
  projectName?: string;
  connectorWorkspaceId?: string;
  projectRoot?: string;
}

export interface SolutionRepository {
  mode: 'server' | 'local';
  context(): Promise<RuntimeContext>;
  list(): Promise<SolutionWorkspaceSummary[]>;
  create(title: string, sourceMode: SourceMode): Promise<SolutionWorkspace>;
  read(workspaceId: string): Promise<SolutionWorkspace>;
  save(workspace: SolutionWorkspace, expectedRevision: number): Promise<SolutionWorkspace>;
  validate(workspaceId: string): Promise<WorkspaceValidation>;
  markdownUrl(workspaceId: string): string;
  remove(workspaceId: string): Promise<void>;
}

const indexKey = 'chatos.solution-studio.index.v1';
const workspacePrefix = 'chatos.solution-studio.workspace.v1.';

function summary(workspace: SolutionWorkspace): SolutionWorkspaceSummary {
  return {
    workspaceId: workspace.workspaceId,
    artifactKey: workspace.artifactKey,
    revision: workspace.revision,
    title: workspace.title,
    sourceMode: workspace.sourceMode,
    requirementCount: workspace.requirements.items.length,
    designSectionCount: workspace.design.sections.length,
    taskCount: workspace.executionPlan.tasks.length,
    completedTaskCount: workspace.executionPlan.tasks.filter((task) => task.status === 'done').length,
    updatedAt: workspace.updatedAt
  };
}

class ServerRepository implements SolutionRepository {
  readonly mode = 'server' as const;
  async context() {
    const response = await fetch('/api/context', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取项目上下文。');
    return response.json() as Promise<RuntimeContext>;
  }
  async list() {
    const response = await fetch('/api/workspaces', { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取方案列表。');
    return (await response.json() as { items: SolutionWorkspaceSummary[] }).items;
  }
  async create(title: string, sourceMode: SourceMode) {
    const response = await fetch('/api/workspaces', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title, sourceMode }) });
    if (!response.ok) throw new Error('无法创建方案工作区。');
    return response.json() as Promise<SolutionWorkspace>;
  }
  async read(workspaceId: string) {
    const response = await fetch(`/api/workspaces/${encodeURIComponent(workspaceId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法打开方案工作区。');
    return response.json() as Promise<SolutionWorkspace>;
  }
  async save(workspace: SolutionWorkspace, expectedRevision: number) {
    const response = await fetch(`/api/workspaces/${encodeURIComponent(workspace.workspaceId)}`, {
      method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ workspace, expectedRevision })
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '保存失败。' })) as { error?: string };
      throw new Error(body.error ?? '保存失败。');
    }
    return response.json() as Promise<SolutionWorkspace>;
  }
  async validate(workspaceId: string) {
    const response = await fetch(`/api/workspaces/${encodeURIComponent(workspaceId)}/validation`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法校验方案。');
    return response.json() as Promise<WorkspaceValidation>;
  }
  markdownUrl(workspaceId: string) { return `/api/workspaces/${encodeURIComponent(workspaceId)}/markdown`; }
  async remove(workspaceId: string) {
    const response = await fetch(`/api/workspaces/${encodeURIComponent(workspaceId)}`, { method: 'DELETE' });
    if (!response.ok) throw new Error('无法删除方案。');
  }
}

class LocalRepository implements SolutionRepository {
  readonly mode = 'local' as const;
  async context(): Promise<RuntimeContext> { return { kind: 'device', isolated: true, hasProjectContext: false, sourceModeHint: 'greenfield' }; }
  async list() {
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    return ids.flatMap((id) => {
      const raw = localStorage.getItem(`${workspacePrefix}${id}`);
      if (!raw) return [];
      try { return [summary(JSON.parse(raw) as SolutionWorkspace)]; } catch { return []; }
    }).sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  }
  async create(title: string, sourceMode: SourceMode): Promise<SolutionWorkspace> {
    const now = new Date().toISOString();
    const workspaceId = `solution-${crypto.randomUUID().slice(0, 8)}`;
    const workspace: SolutionWorkspace = {
      schemaVersion: 1, workspaceId, artifactKey: workspaceId, revision: 1, title: title.trim() || '未命名方案', description: '', sourceMode, createdAt: now, updatedAt: now,
      requirements: { status: 'draft', revision: 0, summary: '', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], items: [], updatedAt: now },
      design: { status: 'draft', revision: 0, basedOnRequirementsRevision: 0, summary: '', sections: [], decisions: [], risks: [], validationStrategy: [], updatedAt: now },
      executionPlan: { status: 'draft', revision: 0, basedOnDesignRevision: 0, objective: '', tasks: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now }
    };
    localStorage.setItem(`${workspacePrefix}${workspaceId}`, JSON.stringify(workspace));
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    localStorage.setItem(indexKey, JSON.stringify([workspaceId, ...ids.filter((id) => id !== workspaceId)]));
    return workspace;
  }
  async read(workspaceId: string) {
    const raw = localStorage.getItem(`${workspacePrefix}${workspaceId}`);
    if (!raw) throw new Error('没有找到这个方案工作区。');
    return JSON.parse(raw) as SolutionWorkspace;
  }
  async save(workspace: SolutionWorkspace, expectedRevision: number) {
    const current = await this.read(workspace.workspaceId);
    if (current.revision !== expectedRevision) throw new Error(`方案已更新到版本 ${current.revision}，请重新打开。`);
    const next = { ...structuredClone(workspace), revision: expectedRevision + 1, updatedAt: new Date().toISOString() };
    localStorage.setItem(`${workspacePrefix}${workspace.workspaceId}`, JSON.stringify(next));
    return next;
  }
  async validate(_workspaceId: string): Promise<WorkspaceValidation> {
    throw new Error('完整校验需要通过 Solution Studio 本地服务运行。');
  }
  markdownUrl(_workspaceId: string) { return '#'; }
  async remove(workspaceId: string) {
    localStorage.removeItem(`${workspacePrefix}${workspaceId}`);
    const ids = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    localStorage.setItem(indexKey, JSON.stringify(ids.filter((id) => id !== workspaceId)));
  }
}

export async function createRepository(): Promise<SolutionRepository> {
  try {
    const response = await fetch('/api/health', { cache: 'no-store' });
    if (response.ok) return new ServerRepository();
  } catch { /* Vite-only mode falls back to browser storage. */ }
  return new LocalRepository();
}
