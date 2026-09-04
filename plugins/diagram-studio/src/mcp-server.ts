import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import {
  DiagramDocumentStore,
  RevisionConflictError,
  writeExportArtifact
} from './document-store.js';
import {
  applyDiagramPatch,
  assertDiagramDocument,
  diagramSummary,
  type DiagramKind,
  type DiagramPatchOperation
} from './schema.js';
import { layoutDiagram } from './layout.js';
import { hasEmbeddedDiagramLayout, plantUmlToDiagram } from './plantuml.js';
import { inspectDiagramQuality, type DiagramQualityProfile } from './quality.js';
import {
  inspectGenerationContract,
  prepareGenerationPermit,
  runtimeScopeFingerprint,
  verifyGenerationPermit,
  type GenerationPlan,
  type GenerationPermitPayload
} from './generation-guides.js';

const SERVER_NAME = 'chatos-diagram-studio';
const SERVER_VERSION = '0.2.1';
const store = new DiagramDocumentStore();
const scopeKey = runtimeScopeFingerprint(store.rootDirectory);

const policy = {
  'chatos/policyVersion': 1,
  'chatos/riskLevel': 'low',
  'chatos/approvalMode': 'none',
  'chatos/timeoutMs': 30_000,
  'chatos/toolResultMaxChars': 80_000
};

const diagramSkillGate = {
  'chatos/skillGate': {
    evidenceArgument: 'skillEvidence',
    allOf: ['diagram-studio'],
    selectByArgument: {
      pointer: '/kind',
      map: {
        architecture: 'diagram-architecture',
        flowchart: 'diagram-flowchart',
        swimlane: 'diagram-swimlane',
        topology: 'diagram-topology',
        sequence: 'diagram-sequence'
      }
    }
  }
};

const skillEvidenceProperty = {
  type: 'array',
  minItems: 2,
  maxItems: 8,
  items: { type: 'string', minLength: 1 },
  description: 'Activation evidence returned by the platform for diagram-studio and the selected diagram-kind Skill. The platform validates and removes this field before local execution.'
} as const;

const generatedDiagramProperties = {
  generationPermit: { type: 'string', minLength: 1, description: 'Signed permit returned by diagram_prepare_generation for this exact scope, kind, title, artifactKey, and plan.' },
  source: { type: 'string', minLength: 1, maxLength: 2_097_152, description: 'Complete PlantUML source using stable unique ASCII aliases for structural declarations.' },
  title: { type: 'string', minLength: 1, maxLength: 240, description: 'User-facing title bound to the generation permit.' },
  kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'], description: 'Explicit diagram kind bound to the generation permit.' },
  artifactKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$', description: 'Stable logical identity bound to the generation permit.' },
  idempotencyKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$', description: 'Stable key for retrying this exact write without creating a duplicate.' },
  layoutDirection: { type: 'string', enum: ['RIGHT', 'DOWN'], description: 'Optional primary reading direction. Defaults from the diagram kind.' },
  nodeEvidence: {
    type: 'array',
    maxItems: 500,
    description: 'Evidence for code-derived nodes, matched by stable PlantUML alias instead of shown in visible labels.',
    items: {
      type: 'object',
      properties: {
        alias: { type: 'string', minLength: 1, maxLength: 128 },
        sourceReferences: {
          type: 'array',
          minItems: 1,
          maxItems: 50,
          items: { type: 'string', minLength: 1, maxLength: 2000 }
        }
      },
      required: ['alias', 'sourceReferences'],
      additionalProperties: false
    }
  },
  requireSourceReferences: { type: 'boolean', default: false, description: 'When true, missing evidence blocks persistence.' },
  responseDetail: { type: 'string', enum: ['summary', 'document'], default: 'summary' }
} as const;

const TOOL_DEFINITIONS = [
  {
    name: 'diagram_prepare_generation',
    description: 'Submit one bounded diagram plan after activating the Diagram Studio router and the dedicated diagram-kind Skill. The current injected scope and artifactKey automatically determine whether this creates a new diagram or revises the existing logical diagram; do not supply an operation or documentId. Returns a scope-, skill-contract-, plan-, kind-, title-, and artifact-bound generationPermit only when the plan is valid.',
    inputSchema: {
      type: 'object',
      properties: {
        skillEvidence: skillEvidenceProperty,
        kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'] },
        mode: { type: 'string', minLength: 1, maxLength: 64 },
        artifactKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$', description: 'Stable logical identity in the injected scope. A new key creates a diagram; an existing key revises that diagram.' },
        title: { type: 'string', minLength: 1, maxLength: 240 },
        plan: {
          type: 'object',
          properties: {
            goal: { type: 'string', minLength: 1, maxLength: 1000 },
            scope: { type: 'string', minLength: 1, maxLength: 4000 },
            excludedDetails: { type: 'array', minItems: 1, maxItems: 100, items: { type: 'string', minLength: 1, maxLength: 1000 } },
            estimatedPrimaryItemCount: { type: 'integer', minimum: 0 },
            estimatedEdgeCount: { type: 'integer', minimum: 0 },
            structure: { type: 'array', maxItems: 100, items: { type: 'string', minLength: 1, maxLength: 1000 } },
            splitPlan: { type: 'array', maxItems: 100, items: { type: 'string', minLength: 1, maxLength: 1000 } },
            splitRationale: { type: 'string', minLength: 1, maxLength: 4000 },
            checklistAcknowledgements: { type: 'array', minItems: 1, maxItems: 100, items: { type: 'string', minLength: 1, maxLength: 128 } }
          },
          required: ['goal', 'scope', 'excludedDetails', 'estimatedPrimaryItemCount', 'estimatedEdgeCount', 'structure', 'splitPlan', 'splitRationale', 'checklistAcknowledgements'],
          additionalProperties: false
        }
      },
      required: ['skillEvidence', 'kind', 'artifactKey', 'title', 'plan'],
      additionalProperties: false
    },
    _meta: { ...policy, ...diagramSkillGate }
  },
  {
    name: 'diagram_commit_generation',
    description: 'Create or revise one editable generated diagram after the dedicated guide and plan gates. Parses PlantUML, lays it out, checks the guide contract and quality profile, records provenance, and persists only when ready. ChatOS project identity is injected; never pass a projectId.',
    inputSchema: {
      type: 'object',
      properties: { skillEvidence: skillEvidenceProperty, ...generatedDiagramProperties },
      required: ['skillEvidence', 'generationPermit', 'source', 'title', 'kind', 'artifactKey'],
      additionalProperties: false
    },
    _meta: { ...policy, ...diagramSkillGate }
  },
  {
    name: 'diagram_list_documents',
    description: 'List all diagram documents in the current injected ChatOS user and project or public scope. This tool does not accept a projectId and cannot switch scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'diagram_list_projects',
    description: 'List Diagram Studio UI classification projects inside the current injected ChatOS scope. These projects cannot switch the outer ChatOS user, project, workspace, or public scope.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false },
    _meta: policy
  },
  {
    name: 'diagram_create_project',
    description: 'Create an optional Diagram Studio UI classification project inside the current injected scope. This is not a required setup step for AI generation.',
    inputSchema: {
      type: 'object',
      properties: {
        name: { type: 'string', minLength: 1, maxLength: 240 },
        description: { type: 'string', maxLength: 4000 }
      },
      required: ['name'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_get_project',
    description: 'Read a Diagram Studio project and its diagram summaries.',
    inputSchema: {
      type: 'object',
      properties: { projectId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_update_project',
    description: 'Rename or update the description of a Diagram Studio project.',
    inputSchema: {
      type: 'object',
      properties: {
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        name: { type: 'string', minLength: 1, maxLength: 240 },
        description: { type: 'string', maxLength: 4000 }
      },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_delete_project',
    description: 'Delete a Diagram Studio project, optionally deleting all diagrams assigned to it.',
    inputSchema: {
      type: 'object',
      properties: {
        projectId: { type: 'string', minLength: 1, maxLength: 128 },
        deleteDocuments: { type: 'boolean', default: false }
      },
      required: ['projectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_create_document',
    description: 'Create an empty editable canvas in the current injected scope for manual drawing. It never inserts a built-in template and is not a setup step for generated PlantUML.',
    inputSchema: {
      type: 'object',
      properties: {
        kind: { type: 'string', enum: ['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'] },
        title: { type: 'string', minLength: 1, maxLength: 240 },
        artifactKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        idempotencyKey: { type: 'string', pattern: '^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$' },
        mode: { type: 'string', enum: ['upsert', 'create_new'], default: 'upsert' }
      },
      required: ['kind'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_import_plantuml',
    description: 'Import PlantUML as one generated editable diagram. A valid generationPermit and current Skill evidence are required; projectId is not accepted.',
    inputSchema: {
      type: 'object',
      properties: { skillEvidence: skillEvidenceProperty, ...generatedDiagramProperties },
      required: ['skillEvidence', 'generationPermit', 'source', 'title', 'kind', 'artifactKey'],
      additionalProperties: false
    },
    _meta: { ...policy, ...diagramSkillGate }
  },
  {
    name: 'diagram_move_document',
    description: 'Move a diagram between UI classification projects inside the current injected ChatOS scope. It cannot move a document across ChatOS users, projects, workspaces, or public scope.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        targetProjectId: { type: 'string', minLength: 1, maxLength: 128 },
        sourceProjectId: { type: 'string', minLength: 1, maxLength: 128 }
      },
      required: ['documentId', 'targetProjectId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_delete_document',
    description: 'Delete a diagram document and remove it from projects in the current ChatOS scope.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_get_document',
    description: 'Read the complete structured diagram document, including stable node and edge IDs.',
    inputSchema: {
      type: 'object',
      properties: { documentId: { type: 'string', minLength: 1, maxLength: 128 } },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_replace_document',
    description: 'Replace a complete diagram using optimistic revision control and a valid generationPermit for this document. Prefer diagram_apply_patch for focused changes.',
    inputSchema: {
      type: 'object',
      properties: {
        expectedRevision: { type: 'integer', minimum: 0 },
        document: { type: 'object' },
        generationPermit: { type: 'string', minLength: 1 }
      },
      required: ['expectedRevision', 'document', 'generationPermit'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_apply_patch',
    description: 'Apply focused changes without replacing unrelated content. A generationPermit is required only when adding/removing nodes or semantic edges; title, description, position, and viewport-only edits remain permit-free.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        generationPermit: { type: 'string', minLength: 1 },
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 1000,
          items: {
            type: 'object',
            properties: {
              op: {
                type: 'string',
                enum: ['set_title', 'set_description', 'upsert_node', 'remove_node', 'move_node', 'upsert_edge', 'remove_edge', 'set_viewport']
              }
            },
            required: ['op'],
            additionalProperties: true
          }
        }
      },
      required: ['documentId', 'expectedRevision', 'operations'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_auto_layout',
    description: 'Automatically arrange a diagram while preserving node IDs and semantic edges.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        expectedRevision: { type: 'integer', minimum: 0 },
        direction: { type: 'string', enum: ['RIGHT', 'DOWN'] }
      },
      required: ['documentId', 'expectedRevision'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_validate',
    description: 'Validate structural correctness and delivery readiness. Do not report completion unless ready is true; blocking warnings require simplifying, splitting, relayout, or adding missing evidence.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128, description: 'Diagram document to inspect.' },
        qualityProfile: { type: 'string', enum: ['balanced', 'architecture-overview', 'architecture-detail'], default: 'balanced', description: 'Readability budget. Use architecture-overview for a system overview and architecture-detail for one bounded context.' },
        requireSourceReferences: { type: 'boolean', default: false, description: 'When true, missing node sourceReferences block delivery readiness for code-derived diagrams.' }
      },
      required: ['documentId'],
      additionalProperties: false
    },
    _meta: policy
  },
  {
    name: 'diagram_export',
    description: 'Export a diagram as managed structured JSON, SVG, or PlantUML source.',
    inputSchema: {
      type: 'object',
      properties: {
        documentId: { type: 'string', minLength: 1, maxLength: 128 },
        format: { type: 'string', enum: ['json', 'svg', 'plantuml'] }
      },
      required: ['documentId', 'format'],
      additionalProperties: false
    },
    _meta: {
      ...policy,
      'chatos/requiredPermissions': ['artifact.create']
    }
  }
] as const;

function objectArguments(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Tool arguments must be an object.');
  return value as Record<string, unknown>;
}

function qualityProfile(value: unknown): DiagramQualityProfile {
  return value === 'architecture-overview' || value === 'architecture-detail' ? value : 'balanced';
}

function documentQualityProfile(
  document: Awaited<ReturnType<DiagramDocumentStore['read']>>,
  requested: unknown
): DiagramQualityProfile {
  const provenanceProfile = document.generationProvenance?.qualityProfile;
  if (provenanceProfile === 'architecture-overview' || provenanceProfile === 'architecture-detail' || provenanceProfile === 'balanced') {
    return provenanceProfile;
  }
  return qualityProfile(requested);
}

function importResult(
  document: Awaited<ReturnType<DiagramDocumentStore['read']>>,
  created: boolean,
  reused: boolean,
  argumentsValue: Record<string, unknown>,
  profile: DiagramQualityProfile
) {
  return {
    document: argumentsValue.responseDetail === 'document' ? document : diagramSummary(document),
    created,
    reused,
    quality: inspectDiagramQuality(document, profile, argumentsValue.requireSourceReferences === true)
  };
}

function requiredDiagramKind(value: unknown): DiagramKind {
  if (typeof value !== 'string' || !['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'].includes(value)) {
    throw new Error('A valid diagram kind is required.');
  }
  return value as DiagramKind;
}

async function currentScopedProject() {
  const projectScope = process.env.CHATOS_CONTEXT_SCOPE === 'project' && Boolean(process.env.CHATOS_PROJECT_ID);
  return store.ensureScopedProject(
    scopeKey,
    projectScope ? process.env.CHATOS_PROJECT_NAME?.trim() || 'ChatOS 项目图形' : '公共图形'
  );
}

function withGenerationProvenance(
  document: Awaited<ReturnType<DiagramDocumentStore['read']>>,
  permit: GenerationPermitPayload
) {
  document.generationProvenance = {
    guideId: permit.guideId,
    guideVersion: permit.guideVersion,
    guideHash: permit.guideHash,
    planHash: permit.planHash,
    permitId: permit.permitId,
    qualityProfile: permit.qualityProfile,
    generatedAt: new Date().toISOString()
  };
  return document;
}

function generationProvenance(permit: GenerationPermitPayload) {
  return {
    guideId: permit.guideId,
    guideVersion: permit.guideVersion,
    guideHash: permit.guideHash,
    planHash: permit.planHash,
    permitId: permit.permitId,
    qualityProfile: permit.qualityProfile,
    generatedAt: new Date().toISOString()
  };
}

function assertGenerationReady(
  document: Awaited<ReturnType<DiagramDocumentStore['read']>>,
  permit: GenerationPermitPayload,
  requireSourceReferences: boolean
) {
  const contractIssues = inspectGenerationContract(document, permit);
  const quality = inspectDiagramQuality(document, permit.qualityProfile, requireSourceReferences);
  if (contractIssues.length > 0 || !quality.ready) {
    throw new Error(`Generated diagram failed the dedicated guide and quality gates; it was not persisted. ${JSON.stringify({ contractIssues, quality })}`);
  }
  return { contractIssues, quality };
}

async function commitGeneratedDiagram(argumentsValue: Record<string, unknown>) {
  const kind = requiredDiagramKind(argumentsValue.kind);
  const source = String(argumentsValue.source);
  const title = String(argumentsValue.title);
  const artifactKey = String(argumentsValue.artifactKey);
  const generationPermit = String(argumentsValue.generationPermit);
  const permit = verifyGenerationPermit(generationPermit, { scopeFingerprint: scopeKey, kind, artifactKey, title });
  const imported = plantUmlToDiagram(source, { title, kind });
  if (imported.kind !== kind) throw new Error(`PlantUML produced ${imported.kind}, but the generation permit requires ${kind}.`);
  const direction = argumentsValue.layoutDirection === 'DOWN'
    ? 'DOWN'
    : argumentsValue.layoutDirection === 'RIGHT'
      ? 'RIGHT'
      : kind === 'flowchart' || kind === 'swimlane' || kind === 'sequence'
        ? 'DOWN'
        : 'RIGHT';
  const document = hasEmbeddedDiagramLayout(source) ? imported : await layoutDiagram(imported, direction);
  document.artifactKey = artifactKey;
  applyNodeEvidence(document, argumentsValue.nodeEvidence);
  withGenerationProvenance(document, permit);
  assertDiagramDocument(document);
  const readiness = assertGenerationReady(document, permit, argumentsValue.requireSourceReferences === true);
  const idempotencyKey = typeof argumentsValue.idempotencyKey === 'string' ? argumentsValue.idempotencyKey : undefined;

  if (permit.operation === 'revise') {
    if (!permit.documentId) throw new Error('Revision permit has no documentId.');
    const current = await store.readInScope(permit.documentId, scopeKey);
    if (current.kind !== kind) throw new Error(`Document ${current.documentId} is ${current.kind}, not ${kind}.`);
    if (current.artifactKey && current.artifactKey !== artifactKey) throw new Error('Revision artifactKey does not match the existing document.');
    if (current.generationProvenance?.permitId === permit.permitId) {
      return { ...importResult(current, false, true, argumentsValue, permit.qualityProfile), contractIssues: [] };
    }
    document.documentId = current.documentId;
    document.createdAt = current.createdAt;
    document.revision = current.revision;
    const saved = await store.replace(document, current.revision);
    return { ...importResult(saved, false, false, argumentsValue, permit.qualityProfile), contractIssues: readiness.contractIssues };
  }

  const project = await currentScopedProject();
  const write = await store.upsertInProject(project.projectId, document, artifactKey, idempotencyKey);
  return { ...importResult(write.document, write.created, write.reused, argumentsValue, permit.qualityProfile), contractIssues: readiness.contractIssues };
}

function runtimeScope(): Record<string, unknown> {
  const kind = process.env.CHATOS_CONTEXT_SCOPE ?? 'device';
  return {
    kind,
    shared: kind === 'device',
    ...(process.env.CHATOS_PROJECT_ID ? { chatosProjectId: process.env.CHATOS_PROJECT_ID } : {}),
    ...(process.env.CHATOS_PROJECT_NAME ? { chatosProjectName: process.env.CHATOS_PROJECT_NAME } : {}),
    ...(process.env.CHATOS_WORKSPACE_ID ? { workspaceId: process.env.CHATOS_WORKSPACE_ID } : {})
  };
}

async function callTool(name: string, rawArguments: unknown): Promise<Record<string, unknown>> {
  const argumentsValue = objectArguments(rawArguments);
  switch (name) {
    case 'diagram_prepare_generation': {
      const artifactKey = String(argumentsValue.artifactKey);
      const existing = (await store.listInScope(scopeKey)).find((document) => document.artifactKey === artifactKey);
      const operation = existing ? 'revise' : 'create';
      const documentId = existing?.documentId;
      const prepared = await prepareGenerationPermit({
        kind: requiredDiagramKind(argumentsValue.kind),
        mode: typeof argumentsValue.mode === 'string' ? argumentsValue.mode : undefined,
        artifactKey,
        operation,
        documentId,
        title: String(argumentsValue.title),
        plan: argumentsValue.plan as GenerationPlan,
        scopeFingerprint: scopeKey
      });
      if (documentId) {
        const current = await store.readInScope(documentId, scopeKey);
        if (current.kind !== prepared.permit.kind) throw new Error(`The selected document is ${current.kind}, but the guide is for ${prepared.permit.kind}.`);
        if (current.artifactKey && current.artifactKey !== prepared.permit.artifactKey) throw new Error('The requested artifactKey does not match the selected document.');
      }
      return {
        generationPermit: prepared.generationPermit,
        permitId: prepared.permit.permitId,
        kind: prepared.permit.kind,
        mode: prepared.permit.mode,
        artifactKey: prepared.permit.artifactKey,
        operation: prepared.permit.operation,
        ...(prepared.permit.documentId ? { documentId: prepared.permit.documentId } : {}),
        qualityProfile: prepared.permit.qualityProfile,
        budgets: {
          maxPrimaryItems: prepared.permit.maxPrimaryItems,
          maxEdges: prepared.permit.maxEdges,
          minStructureItems: prepared.permit.minStructureItems,
          ...(prepared.permit.maxStructureItems === undefined ? {} : { maxStructureItems: prepared.permit.maxStructureItems })
        },
        planHash: prepared.planHash,
        expiresAt: prepared.permit.expiresAt
      };
    }
    case 'diagram_commit_generation':
    case 'diagram_import_plantuml':
      return commitGeneratedDiagram(argumentsValue);
    case 'diagram_list_documents':
      return { scope: runtimeScope(), documents: await store.listInScope(scopeKey) };
    case 'diagram_list_projects':
      return {
        scope: runtimeScope(),
        projects: await store.listProjects(scopeKey)
      };
    case 'diagram_create_project': {
      const project = await store.createProject(
        String(argumentsValue.name),
        typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined,
        scopeKey
      );
      return { scope: runtimeScope(), project };
    }
    case 'diagram_get_project': {
      const projectId = String(argumentsValue.projectId);
      return {
        scope: runtimeScope(),
        project: await store.readProjectInScope(projectId, scopeKey),
        documents: await store.listInProject(projectId, scopeKey)
      };
    }
    case 'diagram_update_project': {
      await store.readProjectInScope(String(argumentsValue.projectId), scopeKey);
      const project = await store.updateProject(String(argumentsValue.projectId), {
        name: typeof argumentsValue.name === 'string' ? argumentsValue.name : undefined,
        description: typeof argumentsValue.description === 'string' ? argumentsValue.description : undefined
      });
      return { project };
    }
    case 'diagram_delete_project': {
      const project = await store.readProjectInScope(String(argumentsValue.projectId), scopeKey);
      if (project.isScopeDefault) throw new Error('The default project for the current ChatOS scope cannot be deleted. Move or delete its diagrams instead.');
      await store.deleteProject(
        String(argumentsValue.projectId),
        argumentsValue.deleteDocuments === true
      );
      return { deleted: true, projectId: String(argumentsValue.projectId) };
    }
    case 'diagram_create_document': {
      const kind = requiredDiagramKind(argumentsValue.kind);
      const title = typeof argumentsValue.title === 'string' ? argumentsValue.title : undefined;
      const artifactKey = typeof argumentsValue.artifactKey === 'string' ? argumentsValue.artifactKey : undefined;
      const idempotencyKey = typeof argumentsValue.idempotencyKey === 'string' ? argumentsValue.idempotencyKey : undefined;
      const project = await currentScopedProject();
      const useUpsert = artifactKey && argumentsValue.mode !== 'create_new';
      if (useUpsert) {
        const write = await store.createOrGetInProject(project.projectId, kind, title, true, artifactKey, idempotencyKey);
        return { document: diagramSummary(write.document), created: write.created, reused: write.reused };
      }
      if (idempotencyKey) {
        const write = await store.createNewInProjectIdempotent(project.projectId, kind, title, true, idempotencyKey);
        return { document: diagramSummary(write.document), created: write.created, reused: write.reused };
      }
      const document = await store.createInProject(project.projectId, kind, title, true);
      return { document: diagramSummary(document), created: true, reused: false };
    }
    case 'diagram_move_document': {
      const moved = await store.moveDocument(
        String(argumentsValue.documentId),
        String(argumentsValue.targetProjectId),
        typeof argumentsValue.sourceProjectId === 'string' ? argumentsValue.sourceProjectId : undefined,
        scopeKey
      );
      return moved;
    }
    case 'diagram_delete_document':
      await store.readInScope(String(argumentsValue.documentId), scopeKey);
      await store.remove(String(argumentsValue.documentId));
      return { deleted: true, documentId: String(argumentsValue.documentId) };
    case 'diagram_get_document':
      return { document: await store.readInScope(String(argumentsValue.documentId), scopeKey) };
    case 'diagram_replace_document': {
      const document: unknown = argumentsValue.document;
      assertDiagramDocument(document);
      const current = await store.readInScope(document.documentId, scopeKey);
      const artifactKey = document.artifactKey ?? current.artifactKey;
      if (!artifactKey) throw new Error('Generated replacement requires a stable artifactKey on the document.');
      const permit = verifyGenerationPermit(String(argumentsValue.generationPermit), {
        scopeFingerprint: scopeKey,
        kind: document.kind,
        artifactKey,
        title: document.title,
        operation: 'revise',
        documentId: document.documentId
      });
      document.artifactKey = artifactKey;
      withGenerationProvenance(document, permit);
      assertGenerationReady(document, permit, false);
      const saved = await store.replace(document, Number(argumentsValue.expectedRevision));
      return { document: saved };
    }
    case 'diagram_apply_patch': {
      const documentId = String(argumentsValue.documentId);
      const current = await store.readInScope(documentId, scopeKey);
      const operations = argumentsValue.operations as DiagramPatchOperation[];
      const changesStructure = operations.some((operation) => ['upsert_node', 'remove_node', 'upsert_edge', 'remove_edge'].includes(operation.op));
      let patchProvenance: ReturnType<typeof generationProvenance> | undefined;
      if (changesStructure) {
        if (typeof argumentsValue.generationPermit !== 'string') throw new Error('A generationPermit is required for structural diagram changes.');
        const artifactKey = current.artifactKey;
        if (!artifactKey) throw new Error('Assign a stable artifactKey through a generated revision before structural AI changes.');
        const permit = verifyGenerationPermit(argumentsValue.generationPermit, {
          scopeFingerprint: scopeKey,
          kind: current.kind,
          artifactKey,
          operation: 'revise',
          documentId
        });
        const candidate = applyDiagramPatch(current, operations);
        withGenerationProvenance(candidate, permit);
        patchProvenance = generationProvenance(permit);
        assertDiagramDocument(candidate);
        assertGenerationReady(candidate, permit, false);
      }
      const saved = await store.patch(
        documentId,
        Number(argumentsValue.expectedRevision),
        operations,
        patchProvenance
      );
      return { document: saved };
    }
    case 'diagram_auto_layout': {
      await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const direction = argumentsValue.direction === 'DOWN' ? 'DOWN' : argumentsValue.direction === 'RIGHT' ? 'RIGHT' : undefined;
      const saved = await store.autoLayout(
        String(argumentsValue.documentId),
        Number(argumentsValue.expectedRevision),
        direction
      );
      return { document: saved };
    }
    case 'diagram_validate': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      assertDiagramDocument(document);
      return {
        document: diagramSummary(document),
        ...inspectDiagramQuality(
          document,
          documentQualityProfile(document, argumentsValue.qualityProfile),
          argumentsValue.requireSourceReferences === true
        )
      };
    }
    case 'diagram_export': {
      const document = await store.readInScope(String(argumentsValue.documentId), scopeKey);
      const format = argumentsValue.format === 'svg' ? 'svg' : argumentsValue.format === 'plantuml' ? 'plantuml' : 'json';
      return await writeExportArtifact(document, format);
    }
    default:
      throw new Error(`Unknown Diagram Studio tool: ${name}`);
  }
}

function applyNodeEvidence(document: Awaited<ReturnType<DiagramDocumentStore['read']>>, value: unknown): void {
  if (!Array.isArray(value)) return;
  const evidenceByAlias = new Map<string, string[]>();
  for (const item of value) {
    if (!item || typeof item !== 'object' || Array.isArray(item)) continue;
    const candidate = item as { alias?: unknown; sourceReferences?: unknown };
    if (typeof candidate.alias !== 'string' || !Array.isArray(candidate.sourceReferences)) continue;
    const references = candidate.sourceReferences.filter((reference): reference is string => typeof reference === 'string' && reference.trim().length > 0);
    if (references.length > 0) evidenceByAlias.set(candidate.alias, [...new Set(references)]);
  }
  for (const node of document.nodes) {
    const alias = typeof node.data.plantUmlId === 'string' ? node.data.plantUmlId : undefined;
    const references = alias ? evidenceByAlias.get(alias) : undefined;
    if (references) node.data.sourceReferences = references;
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
      'chatos/artifacts': [{
        producer_artifact_id: `diagram_${value.sha256}`,
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

async function runMcp(): Promise<void> {
  await store.initialize();
  const server = new Server(
    { name: SERVER_NAME, version: SERVER_VERSION },
    { capabilities: { tools: {} } }
  );
  server.setRequestHandler(ListToolsRequestSchema, async () => ({ tools: [...TOOL_DEFINITIONS] }));
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    try {
      return result(await callTool(request.params.name, request.params.arguments ?? {}));
    } catch (error) {
      return result({
        error: error instanceof Error ? error.message : String(error),
        ...(error instanceof RevisionConflictError ? { actualRevision: error.actualRevision } : {})
      }, true);
    }
  });
  await server.connect(new StdioServerTransport());
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === '--version' || command === '-v') {
    process.stdout.write(`${SERVER_VERSION}\n`);
    return;
  }
  if (command === 'mcp') {
    await runMcp();
    return;
  }
  process.stderr.write([
    'Usage: chatos-diagram-studio mcp',
    '       chatos-diagram-studio --version',
    '',
    'Run `npm run studio` from the plugin directory to open the standalone visual workbench.'
  ].join('\n') + '\n');
  process.exitCode = 2;
}

await main().catch((error) => {
  process.stderr.write(`Diagram Studio failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
