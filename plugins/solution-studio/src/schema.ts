import { randomUUID } from 'node:crypto';
import { dependencyAnalysis, readyTaskIds } from './plan-view.js';

export type SourceMode = 'existing-project' | 'greenfield';
export type DocumentStatus = 'draft' | 'review' | 'approved';
export type RequirementPriority = 'must' | 'should' | 'could';
const requirementPriorityLabels: Record<RequirementPriority, string> = { must: '高', should: '中', could: '低' };
export type TaskStatus = 'planned' | 'in_progress' | 'blocked' | 'done' | 'cancelled';
export type DesignContentType = 'text' | 'architecture' | 'flowchart' | 'ui-svg';
export type RequirementModuleKind = 'foundation' | 'business' | 'delivery';
export type RequirementKind = 'capability' | 'quality' | 'constraint';

export interface RequirementModule {
  id: string;
  title: string;
  kind: RequirementModuleKind;
  description: string;
  order: number;
}

export interface EvidenceReference {
  id: string;
  label: string;
  source: string;
  note?: string;
  confidence: 'verified' | 'user-stated' | 'assumption';
}

export interface RequirementItem {
  id: string;
  moduleId: string;
  kind: RequirementKind;
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
  modules: RequirementModule[];
  items: RequirementItem[];
  updatedAt: string;
}

export interface DesignSection {
  id: string;
  moduleId: string;
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
  moduleId: string;
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

export interface TaskStatusEvent {
  id: string;
  taskId: string;
  from: TaskStatus;
  to: TaskStatus;
  occurredAt: string;
  runtimeSessionId: string;
  reason?: string;
  summary?: string;
  evidence: string[];
}

export interface ExecutionPlan {
  status: DocumentStatus;
  revision: number;
  basedOnDesignRevision: number;
  objective: string;
  tasks: ExecutionTask[];
  statusEvents: TaskStatusEvent[];
  positions: Record<string, PlanNodePosition>;
  viewport: { x: number; y: number; zoom: number };
  updatedAt: string;
}

export interface SolutionWorkspace {
  schemaVersion: 2;
  workspaceId: string;
  artifactKey: string;
  revision: number;
  title: string;
  description: string;
  hostProject?: ChatosProjectBinding;
  projectProfile: ProjectProfile;
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
  moduleCount: number;
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
  graph: {
    redundantEdges: Array<{ source: string; target: string }>;
    longestChainLength: number;
  };
  traceability: {
    requirementCount: number;
    requirementsWithDesign: number;
    requirementsWithTasks: number;
  };
}

export interface DesignBlockExpectation {
  id: string;
  type: DesignContentType;
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

export function createSolutionWorkspace(title: string, sourceMode: SourceMode = 'existing-project', hostProject?: ChatosProjectBinding): SolutionWorkspace {
  const cleanTitle = title.trim() || '未命名方案';
  const now = new Date().toISOString();
  const workspaceId = `solution-${randomUUID().slice(0, 8)}`;
  return {
    schemaVersion: 2,
    workspaceId,
    artifactKey: 'project-plan',
    revision: 0,
    title: cleanTitle.slice(0, 240),
    description: '',
    ...(hostProject ? { hostProject: structuredClone(hostProject) } : {}),
    projectProfile: { background: '', overview: '', projectType: '', deliveryForm: '', targetPlatforms: [] },
    sourceMode,
    createdAt: now,
    updatedAt: now,
    requirements: {
      status: 'draft', revision: 0, summary: '', goals: [], users: [], inScope: [], outOfScope: [], constraints: [], assumptions: [], openQuestions: [], evidence: [], modules: [], items: [], updatedAt: now
    },
    design: {
      status: 'draft', revision: 0, basedOnRequirementsRevision: 0, summary: '', blocks: [], sections: [], decisions: [], risks: [], validationStrategy: [], updatedAt: now
    },
    executionPlan: {
      status: 'draft', revision: 0, basedOnDesignRevision: 0, objective: '', tasks: [], statusEvents: [], positions: {}, viewport: { x: 0, y: 0, zoom: 1 }, updatedAt: now
    }
  };
}

export function assertSolutionWorkspace(value: unknown): asserts value is SolutionWorkspace {
  if (!value || typeof value !== 'object') throw new Error('Solution workspace must be an object.');
  const workspace = value as SolutionWorkspace;
  if (workspace.schemaVersion !== 2) throw new Error('Solution workspace schemaVersion must be 2. Legacy workspaces are not supported.');
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
  if (!workspace.projectProfile || typeof workspace.projectProfile !== 'object') throw new Error('projectProfile is required.');
  assertText(workspace.projectProfile.background, 'projectProfile.background', true);
  assertText(workspace.projectProfile.overview, 'projectProfile.overview', true);
  assertText(workspace.projectProfile.projectType, 'projectProfile.projectType', true);
  assertText(workspace.projectProfile.deliveryForm, 'projectProfile.deliveryForm', true);
  assertStringList(workspace.projectProfile.targetPlatforms, 'projectProfile.targetPlatforms');
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
  if (!Array.isArray(document.evidence) || !Array.isArray(document.modules) || !Array.isArray(document.items)) throw new Error('requirements collections are invalid.');
  assertUniqueIdentifiers(document.evidence, 'requirements.evidence');
  assertUniqueIdentifiers(document.modules, 'requirements.modules');
  assertUniqueIdentifiers(document.items, 'requirements.items');
  const evidenceIds = new Set(document.evidence.map((item) => item.id));
  for (const evidence of document.evidence) {
    assertText(evidence.label, `evidence.${evidence.id}.label`);
    assertText(evidence.source, `evidence.${evidence.id}.source`);
    if (!['verified', 'user-stated', 'assumption'].includes(evidence.confidence)) throw new Error(`evidence.${evidence.id}.confidence is invalid.`);
  }
  for (const module of document.modules) {
    assertText(module.title, `module.${module.id}.title`);
    assertText(module.description, `module.${module.id}.description`, true);
    if (!['foundation', 'business', 'delivery'].includes(module.kind)) throw new Error(`module.${module.id}.kind is invalid.`);
    if (!Number.isSafeInteger(module.order) || module.order < 0) throw new Error(`module.${module.id}.order is invalid.`);
  }
  for (const item of document.items) {
    assertIdentifier(item.moduleId, `requirement.${item.id}.moduleId`);
    if (!['capability', 'quality', 'constraint'].includes(item.kind)) throw new Error(`requirement.${item.id}.kind is invalid.`);
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
    assertIdentifier(section.moduleId, `design.section.${section.id}.moduleId`);
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
  if (!Array.isArray(plan.statusEvents) || plan.statusEvents.length > 20_000) throw new Error('executionPlan.statusEvents is invalid.');
  assertUniqueIdentifiers(plan.tasks, 'executionPlan.tasks');
  assertUniqueIdentifiers(plan.statusEvents, 'executionPlan.statusEvents');
  for (const task of plan.tasks) {
    assertIdentifier(task.moduleId, `task.${task.id}.moduleId`);
    assertText(task.title, `task.${task.id}.title`);
    assertText(task.description, `task.${task.id}.description`, true);
    assertText(task.phase, `task.${task.id}.phase`, true);
    if (!['task', 'review', 'milestone'].includes(task.type)) throw new Error(`task.${task.id}.type is invalid.`);
    if (!['planned', 'in_progress', 'blocked', 'done', 'cancelled'].includes(task.status)) throw new Error(`task.${task.id}.status is invalid.`);
    for (const field of ['dependsOn', 'requirementIds', 'designSectionIds', 'acceptanceCriteria', 'sourceReferences'] as const) assertStringList(task[field], `task.${task.id}.${field}`);
    assertStringList(task.deliverables, `task.${task.id}.deliverables`);
  }
  for (const event of plan.statusEvents) {
    assertIdentifier(event.taskId, `statusEvent.${event.id}.taskId`);
    if (!['planned', 'in_progress', 'blocked', 'done', 'cancelled'].includes(event.from)
      || !['planned', 'in_progress', 'blocked', 'done', 'cancelled'].includes(event.to)) throw new Error(`statusEvent.${event.id}.status is invalid.`);
    if (!Number.isFinite(Date.parse(event.occurredAt))) throw new Error(`statusEvent.${event.id}.occurredAt is invalid.`);
    assertText(event.runtimeSessionId, `statusEvent.${event.id}.runtimeSessionId`);
    if (event.reason !== undefined) assertText(event.reason, `statusEvent.${event.id}.reason`);
    if (event.summary !== undefined) assertText(event.summary, `statusEvent.${event.id}.summary`);
    assertStringList(event.evidence, `statusEvent.${event.id}.evidence`);
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
  const moduleIds = new Set(workspace.requirements.modules.map((item) => item.id));
  const requirementIds = new Set(workspace.requirements.items.map((item) => item.id));
  const evidenceIds = new Set(workspace.requirements.evidence.map((item) => item.id));
  const designSectionIds = new Set(workspace.design.sections.map((item) => item.id));
  const taskIds = new Set(workspace.executionPlan.tasks.map((item) => item.id));

  if (!workspace.projectProfile.background.trim()) issues.push({ code: 'project_background_missing', message: '项目基本信息缺少项目背景。', path: 'projectProfile.background', blocking: true });
  if (!workspace.projectProfile.overview.trim()) issues.push({ code: 'project_overview_missing', message: '项目基本信息缺少整体描述。', path: 'projectProfile.overview', blocking: true });
  if (!workspace.projectProfile.projectType.trim()) issues.push({ code: 'project_type_missing', message: '项目基本信息缺少项目类型。', path: 'projectProfile.projectType', blocking: true });
  if (!workspace.projectProfile.deliveryForm.trim()) issues.push({ code: 'project_delivery_form_missing', message: '项目基本信息缺少交付形态。', path: 'projectProfile.deliveryForm', blocking: true });
  if (!workspace.projectProfile.targetPlatforms.length) issues.push({ code: 'project_target_platforms_missing', message: '项目基本信息缺少目标平台。', path: 'projectProfile.targetPlatforms', blocking: true });

  if (workspace.requirements.modules.length === 0) issues.push({ code: 'modules_empty', message: '项目至少需要一个功能模块。', path: 'requirements.modules', blocking: true });
  if (isDevelopmentProject(workspace.projectProfile.projectType)
    && !workspace.requirements.modules.some((module) => module.kind === 'business')) {
    issues.push({ code: 'business_module_missing', message: '软件项目至少需要一个业务功能模块。', path: 'requirements.modules', blocking: true });
  }

  if (workspace.requirements.items.length === 0) issues.push({ code: 'requirements_empty', message: '至少需要一条结构化需求。', path: 'requirements.items', blocking: true });
  if (workspace.design.sections.length === 0) issues.push({ code: 'design_empty', message: '设计方案至少需要一个章节。', path: 'design.sections', blocking: true });
  if (workspace.design.sections.length > 0 && !workspace.design.blocks?.some((block) => block.type === 'text')) issues.push({ code: 'project_design_missing', message: '项目总体设计缺少技术基线文档。', path: 'design.blocks', blocking: true });
  if (workspace.design.sections.length > 0
    && isDevelopmentProject(workspace.projectProfile?.projectType)
    && !workspace.design.blocks?.some((block) => block.type === 'architecture')) {
    issues.push({
      code: 'project_architecture_missing',
      message: '开发项目的总体设计缺少总体架构图。',
      path: 'design.blocks',
      blocking: true
    });
  }
  if (workspace.executionPlan.tasks.length === 0) issues.push({ code: 'plan_empty', message: '执行计划至少需要一个任务。', path: 'executionPlan.tasks', blocking: true });

  for (const requirement of workspace.requirements.items) {
    if (!moduleIds.has(requirement.moduleId)) issues.push({ code: 'unknown_module', message: `需求 ${requirement.id} 引用了不存在的模块 ${requirement.moduleId}。`, path: `requirements.items.${requirement.id}.moduleId`, blocking: true });
    if (requirement.parentRequirementId === requirement.id) issues.push({ code: 'self_parent_requirement', message: `需求 ${requirement.id} 不能把自身设为父需求。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true });
    else if (requirement.parentRequirementId && !requirementIds.has(requirement.parentRequirementId)) issues.push({ code: 'unknown_parent_requirement', message: `需求 ${requirement.id} 的父需求不存在。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true });
    else if (requirement.parentRequirementId) {
      const parent = workspace.requirements.items.find((item) => item.id === requirement.parentRequirementId);
      if (parent && parent.moduleId !== requirement.moduleId) issues.push({ code: 'parent_module_mismatch', message: `需求 ${requirement.id} 与父需求 ${parent.id} 必须属于同一模块。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true });
    }
    const visited = new Set<string>([requirement.id]);
    let parentId = requirement.parentRequirementId;
    while (parentId) {
      if (visited.has(parentId)) { issues.push({ code: 'requirement_hierarchy_cycle', message: `需求 ${requirement.id} 的父子关系形成循环。`, path: `requirements.items.${requirement.id}.parentRequirementId`, blocking: true }); break; }
      visited.add(parentId);
      parentId = workspace.requirements.items.find((item) => item.id === parentId)?.parentRequirementId;
    }
  }

  for (const section of workspace.design.sections) {
    if (!moduleIds.has(section.moduleId)) issues.push({ code: 'unknown_module', message: `设计章节 ${section.id} 引用了不存在的模块 ${section.moduleId}。`, path: `design.sections.${section.id}.moduleId`, blocking: true });
    if (!section.blocks?.some((block) => block.type === 'text')) issues.push({ code: 'design_section_document_missing', message: `设计方案 ${section.id} 缺少详细技术文档。`, path: `design.sections.${section.id}.blocks`, blocking: true });
    if (isDevelopmentProject(workspace.projectProfile?.projectType) && !section.blocks?.some((block) => block.type === 'architecture' || block.type === 'flowchart')) issues.push({ code: 'design_section_visual_missing', message: `开发项目的设计方案 ${section.id} 缺少架构图或流程图。`, path: `design.sections.${section.id}.blocks`, blocking: true });
    for (const id of section.requirementIds) if (!requirementIds.has(id)) issues.push({ code: 'unknown_requirement', message: `设计章节 ${section.id} 引用了不存在的需求 ${id}。`, path: `design.sections.${section.id}`, blocking: true });
    for (const id of section.evidenceIds) if (!evidenceIds.has(id)) issues.push({ code: 'unknown_evidence', message: `设计章节 ${section.id} 引用了不存在的证据 ${id}。`, path: `design.sections.${section.id}`, blocking: true });
  }
  for (const task of workspace.executionPlan.tasks) {
    if (!moduleIds.has(task.moduleId)) issues.push({ code: 'unknown_module', message: `任务 ${task.id} 引用了不存在的模块 ${task.moduleId}。`, path: `executionPlan.tasks.${task.id}.moduleId`, blocking: true });
    if (task.dependsOn.includes(task.id)) issues.push({ code: 'self_dependency', message: `任务 ${task.id} 不能依赖自身。`, path: `executionPlan.tasks.${task.id}.dependsOn`, blocking: true });
    for (const id of task.dependsOn) if (!taskIds.has(id)) issues.push({ code: 'unknown_dependency', message: `任务 ${task.id} 依赖不存在的任务 ${id}。`, path: `executionPlan.tasks.${task.id}.dependsOn`, blocking: true });
    for (const id of task.requirementIds) if (!requirementIds.has(id)) issues.push({ code: 'unknown_requirement', message: `任务 ${task.id} 引用了不存在的需求 ${id}。`, path: `executionPlan.tasks.${task.id}.requirementIds`, blocking: true });
    for (const id of task.designSectionIds) if (!designSectionIds.has(id)) issues.push({ code: 'unknown_design_section', message: `任务 ${task.id} 引用了不存在的设计章节 ${id}。`, path: `executionPlan.tasks.${task.id}.designSectionIds`, blocking: true });
    if (task.status === 'blocked' && !task.blockedReason?.trim()) issues.push({ code: 'blocked_without_reason', message: `任务 ${task.id} 已阻塞但没有说明原因。`, path: `executionPlan.tasks.${task.id}.blockedReason`, blocking: false });
    if (task.acceptanceCriteria.length === 0 && task.type !== 'milestone') issues.push({ code: 'task_without_acceptance', message: `任务 ${task.id} 缺少验收条件。`, path: `executionPlan.tasks.${task.id}.acceptanceCriteria`, blocking: true });
  }
  for (const event of workspace.executionPlan.statusEvents) {
    if (!taskIds.has(event.taskId)) issues.push({ code: 'unknown_status_event_task', message: `状态事件 ${event.id} 引用了不存在的任务 ${event.taskId}。`, path: `executionPlan.statusEvents.${event.id}.taskId`, blocking: true });
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

  const graph = dependencyAnalysis(workspace.executionPlan.tasks);
  const topologicalOrder = graph.topologicalOrder;
  if (topologicalOrder.length !== workspace.executionPlan.tasks.length) issues.push({ code: 'dependency_cycle', message: '执行计划包含循环依赖，无法计算执行顺序。', path: 'executionPlan.tasks', blocking: true });
  for (const edge of graph.redundantEdges) {
    issues.push({ code: 'redundant_dependency', message: `任务 ${edge.target} 对 ${edge.source} 的依赖可由其他路径推导，必须移除冗余直接边。`, path: `executionPlan.tasks.${edge.target}.dependsOn`, blocking: true });
  }
  if (graph.longestChainLength > 10) issues.push({ code: 'dependency_chain_too_long', message: `执行计划最长依赖链为 ${graph.longestChainLength}，超过建议上限 10。`, path: 'executionPlan.tasks', blocking: false });
  if (workspace.executionPlan.tasks.length > Math.max(12, workspace.requirements.items.length * 3)) issues.push({ code: 'plan_too_dense', message: '任务总量相对需求数量过密，请合并机械性步骤。', path: 'executionPlan.tasks', blocking: false });
  for (const requirement of workspace.requirements.items) {
    const count = workspace.executionPlan.tasks.filter((task) => task.requirementIds.includes(requirement.id)).length;
    if (count > 8) issues.push({ code: 'requirement_task_density_high', message: `需求 ${requirement.id} 直接关联 ${count} 个任务，超过建议上限 8。`, path: `requirements.items.${requirement.id}`, blocking: false });
  }
  for (const task of workspace.executionPlan.tasks) {
    if (new Set(task.dependsOn).size > 4) issues.push({ code: 'task_dependency_fan_in_high', message: `任务 ${task.id} 有 ${new Set(task.dependsOn).size} 个直接前置，超过建议上限 4。`, path: `executionPlan.tasks.${task.id}.dependsOn`, blocking: false });
    if (new Set(task.requirementIds).size > 3) issues.push({ code: 'task_requirement_fan_out_high', message: `任务 ${task.id} 关联 ${new Set(task.requirementIds).size} 个需求，超过建议上限 3。`, path: `executionPlan.tasks.${task.id}.requirementIds`, blocking: false });
  }

  const designedRequirements = new Set(workspace.design.sections.flatMap((section) => section.requirementIds));
  const plannedRequirements = new Set(workspace.executionPlan.tasks.flatMap((task) => task.requirementIds));
  const blocking = issues.some((issue) => issue.blocking);
  const approved = workspace.requirements.status === 'approved' && workspace.design.status === 'approved' && workspace.executionPlan.status === 'approved';
  return {
    valid: !blocking,
    ready: !blocking && approved,
    issues,
    topologicalOrder,
    readyTaskIds: readyTaskIds(workspace.executionPlan.tasks),
    graph: { redundantEdges: graph.redundantEdges.map(({ source, target }) => ({ source, target })), longestChainLength: graph.longestChainLength },
    traceability: { requirementCount: requirementIds.size, requirementsWithDesign: designedRequirements.size, requirementsWithTasks: plannedRequirements.size }
  };
}

function isDevelopmentProject(projectType: string | undefined): boolean {
  const normalized = projectType?.trim().toLowerCase() ?? '';
  return ['开发', '软件', '游戏', '网站', '应用', 'development', 'software', 'game', 'web', 'app'].some((marker) => normalized.includes(marker));
}

export function designBlocks(workspace: SolutionWorkspace): DesignContentBlock[] {
  return [
    ...(workspace.design.blocks ?? []),
    ...workspace.design.sections.flatMap((section) => section.blocks ?? [])
  ];
}

export function validateDesignBlockExpectations(workspace: SolutionWorkspace, expectations: DesignBlockExpectation[]): ValidationIssue[] {
  const blocks = new Map(designBlocks(workspace).map((block) => [block.id, block]));
  const issues: ValidationIssue[] = [];
  for (const expectation of expectations) {
    assertIdentifier(expectation.id, 'expectedDesignBlocks.id');
    if (!['text', 'architecture', 'flowchart', 'ui-svg'].includes(expectation.type)) throw new Error(`Expected design block ${expectation.id} type is invalid.`);
    const block = blocks.get(expectation.id);
    if (!block) {
      issues.push({ code: 'expected_design_block_missing', message: `交付校验未找到设计块 ${expectation.id}。`, path: 'design.blocks', blocking: true });
    } else if (block.type !== expectation.type) {
      issues.push({ code: 'expected_design_block_type_mismatch', message: `设计块 ${expectation.id} 应为 ${expectation.type}，实际为 ${block.type}。`, path: `design.blocks.${expectation.id}.type`, blocking: true });
    } else if (!block.content.trim()) {
      issues.push({ code: 'expected_design_block_empty', message: `设计块 ${expectation.id} 内容为空。`, path: `design.blocks.${expectation.id}.content`, blocking: true });
    }
  }
  return issues;
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
    moduleCount: workspace.requirements.modules.length,
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
  lines.push('## 项目基本信息', '', `- 项目类型：${workspace.projectProfile.projectType || '未填写'}`, `- 交付形态：${workspace.projectProfile.deliveryForm || '未填写'}`, `- 目标平台：${workspace.projectProfile.targetPlatforms.join('、') || '未填写'}`, '', '### 项目背景', '', workspace.projectProfile.background || '未填写', '', '### 整体描述', '', workspace.projectProfile.overview || '未填写', '');
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
  lines.push('### 功能模块与结构化需求', '');
  for (const module of [...workspace.requirements.modules].sort((left, right) => left.order - right.order)) {
    lines.push(`#### ${module.id} · ${module.title}`, '', module.description, '', `- 模块类型：${module.kind}`, '');
    for (const item of workspace.requirements.items.filter((candidate) => candidate.moduleId === module.id)) {
      lines.push(`##### ${item.id} · ${item.title}`, '', item.description, '', `- 需求类型：${item.kind}`, `- 父需求：${item.parentRequirementId ?? '无'}`, `- 优先级：${requirementPriorityLabels[item.priority]}`, `- 设计方案：${item.selectedDesignSectionId ?? '尚未创建'}`, '', '**验收条件**', '', ...item.acceptanceCriteria.map((criterion) => `- ${criterion}`), '');
    }
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
    lines.push(`### ${section.id} · ${section.title}`, '', section.body, '', `所属模块：${section.moduleId}`, `回应需求：${section.requirementIds.join(', ') || '无'}`, '');
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
    lines.push(`### ${task.id} · ${task.title}`, '', task.description, '', `- 模块：${task.moduleId}`, `- 类型：${task.type}`, `- 阶段：${task.phase || '未分阶段'}`, `- 状态：${task.status}`, `- 前置：${task.dependsOn.join(', ') || '无'}`, `- 关联需求：${task.requirementIds.join(', ') || '无'}`, `- 关联设计：${task.designSectionIds.join(', ') || '无'}`, '');
    if (task.deliverables.length > 0) lines.push('**交付物**', '', ...task.deliverables.map((item) => `- ${item}`), '');
    if (task.acceptanceCriteria.length > 0) lines.push('**验收条件**', '', ...task.acceptanceCriteria.map((criterion) => `- ${criterion}`), '');
  }
  return `${lines.join('\n').trim()}\n`;
}
