import type { DiagramDocument, DiagramNode } from './schema.js';
import { parsePlantUmlStructural } from './plantuml.js';

export type DiagramQualityProfile = 'balanced' | 'architecture-overview' | 'architecture-detail';

export interface DiagramQualityIssue {
  code: string;
  message: string;
  blocking?: boolean;
  nodeIds?: string[];
}

export interface DiagramQualityReport {
  valid: boolean;
  ready: boolean;
  profile: DiagramQualityProfile;
  metrics: {
    nodeCount: number;
    componentCount: number;
    edgeCount: number;
    containerCount: number;
    declaredContainerCount?: number;
    maxContainerChildren: number;
    maxFanOut: number;
    maxFanIn: number;
    unlabeledEdgeCount: number;
    missingSourceReferenceCount: number;
    isolatedNodeCount: number;
    overlapCount: number;
    childOverflowCount: number;
    width: number;
    height: number;
    aspectRatio: number;
  };
  errors: DiagramQualityIssue[];
  warnings: DiagramQualityIssue[];
}

export function inspectDiagramQuality(
  document: DiagramDocument,
  profile: DiagramQualityProfile = 'balanced',
  requireSourceReferences = false
): DiagramQualityReport {
  const components = document.nodes.filter((node) => node.data.shape !== 'container' && node.data.shape !== 'lane');
  const containers = document.nodes.filter((node) => node.data.shape === 'container');
  const connected = new Set(document.edges.flatMap((edge) => [edge.source, edge.target]));
  const isolatedNodeIds = components.filter((node) => !connected.has(node.id)).map((node) => node.id);
  const missingSourceReferenceIds = components
    .filter((node) => (node.data.sourceReferences?.length ?? 0) === 0)
    .map((node) => node.id);
  const childCounts = containers.map((container) => document.nodes.filter((node) => node.parentId === container.id).length);
  const fanOut = new Map<string, number>();
  const fanIn = new Map<string, number>();
  for (const edge of document.edges) {
    fanOut.set(edge.source, (fanOut.get(edge.source) ?? 0) + 1);
    fanIn.set(edge.target, (fanIn.get(edge.target) ?? 0) + 1);
  }
  const rectangles = components.slice(0, 300).map((node) => nodeRectangle(document.nodes, node));
  let overlapCount = 0;
  for (let left = 0; left < rectangles.length; left += 1) {
    for (let right = left + 1; right < rectangles.length; right += 1) {
      if (isAncestor(document.nodes, rectangles[left].id, rectangles[right].id)
        || isAncestor(document.nodes, rectangles[right].id, rectangles[left].id)) continue;
      if (rectanglesOverlap(rectangles[left], rectangles[right])) overlapCount += 1;
    }
  }
  let childOverflowCount = 0;
  for (const node of document.nodes) {
    if (!node.parentId) continue;
    const parent = document.nodes.find((candidate) => candidate.id === node.parentId);
    if (!parent) continue;
    const width = node.width ?? defaultNodeSize(node).width;
    const height = node.height ?? defaultNodeSize(node).height;
    const parentWidth = parent.width ?? defaultNodeSize(parent).width;
    const parentHeight = parent.height ?? defaultNodeSize(parent).height;
    if (node.position.x < 0 || node.position.y < 0
      || node.position.x + width > parentWidth + 0.5
      || node.position.y + height > parentHeight + 0.5) childOverflowCount += 1;
  }
  const bounds = diagramBounds(document.nodes);
  const declaredContainerCount = sourceContainerCount(document);
  const errors: DiagramQualityIssue[] = [];
  const warnings: DiagramQualityIssue[] = [];
  if (declaredContainerCount !== undefined && declaredContainerCount !== containers.length) {
    errors.push({
      code: 'container_structure_lost',
      message: `PlantUML declares ${declaredContainerCount} structural groups, but the editable document contains ${containers.length}.`,
      blocking: true
    });
  }
  if (overlapCount > 0) {
    errors.push({ code: 'node_overlaps', message: `${overlapCount} node overlap(s) were detected.`, blocking: true });
  }
  if (childOverflowCount > 0) {
    errors.push({ code: 'container_overflow', message: `${childOverflowCount} contained node(s) extend outside their parent.`, blocking: true });
  }
  if (isolatedNodeIds.length > 0) {
    warnings.push({ code: 'isolated_nodes', message: 'Some nodes have no semantic connection.', nodeIds: isolatedNodeIds });
  }
  if (missingSourceReferenceIds.length > 0) {
    warnings.push({
      code: 'missing_source_references',
      message: `${missingSourceReferenceIds.length} node(s) have no source evidence.`,
      nodeIds: missingSourceReferenceIds,
      blocking: requireSourceReferences
    });
  }
  const maxComponents = profile === 'architecture-overview' ? 12 : 20;
  const maxEdges = profile === 'architecture-overview' ? 18 : profile === 'architecture-detail' ? 28 : 30;
  if (document.kind === 'architecture' && components.length > maxComponents) {
    warnings.push({
      code: 'architecture_too_many_components',
      message: `${components.length} components exceed the ${profile} readability budget of ${maxComponents}; split the diagram by level or bounded context.`,
      blocking: true
    });
  }
  if (document.kind === 'architecture' && document.edges.length > maxEdges) {
    warnings.push({
      code: 'architecture_too_many_edges',
      message: `${document.edges.length} edges exceed the ${profile} readability budget of ${maxEdges}; aggregate repeated routes and dependencies.`,
      blocking: true
    });
  }
  if (document.kind !== 'architecture' && components.length > 24) {
    warnings.push({
      code: 'diagram_too_many_nodes',
      message: `${components.length} nodes make this diagram difficult to scan; split independent outcomes or scenarios.`,
      blocking: true
    });
  }
  const maxFanOut = maximum(fanOut.values());
  const maxFanIn = maximum(fanIn.values());
  if (document.kind === 'architecture' && Math.max(maxFanOut, maxFanIn) > 7) {
    warnings.push({
      code: 'architecture_hub_overloaded',
      message: `One component has ${Math.max(maxFanOut, maxFanIn)} direct edges; use a boundary-level relationship or split the detail view.`,
      blocking: profile === 'architecture-overview'
    });
  }
  if (containers.length === 0 && document.kind === 'architecture' && components.length >= 8) {
    warnings.push({
      code: 'flat_architecture',
      message: 'Architecture has many components but no system or layer boundaries.',
      blocking: profile === 'architecture-overview'
    });
  }
  const maxContainerChildren = maximum(childCounts);
  if (document.kind === 'architecture' && maxContainerChildren > 8) {
    warnings.push({
      code: 'container_too_dense',
      message: `A boundary contains ${maxContainerChildren} direct children; create a focused detail diagram.`,
      blocking: profile === 'architecture-overview'
    });
  }
  const aspectRatio = bounds.height > 0 && bounds.width > 0
    ? Math.max(bounds.width / bounds.height, bounds.height / bounds.width)
    : 1;
  if (components.length >= 6 && aspectRatio > 5.5) {
    warnings.push({
      code: 'extreme_aspect_ratio',
      message: `Diagram aspect ratio ${aspectRatio.toFixed(1)}:1 will make labels too small at fit-to-view.`,
      blocking: true
    });
  }
  const unlabeledEdgeCount = document.edges.filter((edge) => !(edge.label ?? edge.data?.relation ?? '').trim()).length;
  if (document.kind === 'architecture' && document.edges.length >= 8 && unlabeledEdgeCount / document.edges.length > 0.55) {
    warnings.push({
      code: 'too_many_unlabeled_edges',
      message: `${unlabeledEdgeCount} of ${document.edges.length} architecture edges are unlabeled; keep only meaningful dependencies and name their semantics.`
    });
  }
  return {
    valid: errors.length === 0,
    ready: errors.length === 0 && !warnings.some((issue) => issue.blocking),
    profile,
    metrics: {
      nodeCount: document.nodes.length,
      componentCount: components.length,
      edgeCount: document.edges.length,
      containerCount: containers.length,
      declaredContainerCount,
      maxContainerChildren,
      maxFanOut,
      maxFanIn,
      unlabeledEdgeCount,
      missingSourceReferenceCount: missingSourceReferenceIds.length,
      isolatedNodeCount: isolatedNodeIds.length,
      overlapCount,
      childOverflowCount,
      width: Math.round(bounds.width),
      height: Math.round(bounds.height),
      aspectRatio: Number(aspectRatio.toFixed(2))
    },
    errors,
    warnings
  };
}

function sourceContainerCount(document: DiagramDocument): number | undefined {
  if (!document.notation?.source || (document.kind !== 'architecture' && document.kind !== 'topology')) return undefined;
  try {
    return parsePlantUmlStructural(document.notation.source).nodes.filter((node) => node.container).length;
  } catch {
    return undefined;
  }
}

function maximum(values: Iterable<number>): number {
  let result = 0;
  for (const value of values) result = Math.max(result, value);
  return result;
}

function defaultNodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.data.shape === 'container') return { width: 300, height: 180 };
  if (node.data.shape === 'lifeline') return { width: 160, height: 560 };
  if (node.data.shape === 'activation') return { width: 14, height: 120 };
  if (node.data.shape === 'fragment') return { width: 620, height: 220 };
  if (node.data.shape === 'diamond') return { width: 150, height: 110 };
  if (node.data.shape === 'circle') return { width: 116, height: 116 };
  return { width: 190, height: 82 };
}

function absolutePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absolutePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
}

function nodeRectangle(nodes: DiagramNode[], node: DiagramNode) {
  const position = absolutePosition(nodes, node);
  const size = defaultNodeSize(node);
  return { id: node.id, x: position.x, y: position.y, width: node.width ?? size.width, height: node.height ?? size.height };
}

function rectanglesOverlap(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number }
): boolean {
  const padding = 2;
  return left.x + padding < right.x + right.width
    && left.x + left.width > right.x + padding
    && left.y + padding < right.y + right.height
    && left.y + left.height > right.y + padding;
}

function isAncestor(nodes: DiagramNode[], possibleAncestorId: string, nodeId: string): boolean {
  let current = nodes.find((node) => node.id === nodeId);
  const seen = new Set<string>();
  while (current?.parentId && !seen.has(current.parentId)) {
    if (current.parentId === possibleAncestorId) return true;
    seen.add(current.parentId);
    current = nodes.find((node) => node.id === current?.parentId);
  }
  return false;
}

function diagramBounds(nodes: DiagramNode[]): { width: number; height: number } {
  if (nodes.length === 0) return { width: 0, height: 0 };
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const node of nodes) {
    if (node.parentId) continue;
    const size = defaultNodeSize(node);
    minX = Math.min(minX, node.position.x);
    minY = Math.min(minY, node.position.y);
    maxX = Math.max(maxX, node.position.x + (node.width ?? size.width));
    maxY = Math.max(maxY, node.position.y + (node.height ?? size.height));
  }
  return { width: Math.max(0, maxX - minX), height: Math.max(0, maxY - minY) };
}
