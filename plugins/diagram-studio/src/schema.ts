export type DiagramKind = 'architecture' | 'flowchart' | 'swimlane' | 'topology' | 'sequence';

export interface DiagramProject {
  schemaVersion: 1;
  projectId: string;
  name: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  diagramIds: string[];
}

export interface DiagramProjectSummary {
  projectId: string;
  name: string;
  description?: string;
  diagramCount: number;
  diagramIds: string[];
  createdAt: string;
  updatedAt: string;
}
export type DiagramNodeShape =
  | 'rounded'
  | 'rectangle'
  | 'diamond'
  | 'circle'
  | 'cylinder'
  | 'text'
  | 'lane'
  | 'lifeline'
  | 'activation'
  | 'fragment'
  | 'container';
export type DiagramNodeIcon =
  | 'user'
  | 'terminal'
  | 'mobile'
  | 'browser'
  | 'server'
  | 'api'
  | 'cloud'
  | 'database'
  | 'cache'
  | 'storage'
  | 'queue'
  | 'network'
  | 'shield'
  | 'container'
  | 'cluster'
  | 'monitor'
  | 'document'
  | 'note';
export type DiagramNodeCategory =
  | 'client'
  | 'service'
  | 'database'
  | 'queue'
  | 'external'
  | 'decision'
  | 'process'
  | 'terminal'
  | 'network'
  | 'lane'
  | 'note';

export interface DiagramPoint {
  x: number;
  y: number;
}

export interface DiagramNodeData extends Record<string, unknown> {
  label: string;
  subtitle?: string;
  description?: string;
  category: DiagramNodeCategory;
  shape: DiagramNodeShape;
  icon?: DiagramNodeIcon;
  showLabel?: boolean;
  borderStyle?: 'solid' | 'dashed' | 'dotted' | 'none';
  borderWidth?: number;
  fillColor?: string;
  borderColor?: string;
  color?: string;
  textColor?: string;
  fontSize?: number;
  fontWeight?: number;
  sequenceOwnerId?: string;
  sequenceSlot?: number;
  plantUmlId?: string;
  plantUmlType?: string;
  sourceReferences?: string[];
}

export interface DiagramNode {
  id: string;
  type: 'diagramNode' | 'laneNode';
  position: DiagramPoint;
  data: DiagramNodeData;
  width?: number;
  height?: number;
  parentId?: string;
  extent?: 'parent';
  zIndex?: number;
}

export interface DiagramEdgeData extends Record<string, unknown> {
  relation?: string;
  description?: string;
  dashed?: boolean;
  lineStyle?: 'solid' | 'dashed' | 'dotted';
  startMarker?: 'none' | 'arrow';
  endMarker?: 'none' | 'arrow';
  strokeWidth?: number;
  color?: string;
  fontSize?: number;
  plantUmlId?: string;
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string;
  targetHandle?: string;
  label?: string;
  type?: 'smoothstep' | 'bezier' | 'straight';
  animated?: boolean;
  data?: DiagramEdgeData;
}

export interface DiagramViewport {
  x: number;
  y: number;
  zoom: number;
}

export type PlantUmlDialect = 'sequence' | 'activity' | 'component' | 'deployment';

export interface PlantUmlNotation {
  format: 'plantuml';
  dialect: PlantUmlDialect;
  source?: string;
  opaqueBlocks?: string[];
  lastSyncedRevision?: number;
}

export interface DiagramDocument {
  schemaVersion: 1;
  documentId: string;
  artifactKey?: string;
  revision: number;
  kind: DiagramKind;
  title: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  viewport: DiagramViewport;
  notation?: PlantUmlNotation;
  metadata?: Record<string, string>;
}

export type DiagramPatchOperation =
  | { op: 'set_title'; title: string }
  | { op: 'set_description'; description: string }
  | { op: 'upsert_node'; node: DiagramNode }
  | { op: 'remove_node'; nodeId: string }
  | { op: 'move_node'; nodeId: string; position: DiagramPoint }
  | { op: 'upsert_edge'; edge: DiagramEdge }
  | { op: 'remove_edge'; edgeId: string }
  | { op: 'set_viewport'; viewport: DiagramViewport };

const identifierPattern = /^[a-zA-Z0-9][a-zA-Z0-9_-]{0,127}$/;

export function assertIdentifier(value: string, label: string): void {
  if (!identifierPattern.test(value)) {
    throw new Error(`${label} must use letters, digits, hyphens, or underscores.`);
  }
}

export function assertDiagramProject(value: unknown): asserts value is DiagramProject {
  if (!value || typeof value !== 'object') throw new Error('Diagram project must be an object.');
  const project = value as Partial<DiagramProject>;
  if (project.schemaVersion !== 1) throw new Error('Unsupported project schema version.');
  if (typeof project.projectId !== 'string') throw new Error('Project projectId is required.');
  assertIdentifier(project.projectId, 'projectId');
  if (typeof project.name !== 'string' || project.name.trim().length === 0 || project.name.length > 240) {
    throw new Error('Project name must contain 1 to 240 characters.');
  }
  if (!Array.isArray(project.diagramIds) || project.diagramIds.length > 5000 || project.diagramIds.some((id) => typeof id !== 'string')) {
    throw new Error('Project diagramIds must be an array of identifiers.');
  }
}

export function diagramProjectSummary(project: DiagramProject): DiagramProjectSummary {
  return {
    projectId: project.projectId,
    name: project.name,
    description: project.description,
    diagramCount: project.diagramIds.length,
    diagramIds: [...project.diagramIds],
    createdAt: project.createdAt,
    updatedAt: project.updatedAt
  };
}

export function assertDiagramDocument(value: unknown): asserts value is DiagramDocument {
  if (!value || typeof value !== 'object') throw new Error('Diagram document must be an object.');
  const document = value as Partial<DiagramDocument>;
  if (document.schemaVersion !== 1) throw new Error('Unsupported diagram schema version.');
  if (typeof document.documentId !== 'string') throw new Error('Diagram documentId is required.');
  assertIdentifier(document.documentId, 'documentId');
  if (document.artifactKey !== undefined) {
    if (typeof document.artifactKey !== 'string') throw new Error('Diagram artifactKey must be a string.');
    assertIdentifier(document.artifactKey, 'artifactKey');
  }
  if (!['architecture', 'flowchart', 'swimlane', 'topology', 'sequence'].includes(document.kind ?? '')) {
    throw new Error('Diagram kind is invalid.');
  }
  if (typeof document.title !== 'string' || document.title.trim().length === 0 || document.title.length > 240) {
    throw new Error('Diagram title must contain 1 to 240 characters.');
  }
  if (!Number.isSafeInteger(document.revision) || (document.revision ?? -1) < 0) {
    throw new Error('Diagram revision is invalid.');
  }
  if (!Array.isArray(document.nodes) || document.nodes.length > 5000) {
    throw new Error('Diagram nodes must be an array with at most 5000 items.');
  }
  if (!Array.isArray(document.edges) || document.edges.length > 10000) {
    throw new Error('Diagram edges must be an array with at most 10000 items.');
  }
  if (document.notation) {
    if (document.notation.format !== 'plantuml') throw new Error('Diagram notation format is invalid.');
    if (!['sequence', 'activity', 'component', 'deployment'].includes(document.notation.dialect)) {
      throw new Error('Diagram PlantUML dialect is invalid.');
    }
    if (document.notation.source !== undefined && (typeof document.notation.source !== 'string' || document.notation.source.length > 2 * 1024 * 1024)) {
      throw new Error('Diagram PlantUML source is invalid or too large.');
    }
    if (document.notation.opaqueBlocks !== undefined && (!Array.isArray(document.notation.opaqueBlocks) || document.notation.opaqueBlocks.some((item) => typeof item !== 'string'))) {
      throw new Error('Diagram PlantUML opaque blocks are invalid.');
    }
  }
  const nodeIds = new Set<string>();
  for (const node of document.nodes) {
    assertIdentifier(node.id, 'node id');
    if (nodeIds.has(node.id)) throw new Error(`Duplicate node id: ${node.id}`);
    nodeIds.add(node.id);
    if (!node.position || !Number.isFinite(node.position.x) || !Number.isFinite(node.position.y)) {
      throw new Error(`Node ${node.id} has an invalid position.`);
    }
    if (!node.data || typeof node.data.label !== 'string' || node.data.label.trim().length === 0) {
      throw new Error(`Node ${node.id} must have a label.`);
    }
  }
  const edgeIds = new Set<string>();
  for (const edge of document.edges) {
    assertIdentifier(edge.id, 'edge id');
    if (edgeIds.has(edge.id)) throw new Error(`Duplicate edge id: ${edge.id}`);
    edgeIds.add(edge.id);
    if (!nodeIds.has(edge.source) || !nodeIds.has(edge.target)) {
      throw new Error(`Edge ${edge.id} references a missing node.`);
    }
  }
}

export function applyDiagramPatch(
  document: DiagramDocument,
  operations: DiagramPatchOperation[]
): DiagramDocument {
  if (operations.length > 1000) throw new Error('A patch may contain at most 1000 operations.');
  let next = structuredClone(document);
  for (const operation of operations) {
    switch (operation.op) {
      case 'set_title':
        next.title = operation.title.trim();
        break;
      case 'set_description':
        next.description = operation.description;
        break;
      case 'upsert_node': {
        const index = next.nodes.findIndex((node) => node.id === operation.node.id);
        if (index >= 0) next.nodes[index] = structuredClone(operation.node);
        else next.nodes.push(structuredClone(operation.node));
        break;
      }
      case 'remove_node':
        next.nodes = next.nodes.filter((node) => node.id !== operation.nodeId && node.parentId !== operation.nodeId);
        next.edges = next.edges.filter((edge) => edge.source !== operation.nodeId && edge.target !== operation.nodeId);
        break;
      case 'move_node': {
        const node = next.nodes.find((candidate) => candidate.id === operation.nodeId);
        if (!node) throw new Error(`Node not found: ${operation.nodeId}`);
        node.position = { ...operation.position };
        break;
      }
      case 'upsert_edge': {
        const index = next.edges.findIndex((edge) => edge.id === operation.edge.id);
        if (index >= 0) next.edges[index] = structuredClone(operation.edge);
        else next.edges.push(structuredClone(operation.edge));
        break;
      }
      case 'remove_edge':
        next.edges = next.edges.filter((edge) => edge.id !== operation.edgeId);
        break;
      case 'set_viewport':
        next.viewport = { ...operation.viewport };
        break;
    }
  }
  assertDiagramDocument(next);
  return next;
}

export function diagramSummary(document: DiagramDocument) {
  return {
    documentId: document.documentId,
    artifactKey: document.artifactKey,
    revision: document.revision,
    kind: document.kind,
    title: document.title,
    nodeCount: document.nodes.length,
    edgeCount: document.edges.length,
    updatedAt: document.updatedAt
  };
}
