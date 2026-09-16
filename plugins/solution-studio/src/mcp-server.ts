import { createHash } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { assertSolutionWorkspace, validateWorkspace, workspaceSummary, workspaceToMarkdown, type RequirementsDocument, type SolutionDesignDocument, type ExecutionPlan, type SourceMode, type TaskStatus } from './schema.js';
import { SolutionWorkspaceStore } from './store.js';
import { readHostRuntimeContext } from './runtime-context.js';

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
  artifactKey: { type: 'string', pattern: '^[A-Za-z0-9][A-Za-z0-9._:-]{0,159}$', description: 'Stable logical identity for one solution workspace inside the injected project scope.' },
  title: { type: 'string', minLength: 1, maxLength: 240 },
  sourceMode: { type: 'string', enum: ['existing-project', 'greenfield'] }
} as const;

const TOOL_DEFINITIONS = [
  {
    name: 'solution_get_active_context',
    description: 'Read the injected project context and list existing Solution Studio workspaces. Use this before creating or revising planning artifacts.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'solution_get_workspace',
    description: 'Read one complete requirements, design, and execution-plan workspace by workspaceId or artifactKey.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' }, artifactKey: { type: 'string' } }, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'solution_upsert_requirements',
    description: 'Create or revise the structured requirements for an existing or greenfield project. Preserve verified evidence separately from assumptions and open questions.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, requirements: { type: 'object', description: 'Complete RequirementsDocument business content.' } },
      required: ['artifactKey', 'title', 'sourceMode', 'requirements'], additionalProperties: false
    },
    _meta: gate('solution-discovery')
  },
  {
    name: 'solution_upsert_design',
    description: 'Create or revise the solution design, linked to a concrete requirements revision and requirement IDs.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, design: { type: 'object', description: 'Complete SolutionDesignDocument business content.' } },
      required: ['artifactKey', 'title', 'sourceMode', 'design'], additionalProperties: false
    },
    _meta: gate('solution-design')
  },
  {
    name: 'solution_upsert_execution_plan',
    description: 'Create or revise a dependency-aware execution plan. tasks[].dependsOn is canonical; the service rejects unknown references, self-dependencies, and dependency cycles.',
    inputSchema: {
      type: 'object', properties: { ...workspaceIdentityProperties, executionPlan: { type: 'object', description: 'Complete ExecutionPlan business content including tasks and dependsOn relationships.' } },
      required: ['artifactKey', 'title', 'sourceMode', 'executionPlan'], additionalProperties: false
    },
    _meta: gate('solution-execution-plan')
  },
  {
    name: 'solution_update_task',
    description: 'Apply a focused task status update with optimistic revision control. This does not execute the task or broaden authorization.',
    inputSchema: {
      type: 'object', properties: {
        workspaceId: { type: 'string' }, expectedRevision: { type: 'integer', minimum: 0 }, taskId: { type: 'string' },
        status: { type: 'string', enum: ['planned', 'in_progress', 'blocked', 'done', 'cancelled'] }, blockedReason: { type: 'string', maxLength: 4000 }
      }, required: ['workspaceId', 'expectedRevision', 'taskId', 'status'], additionalProperties: false
    },
    _meta: gate('solution-execution-plan')
  },
  {
    name: 'solution_validate',
    description: 'Validate document completeness, requirement-to-design-to-task traceability, dependency integrity, topological order, and the current ready-to-execute task set.',
    inputSchema: { type: 'object', properties: { workspaceId: { type: 'string' }, artifactKey: { type: 'string' } }, additionalProperties: false },
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
  }
] as const;

function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

function runtimeScope() {
  return runtimeContext;
}

async function resolveWorkspace(argumentsValue: Record<string, unknown>) {
  if (typeof argumentsValue.workspaceId === 'string') return store.read(argumentsValue.workspaceId);
  if (typeof argumentsValue.artifactKey === 'string') {
    const workspace = await store.findByArtifactKey(argumentsValue.artifactKey);
    if (workspace) return workspace;
  }
  throw new Error('A matching Solution Studio workspace was not found.');
}

function requiredSourceMode(value: unknown): SourceMode {
  if (value !== 'existing-project' && value !== 'greenfield') throw new Error('sourceMode is invalid.');
  return value;
}

async function exportWorkspace(workspaceId: string, format: 'json' | 'markdown') {
  const workspace = await store.read(workspaceId);
  const validation = validateWorkspace(workspace);
  const directory = path.resolve(process.env.CHATOS_PLUGIN_ARTIFACT_DIR ?? process.env.SOLUTION_STUDIO_EXPORT_DIR ?? path.join(process.cwd(), 'exports'));
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

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const input = objectArguments(rawArguments);
  switch (name) {
    case 'solution_get_active_context':
      return { scope: runtimeScope(), workspaces: await store.list() };
    case 'solution_get_workspace': {
      const workspace = await resolveWorkspace(input);
      return { workspace, validation: validateWorkspace(workspace) };
    }
    case 'solution_upsert_requirements': {
      const requirements = input.requirements as RequirementsDocument;
      const result = await store.upsert(String(input.artifactKey), String(input.title), requiredSourceMode(input.sourceMode), (workspace) => {
        workspace.requirements = { ...requirements, revision: workspace.requirements.revision + 1, updatedAt: new Date().toISOString() };
        return workspace;
      });
      return { created: result.created, workspace: workspaceSummary(result.workspace), requirements: result.workspace.requirements, validation: validateWorkspace(result.workspace) };
    }
    case 'solution_upsert_design': {
      const design = input.design as SolutionDesignDocument;
      const result = await store.upsert(String(input.artifactKey), String(input.title), requiredSourceMode(input.sourceMode), (workspace) => {
        workspace.design = { ...design, revision: workspace.design.revision + 1, updatedAt: new Date().toISOString() };
        return workspace;
      });
      return { created: result.created, workspace: workspaceSummary(result.workspace), design: result.workspace.design, validation: validateWorkspace(result.workspace) };
    }
    case 'solution_upsert_execution_plan': {
      const executionPlan = input.executionPlan as ExecutionPlan;
      const result = await store.upsert(String(input.artifactKey), String(input.title), requiredSourceMode(input.sourceMode), (workspace) => {
        workspace.executionPlan = { ...executionPlan, revision: workspace.executionPlan.revision + 1, updatedAt: new Date().toISOString() };
        const validation = validateWorkspace(workspace);
        const dependencyIssues = validation.issues.filter((issue) => ['unknown_dependency', 'self_dependency', 'dependency_cycle'].includes(issue.code));
        if (dependencyIssues.length > 0) throw new Error(dependencyIssues.map((issue) => issue.message).join(' '));
        return workspace;
      });
      const validation = validateWorkspace(result.workspace);
      return { created: result.created, workspace: workspaceSummary(result.workspace), executionPlan: result.workspace.executionPlan, validation };
    }
    case 'solution_update_task': {
      const workspace = await store.read(String(input.workspaceId));
      const task = workspace.executionPlan.tasks.find((candidate) => candidate.id === String(input.taskId));
      if (!task) throw new Error(`Task not found: ${String(input.taskId)}`);
      task.status = input.status as TaskStatus;
      task.blockedReason = typeof input.blockedReason === 'string' && input.blockedReason.trim() ? input.blockedReason.trim() : undefined;
      workspace.executionPlan.revision += 1;
      workspace.executionPlan.updatedAt = new Date().toISOString();
      const saved = await store.replace(workspace, Number(input.expectedRevision));
      return { workspace: workspaceSummary(saved), task: saved.executionPlan.tasks.find((candidate) => candidate.id === task.id), validation: validateWorkspace(saved) };
    }
    case 'solution_validate': {
      const workspace = await resolveWorkspace(input);
      return { workspace: workspaceSummary(workspace), validation: validateWorkspace(workspace) };
    }
    case 'solution_export':
      return exportWorkspace(String(input.workspaceId), input.format === 'markdown' ? 'markdown' : 'json');
    default:
      throw new Error(`Unknown Solution Studio tool: ${name}`);
  }
}

function result(value: Record<string, unknown>, isError = false) {
  const response: Record<string, unknown> = {
    content: [{ type: 'text', text: JSON.stringify(value, null, 2) }],
    structuredContent: value,
    isError
  };
  if (!isError && typeof value.relativePath === 'string') {
    response._meta = {
      'chatos/artifacts': [{ producer_artifact_id: `solution_${String(value.sha256)}`, relative_path: value.relativePath, display_name: value.relativePath, mime_type: value.mimeType }]
    };
  }
  return response;
}

const server = new Server({ name: 'chatos-solution-studio', version: '0.1.0' }, { capabilities: { tools: {} } });
server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: TOOL_DEFINITIONS }));
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  try { return result(await callTool(request.params.name, request.params.arguments)); }
  catch (error) { return result({ error: error instanceof Error ? error.message : String(error) }, true); }
});

await server.connect(new StdioServerTransport());
