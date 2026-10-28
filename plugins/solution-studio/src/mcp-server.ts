import { createHash, randomUUID } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { assertSolutionWorkspace, validateDesignBlockExpectations, validateWorkspace, workspaceSummary, workspaceToMarkdown, type DesignBlockExpectation, type DesignContentBlock, type ProjectProfile, type RequirementsDocument, type SolutionDesignDocument, type ExecutionPlan, type SourceMode, type TaskStatus, type ValidationIssue } from './schema.js';
import { SolutionWorkspaceStore } from './store.js';
import { readHostRuntimeContext } from './runtime-context.js';
import { executionPlanInputSchema, executionTaskInputSchema, normalizeAuthoringPlan } from './execution-plan-input.js';

const store = new SolutionWorkspaceStore();
const runtimeContext = readHostRuntimeContext();
await store.initialize();

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 120_000
};

const gate = (...skills: string[]) => ({
  ...policy,
  'chatos/skillGate': { allOf: ['solution-studio', ...skills] }
});

const workspaceIdentityProperties = {
  title: { type: 'string', minLength: 1, maxLength: 240 },
  sourceMode: { type: 'string', enum: ['existing-project', 'greenfield'] }
} as const;

const projectProfileSchema = {
  type: 'object',
  properties: {
    background: { type: 'string' }, overview: { type: 'string' }, projectType: { type: 'string' },
    deliveryForm: { type: 'string' }, targetPlatforms: { type: 'array', items: { type: 'string' } }
  },
  required: ['background', 'overview', 'projectType', 'deliveryForm', 'targetPlatforms'],
  additionalProperties: false
} as const;

type MCPRole = 'authoring' | 'executor';
const role = process.env.SOLUTION_STUDIO_MCP_ROLE as MCPRole;
if (role !== 'authoring' && role !== 'executor') throw new Error('SOLUTION_STUDIO_MCP_ROLE must be authoring or executor.');

const AUTHORING_TOOL_DEFINITIONS = [
  {
    name: 'solution_get_active_context',
    description: 'Read the injected project context and its single current project plan. Use this before creating or revising planning artifacts.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: gate()
  },
  {
    name: 'solution_get_workspace',
    description: 'Read the current project plan, optionally checking it by workspaceId.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' } }, additionalProperties: false },
    _meta: gate()
  },
  {
    name: 'solution_archive_workspace_and_create',
    description: 'Archive the complete current workspace as immutable history and create a blank active workspace for a full re-plan. Use only when the user explicitly asks to discard the current planning direction and start over; never use this for an incremental revision.',
    inputSchema: {
      type: 'object', properties: {
        workspaceId: { type: 'string' }, expectedRevision: { type: 'integer', minimum: 0 },
        newTitle: { type: 'string', minLength: 1, maxLength: 240 }, sourceMode: { type: 'string', enum: ['existing-project', 'greenfield'] },
        reason: { type: 'string', minLength: 1, maxLength: 2000 }, confirmation: { type: 'string', enum: ['archive_and_restart'] }
      }, required: ['workspaceId', 'expectedRevision', 'newTitle', 'sourceMode', 'reason', 'confirmation'], additionalProperties: false
    },
    _meta: gate()
  },
  {
    name: 'solution_upsert_requirements',
    description: 'Create or revise the structured requirements for an existing or greenfield project. Preserve verified evidence separately from assumptions and open questions.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, description: { type: 'string', maxLength: 100000 }, projectProfile: projectProfileSchema, requirements: { type: 'object', description: 'Complete RequirementsDocument business content.' } },
      required: ['title', 'description', 'projectProfile', 'requirements'], additionalProperties: false
    },
    _meta: gate('solution-discovery')
  },
  {
    name: 'solution_upsert_design',
    description: 'Create or revise the solution design inside Solution Studio isolated storage. Visual blocks must contain inline SVG; project-directory files are not accepted as deliverables.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, design: { type: 'object', description: 'Complete SolutionDesignDocument business content.' } },
      required: ['title', 'design'], additionalProperties: false
    },
    _meta: gate('solution-design')
  },
  {
    name: 'solution_upsert_design_block',
    description: 'Atomically create or replace one inline design block in the saved Solution Studio workspace. Use this for substantial SVG blocks instead of writing .svg or .md deliverables into the project directory.',
    inputSchema: {
      type: 'object', properties: {
        workspaceId: { type: 'string' }, expectedRevision: { type: 'integer', minimum: 0 },
        sectionId: { type: ['string', 'null'], description: 'Design section ID, or null for a project-level design block.' },
        block: { type: 'object', properties: { id: { type: 'string' }, type: { type: 'string', enum: ['text', 'architecture', 'flowchart', 'ui-svg'] }, title: { type: 'string' }, content: { type: 'string' } }, required: ['id', 'type', 'title', 'content'], additionalProperties: false }
      }, required: ['workspaceId', 'expectedRevision', 'sectionId', 'block'], additionalProperties: false
    },
    _meta: gate('solution-design')
  },
  {
    name: 'solution_upsert_execution_plan',
    description: 'Create or revise a dependency-aware execution plan. tasks[].dependsOn is canonical; the service rejects unknown references, self-dependencies, and dependency cycles.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, executionPlan: executionPlanInputSchema },
      required: ['title', 'executionPlan'], additionalProperties: false
    },
    _meta: gate('solution-execution-plan')
  },
  {
    name: 'solution_append_execution_tasks',
    description: 'Atomically append new planned tasks to the existing execution plan without resubmitting or rewriting completed task definitions. Use this when a design increment adds work to a plan that already has execution history.',
    inputSchema: {
      type: 'object',
      properties: {
        workspaceId: { type: 'string' }, expectedRevision: { type: 'integer', minimum: 0 },
        basedOnDesignRevision: { type: 'integer', minimum: 0 },
        tasks: { type: 'array', minItems: 1, maxItems: 2_000, items: executionTaskInputSchema }
      },
      required: ['workspaceId', 'expectedRevision', 'basedOnDesignRevision', 'tasks'],
      additionalProperties: false
    },
    _meta: gate('solution-execution-plan')
  },
  {
    name: 'solution_validate',
    description: 'Validate document completeness, requirement-to-design-to-task traceability, dependency integrity, topological order, and the current ready-to-execute task set.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' } }, additionalProperties: false },
    _meta: gate('solution-validation')
  },
  {
    name: 'solution_export',
    description: 'Export a solution workspace as structured JSON or readable Markdown after validation.',
    inputSchema: {
      type: 'object', properties: { workspaceId: { type: 'string' }, format: { type: 'string', enum: ['json', 'markdown'] } },
      required: ['workspaceId', 'format'], additionalProperties: false
    },
    _meta: { ...gate('solution-validation'), 'chatos/requiredPermissions': ['artifact.create'] }
  },
  {
    name: 'solution_finalize',
    description: 'Final delivery gate for Solution Studio. Re-read the saved workspace, verify requested block IDs and delivery scope, create an internal Markdown artifact, and return a completion proof. A planning mutation is not complete until this succeeds.',
    inputSchema: {
      type: 'object', properties: {
        workspaceId: { type: 'string' }, expectedRevision: { type: 'integer', minimum: 0 },
        scope: { type: 'string', enum: ['requirements', 'design', 'plan', 'full'] },
        expectedDesignBlocks: { type: 'array', items: { type: 'object', properties: { id: { type: 'string' }, type: { type: 'string', enum: ['text', 'architecture', 'flowchart', 'ui-svg'] } }, required: ['id', 'type'], additionalProperties: false } }
      }, required: ['workspaceId', 'expectedRevision', 'scope', 'expectedDesignBlocks'], additionalProperties: false
    },
    _meta: { ...gate('solution-validation'), 'chatos/requiredPermissions': ['artifact.create'] }
  }
] as const;

const executorGate = () => ({
  ...policy,
  'chatos/skillGate': { allOf: ['solution-studio', 'solution-task-execution'] }
});

const executorIdentityProperties = {
  workspaceId: { type: 'string' },
  taskId: { type: 'string' },
  expectedRevision: { type: 'integer', minimum: 0 }
} as const;

const EXECUTOR_TOOL_DEFINITIONS = [
  {
    name: 'solution_get_active_context',
    description: 'Read the active project context and current Solution Studio workspace summary before selecting work.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: executorGate()
  },
  {
    name: 'solution_get_workspace_summary',
    description: 'Read the workspace summary, validation result, modules, and ready task IDs without exposing authoring mutations.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' } }, additionalProperties: false },
    _meta: executorGate()
  },
  {
    name: 'solution_list_tasks',
    description: 'List executable task definitions. Filter by module, status, or ready state.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' }, moduleId: { type: 'string' }, status: { type: 'string', enum: ['planned', 'in_progress', 'blocked', 'done', 'cancelled'] }, readyOnly: { type: 'boolean' } }, additionalProperties: false },
    _meta: executorGate()
  },
  {
    name: 'solution_get_task',
    description: 'Read one task, its prerequisites, acceptance criteria, and current execution state.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' }, taskId: { type: 'string' } }, required: ['workspaceId', 'taskId'], additionalProperties: false },
    _meta: executorGate()
  },
  ...(['claim', 'resume', 'block', 'complete', 'cancel'] as const).map((action) => ({
    name: `solution_${action}_task`,
    description: ({
      claim: 'Claim a ready planned task and move it to in_progress.',
      resume: 'Resume a blocked task after its blocking condition is resolved.',
      block: 'Mark an in-progress task blocked with a concrete reason.',
      complete: 'Complete an in-progress task with a verification summary and optional evidence.',
      cancel: 'Cancel a planned, in-progress, or blocked task with a concrete reason.'
    } as const)[action],
    inputSchema: {
      type: 'object',
      properties: {
        ...executorIdentityProperties,
        reason: { type: 'string', minLength: 1, maxLength: 4000 },
        summary: { type: 'string', minLength: 1, maxLength: 12000 },
        evidence: { type: 'array', items: { type: 'string', minLength: 1, maxLength: 4000 }, maxItems: 64 }
      },
      required: ['workspaceId', 'taskId', 'expectedRevision', ...(['block', 'cancel'].includes(action) ? ['reason'] : []), ...(action === 'complete' ? ['summary'] : [])],
      additionalProperties: false
    },
    _meta: executorGate()
  }))
] as const;

const TOOL_DEFINITIONS = role === 'authoring' ? AUTHORING_TOOL_DEFINITIONS : EXECUTOR_TOOL_DEFINITIONS;
const ALLOWED_TOOL_NAMES = new Set<string>(TOOL_DEFINITIONS.map((tool) => tool.name));

function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

function runtimeScope() {
  return runtimeContext;
}

async function resolveWorkspace(argumentsValue: Record<string, unknown>) {
  if (typeof argumentsValue.workspaceId === 'string') return store.read(argumentsValue.workspaceId);
  const current = await store.getCurrent();
  if (current) return current;
  throw new Error('The active ChatOS project does not have a Solution Studio plan yet.');
}

function effectiveSourceMode(value: unknown): SourceMode {
  if (value === undefined) return runtimeContext.sourceModeHint;
  if (value !== 'existing-project' && value !== 'greenfield') throw new Error('sourceMode is invalid.');
  return value;
}

async function exportWorkspace(workspaceId: string, format: 'json' | 'markdown') {
  const workspace = await store.read(workspaceId);
  const validation = validateWorkspace(workspace);
  const directory = path.resolve(process.env.CHATOS_PLUGIN_ARTIFACT_DIR ?? path.join(process.cwd(), 'exports'));
  await fs.mkdir(directory, { recursive: true });
  const safeTitle = workspace.title.replace(/[^a-zA-Z0-9\u4e00-\u9fff_-]+/g, '-').replace(/^-+|-+$/g, '') || workspace.workspaceId;
  const extension = format === 'json' ? 'solution.json' : 'md';
  const relativePath = `${safeTitle}-${workspace.workspaceId}.${extension}`;
  const body = format === 'json' ? `${JSON.stringify(workspace, null, 2)}\n` : workspaceToMarkdown(workspace);
  const bytes = Buffer.from(body, 'utf8');
  await fs.writeFile(path.join(directory, relativePath), bytes, { mode: 0o600 });
  return {
    relativePath,
    mimeType: format === 'json' ? 'application/vnd.chatos.solution-workspace+json' : 'text/markdown',
    size: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    validation
  };
}

function completionRequirement(workspaceId: string) {
  return { id: `solution-studio:${workspaceId}`, verifier: 'solution_finalize' };
}

function normalizeDesignLinks(requirements: RequirementsDocument, design: SolutionDesignDocument): void {
  for (const requirement of requirements.items) {
    const sections = design.sections.filter((section) => section.requirementIds.includes(requirement.id));
    requirement.selectedDesignSectionId = sections.length === 1 ? sections[0].id : undefined;
  }
}

function taskDefinition(task: ExecutionPlan['tasks'][number]) {
  const { status: _status, blockedReason: _blockedReason, ...definition } = task;
  return definition;
}

function protectedAuthoringPlan(current: ExecutionPlan, proposed: ExecutionPlan): ExecutionPlan {
  if (proposed.statusEvents.length !== 0) throw new Error('方案创建器不能提交任务状态事件。');
  const currentById = new Map(current.tasks.map((task) => [task.id, task]));
  const proposedById = new Map(proposed.tasks.map((task) => [task.id, task]));
  for (const task of proposed.tasks) {
    const previous = currentById.get(task.id);
    if (!previous && task.status !== 'planned') throw new Error(`新任务 ${task.id} 的状态必须是 planned。`);
    if (previous && previous.status !== 'planned') {
      if (!isDeepStrictEqual(taskDefinition(previous), taskDefinition(task))) {
        throw new Error(`任务 ${task.id} 已进入 ${previous.status}，方案创建器不能修改其定义或依赖。`);
      }
    }
  }
  for (const task of current.tasks) {
    if (task.status !== 'planned' && !proposedById.has(task.id)) throw new Error(`任务 ${task.id} 已进入 ${task.status}，方案创建器不能删除。`);
  }
  return {
    ...proposed,
    tasks: proposed.tasks.map((task) => {
      const previous = currentById.get(task.id);
      return previous ? { ...task, status: previous.status, blockedReason: previous.blockedReason } : { ...task, status: 'planned', blockedReason: undefined };
    }),
    statusEvents: structuredClone(current.statusEvents)
  };
}

type ExecutionAction = 'claim' | 'resume' | 'block' | 'complete' | 'cancel';

function targetStatus(action: ExecutionAction): TaskStatus {
  return action === 'block' ? 'blocked' : action === 'complete' ? 'done' : action === 'cancel' ? 'cancelled' : 'in_progress';
}

function applyExecutionAction(workspace: Awaited<ReturnType<typeof store.read>>, action: ExecutionAction, input: Record<string, unknown>) {
  const task = workspace.executionPlan.tasks.find((candidate) => candidate.id === String(input.taskId));
  if (!task) throw new Error(`Task not found: ${String(input.taskId)}`);
  const allowed: Record<ExecutionAction, TaskStatus[]> = {
    claim: ['planned'], resume: ['blocked'], block: ['in_progress'], complete: ['in_progress'], cancel: ['planned', 'in_progress', 'blocked']
  };
  if (!allowed[action].includes(task.status)) throw new Error(`任务 ${task.id} 不能从 ${task.status} 执行 ${action}。`);
  if ((action === 'claim' || action === 'resume') && !task.dependsOn.every((id) => workspace.executionPlan.tasks.find((candidate) => candidate.id === id)?.status === 'done')) {
    throw new Error(`任务 ${task.id} 的前置任务尚未全部完成，不能领取。`);
  }
  const reason = typeof input.reason === 'string' ? input.reason.trim() : '';
  const summary = typeof input.summary === 'string' ? input.summary.trim() : '';
  if ((action === 'block' || action === 'cancel') && !reason) throw new Error(`${action} 必须提供原因。`);
  if (action === 'complete' && !summary) throw new Error('complete 必须提供完成摘要。');
  const from = task.status;
  const to = targetStatus(action);
  task.status = to;
  task.blockedReason = to === 'blocked' ? reason : undefined;
  workspace.executionPlan.statusEvents.push({
    id: `event-${randomUUID()}`,
    taskId: task.id,
    from,
    to,
    occurredAt: new Date().toISOString(),
    runtimeSessionId: process.env.CHATOS_PLUGIN_RUNTIME_SESSION_ID ?? 'standalone-executor',
    ...(reason ? { reason } : {}),
    ...(summary ? { summary } : {}),
    evidence: Array.isArray(input.evidence) ? input.evidence.map(String) : []
  });
  workspace.executionPlan.revision += 1;
  workspace.executionPlan.updatedAt = new Date().toISOString();
  return task;
}

function blockingIssuesForScope(issues: ValidationIssue[], scope: string): ValidationIssue[] {
  const planCodes = new Set(['plan_empty', 'requirement_without_task', 'stale_plan', 'unknown_dependency', 'self_dependency', 'dependency_cycle', 'task_without_acceptance', 'unknown_design_section']);
  const designCodes = new Set(['design_empty', 'project_design_missing', 'project_architecture_missing', 'design_section_document_missing', 'design_section_visual_missing', 'requirement_without_design', 'stale_design', 'multiple_designs_per_requirement', 'design_link_mismatch', 'invalid_selected_design']);
  return issues.filter((issue) => issue.blocking && (
    scope === 'full'
      || (scope === 'plan' && !designCodes.has(issue.code))
      || (scope === 'design' && !planCodes.has(issue.code))
      || (scope === 'requirements' && !planCodes.has(issue.code) && !designCodes.has(issue.code))
  ));
}

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  if (!ALLOWED_TOOL_NAMES.has(name)) throw new Error(`Tool ${name} is not available in the ${role} MCP component.`);
  const input = objectArguments(rawArguments);
  switch (name) {
    case 'solution_get_active_context':
      return { scope: runtimeScope(), workspace: await store.getCurrent().then((workspace) => workspace ? workspaceSummary(workspace) : null) };
    case 'solution_get_workspace': {
      const workspace = await resolveWorkspace(input);
      return { workspace, validation: validateWorkspace(workspace) };
    }
    case 'solution_archive_workspace_and_create': {
      if (input.confirmation !== 'archive_and_restart') throw new Error('必须明确确认归档当前方案并重新规划。');
      const result = await store.archiveAndCreate({
        workspaceId: String(input.workspaceId), expectedRevision: Number(input.expectedRevision),
        title: String(input.newTitle), sourceMode: effectiveSourceMode(input.sourceMode), reason: String(input.reason)
      });
      return {
        archived: result.archived,
        workspace: workspaceSummary(result.workspace),
        validation: validateWorkspace(result.workspace),
        message: '旧方案已归档为只读历史，新活动方案已创建且不继承旧需求、设计、任务或状态。'
      };
    }
    case 'solution_get_workspace_summary': {
      const workspace = await resolveWorkspace(input);
      const validation = validateWorkspace(workspace);
      return { workspace: workspaceSummary(workspace), modules: workspace.requirements.modules, validation };
    }
    case 'solution_list_tasks': {
      const workspace = await resolveWorkspace(input);
      const ready = new Set(validateWorkspace(workspace).readyTaskIds);
      const tasks = workspace.executionPlan.tasks.filter((task) =>
        (typeof input.moduleId !== 'string' || task.moduleId === input.moduleId)
        && (typeof input.status !== 'string' || task.status === input.status)
        && (input.readyOnly !== true || ready.has(task.id)));
      return { workspace: workspaceSummary(workspace), tasks, readyTaskIds: tasks.filter((task) => ready.has(task.id)).map((task) => task.id) };
    }
    case 'solution_get_task': {
      const workspace = await store.read(String(input.workspaceId));
      const task = workspace.executionPlan.tasks.find((candidate) => candidate.id === String(input.taskId));
      if (!task) throw new Error(`Task not found: ${String(input.taskId)}`);
      return { workspace: workspaceSummary(workspace), task, prerequisiteTasks: task.dependsOn.map((id) => workspace.executionPlan.tasks.find((candidate) => candidate.id === id)).filter(Boolean) };
    }
    case 'solution_upsert_requirements': {
      const requirements = input.requirements as RequirementsDocument;
      const result = await store.upsert(String(input.title), effectiveSourceMode(input.sourceMode), (workspace) => {
        workspace.description = String(input.description);
        workspace.projectProfile = structuredClone(input.projectProfile as ProjectProfile);
        workspace.requirements = { ...requirements, revision: workspace.requirements.revision + 1, updatedAt: new Date().toISOString() };
        return workspace;
      });
      return { created: result.created, workspace: workspaceSummary(result.workspace), requirements: result.workspace.requirements, validation: validateWorkspace(result.workspace), completionRequirement: completionRequirement(result.workspace.workspaceId) };
    }
    case 'solution_upsert_design': {
      const design = input.design as SolutionDesignDocument;
      const result = await store.upsert(String(input.title), effectiveSourceMode(input.sourceMode), (workspace) => {
        workspace.design = { ...design, revision: workspace.design.revision + 1, updatedAt: new Date().toISOString() };
        normalizeDesignLinks(workspace.requirements, workspace.design);
        return workspace;
      });
      return { created: result.created, workspace: workspaceSummary(result.workspace), design: result.workspace.design, validation: validateWorkspace(result.workspace), completionRequirement: completionRequirement(result.workspace.workspaceId) };
    }
    case 'solution_upsert_design_block': {
      const workspace = await store.read(String(input.workspaceId));
      const block = structuredClone(input.block as DesignContentBlock);
      if (input.sectionId === null) {
        const blocks = workspace.design.blocks ?? [];
        workspace.design.blocks = [...blocks.filter((candidate) => candidate.id !== block.id), block];
      } else {
        const section = workspace.design.sections.find((candidate) => candidate.id === String(input.sectionId));
        if (!section) throw new Error(`Design section not found: ${String(input.sectionId)}`);
        section.blocks = [...(section.blocks ?? []).filter((candidate) => candidate.id !== block.id), block];
      }
      workspace.design.revision += 1;
      workspace.design.updatedAt = new Date().toISOString();
      normalizeDesignLinks(workspace.requirements, workspace.design);
      const saved = await store.replace(workspace, Number(input.expectedRevision));
      const savedBlocks = input.sectionId === null ? saved.design.blocks : saved.design.sections.find((section) => section.id === input.sectionId)?.blocks;
      return { workspace: workspaceSummary(saved), block: savedBlocks?.find((candidate) => candidate.id === block.id), validation: validateWorkspace(saved), completionRequirement: completionRequirement(saved.workspaceId) };
    }
    case 'solution_upsert_execution_plan': {
      const result = await store.upsert(String(input.title), effectiveSourceMode(input.sourceMode), (workspace) => {
        const executionPlan = normalizeAuthoringPlan(input.executionPlan, workspace.executionPlan);
        workspace.executionPlan = protectedAuthoringPlan(workspace.executionPlan, executionPlan);
        const validation = validateWorkspace(workspace);
        const dependencyIssues = validation.issues.filter((issue) => ['unknown_dependency', 'self_dependency', 'dependency_cycle', 'redundant_dependency'].includes(issue.code));
        if (dependencyIssues.length > 0) throw new Error(dependencyIssues.map((issue) => issue.message).join(' '));
        return workspace;
      });
      const validation = validateWorkspace(result.workspace);
      return { created: result.created, workspace: workspaceSummary(result.workspace), executionPlan: result.workspace.executionPlan, validation, completionRequirement: completionRequirement(result.workspace.workspaceId) };
    }
    case 'solution_append_execution_tasks': {
      const workspace = await store.read(String(input.workspaceId));
      const newTasks = structuredClone(input.tasks as ExecutionPlan['tasks']);
      const currentIds = new Set(workspace.executionPlan.tasks.map((task) => task.id));
      for (const task of newTasks) {
        if (currentIds.has(task.id)) throw new Error(`任务 ${task.id} 已存在；增量追加只能提交新任务。`);
        if (task.status !== 'planned') throw new Error(`新任务 ${task.id} 的状态必须是 planned。`);
        currentIds.add(task.id);
      }
      const executionPlan = normalizeAuthoringPlan({
        status: workspace.executionPlan.status,
        basedOnDesignRevision: Number(input.basedOnDesignRevision),
        objective: workspace.executionPlan.objective,
        tasks: [...workspace.executionPlan.tasks, ...newTasks]
      }, workspace.executionPlan);
      workspace.executionPlan = protectedAuthoringPlan(workspace.executionPlan, executionPlan);
      const validation = validateWorkspace(workspace);
      const dependencyIssues = validation.issues.filter((issue) => ['unknown_dependency', 'self_dependency', 'dependency_cycle', 'redundant_dependency'].includes(issue.code));
      if (dependencyIssues.length > 0) throw new Error(dependencyIssues.map((issue) => issue.message).join(' '));
      const saved = await store.replace(workspace, Number(input.expectedRevision));
      return {
        workspace: workspaceSummary(saved),
        appendedTaskIds: newTasks.map((task) => task.id),
        executionPlan: saved.executionPlan,
        validation: validateWorkspace(saved),
        completionRequirement: completionRequirement(saved.workspaceId)
      };
    }
    case 'solution_claim_task':
    case 'solution_resume_task':
    case 'solution_block_task':
    case 'solution_complete_task':
    case 'solution_cancel_task': {
      const workspace = await store.read(String(input.workspaceId));
      const action = name.slice('solution_'.length, -'_task'.length) as ExecutionAction;
      const task = applyExecutionAction(workspace, action, input);
      const saved = await store.replace(workspace, Number(input.expectedRevision));
      const validation = validateWorkspace(saved);
      return { workspace: workspaceSummary(saved), task: saved.executionPlan.tasks.find((candidate) => candidate.id === task.id), readyTaskIds: validation.readyTaskIds, validation };
    }
    case 'solution_validate': {
      const workspace = await resolveWorkspace(input);
      return { workspace: workspaceSummary(workspace), validation: validateWorkspace(workspace) };
    }
    case 'solution_export':
      return exportWorkspace(String(input.workspaceId), input.format === 'markdown' ? 'markdown' : 'json');
    case 'solution_finalize': {
      const workspace = await store.read(String(input.workspaceId));
      if (workspace.revision !== Number(input.expectedRevision)) throw new Error(`Solution workspace revision conflict. Current revision is ${workspace.revision}.`);
      const scope = String(input.scope);
      const expectedDesignBlocks = input.expectedDesignBlocks as DesignBlockExpectation[];
      const validation = validateWorkspace(workspace);
      const issues = [...blockingIssuesForScope(validation.issues, scope), ...validateDesignBlockExpectations(workspace, expectedDesignBlocks)];
      if (issues.length > 0) throw new Error(issues.map((issue) => issue.message).join(' '));
      const artifact = await exportWorkspace(workspace.workspaceId, 'markdown');
      return {
        ...artifact,
        workspace: workspaceSummary(workspace),
        scope,
        verifiedDesignBlocks: expectedDesignBlocks,
        completionProof: { ...completionRequirement(workspace.workspaceId), revision: workspace.revision, sha256: artifact.sha256 }
      };
    }
    default:
      throw new Error(`Unknown Solution Studio tool: ${name}`);
  }
}

function result(value: Record<string, unknown>, isError = false) {
  if (role === 'authoring' && !isError && value.validation && !value.completionProof) {
    const validation = value.validation as { issues: ValidationIssue[] };
    const codes = new Set(validation.issues.filter(issue => issue.blocking).map(issue => issue.code));
    value.continuation = {
      kind: 'authoring-work-remaining',
      instruction: 'Successful persistence is not final delivery. Validation gaps describe remaining authoring/repair work, not a terminal Task failure. Continue only the stages required by the user; do not expand a requirements-only request into a full plan.',
      suggestedActions: [
        ...(codes.has('design_empty') || codes.has('stale_design') ? [{ skill: 'solution-design', tool: 'solution_upsert_design', when: 'Design or full planning is requested', reason: 'Author or refresh the design against the current requirements revision.' }] : []),
        ...(codes.has('plan_empty') || codes.has('stale_plan') ? [{ skill: 'solution-execution-plan', tool: 'solution_upsert_execution_plan', when: 'Execution planning is requested; after the design is coherent', reason: 'Author or refresh tasks and dependencies against the current design revision.' }] : []),
        { skill: 'solution-validation', tool: 'solution_finalize', when: 'All requested content is saved, rendered where required, and its scoped validation gaps are repaired', reason: 'Finalize the actual requested scope at the current workspace revision.' }
      ]
    };
  }
  const response: Record<string, unknown> = {
    content: [{ type: 'text', text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
    isError
  };
  if (!isError && typeof value.relativePath === 'string') {
    response._meta = {
      'chatos/artifacts': [{
        producer_artifact_id: `solution_${String(value.sha256)}`,
        relative_path: value.relativePath,
        display_name: value.relativePath,
        media_type: value.mimeType,
        size_bytes: value.size,
        sha256: value.sha256
      }]
    };
  }
  return response;
}

const server = new Server({ name: `chatos-solution-studio-${role}`, version: '0.2.4' }, { capabilities: { tools: {} } });
server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: TOOL_DEFINITIONS }));
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  try { return result(await callTool(request.params.name, request.params.arguments)); }
  catch (error) { return result({ error: error instanceof Error ? error.message : String(error) }, true); }
});

await server.connect(new StdioServerTransport());
