import { randomUUID } from 'node:crypto';

export type SourceMode = 'existing-project' | 'greenfield';
export type DocumentStatus = 'draft' | 'review' | 'approved';
export type RequirementPriority = 'must' | 'should' | 'could';
const requirementPriorityLabels: Record<RequirementPriority, string> = { must: '高', should: '中', could: '低' };
export type TaskStatus = 'planned' | 'in_progress' | 'blocked' | 'done' | 'cancelled';
export type DesignContentType = 'text' | 'architecture' | 'flowchart' | 'ui-svg';

export interface EvidenceReference {
  id: string;
  label: string;
  source: string;
  note?: string;
  confidence: 'verified' | 'user-stated' | 'assumption';
}

export interface RequirementItem {
  id: string;
  parentRequirementId?: string;
  title: string;
  description: string;
  priority: RequirementPriority;
  acceptanceCriteria: string[];
  evidenceIds: string[];
  selectedDesignSectionId?: string;
}

export interface ProjectProfile {
  background: string;
  overview: string;
  projectType: string;
  deliveryForm: string;
  targetPlatforms: string[];
}

export interface ChatosProjectBinding {
  projectId: string;
  projectName?: string;
  connectorWorkspaceId?: string;
  contextScopeId?: string;
}

export interface RequirementsDocument {
  status: DocumentStatus;
  revision: number;
  summary: string;
  goals: string[];
  users: string[];
  inScope: string[];
  outOfScope: string[];
  constraints: string[];
  assumptions: string[];
  openQuestions: string[];
  evidence: EvidenceReference[];
  items: RequirementItem[];
  updatedAt: string;
}

export interface DesignSection {
  id: string;
  title: string;
  body: string;
  requirementIds: string[];
  evidenceIds: string[];
  blocks?: DesignContentBlock[];
}

export interface DesignContentBlock {
  id: string;
  type: DesignContentType;
  title: string;
  content: string;
}

export interface DesignDecision {
  id: string;
  title: string;
  decision: string;
  rationale: string;
  alternatives: string[];
  consequences: string[];
  requirementIds: string[];
}

export interface SolutionDesignDocument {
  status: DocumentStatus;
  revision: number;
  basedOnRequirementsRevision: number;
  summary: string;
  blocks?: DesignContentBlock[];
  sections: DesignSection[];
  decisions: DesignDecision[];
  risks: string[];
  validationStrategy: string[];
  updatedAt: string;
}

export interface ExecutionTask {
  id: string;
  title: string;
  description: string;
  type: 'task' | 'review' | 'milestone';
  phase: string;
  dependsOn: string[];
  status: TaskStatus;
  blockedReason?: string;
  requirementIds: string[];
  designSectionIds: string[];
  deliverables: string[];
  acceptanceCriteria: string[];
  sourceReferences: string[];
}

export interface PlanNodePosition { x: number; y: number }

export interface ExecutionPlan {
  status: DocumentStatus;
  revision: number;
  basedOnDesignRevision: number;
  objective: string;
  tasks: ExecutionTask[];
  positions: Record<string, PlanNodePosition>;
  viewport: { x: number; y: number; zoom: number };
  updatedAt: string;
}

export interface SolutionWorkspace {
  schemaVersion: 1;
  workspaceId: string;
  artifactKey: string;
  revision: number;
  title: string;
  description: string;
  hostProject?: ChatosProjectBinding;
  projectProfile?: ProjectProfile;
  sourceMode: SourceMode;
  createdAt: string;
  updatedAt: string;
  requirements: RequirementsDocument;
  design: SolutionDesignDocument;
  executionPlan: ExecutionPlan;
}

export interface SolutionWorkspaceSummary {
  workspaceId: string;
  artifactKey: string;
  revision: number;
  title: string;
  hostProjectId?: string;
  hostProjectName?: string;
  sourceMode: SourceMode;
  requirementCount: number;
  designSectionCount: number;
  taskCount: number;
  completedTaskCount: number;
  updatedAt: string;
}

export interface ValidationIssue {
  code: string;
  message: string;
  path?: string;
  blocking: boolean;
}

export interface WorkspaceValidation {
  valid: boolean;
  ready: boolean;
  issues: ValidationIssue[];
  topologicalOrder: string[];
  readyTaskIds: string[];
  traceability: {
    requirementCount: number;
    requirementsWithDesign: number;
    requirementsWithTasks: number;
  };
}

const identifierPattern = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$/;

export function assertIdentifier(value: unknown, label: string): asserts value is string {
  if (typeof value !== 'string' || !identifierPattern.test(value)) throw new Error(`${label} is invalid.`);
}

function assertText(value: unknown, label: string, allowEmpty = false): asserts value is string {
  if (typeof value !== 'string' || (!allowEmpty && !value.trim()) || value.length > 100_000) throw new Error(`${label} is invalid.`);
}

function assertStringList(value: unknown, label: string): asserts value is string[] {
  if (!Array.isArray(value) || value.length > 5_000 || value.some((item) => typeof item !== 'string' || item.length > 10_000)) throw new Error(`${label} is invalid.`);
}

function assertUniqueIdentifiers(items: readonly { id: string }[], label: string): void {
  const ids = new Set<string>();
  for (const item of items) {
    assertIdentifier(item.id, `${label}.id`);
    if (ids.has(item.id)) throw new Error(`${label} contains duplicate id ${item.id}.`);
    ids.add(item.id);
  }
}

export function createSolutionWorkspace(title: string, sourceMode: SourceMode = 'existing-project', artifactKey?: string, hostProject?: ChatosProjectBinding): SolutionWorkspace {
  const cleanTitle = title.trim() || '未命名方案';
  const now = new Date().toISOString();
  const workspaceId = `solution-${randomUUID().slice(0, 8)}`;
  return {
    schemaVersion: 1,
    workspaceId,
    artifactKey: artifactKey?.trim() || workspaceId,
    revision: 0,
    title: cleanTitle.slice(0, 240),
    description: '',
    ...(hostProject ? { hostProject: structuredClone(hostProject) } : {}),
    projectProfile: { background: '', overview: '', projectType: '', deliveryForm: '', targetPlatforms: [] },
    sourceMode,
    createdAt: now,
    updatedAt: now,
    requirements: {
      status: 'draft', revision: 0, summary: '', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], items: [], updatedAt: now
    },
    design: {
      status: 'draft', revision: 0, basedOnRequirementsRevision: 0, summary: '', blocks: [], sections: [], decisions: [], risks: [], validationStrategy: [], updatedAt: now
    },
    executionPlan: {
      status: 'draft', revision: 0, basedOnDesignRevision: 0, objective: '', tasks: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now
    }
  };
}

export function assertSolutionWorkspace(value: unknown): asserts value is SolutionWorkspace {
  if (!value || typeof value !== 'object') throw new Error('Solution workspace must be an object.');
  const workspace = value as SolutionWorkspace;
  if (workspace.schemaVersion !== 1) throw new Error('Unsupported solution workspace schema version.');
  assertIdentifier(workspace.workspaceId, 'workspaceId');
  assertIdentifier(workspace.artifactKey, 'artifactKey');
  if (!Number.isSafeInteger(workspace.revision) || workspace.revision < 0) throw new Error('revision is invalid.');
  assertText(workspace.title, 'title');
  assertText(workspace.description, 'description', true);
  if (workspace.hostProject !== undefined) {
    assertIdentifier(workspace.hostProject.projectId, 'hostProject.projectId');
    if (workspace.hostProject.projectName !== undefined) assertText(workspace.hostProject.projectName, 'hostProject.projectName');
    if (workspace.hostProject.connectorWorkspaceId !== undefined) assertIdentifier(workspace.hostProject.connectorWorkspaceId, 'hostProject.connectorWorkspaceId');
    if (workspace.hostProject.contextScopeId !== undefined) assertIdentifier(workspace.hostProject.contextScopeId, 'hostProject.contextScopeId');
  }
  if (workspace.projectProfile !== undefined) {
    assertText(workspace.projectProfile.background, 'projectProfile.background', true);
    assertText(workspace.projectProfile.overview, 'projectProfile.overview', true);
    assertText(workspace.projectProfile.projectType, 'projectProfile.projectType', true);
    assertText(workspace.projectProfile.deliveryForm, 'projectProfile.deliveryForm', true);
    assertStringList(workspace.projectProfile.targetPlatforms, 'projectProfile.targetPlatforms');
  }
  if (!['existing-project', 'greenfield'].includes(workspace.sourceMode)) throw new Error('sourceMode is invalid.');
  if (!Number.isFinite(Date.parse(workspace.createdAt)) || !Number.isFinite(Date.parse(workspace.updatedAt))) throw new Error('Workspace timestamps are invalid.');
  assertRequirements(workspace.requirements);
  assertDesign(workspace.design);
  assertExecutionPlan(workspace.executionPlan);
}

function assertDocumentHeader(document: { status: DocumentStatus; revision: number; updatedAt: string }, label: string): void {
  if (!['draft', 'review', 'approved'].includes(document.status)) throw new Error(`${label}.status is invalid.`);
  if (!Number.isSafeInteger(document.revision) || document.revision < 0) throw new Error(`${label}.revision is invalid.`);
  if (!Number.isFinite(Date.parse(document.updatedAt))) throw new Error(`${label}.updatedAt is invalid.`);
}

function assertRequirements(document: RequirementsDocument): void {
  if (!document || typeof document !== 'object') throw new Error('requirements is invalid.');
  assertDocumentHeader(document, 'requirements');
  assertText(document.summary, 'requirements.summary', true);
  for (const field of ['goals', 'users', 'inScope', 'outOfScope', 'constraints', 'assumptions', 'openQuestions'] as const) assertStringList(document[field], `requirements.${field}`);
  if (!Array.isArray(document.evidence) || !Array.isArray(document.items)) throw new Error('requirements collections are invalid.');
  assertUniqueIdentifiers(document.evidence, 'requirements.evidence');
  assertUniqueIdentifiers(document.items, 'requirements.items');
  const evidenceIds = new Set(document.evidence.map((item) => item.id));
  for (const evidence of document.evidence) {
    assertText(evidence.label, `evidence.${evidence.id}.label`);
    assertText(evidence.source, `evidence.${evidence.id}.source`);
    if (!['verified', 'user-stated', 'assumption'].includes(evidence.confidence)) throw new Error(`evidence.${evidence.id}.confidence is invalid.`);
  }
  for (const item of document.items) {
    if (item.parentRequirementId !== undefined) assertIdentifier(item.parentRequirementId, `requirement.${item.id}.parentRequirementId`);
    assertText(item.title, `requirement.${item.id}.title`);
    assertText(item.description, `requirement.${item.id}.description`);
    if (!['must', 'should', 'could'].includes(item.priority)) throw new Error(`requirement.${item.id}.priority is invalid.`);
    assertStringList(item.acceptanceCriteria, `requirement.${item.id}.acceptanceCriteria`);
    assertStringList(item.evidenceIds, `requirement.${item.id}.evidenceIds`);
    if (item.selectedDesignSectionId !== undefined) assertIdentifier(item.selectedDesignSectionId, `requirement.${item.id}.selectedDesignSectionId`);
    if (item.evidenceIds.some((id) => !evidenceIds.has(id))) throw new Error(`Requirement ${item.id} references unknown evidence.`);
  }
}

function assertDesign(document: SolutionDesignDocument): void {
  if (!document || typeof document !== 'object') throw new Error('design is invalid.');
  assertDocumentHeader(document, 'design');
  if (!Number.isSafeInteger(document.basedOnRequirementsRevision) || document.basedOnRequirementsRevision < 0) throw new Error('design requirement revision is invalid.');
  assertText(document.summary, 'design.summary', true);
  if (!Array.isArray(document.sections) || !Array.isArray(document.decisions)) throw new Error('design collections are invalid.');
  if (document.blocks !== undefined) assertDesignBlocks(document.blocks, 'design.blocks');
  assertUniqueIdentifiers(document.sections, 'design.sections');
  assertUniqueIdentifiers(document.decisions, 'design.decisions');
  assertStringList(document.risks, 'design.risks');
  assertStringList(document.validationStrategy, 'design.validationStrategy');
  for (const section of document.sections) {
    assertText(section.title, `design.section.${section.id}.title`);
    assertText(section.body, `design.section.${section.id}.body`, true);
    assertStringList(section.requirementIds, `design.section.${section.id}.requirementIds`);
    assertStringList(section.evidenceIds, `design.section.${section.id}.evidenceIds`);
    if (section.blocks !== undefined) assertDesignBlocks(section.blocks, `design.section.${section.id}.blocks`);
  }
  for (const decision of document.decisions) {
    assertText(decision.title, `design.decision.${decision.id}.title`);
    assertText(decision.decision, `design.decision.${decision.id}.decision`);
    assertText(decision.rationale, `design.decision.${decision.id}.rationale`);
    assertStringList(decision.alternatives, `design.decision.${decision.id}.alternatives`);
    assertStringList(decision.consequences, `design.decision.${decision.id}.consequences`);
    assertStringList(decision.requirementIds, `design.decision.${decision.id}.requirementIds`);
  }
}

function assertDesignBlocks(blocks: DesignContentBlock[], label: string): void {
  if (!Array.isArray(blocks)) throw new Error(`${label} is invalid.`);
  assertUniqueIdentifiers(blocks, label);
  for (const block of blocks) {
    if (!['text', 'architecture', 'flowchart', 'ui-svg'].includes(block.type)) throw new Error(`design block ${block.id} type is invalid.`);
    assertText(block.title, `design.block.${block.id}.title`);
    assertText(block.content, `design.block.${block.id}.content`);
    if (block.type !== 'text') assertRenderableSvg(block.content, block.id);
  }
}

function assertRenderableSvg(content: string, blockId: string): void {
  const svg = content.trim();
  if (!/^<svg[\s>][\s\S]*<\/svg>$/i.test(svg)) throw new Error(`design block ${blockId} must contain standalone SVG code.`);
  const openingTag = svg.match(/^<svg\b[^>]*>/i)?.[0] ?? '';
  if (!/\bviewBox\s*=\s*["'][^"']+["']/i.test(openingTag)) throw new Error(`design block ${blockId} SVG must declare a viewBox.`);
  if (/<script\b|<foreignObject\b|\son[a-z]+\s*=|\b(?:href|src)\s*=\s*["'](?:https?:|\/\/|data:)/i.test(svg)) {
    throw new Error(`design block ${blockId} SVG must be self-contained and inert.`);
  }
  const visibleBody = svg
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/<(?:defs|style|title|desc|metadata)\b[^>]*>[\s\S]*?<\/(?:defs|style|title|desc|metadata)>/gi, '');
  if (!/<(?:path|rect|circle|ellipse|line|polyline|polygon|text|image|use)\b/i.test(visibleBody)) {
    throw new Error(`design block ${blockId} SVG does not contain visible diagram content.`);
  }
}

function assertExecutionPlan(plan: ExecutionPlan): void {
  if (!plan || typeof plan !== 'object') throw new Error('executionPlan is invalid.');
  assertDocumentHeader(plan, 'executionPlan');
  if (!Number.isSafeInteger(plan.basedOnDesignRevision) || plan.basedOnDesignRevision < 0) throw new Error('executionPlan design revision is invalid.');
  assertText(plan.objective, 'executionPlan.objective', true);
  if (!Array.isArray(plan.tasks) || plan.tasks.length > 2_000) throw new Error('executionPlan.tasks is invalid.');
  assertUniqueIdentifiers(plan.tasks, 'executionPlan.tasks');
  for (const task of plan.tasks) {
    assertText(task.title, `task.${task.id}.title`);
    assertText(task.description, `task.${task.id}.description`, true);
    assertText(task.phase, `task.${task.id}.phase`, true);
    if (!['task', 'review', 'milestone'].includes(task.type)) throw new Error(`task.${task.id}.type is invalid.`);
    if (!['planned', 'in_progress', 'blocked', 'done', 'cancelled'].includes(task.status)) throw new Error(`task.${task.id}.status is invalid.`);
    for (const field of ['dependsOn', 'requirementIds', 'designSectionIds', 'acceptanceCriteria', 'sourceReferences'] as const) assertStringList(task[field], `task.${task.id}.${field}`);
    assertStringList(task.deliverables, `task.${task.id}.deliverables`);
  }
  if (!plan.positions || typeof plan.positions !== 'object') throw new Error('executionPlan.positions is invalid.');
  for (const [id, position] of Object.entries(plan.positions)) {
    assertIdentifier(id, 'executionPlan.positions key');
    if (!Number.isFinite(position.x) || !Number.isFinite(position.y)) throw new Error(`Position for ${id} is invalid.`);
  }
  if (!plan.viewport || !Number.isFinite(plan.viewport.x) || !Number.isFinite(plan.viewport.y) || !Number.isFinite(plan.viewport.zoom)) throw new Error('executionPlan.viewport is invalid.');
}

export function validateWorkspace(workspace: SolutionWorkspace): WorkspaceValidation {
  assertSolutionWorkspace(workspace);
  const issues: ValidationIssue[] = [];
  const requirementIds = new Set(workspace.requirements.items.map((item) => item.id));
  const evidenceIds = new Set(workspace.requirements.evidence.map((item) => item.id));
  const designSectionIds = new Set(workspace.design.sections.map((item) => item.id));
  const taskIds = new Set(workspace.executionPlan.tasks.map((item) => item.id));

  if (workspace.requirements.items.length === 0) issues.push({ code: 'requirements_empty', message: '至少需要一条结构化需求。', path: 'requirements.items', blocking: true });
  if (workspace.design.sections.length === 0) issues.push({ code: 'design_empty', message: '设计方案至少需要一个章节。', path: 'design.sections', blocking: true });
  if (workspace.design.sections.length > 0 && !workspace.design.blocks?.some((block) => block.type === 'text')) issues.push({ code: 'project_design_missing', message: '项目总体设计缺少技术基线文档。', path: 'design.blocks', blocking: true });
  if (workspace.design.sections.length > 0 && !workspace.design.blocks?.some((block) => block.type === 'architecture')) issues.push({ code: 'project_architecture_missing', message: '项目总体设计缺少总体架构图。', path: 'design.blocks', blocking: true });
  if (workspace.executionPlan.tasks.length === 0) issues.push({ code: 'plan_empty', message: '执行计划至少需要一个任务。', path: 'executionPlan.tasks', blocking: true });

  for (const requirement of workspace.requirements.items) {
    if (requirement.parentRequirementId === requirement.id) issues.push({ code: 'self_parent_requirement', message: `需求 ${requirement.id} 不能把自身设为父需求。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true });
    else if (requirement.parentRequirementId && !requirementIds.has(requirement.parentRequirementId)) issues.push({ code: 'unknown_parent_requirement', message: `需求 ${requirement.id} 的父需求不存在。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true });
    const visited = new Set<string>([requirement.id]);
    let parentId = requirement.parentRequirementId;
    while (parentId) {
      if (visited.has(parentId)) { issues.push({ code: 'requirement_hierarchy_cycle', message: `需求 ${requirement.id} 的父子关系形成循环。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true }); break; }
      visited.add(parentId);
      parentId = workspace.requirements.items.find((item) => item.id === parentId)?.parentRequirementId;
    }
  }

  for (const section of workspace.design.sections) {
    if (!section.blocks?.some((block) => block.type === 'text')) issues.push({ code: 'design_section_document_missing', message: `设计方案 ${section.id} 缺少详细技术文档。`, path: `design.sections.${section.id}.blocks`, blocking: true });
    for (const id of section.requirementIds) if (!requirementIds.has(id)) issues.push({ code: 'unknown_requirement', message: `设计章节 ${section.id} 引用了不存在的需求 ${id}。`, path: `design.sections.${section.id}`, blocking: true });
    for (const id of section.evidenceIds) if (!evidenceIds.has(id)) issues.push({ code: 'unknown_evidence', message: `设计章节 ${section.id} 引用了不存在的证据 ${id}。`, path: `design.sections.${section.id}`, blocking: true });
  }
  for (const task of workspace.executionPlan.tasks) {
    if (task.dependsOn.includes(task.id)) issues.push({ code: 'self_dependency', message: `任务 ${task.id} 不能依赖自身。`, path: `executionPlan.tasks.${task.id}.dependsOn`, blocking: true });
    for (const id of task.dependsOn) if (!taskIds.has(id)) issues.push({ code: 'unknown_dependency', message: `任务 ${task.id} 依赖不存在的任务 ${id}。`, path: `executionPlan.tasks.${task.id}.dependsOn`, blocking: true });
    for (const id of task.requirementIds) if (!requirementIds.has(id)) issues.push({ code: 'unknown_requirement', message: `任务 ${task.id} 引用了不存在的需求 ${id}。`, path: `executionPlan.tasks.${task.id}.requirementIds`, blocking: true });
    for (const id of task.designSectionIds) if (!designSectionIds.has(id)) issues.push({ code: 'unknown_design_section', message: `任务 ${task.id} 引用了不存在的设计章节 ${id}。`, path: `executionPlan.tasks.${task.id}.designSectionIds`, blocking: true });
    if (task.status === 'blocked' && !task.blockedReason?.trim()) issues.push({ code: 'blocked_without_reason', message: `任务 ${task.id} 已阻塞但没有说明原因。`, path: `executionPlan.tasks.${task.id}.blockedReason`, blocking: false });
    if (task.acceptanceCriteria.length === 0 && task.type !== 'milestone') issues.push({ code: 'task_without_acceptance', message: `任务 ${task.id} 缺少验收条件。`, path: `executionPlan.tasks.${task.id}.acceptanceCriteria`, blocking: true });
  }

  for (const requirement of workspace.requirements.items) {
    const requirementDesigns = workspace.design.sections.filter((section) => section.requirementIds.includes(requirement.id));
    if (requirementDesigns.length > 1) issues.push({ code: 'multiple_designs_per_requirement', message: `需求 ${requirement.id} 只能对应一个确定的设计方案。`, path: `requirements.items.${requirement.id}`, blocking: true });
    if (requirementDesigns.length === 1 && requirement.selectedDesignSectionId !== requirementDesigns[0].id) issues.push({ code: 'design_link_mismatch', message: `需求 ${requirement.id} 未关联其唯一设计方案。`, path: `requirements.items.${requirement.id}.selectedDesignSectionId`, blocking: true });
    if (requirement.selectedDesignSectionId) {
      const selected = workspace.design.sections.find((section) => section.id === requirement.selectedDesignSectionId);
      if (!selected || !selected.requirementIds.includes(requirement.id)) issues.push({ code: 'invalid_selected_design', message: `需求 ${requirement.id} 选择的设计方案无效。`, path: `requirements.items.${requirement.id}.selectedDesignSectionId`, blocking: true });
    }
    if (!workspace.design.sections.some((section) => section.requirementIds.includes(requirement.id))) {
      issues.push({ code: 'requirement_without_design', message: `需求 ${requirement.id} 尚未进入设计方案。`, path: `requirements.items.${requirement.id}`, blocking: true });
    }
    if (!workspace.executionPlan.tasks.some((task) => task.requirementIds.includes(requirement.id))) {
      issues.push({ code: 'requirement_without_task', message: `需求 ${requirement.id} 尚未进入执行计划。`, path: `requirements.items.${requirement.id}`, blocking: true });
    }
  }
  if (workspace.design.basedOnRequirementsRevision !== workspace.requirements.revision) {
    issues.push({ code: 'stale_design', message: '设计方案基于的需求版本已过期。', path: 'design.basedOnRequirementsRevision', blocking: true });
  }
  if (workspace.executionPlan.basedOnDesignRevision !== workspace.design.revision) {
    issues.push({ code: 'stale_plan', message: '执行计划基于的设计版本已过期。', path: 'executionPlan.basedOnDesignRevision', blocking: true });
  }

  const indegree = new Map<string, number>(workspace.executionPlan.tasks.map((task) => [task.id, 0]));
  const outgoing = new Map<string, string[]>(workspace.executionPlan.tasks.map((task) => [task.id, []]));
  for (const task of workspace.executionPlan.tasks) {
    for (const dependency of task.dependsOn) {
      if (!taskIds.has(dependency) || dependency === task.id) continue;
      indegree.set(task.id, (indegree.get(task.id) ?? 0) + 1);
      outgoing.get(dependency)?.push(task.id);
    }
  }
  const queue = [...indegree.entries()].filter(([, count]) => count === 0).map(([id]) => id).sort();
  const topologicalOrder: string[] = [];
  while (queue.length > 0) {
    const id = queue.shift()!;
    topologicalOrder.push(id);
    for (const target of outgoing.get(id) ?? []) {
      const next = (indegree.get(target) ?? 0) - 1;
      indegree.set(target, next);
      if (next === 0) queue.push(target);
    }
  }
  if (topologicalOrder.length !== workspace.executionPlan.tasks.length) issues.push({ code: 'dependency_cycle', message: '执行计划包含循环依赖，无法计算执行顺序。', path: 'executionPlan.tasks', blocking: true });

  const doneIds = new Set(workspace.executionPlan.tasks.filter((task) => task.status === 'done').map((task) => task.id));
  const readyTaskIds = workspace.executionPlan.tasks
    .filter((task) => task.status === 'planned' && task.dependsOn.every((id) => doneIds.has(id)))
    .map((task) => task.id);
  const designedRequirements = new Set(workspace.design.sections.flatMap((section) => section.requirementIds));
  const plannedRequirements = new Set(workspace.executionPlan.tasks.flatMap((task) => task.requirementIds));
  const blocking = issues.some((issue) => issue.blocking);
  const approved = workspace.requirements.status === 'approved' && workspace.design.status === 'approved' && workspace.executionPlan.status === 'approved';
  return {
    valid: !blocking,
    ready: !blocking && approved,
    issues,
    topologicalOrder,
    readyTaskIds,
    traceability: { requirementCount: requirementIds.size, requirementsWithDesign: designedRequirements.size, requirementsWithTasks: plannedRequirements.size }
  };
}

export function workspaceSummary(workspace: SolutionWorkspace): SolutionWorkspaceSummary {
  return {
    workspaceId: workspace.workspaceId,
    artifactKey: workspace.artifactKey,
    revision: workspace.revision,
    title: workspace.title,
    ...(workspace.hostProject?.projectId ? { hostProjectId: workspace.hostProject.projectId } : {}),
    ...(workspace.hostProject?.projectName ? { hostProjectName: workspace.hostProject.projectName } : {}),
    sourceMode: workspace.sourceMode,
    requirementCount: workspace.requirements.items.length,
    designSectionCount: workspace.design.sections.length,
    taskCount: workspace.executionPlan.tasks.length,
    completedTaskCount: workspace.executionPlan.tasks.filter((task) => task.status === 'done').length,
    updatedAt: workspace.updatedAt
  };
}

export function workspaceToMarkdown(workspace: SolutionWorkspace): string {
  const lines = [`# ${workspace.title}`, '', workspace.description, ''];
  if (workspace.hostProject) {
    lines.push('## ChatOS 项目绑定', '', `- 项目：${workspace.hostProject.projectName ?? '未命名项目'}`, `- 项目 ID：${workspace.hostProject.projectId}`, ...(workspace.hostProject.connectorWorkspaceId ? [`- 工作区 ID：${workspace.hostProject.connectorWorkspaceId}`] : []), '');
  }
  if (workspace.projectProfile) {
    lines.push('## 项目基本信息', '', `- 项目类型：${workspace.projectProfile.projectType || '未填写'}`, `- 交付形态：${workspace.projectProfile.deliveryForm || '未填写'}`, `- 目标平台：${workspace.projectProfile.targetPlatforms.join('、') || '未填写'}`, '', '### 项目背景', '', workspace.projectProfile.background || '未填写', '', '### 整体描述', '', workspace.projectProfile.overview || '未填写', '');
  }
  lines.push('## 项目总需求', '', workspace.requirements.summary, '');
  const pushList = (title: string, values: string[]) => {
    if (values.length === 0) return;
    lines.push(`### ${title}`, '', ...values.map((value) => `- ${value}`), '');
  };
  pushList('目标', workspace.requirements.goals);
  pushList('目标用户', workspace.requirements.users);
  pushList('范围内', workspace.requirements.inScope);
  pushList('范围外', workspace.requirements.outOfScope);
  pushList('约束', workspace.requirements.constraints);
  pushList('假设', workspace.requirements.assumptions);
  pushList('待确认问题', workspace.requirements.openQuestions);
  if (workspace.requirements.evidence.length > 0) {
    lines.push('### 证据与来源', '');
    for (const evidence of workspace.requirements.evidence) lines.push(`- **${evidence.id} · ${evidence.label}**：${evidence.source}${evidence.note ? `（${evidence.note}）` : ''} · ${evidence.confidence}`);
    lines.push('');
  }
  lines.push('### 结构化需求', '');
  for (const item of workspace.requirements.items) {
    lines.push(`#### ${item.id} · ${item.title}`, '', item.description, '', `- 父需求：${item.parentRequirementId ?? '无'}`, `- 优先级：${requirementPriorityLabels[item.priority]}`, `- 设计方案：${item.selectedDesignSectionId ?? '尚未创建'}`, '', '**验收条件**', '', ...item.acceptanceCriteria.map((criterion) => `- ${criterion}`), '');
  }
  lines.push('## 项目总体设计', '', workspace.design.summary, '');
  for (const block of workspace.design.blocks ?? []) {
    const typeLabel = ({ text: '文本', architecture: '架构图', flowchart: '流程图', 'ui-svg': '页面设计图' } as const)[block.type];
    lines.push(`### ${block.id} · ${block.title}（${typeLabel}）`, '');
    if (block.type === 'text') lines.push(block.content, '');
    else lines.push('```svg', block.content, '```', '');
  }
  lines.push('## 需求详细设计', '');
  for (const section of workspace.design.sections) {
    lines.push(`### ${section.id} · ${section.title}`, '', section.body, '', `回应需求：${section.requirementIds.join(', ') || '无'}`, '');
    for (const block of section.blocks ?? []) {
      const typeLabel = ({ text: '文本', architecture: '架构图', flowchart: '流程图', 'ui-svg': '页面设计图' } as const)[block.type];
      lines.push(`#### ${block.id} · ${block.title}（${typeLabel}）`, '');
      if (block.type === 'text') lines.push(block.content, '');
      else lines.push('```svg', block.content, '```', '');
    }
  }
  if (workspace.design.decisions.length > 0) {
    lines.push('### 设计决策', '');
    for (const decision of workspace.design.decisions) {
      lines.push(`#### ${decision.id} · ${decision.title}`, '', `**决定：** ${decision.decision}`, '', `**理由：** ${decision.rationale}`, '', '**备选：**', '', ...decision.alternatives.map((item) => `- ${item}`), '', '**影响：**', '', ...decision.consequences.map((item) => `- ${item}`), '', `回应需求：${decision.requirementIds.join(', ')}`, '');
    }
  }
  pushList('风险与权衡', workspace.design.risks);
  pushList('验证策略', workspace.design.validationStrategy);
  lines.push('## 执行计划', '', workspace.executionPlan.objective, '');
  for (const task of workspace.executionPlan.tasks) {
    lines.push(`### ${task.id} · ${task.title}`, '', task.description, '', `- 类型：${task.type}`, `- 阶段：${task.phase || '未分阶段'}`, `- 状态：${task.status}`, `- 前置：${task.dependsOn.join(', ') || '无'}`, `- 关联需求：${task.requirementIds.join(', ') || '无'}`, `- 关联设计：${task.designSectionIds.join(', ') || '无'}`, '');
    if (task.deliverables.length > 0) lines.push('**交付物**', '', ...task.deliverables.map((item) => `- ${item}`), '');
    if (task.acceptanceCriteria.length > 0) lines.push('**验收条件**', '', ...task.acceptanceCriteria.map((criterion) => `- ${criterion}`), '');
  }
  return `${lines.join('\n').trim()}\n`;
}
