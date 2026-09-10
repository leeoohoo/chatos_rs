import type { WebDesignDocument, WebDesignProject, WebDesignProjectSummary } from '../../src/schema';
import { createBlankWebsite, createLandingPage } from '../../src/templates';
import type { WorkspaceCamera } from '../../src/v2/workspace-camera';
import type { SceneEditorCommandRequest, SceneEditorCommandResult } from '../../src/v2/scene-editor-command';
import type { SceneDocument } from '../../src/v2/scene-schema';
import type { SceneHistoryStatus } from '../../src/v2/scene-store';
import type { AnnotationAiTask } from '../../src/v2/annotation-ai-protocol';
import type { WorkspaceArtboardPlacement, WorkspacePlacementDocument } from '../../src/v2/workspace-placement-store';

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
  isolated: boolean;
  hasProjectContext: boolean;
  projectName?: string;
  defaultProjectId?: string;
}

export interface SceneAnnotationAiContext {
  task: AnnotationAiTask;
  capture: {
    artifact: { artifactId: string; revision: number; viewportWidth?: number };
    pageId: string;
    rootNodeId: string;
    width: number;
    height: number;
    groundingCount: number;
  };
  visual: Record<string, unknown>;
  imageDataUrl?: string;
  nextAction: {
    tool: string;
    documentId: string;
    pageId: string;
    targetNodeId: string;
    sceneRevision: number;
  };
}

export interface GenerationPlanSummary {
  planId: string;
  revision: number;
  status: string;
  mode: string;
  objective: string;
  audience: string[];
  pages: Array<{
    pageId: string;
    name: string;
    purpose: string;
    order: number;
    status: string;
    stepCounts: Record<string, number>;
    design?: {
      artDirection: string;
      compositionIntent: string;
      typographyIntent: string;
      imageStrategy: string;
      contentHierarchy: string[];
      designAcceptanceCriteria: string[];
      interactionIntents: string[];
    };
  }>;
  activePage?: { pageId: string; name: string; status: string };
  activeStep?: {
    stepId: string;
    title: string;
    kind: string;
    status: string;
    target: { sectionKey?: string; nodeIds: string[]; viewportWidths: number[] };
    activeAttemptId?: string;
  };
  nextAction: Record<string, unknown>;
}

export interface GenerationReviewArtifact {
  artifactId: string;
  kind: string;
  revision: number;
  viewportWidth?: number;
  uri?: string;
  createdAt: string;
}

export interface GenerationStepReview {
  plan: GenerationPlanSummary;
  page: { pageId: string; name: string };
  step: { stepId: string; title: string; kind: string; status: string; target: { sectionKey?: string; nodeIds: string[]; viewportWidths: number[] } };
  attempt: {
    attemptId: string;
    status: string;
    baseRevision: number;
    committedRevision?: number;
    artifacts: GenerationReviewArtifact[];
    error?: { code: string; message: string; retryable: boolean; issueIds: string[] };
    createdAt: string;
    updatedAt: string;
  };
  candidate?: {
    candidateId: string;
    pageId: string;
    stepId: string;
    attemptId: string;
    baseRevision: number;
    qualitySummary: string;
    issueIds: string[];
    protectionConflicts: Array<{ nodeId: string; protectedPath: string[]; requestedPath: string[]; reason: string }>;
    artifacts: GenerationReviewArtifact[];
  };
  nextAction: Record<string, unknown>;
}

export interface DesignRepository {
  mode: 'server' | 'local';
  runtimeContext(): Promise<WebDesignRuntimeContext>;
  list(): Promise<DesignSummary[]>;
  listProjects(): Promise<WebDesignProjectSummary[]>;
  readProject(projectId: string): Promise<WebDesignProject>;
  read(documentId: string): Promise<WebDesignDocument>;
  create(title?: string): Promise<WebDesignDocument>;
  createInProject(projectId: string, title?: string, blank?: boolean): Promise<WebDesignDocument>;
  save(document: WebDesignDocument, expectedRevision: number): Promise<WebDesignDocument>;
  readScene(documentId: string): Promise<SceneDocument>;
  editScene(documentId: string, request: SceneEditorCommandRequest): Promise<SceneEditorCommandResult>;
  prepareSceneAnnotationTask(documentId: string, input: { nodeId: string; annotationId: string; viewportWidth: number }): Promise<SceneAnnotationAiContext>;
  readSceneHistory(documentId: string): Promise<SceneHistoryStatus>;
  undoScene(documentId: string, expectedRevision: number): Promise<SceneDocument>;
  redoScene(documentId: string, expectedRevision: number): Promise<SceneDocument>;
  readGenerationPlan(documentId: string): Promise<GenerationPlanSummary | undefined>;
  inspectGenerationStep(documentId: string, stepId: string, attemptId?: string): Promise<GenerationStepReview>;
  acceptGenerationStep(documentId: string, planRevision: number, stepId: string, attemptId: string, approveConflicts?: boolean): Promise<{ plan: GenerationPlanSummary; status: string }>;
  rejectGenerationStep(documentId: string, planRevision: number, stepId: string, attemptId: string, reason: string): Promise<{ plan: GenerationPlanSummary; status: string }>;
  rollbackGenerationStep(documentId: string, planRevision: number, stepId: string): Promise<{ plan: GenerationPlanSummary; status: string }>;
  pauseGeneration(documentId: string, planRevision: number): Promise<{ plan: GenerationPlanSummary }>;
  resumeGeneration(documentId: string, planRevision: number): Promise<{ plan: GenerationPlanSummary }>;
  generationArtifactImageUrl(documentId: string, artifactId: string): string;
  readWorkspace(documentId: string): Promise<WorkspacePlacementDocument>;
  saveWorkspaceCamera(documentId: string, camera: WorkspaceCamera): Promise<WorkspacePlacementDocument>;
  saveWorkspaceArtboards(documentId: string, artboards: readonly WorkspaceArtboardPlacement[]): Promise<WorkspacePlacementDocument>;
  remove(documentId: string): Promise<void>;
}

const indexKey = 'chatos.web-design-studio.index.v1';
const documentPrefix = 'chatos.web-design-studio.document.v1.';
const projectIndexKey = 'chatos.web-design-studio.project-index.v1';
const projectPrefix = 'chatos.web-design-studio.project.v1.';
const workspacePrefix = 'chatos.web-design-studio.workspace.v4.';

function createLocalWorkspace(documentId: string, camera: WorkspaceCamera = { x: 0, y: 0, zoom: 1 }): WorkspacePlacementDocument {
  const now = new Date().toISOString();
  return { schemaVersion: 2, documentId, revision: 1, camera, artboards: [], createdAt: now, updatedAt: now };
}

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
    return { kind: 'device', isolated: true, hasProjectContext: false };
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

  private async createDefaultProject(name: string, description?: string): Promise<WebDesignProject> {
    const trimmedName = name.trim();
    if (!trimmedName) throw new Error('请填写项目名称。');
    const now = new Date().toISOString();
    const project: WebDesignProject = { schemaVersion: 1, projectId: `project-${crypto.randomUUID().slice(0, 8)}`, name: trimmedName.slice(0, 240), description: description?.trim().slice(0, 4000) || undefined, createdAt: now, updatedAt: now, designIds: [] };
    await this.persistProject(project);
    return project;
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

  async readScene(_documentId: string): Promise<SceneDocument> {
    throw new Error('Scene v2 编辑需要通过 Web Design Studio 服务运行。');
  }

  async editScene(_documentId: string, _request: SceneEditorCommandRequest): Promise<SceneEditorCommandResult> {
    throw new Error('Scene v2 编辑需要通过 Web Design Studio 服务运行。');
  }

  async prepareSceneAnnotationTask(_documentId: string, _input: { nodeId: string; annotationId: string; viewportWidth: number }): Promise<SceneAnnotationAiContext> {
    throw new Error('Scene v2 视觉批注需要通过 Web Design Studio 服务运行。');
  }

  async readSceneHistory(_documentId: string): Promise<SceneHistoryStatus> {
    throw new Error('Scene v2 历史需要通过 Web Design Studio 服务运行。');
  }

  async undoScene(_documentId: string, _expectedRevision: number): Promise<SceneDocument> {
    throw new Error('Scene v2 撤销需要通过 Web Design Studio 服务运行。');
  }

  async redoScene(_documentId: string, _expectedRevision: number): Promise<SceneDocument> {
    throw new Error('Scene v2 重做需要通过 Web Design Studio 服务运行。');
  }

  async readGenerationPlan(_documentId: string): Promise<GenerationPlanSummary | undefined> { return undefined; }
  async inspectGenerationStep(_documentId: string, _stepId: string, _attemptId?: string): Promise<GenerationStepReview> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  async acceptGenerationStep(_documentId: string, _planRevision: number, _stepId: string, _attemptId: string, _approveConflicts?: boolean): Promise<{ plan: GenerationPlanSummary; status: string }> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  async rejectGenerationStep(_documentId: string, _planRevision: number, _stepId: string, _attemptId: string, _reason: string): Promise<{ plan: GenerationPlanSummary; status: string }> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  async rollbackGenerationStep(_documentId: string, _planRevision: number, _stepId: string): Promise<{ plan: GenerationPlanSummary; status: string }> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  async pauseGeneration(_documentId: string, _planRevision: number): Promise<{ plan: GenerationPlanSummary }> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  async resumeGeneration(_documentId: string, _planRevision: number): Promise<{ plan: GenerationPlanSummary }> { throw new Error('AI 分步审阅需要通过 Web Design Studio 服务运行。'); }
  generationArtifactImageUrl(_documentId: string, _artifactId: string): string { return ''; }

  async readWorkspace(documentId: string): Promise<WorkspacePlacementDocument> {
    const raw = localStorage.getItem(`${workspacePrefix}${documentId}`);
    if (raw) return JSON.parse(raw) as WorkspacePlacementDocument;
    const created = createLocalWorkspace(documentId);
    localStorage.setItem(`${workspacePrefix}${documentId}`, JSON.stringify(created));
    return structuredClone(created);
  }

  async saveWorkspaceCamera(documentId: string, camera: WorkspaceCamera): Promise<WorkspacePlacementDocument> {
    const current = await this.readWorkspace(documentId);
    const next = { ...current, revision: current.revision + 1, camera: structuredClone(camera), updatedAt: new Date().toISOString() };
    localStorage.setItem(`${workspacePrefix}${documentId}`, JSON.stringify(next));
    return structuredClone(next);
  }

  async saveWorkspaceArtboards(documentId: string, artboards: readonly WorkspaceArtboardPlacement[]): Promise<WorkspacePlacementDocument> {
    const current = await this.readWorkspace(documentId);
    const next: WorkspacePlacementDocument = { ...current, revision: current.revision + 1, artboards: artboards.map((artboard) => structuredClone(artboard)), updatedAt: new Date().toISOString() };
    localStorage.setItem(`${workspacePrefix}${documentId}`, JSON.stringify(next));
    return structuredClone(next);
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
    const projects: WebDesignProject[] = [];
    const assigned = new Set<string>();
    for (const projectId of projectIds) {
      const raw = localStorage.getItem(`${projectPrefix}${projectId}`);
      if (!raw) continue;
      try {
        const project = JSON.parse(raw) as WebDesignProject;
        projects.push(project);
        project.designIds.forEach((id) => assigned.add(id));
      }
      catch { /* Ignore malformed projects during migration. */ }
    }
    const documentIds = JSON.parse(localStorage.getItem(indexKey) ?? '[]') as string[];
    const unassigned = documentIds.filter((id) => !assigned.has(id) && localStorage.getItem(`${documentPrefix}${id}`));
    const primary = projects[0] ?? await this.createDefaultProject('公共网站设计');
    const designIds = [...new Set([...projects.flatMap((project) => project.designIds), ...unassigned])];
    await this.persistProject({ ...primary, designIds, updatedAt: new Date().toISOString() });
    for (const duplicate of projects.slice(1)) localStorage.removeItem(`${projectPrefix}${duplicate.projectId}`);
    localStorage.setItem(projectIndexKey, JSON.stringify([primary.projectId]));
  }
}

class ServerRepository implements DesignRepository {
  readonly mode = 'server' as const;

  async runtimeContext(): Promise<WebDesignRuntimeContext> {
    const response = await fetch('/api/context', { cache: 'no-store' });
    if (!response.ok) return { kind: 'device', isolated: true, hasProjectContext: false };
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
    const context = await this.runtimeContext();
    if (context.defaultProjectId !== projectId) throw new Error('当前设计范围已经由 ChatOS 锁定。');
    const response = await fetch('/api/documents', {
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

  async readScene(documentId: string): Promise<SceneDocument> {
    const response = await fetch(`/api/scenes/${encodeURIComponent(documentId)}`, { cache: 'no-store' });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '无法读取 Scene v2 设计。' })) as { error?: string };
      throw new Error(body.error ?? '无法读取 Scene v2 设计。');
    }
    return response.json() as Promise<SceneDocument>;
  }

  async editScene(documentId: string, request: SceneEditorCommandRequest): Promise<SceneEditorCommandResult> {
    const response = await fetch(`/api/scenes/${encodeURIComponent(documentId)}/commands`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request)
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: 'Scene v2 编辑失败。' })) as { error?: string };
      throw new Error(body.error ?? 'Scene v2 编辑失败。');
    }
    return response.json() as Promise<SceneEditorCommandResult>;
  }

  async prepareSceneAnnotationTask(documentId: string, input: { nodeId: string; annotationId: string; viewportWidth: number }): Promise<SceneAnnotationAiContext> {
    const response = await fetch(`/api/scenes/${encodeURIComponent(documentId)}/annotation-ai-context`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(input)
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: '无法准备 Scene 视觉批注。' })) as { error?: string };
      throw new Error(body.error ?? '无法准备 Scene 视觉批注。');
    }
    const payload = await response.json() as {
      task: AnnotationAiTask;
      visualContext: Record<string, unknown> & { capture?: SceneAnnotationAiContext['capture'] };
      __images?: Array<{ data: string; mimeType: string }>;
      nextAction: { tool: string; documentId: string; pageId: string; targetNodeId: string; baseRevision?: number; sceneRevision?: number };
    };
    if (!payload.visualContext.capture) throw new Error('Scene 视觉批注缺少截图信息。');
    const image = payload.__images?.[0];
    return {
      task: payload.task,
      capture: payload.visualContext.capture,
      visual: payload.visualContext,
      ...(image ? { imageDataUrl: `data:${image.mimeType};base64,${image.data}` } : {}),
      nextAction: {
        tool: payload.nextAction.tool,
        documentId: payload.nextAction.documentId,
        pageId: payload.nextAction.pageId,
        targetNodeId: payload.nextAction.targetNodeId,
        sceneRevision: payload.task.baseRevision
      }
    };
  }

  async readSceneHistory(documentId: string): Promise<SceneHistoryStatus> {
    const response = await fetch(`/api/scenes/${encodeURIComponent(documentId)}/history`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取 Scene v2 编辑历史。');
    return response.json() as Promise<SceneHistoryStatus>;
  }

  async undoScene(documentId: string, expectedRevision: number): Promise<SceneDocument> {
    return this.sceneHistoryMutation(documentId, 'undo', expectedRevision);
  }

  async redoScene(documentId: string, expectedRevision: number): Promise<SceneDocument> {
    return this.sceneHistoryMutation(documentId, 'redo', expectedRevision);
  }

  async readGenerationPlan(documentId: string): Promise<GenerationPlanSummary | undefined> {
    const response = await fetch(`/api/generation/${encodeURIComponent(documentId)}/plan`, { cache: 'no-store' });
    if (response.status === 404) return undefined;
    if (!response.ok) throw new Error('无法读取 AI 设计进度。');
    return (await response.json() as { plan: GenerationPlanSummary }).plan;
  }

  async inspectGenerationStep(documentId: string, stepId: string, attemptId?: string): Promise<GenerationStepReview> {
    const query = attemptId ? `?attemptId=${encodeURIComponent(attemptId)}` : '';
    const response = await fetch(`/api/generation/${encodeURIComponent(documentId)}/steps/${encodeURIComponent(stepId)}${query}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取当前 AI 设计候选。');
    return response.json() as Promise<GenerationStepReview>;
  }

  private async generationMutation<T>(url: string, body: Record<string, unknown>): Promise<T> {
    const response = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    if (!response.ok) {
      const payload = await response.json().catch(() => ({ error: 'AI 设计操作失败。' })) as { error?: string };
      throw new Error(payload.error ?? 'AI 设计操作失败。');
    }
    return response.json() as Promise<T>;
  }

  acceptGenerationStep(documentId: string, planRevision: number, stepId: string, attemptId: string, approveConflicts = false): Promise<{ plan: GenerationPlanSummary; status: string }> {
    return this.generationMutation(`/api/generation/${encodeURIComponent(documentId)}/steps/${encodeURIComponent(stepId)}/accept`, { expectedPlanRevision: planRevision, attemptId, approveSoftProtectionConflicts: approveConflicts });
  }

  rejectGenerationStep(documentId: string, planRevision: number, stepId: string, attemptId: string, reason: string): Promise<{ plan: GenerationPlanSummary; status: string }> {
    return this.generationMutation(`/api/generation/${encodeURIComponent(documentId)}/steps/${encodeURIComponent(stepId)}/reject`, { expectedPlanRevision: planRevision, attemptId, reason });
  }

  rollbackGenerationStep(documentId: string, planRevision: number, stepId: string): Promise<{ plan: GenerationPlanSummary; status: string }> {
    return this.generationMutation(`/api/generation/${encodeURIComponent(documentId)}/steps/${encodeURIComponent(stepId)}/rollback`, { expectedPlanRevision: planRevision });
  }

  pauseGeneration(documentId: string, planRevision: number): Promise<{ plan: GenerationPlanSummary }> {
    return this.generationMutation(`/api/generation/${encodeURIComponent(documentId)}/pause`, { expectedPlanRevision: planRevision });
  }

  resumeGeneration(documentId: string, planRevision: number): Promise<{ plan: GenerationPlanSummary }> {
    return this.generationMutation(`/api/generation/${encodeURIComponent(documentId)}/resume`, { expectedPlanRevision: planRevision });
  }

  generationArtifactImageUrl(documentId: string, artifactId: string): string {
    return `/api/generation/${encodeURIComponent(documentId)}/artifacts/${encodeURIComponent(artifactId)}/image`;
  }

  private async sceneHistoryMutation(documentId: string, action: 'undo' | 'redo', expectedRevision: number): Promise<SceneDocument> {
    const response = await fetch(`/api/scenes/${encodeURIComponent(documentId)}/${action}`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ expectedRevision })
    });
    if (!response.ok) {
      const body = await response.json().catch(() => ({ error: `${action === 'undo' ? '撤销' : '重做'}失败。` })) as { error?: string };
      throw new Error(body.error ?? `${action === 'undo' ? '撤销' : '重做'}失败。`);
    }
    return response.json() as Promise<SceneDocument>;
  }

  async readWorkspace(documentId: string): Promise<WorkspacePlacementDocument> {
    const response = await fetch(`/api/workspace/${encodeURIComponent(documentId)}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('无法读取画布视图。');
    return response.json() as Promise<WorkspacePlacementDocument>;
  }

  async saveWorkspaceCamera(documentId: string, camera: WorkspaceCamera): Promise<WorkspacePlacementDocument> {
    const response = await fetch(`/api/workspace/${encodeURIComponent(documentId)}/camera`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ camera })
    });
    if (!response.ok) throw new Error('无法保存画布视图。');
    return response.json() as Promise<WorkspacePlacementDocument>;
  }

  async saveWorkspaceArtboards(documentId: string, artboards: readonly WorkspaceArtboardPlacement[]): Promise<WorkspacePlacementDocument> {
    const response = await fetch(`/api/workspace/${encodeURIComponent(documentId)}/artboards`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ artboards })
    });
    if (!response.ok) throw new Error('无法保存响应式画板。');
    return response.json() as Promise<WorkspacePlacementDocument>;
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
