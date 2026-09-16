import type { DiagramDocument, DiagramNode } from './schema.js';
import { parsePlantUmlStructural } from './plantuml.js';
import { analyzeMindMap, isMindMapNode } from './mindmap.js';

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
    reciprocalRelationshipCount: number;
    maxCrossBoundaryFan: number;
    maxBoundaryPairEdges: number;
    unlabeledEdgeCount: number;
    missingSourceReferenceCount: number;
    isolatedNodeCount: number;
    overlapCount: number;
    childOverflowCount: number;
    width: number;
    height: number;
    aspectRatio: number;
    architectureLongestPathNodeCount?: number;
    architectureLongestPathRatio?: number;
    architectureProcessLikeEdgeCount?: number;
    mindmapRootCount?: number;
    mindmapMaxDepth?: number;
    mindmapMaxChildren?: number;
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
  const architectureSemantics = document.kind === 'architecture'
    ? inspectArchitectureSemantics(document, components)
    : undefined;
  const connected = new Set(document.edges.flatMap((edge) => [edge.source, edge.target]));
  const isolatedNodeIds = components.filter((node) => !connected.has(node.id)).map((node) => node.id);
  const missingSourceReferenceIds = components
    .filter((node) => (node.data.sourceReferences?.length ?? 0) === 0)
    .map((node) => node.id);
  const childCounts = containers.map((container) => document.nodes.filter((node) => node.parentId === container.id).length);
  const fanOut = new Map<string, number>();
  const fanIn = new Map<string, number>();
  const directedPairs = new Set<string>();
  const crossBoundaryFan = new Map<string, number>();
  const boundaryPairEdges = new Map<string, number>();
  for (const edge of document.edges) {
    fanOut.set(edge.source, (fanOut.get(edge.source) ?? 0) + 1);
    fanIn.set(edge.target, (fanIn.get(edge.target) ?? 0) + 1);
    directedPairs.add(`${edge.source}\u0000${edge.target}`);
    const sourceBoundary = topLevelNodeId(document.nodes, edge.source);
    const targetBoundary = topLevelNodeId(document.nodes, edge.target);
    if (sourceBoundary && targetBoundary && sourceBoundary !== targetBoundary) {
      crossBoundaryFan.set(edge.source, (crossBoundaryFan.get(edge.source) ?? 0) + 1);
      crossBoundaryFan.set(edge.target, (crossBoundaryFan.get(edge.target) ?? 0) + 1);
      const boundaryPair = [sourceBoundary, targetBoundary].sort().join('\u0000');
      boundaryPairEdges.set(boundaryPair, (boundaryPairEdges.get(boundaryPair) ?? 0) + 1);
    }
  }
  const reciprocalPairs = new Set(document.edges
    .filter((edge) => directedPairs.has(`${edge.target}\u0000${edge.source}`))
    .map((edge) => [edge.source, edge.target].sort().join('\u0000')));
  const reciprocalRelationships = reciprocalPairs.size;
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
  const mindmap = document.kind === 'mindmap' ? analyzeMindMap(document) : undefined;
  if (mindmap) {
    const explicitRoots = mindmap.roots.filter((node) => node.data.shape === 'mindmap-root');
    if (mindmap.roots.length === 0 || explicitRoots.length === 0) {
      errors.push({ code: 'mindmap_missing_root', message: 'Mind map must contain exactly one central topic.', blocking: true });
    } else if (mindmap.roots.length !== 1 || explicitRoots.length !== 1) {
      errors.push({ code: 'mindmap_multiple_roots', message: `Mind map contains ${mindmap.roots.length} root candidates and ${explicitRoots.length} central topics; exactly one is required.`, blocking: true, nodeIds: mindmap.roots.map((node) => node.id) });
    }
    if (mindmap.multipleParentNodeIds.length > 0) {
      errors.push({ code: 'mindmap_multiple_parents', message: 'Every non-root mind-map topic must have exactly one parent.', blocking: true, nodeIds: mindmap.multipleParentNodeIds });
    }
    if (mindmap.cycleNodeIds.length > 0) {
      errors.push({ code: 'mindmap_cycle', message: 'Mind-map branches must form an acyclic hierarchy.', blocking: true, nodeIds: mindmap.cycleNodeIds });
    }
    if (mindmap.orphanNodeIds.length > 0) {
      errors.push({ code: 'mindmap_invalid_nodes', message: 'Mind maps may contain central topics, branch topics, and standalone text notes only.', blocking: true, nodeIds: mindmap.orphanNodeIds });
    }
    const semanticNodes = document.nodes.filter(isMindMapNode);
    if (document.edges.length !== Math.max(0, semanticNodes.length - 1)) {
      errors.push({ code: 'mindmap_not_a_tree', message: `A ${semanticNodes.length}-topic mind map requires exactly ${Math.max(0, semanticNodes.length - 1)} parent-child branches.`, blocking: true });
    }
    if (mindmap.maxDepth > 4) {
      warnings.push({ code: 'mindmap_too_deep', message: `Mind-map depth ${mindmap.maxDepth} exceeds the recommended four levels; split detailed branches into another map.`, blocking: true });
    }
    if (mindmap.maxChildren > 8) {
      warnings.push({ code: 'mindmap_branch_overloaded', message: `One topic has ${mindmap.maxChildren} direct children; regroup or split the branch.`, blocking: true });
    }
    const longLabels = semanticNodes.filter((node) => [...node.data.label].length > (node.data.shape === 'mindmap-root' ? 32 : 24)).map((node) => node.id);
    if (longLabels.length > 0) {
      warnings.push({ code: 'mindmap_label_too_long', message: 'Mind-map topics should be short phrases instead of paragraph content.', blocking: true, nodeIds: longLabels });
    }
  }
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
  const maxComponents = profile === 'architecture-overview' ? 10 : profile === 'architecture-detail' ? 16 : 20;
  const maxEdges = profile === 'architecture-overview' ? 12 : profile === 'architecture-detail' ? 22 : 30;
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
  if (document.kind !== 'architecture' && document.kind !== 'mindmap' && components.length > 24) {
    warnings.push({
      code: 'diagram_too_many_nodes',
      message: `${components.length} nodes make this diagram difficult to scan; split independent outcomes or scenarios.`,
      blocking: true
    });
  }
  const maxFanOut = maximum(fanOut.values());
  const maxFanIn = maximum(fanIn.values());
  const maxCrossBoundaryFan = maximum(crossBoundaryFan.values());
  const maxBoundaryPairEdges = maximum(boundaryPairEdges.values());
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
  if (document.kind === 'architecture' && profile === 'architecture-overview' && reciprocalRelationships > 0) {
    warnings.push({
      code: 'architecture_reciprocal_relationships',
      message: `${reciprocalRelationships} reciprocal relationship pair(s) turn the overview into a runtime flow. Aggregate request/result semantics into one relationship or move the callback to a focused detail diagram.`,
      blocking: true
    });
  }
  if (document.kind === 'architecture' && profile === 'architecture-overview' && maxCrossBoundaryFan > 4) {
    warnings.push({
      code: 'architecture_cross_boundary_hub',
      message: `One component participates in ${maxCrossBoundaryFan} cross-boundary relationships; aggregate shared data/runtime dependencies or split the concern into a detail diagram.`,
      blocking: true
    });
  }
  if (document.kind === 'architecture' && profile === 'architecture-overview' && maxBoundaryPairEdges > 2) {
    warnings.push({
      code: 'architecture_boundary_pair_too_dense',
      message: `${maxBoundaryPairEdges} relationships connect the same pair of boundaries; replace them with one boundary-level relationship or create a focused detail diagram.`,
      blocking: true
    });
  }
  if (document.kind === 'architecture' && profile === 'architecture-overview' && containers.length > 5) {
    warnings.push({
      code: 'architecture_too_many_boundaries',
      message: `${containers.length} boundaries fragment the overview; merge equivalent ownership areas or move internal boundaries to detail diagrams.`,
      blocking: true
    });
  }
  if (document.kind === 'architecture' && profile === 'architecture-overview' && architectureSemantics) {
    const looksLikeRuntimeChain = architectureSemantics.longestPathNodeCount >= 5
      && architectureSemantics.longestPathRatio >= 0.55
      && architectureSemantics.processLikeEdgeCount >= 2;
    if (looksLikeRuntimeChain) {
      warnings.push({
        code: 'architecture_flow_like_chain',
        message: `A ${architectureSemantics.longestPathNodeCount}-component directed chain uses ${architectureSemantics.processLikeEdgeCount} process-like relationship labels. Keep stable dependencies in the overview and move execution order to a flowchart or sequence diagram.`,
        blocking: true
      });
    }
    if (architectureSemantics.processLikeEdgeCount >= Math.max(3, Math.ceil(document.edges.length * 0.3))) {
      warnings.push({
        code: 'architecture_runtime_step_labels',
        message: `${architectureSemantics.processLikeEdgeCount} architecture relationships describe multi-step runtime actions. Replace them with durable contracts or split the runtime scenario into another diagram.`,
        blocking: true
      });
    }
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
  const maximumAspectRatio = document.kind === 'architecture' && profile === 'architecture-overview' ? 4 : 5.5;
  if (document.kind !== 'mindmap' && components.length >= 6 && aspectRatio > maximumAspectRatio) {
    warnings.push({
      code: 'extreme_aspect_ratio',
      message: `Diagram aspect ratio ${aspectRatio.toFixed(1)}:1 exceeds the ${maximumAspectRatio.toFixed(1)}:1 readability limit and will make labels too small at fit-to-view.`,
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
      reciprocalRelationshipCount: reciprocalRelationships,
      maxCrossBoundaryFan,
      maxBoundaryPairEdges,
      unlabeledEdgeCount,
      missingSourceReferenceCount: missingSourceReferenceIds.length,
      isolatedNodeCount: isolatedNodeIds.length,
      overlapCount,
      childOverflowCount,
      width: Math.round(bounds.width),
      height: Math.round(bounds.height),
      aspectRatio: Number(aspectRatio.toFixed(2))
      ,...(architectureSemantics ? {
        architectureLongestPathNodeCount: architectureSemantics.longestPathNodeCount,
        architectureLongestPathRatio: Number(architectureSemantics.longestPathRatio.toFixed(2)),
        architectureProcessLikeEdgeCount: architectureSemantics.processLikeEdgeCount
      } : {})
      ,...(mindmap ? {
        mindmapRootCount: mindmap.roots.length,
        mindmapMaxDepth: mindmap.maxDepth,
        mindmapMaxChildren: mindmap.maxChildren
      } : {})
    },
    errors,
    warnings
  };
}

function inspectArchitectureSemantics(document: DiagramDocument, components: DiagramNode[]): {
  longestPathNodeCount: number;
  longestPathRatio: number;
  processLikeEdgeCount: number;
} {
  const semanticNodes = components.filter((node) => !['text', 'activation', 'fragment'].includes(node.data.shape));
  const nodeIds = new Set(semanticNodes.map((node) => node.id));
  const outgoing = new Map(semanticNodes.map((node) => [node.id, new Set<string>()]));
  for (const edge of document.edges) {
    if (nodeIds.has(edge.source) && nodeIds.has(edge.target) && edge.source !== edge.target) {
      outgoing.get(edge.source)!.add(edge.target);
    }
  }
  const componentsByCycle = stronglyConnectedArchitectureComponents([...nodeIds], outgoing);
  const cycleByNode = new Map<string, number>();
  componentsByCycle.forEach((component, index) => component.forEach((nodeId) => cycleByNode.set(nodeId, index)));
  const cycleOutgoing = new Map(componentsByCycle.map((_, index) => [index, new Set<number>()]));
  const cycleIncoming = new Map(componentsByCycle.map((_, index) => [index, 0]));
  for (const [source, targets] of outgoing) {
    const sourceCycle = cycleByNode.get(source)!;
    for (const target of targets) {
      const targetCycle = cycleByNode.get(target)!;
      if (sourceCycle === targetCycle || cycleOutgoing.get(sourceCycle)!.has(targetCycle)) continue;
      cycleOutgoing.get(sourceCycle)!.add(targetCycle);
      cycleIncoming.set(targetCycle, (cycleIncoming.get(targetCycle) ?? 0) + 1);
    }
  }
  const longestByCycle = new Map(componentsByCycle.map((component, index) => [index, component.length]));
  const queue = componentsByCycle.map((_, index) => index).filter((index) => cycleIncoming.get(index) === 0);
  while (queue.length > 0) {
    const current = queue.shift()!;
    for (const target of cycleOutgoing.get(current) ?? []) {
      longestByCycle.set(target, Math.max(
        longestByCycle.get(target) ?? 0,
        (longestByCycle.get(current) ?? 0) + componentsByCycle[target].length
      ));
      cycleIncoming.set(target, (cycleIncoming.get(target) ?? 0) - 1);
      if (cycleIncoming.get(target) === 0) queue.push(target);
    }
  }
  const longestPathNodeCount = maximum(longestByCycle.values());
  return {
    longestPathNodeCount,
    longestPathRatio: semanticNodes.length > 0 ? longestPathNodeCount / semanticNodes.length : 0,
    processLikeEdgeCount: document.edges.filter((edge) => isProcessLikeArchitectureLabel(edge.label ?? edge.data?.relation ?? '')).length
  };
}

function isProcessLikeArchitectureLabel(value: string): boolean {
  const label = value.trim();
  if (!label) return false;
  if (/^\s*\d+[.)、:：-]/u.test(label) || /(?:然后|随后|之后|完成后|成功后|失败后|先.+再)/u.test(label)) return true;
  const chineseRuntimeVerbs = label.match(/扫描|注册|读取|加载|创建|查询|更新|返回|回调|重试|执行|调用|管理|保存|发送|接收|解析|校验|获取|生成/gu)?.length ?? 0;
  const englishRuntimeVerbs = label.match(/\b(?:scan|register|read|load|create|query|update|return|callback|retry|execute|invoke|manage|save|send|receive|parse|validate|fetch|generate)(?:s|ed|ing)?\b/giu)?.length ?? 0;
  return chineseRuntimeVerbs + englishRuntimeVerbs >= 2;
}

function stronglyConnectedArchitectureComponents(nodeIds: string[], outgoing: Map<string, Set<string>>): string[][] {
  let nextIndex = 0;
  const indexes = new Map<string, number>();
  const lowLinks = new Map<string, number>();
  const stack: string[] = [];
  const onStack = new Set<string>();
  const components: string[][] = [];
  const visit = (nodeId: string) => {
    indexes.set(nodeId, nextIndex);
    lowLinks.set(nodeId, nextIndex);
    nextIndex += 1;
    stack.push(nodeId);
    onStack.add(nodeId);
    for (const target of outgoing.get(nodeId) ?? []) {
      if (!indexes.has(target)) {
        visit(target);
        lowLinks.set(nodeId, Math.min(lowLinks.get(nodeId)!, lowLinks.get(target)!));
      } else if (onStack.has(target)) {
        lowLinks.set(nodeId, Math.min(lowLinks.get(nodeId)!, indexes.get(target)!));
      }
    }
    if (lowLinks.get(nodeId) !== indexes.get(nodeId)) return;
    const component: string[] = [];
    while (stack.length > 0) {
      const member = stack.pop()!;
      onStack.delete(member);
      component.push(member);
      if (member === nodeId) break;
    }
    components.push(component);
  };
  nodeIds.forEach((nodeId) => { if (!indexes.has(nodeId)) visit(nodeId); });
  return components;
}

function topLevelNodeId(nodes: DiagramNode[], nodeId: string): string | undefined {
  let current = nodes.find((node) => node.id === nodeId);
  const seen = new Set<string>();
  while (current?.parentId && !seen.has(current.id)) {
    seen.add(current.id);
    current = nodes.find((node) => node.id === current!.parentId);
  }
  return current?.id;
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
  if (node.data.shape === 'mindmap-root') return { width: 200, height: 64 };
  if (node.data.shape === 'mindmap-topic') return { width: 150, height: 46 };
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
