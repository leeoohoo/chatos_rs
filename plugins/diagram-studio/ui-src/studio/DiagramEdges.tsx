import { useEffect, useRef, useState } from 'react';
import { BaseEdge, MarkerType, type EdgeProps } from '@xyflow/react';
import type { DiagramDocument, DiagramEdge } from '../../src/schema';
import { absoluteNodePosition, defaultNodeSize } from './DiagramStudioSupport';

type RoutingPoint = { x: number; y: number };
type RoutingObstacle = RoutingPoint & { width: number; height: number };
type SmartEdgeRuntimeData = DiagramEdge['data'] & {
  routingObstacles?: RoutingObstacle[];
  routingOffset?: number;
};

export function runtimeEdgesForDocument(document: DiagramDocument): DiagramEdge[] {
  if (document.kind !== 'flowchart' && document.kind !== 'swimlane') return document.edges;
  const edges = document.edges.map((edge) => ({ ...edge }));
  const nodeById = new Map(document.nodes.map((node) => [node.id, node]));
  const generatedEdgeIndexes = new Set<number>();

  edges.forEach((edge, edgeIndex) => {
    if (typeof edge.data?.plantUmlId !== 'string') return;
    const source = nodeById.get(edge.source);
    const target = nodeById.get(edge.target);
    if (!source || !target) return;
    const sourcePosition = absoluteNodePosition(document.nodes, source);
    const targetPosition = absoluteNodePosition(document.nodes, target);
    const sourceSize = defaultNodeSize(source);
    const targetSize = defaultNodeSize(target);
    const sourceCenter = {
      x: sourcePosition.x + (source.width ?? sourceSize.width) / 2,
      y: sourcePosition.y + (source.height ?? sourceSize.height) / 2
    };
    const targetCenter = {
      x: targetPosition.x + (target.width ?? targetSize.width) / 2,
      y: targetPosition.y + (target.height ?? targetSize.height) / 2
    };
    const horizontalDistance = Math.abs(targetCenter.x - sourceCenter.x);
    const verticalDistance = Math.abs(targetCenter.y - sourceCenter.y);
    const vertical = verticalDistance >= horizontalDistance * 0.65;
    edge.sourceHandle = vertical
      ? targetCenter.y >= sourceCenter.y ? 'bottom' : 'top'
      : targetCenter.x >= sourceCenter.x ? 'right' : 'left';
    edge.targetHandle = vertical
      ? targetCenter.y >= sourceCenter.y ? 'top' : 'bottom'
      : targetCenter.x >= sourceCenter.x ? 'left' : 'right';
    generatedEdgeIndexes.add(edgeIndex);
  });

  const decisionOutgoing = new Map<string, Array<{ edgeIndex: number; targetCenter: { x: number; y: number } }>>();
  for (const edgeIndex of generatedEdgeIndexes) {
    const edge = edges[edgeIndex];
    const source = nodeById.get(edge.source);
    const target = nodeById.get(edge.target);
    if (source?.data.shape !== 'diamond' || !target) continue;
    const targetPosition = absoluteNodePosition(document.nodes, target);
    const targetSize = defaultNodeSize(target);
    const entries = decisionOutgoing.get(source.id) ?? [];
    entries.push({
      edgeIndex,
      targetCenter: {
        x: targetPosition.x + (target.width ?? targetSize.width) / 2,
        y: targetPosition.y + (target.height ?? targetSize.height) / 2
      }
    });
    decisionOutgoing.set(source.id, entries);
  }
  for (const [sourceId, entries] of decisionOutgoing) {
    if (entries.length < 2) continue;
    const source = nodeById.get(sourceId)!;
    const sourcePosition = absoluteNodePosition(document.nodes, source);
    const sourceSize = defaultNodeSize(source);
    const sourceCenter = {
      x: sourcePosition.x + (source.width ?? sourceSize.width) / 2,
      y: sourcePosition.y + (source.height ?? sourceSize.height) / 2
    };
    const sourceWidth = source.width ?? sourceSize.width;
    const downward = entries
      .filter((entry) => entry.targetCenter.y > sourceCenter.y)
      .sort((left, right) => Math.abs(left.targetCenter.x - sourceCenter.x) - Math.abs(right.targetCenter.x - sourceCenter.x));
    const primary = downward[0]
      && Math.abs(downward[0].targetCenter.x - sourceCenter.x) <= sourceWidth * 0.75
      ? downward[0]
      : undefined;
    for (const entry of entries) {
      edges[entry.edgeIndex].sourceHandle = entry === primary
        ? 'bottom'
        : entry.targetCenter.x < sourceCenter.x ? 'left' : 'right';
    }
  }

  const groups = new Map<string, Array<{ edgeIndex: number; endpoint: 'source' | 'target'; otherNodeId: string; side: string }>>();
  const addEndpoint = (entry: { edgeIndex: number; endpoint: 'source' | 'target'; nodeId: string; otherNodeId: string; side: string }) => {
    if (nodeById.get(entry.nodeId)?.data.shape === 'diamond') return;
    const key = `${entry.nodeId}\u0000${entry.side}`;
    const entries = groups.get(key) ?? [];
    entries.push(entry);
    groups.set(key, entries);
  };
  for (const edgeIndex of generatedEdgeIndexes) {
    const edge = edges[edgeIndex];
    addEndpoint({ edgeIndex, endpoint: 'source', nodeId: edge.source, otherNodeId: edge.target, side: baseHandleSide(edge.sourceHandle) });
    addEndpoint({ edgeIndex, endpoint: 'target', nodeId: edge.target, otherNodeId: edge.source, side: baseHandleSide(edge.targetHandle) });
  }
  const center = (nodeId: string) => {
    const node = nodeById.get(nodeId);
    if (!node) return { x: 0, y: 0 };
    const position = absoluteNodePosition(document.nodes, node);
    const size = defaultNodeSize(node);
    return { x: position.x + (node.width ?? size.width) / 2, y: position.y + (node.height ?? size.height) / 2 };
  };
  for (const endpoints of groups.values()) {
    if (endpoints.length < 2) continue;
    endpoints.sort((left, right) => {
      const leftCenter = center(left.otherNodeId);
      const rightCenter = center(right.otherNodeId);
      return left.side === 'left' || left.side === 'right'
        ? leftCenter.y - rightCenter.y
        : leftCenter.x - rightCenter.x;
    });
    endpoints.forEach((endpoint, index) => {
      const handle = `${endpoint.side}-${Math.round(index * 6 / Math.max(1, endpoints.length - 1))}`;
      if (endpoint.endpoint === 'source') edges[endpoint.edgeIndex].sourceHandle = handle;
      else edges[endpoint.edgeIndex].targetHandle = handle;
    });
  }
  return edges;
}

function baseHandleSide(handle: string | undefined): 'left' | 'right' | 'top' | 'bottom' {
  const side = handle?.split('-', 1)[0];
  return side === 'left' || side === 'top' || side === 'bottom' ? side : 'right';
}

export function routingOffsetForEdge(document: DiagramDocument, edge: DiagramEdge, edgeIndex: number): number {
  const siblings = document.edges
    .map((candidate, index) => ({ candidate, index }))
    .filter(({ candidate }) => candidate.source === edge.source || candidate.target === edge.target)
    .sort((left, right) => {
      const leftOtherId = left.candidate.source === edge.source ? left.candidate.target : left.candidate.source;
      const rightOtherId = right.candidate.source === edge.source ? right.candidate.target : right.candidate.source;
      const leftNode = document.nodes.find((node) => node.id === leftOtherId);
      const rightNode = document.nodes.find((node) => node.id === rightOtherId);
      const leftPosition = leftNode ? absoluteNodePosition(document.nodes, leftNode) : { x: 0, y: 0 };
      const rightPosition = rightNode ? absoluteNodePosition(document.nodes, rightNode) : { x: 0, y: 0 };
      return leftPosition.x - rightPosition.x || leftPosition.y - rightPosition.y || left.index - right.index;
    });
  if (siblings.length < 2) return 0;
  const siblingIndex = siblings.findIndex(({ index }) => index === edgeIndex);
  return (siblingIndex - (siblings.length - 1) / 2) * 12;
}

export function routingObstaclesForEdge(document: DiagramDocument, edge: DiagramEdge): RoutingObstacle[] {
  const excluded = new Set<string>([edge.source, edge.target]);
  for (const endpointId of [edge.source, edge.target]) {
    let current = document.nodes.find((node) => node.id === endpointId);
    while (current?.parentId && !excluded.has(current.parentId)) {
      excluded.add(current.parentId);
      current = document.nodes.find((node) => node.id === current!.parentId);
    }
  }
  return document.nodes
    .filter((node) => !excluded.has(node.id) && node.data.shape !== 'lane')
    .map((node) => {
      const position = absoluteNodePosition(document.nodes, node);
      const size = defaultNodeSize(node);
      return { x: position.x, y: position.y, width: node.width ?? size.width, height: node.height ?? size.height };
    });
}

export function SmartOrthogonalEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerStart,
  markerEnd,
  style,
  label,
  labelStyle,
  labelBgStyle,
  labelBgPadding,
  labelBgBorderRadius,
  interactionWidth,
  data
}: EdgeProps) {
  const runtime = data as SmartEdgeRuntimeData | undefined;
  const points = routeOrthogonalEdge(
    { x: sourceX, y: sourceY },
    { x: targetX, y: targetY },
    String(sourcePosition),
    String(targetPosition),
    runtime?.routingObstacles ?? [],
    runtime?.routingOffset ?? 0
  );
  const path = points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`).join(' ');
  const labelPoint = routeLabelPoint(points);
  return <BaseEdge
    id={id}
    path={path}
    labelX={labelPoint.x}
    labelY={labelPoint.y}
    markerStart={markerStart}
    markerEnd={markerEnd}
    style={style}
    label={label}
    labelStyle={labelStyle}
    labelBgStyle={labelBgStyle}
    labelBgPadding={labelBgPadding}
    labelBgBorderRadius={labelBgBorderRadius}
    interactionWidth={interactionWidth}
  />;
}

export function routeOrthogonalEdge(
  source: RoutingPoint,
  target: RoutingPoint,
  sourcePosition: string,
  targetPosition: string,
  obstacles: RoutingObstacle[],
  laneOffset: number
): RoutingPoint[] {
  const clearance = 18;
  const expanded = obstacles.map((obstacle) => ({
    x: obstacle.x - clearance,
    y: obstacle.y - clearance,
    width: obstacle.width + clearance * 2,
    height: obstacle.height + clearance * 2
  }));
  const sourceStub = routingStub(source, sourcePosition, 24);
  const targetStub = routingStub(target, targetPosition, 24);
  const xCandidates = uniqueNumbers([
    (sourceStub.x + targetStub.x) / 2 + laneOffset,
    ...expanded.flatMap((obstacle) => [obstacle.x - 12 + laneOffset, obstacle.x + obstacle.width + 12 + laneOffset])
  ]);
  const yCandidates = uniqueNumbers([
    (sourceStub.y + targetStub.y) / 2 + laneOffset,
    ...expanded.flatMap((obstacle) => [obstacle.y - 12 + laneOffset, obstacle.y + obstacle.height + 12 + laneOffset])
  ]);
  const candidates: RoutingPoint[][] = [
    ...xCandidates.map((x) => [source, sourceStub, { x, y: sourceStub.y }, { x, y: targetStub.y }, targetStub, target]),
    ...yCandidates.map((y) => [source, sourceStub, { x: sourceStub.x, y }, { x: targetStub.x, y }, targetStub, target])
  ].map(simplifyRoute);
  return candidates.sort((left, right) => routeScore(left, expanded) - routeScore(right, expanded))[0] ?? [source, target];
}

export function routingStub(point: RoutingPoint, position: string, distance: number): RoutingPoint {
  switch (position.toLowerCase()) {
    case 'left': return { x: point.x - distance, y: point.y };
    case 'top': return { x: point.x, y: point.y - distance };
    case 'bottom': return { x: point.x, y: point.y + distance };
    default: return { x: point.x + distance, y: point.y };
  }
}

export function uniqueNumbers(values: number[]): number[] {
  const seen = new Set<number>();
  return values.filter((value) => {
    const rounded = Math.round(value * 2) / 2;
    if (seen.has(rounded)) return false;
    seen.add(rounded);
    return true;
  });
}

export function simplifyRoute(points: RoutingPoint[]): RoutingPoint[] {
  const deduplicated = points.filter((point, index) => index === 0 || point.x !== points[index - 1].x || point.y !== points[index - 1].y);
  return deduplicated.filter((point, index) => {
    if (index === 0 || index === deduplicated.length - 1) return true;
    const previous = deduplicated[index - 1];
    const next = deduplicated[index + 1];
    return !((previous.x === point.x && point.x === next.x) || (previous.y === point.y && point.y === next.y));
  });
}

export function routeScore(points: RoutingPoint[], obstacles: RoutingObstacle[]): number {
  let length = 0;
  let intersections = 0;
  for (let index = 1; index < points.length; index += 1) {
    const start = points[index - 1];
    const end = points[index];
    length += Math.abs(end.x - start.x) + Math.abs(end.y - start.y);
    intersections += obstacles.filter((obstacle) => segmentIntersectsObstacle(start, end, obstacle)).length;
  }
  return intersections * 1_000_000 + length + Math.max(0, points.length - 2) * 28;
}

export function segmentIntersectsObstacle(start: RoutingPoint, end: RoutingPoint, obstacle: RoutingObstacle): boolean {
  const right = obstacle.x + obstacle.width;
  const bottom = obstacle.y + obstacle.height;
  if (start.x === end.x) {
    return start.x > obstacle.x && start.x < right
      && Math.max(Math.min(start.y, end.y), obstacle.y) < Math.min(Math.max(start.y, end.y), bottom);
  }
  if (start.y === end.y) {
    return start.y > obstacle.y && start.y < bottom
      && Math.max(Math.min(start.x, end.x), obstacle.x) < Math.min(Math.max(start.x, end.x), right);
  }
  return true;
}

export function routeLabelPoint(points: RoutingPoint[]): RoutingPoint {
  let best = { x: (points[0]?.x ?? 0), y: (points[0]?.y ?? 0), length: -1, horizontal: false };
  for (let index = 1; index < points.length; index += 1) {
    const start = points[index - 1];
    const end = points[index];
    const horizontal = start.y === end.y;
    const length = Math.abs(end.x - start.x) + Math.abs(end.y - start.y);
    const weightedLength = horizontal ? length + 10_000 : length;
    const bestWeightedLength = best.horizontal ? best.length + 10_000 : best.length;
    if (weightedLength <= bestWeightedLength) continue;
    best = { x: (start.x + end.x) / 2, y: (start.y + end.y) / 2, length, horizontal };
  }
  return { x: best.x, y: best.y - (best.horizontal ? 10 : 0) };
}

type SequenceMessageRuntimeData = DiagramEdge['data'] & {
  onVerticalMoveStart?: (edgeId: string) => void;
  onVerticalMove?: (edgeId: string, clientY: number) => void;
  onVerticalMoveEnd?: () => void;
  onSelect?: (edgeId: string) => void;
};

export function SequenceMessageEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  markerStart,
  markerEnd,
  style,
  label,
  labelStyle,
  labelBgStyle,
  labelBgPadding,
  labelBgBorderRadius,
  interactionWidth,
  data
}: EdgeProps) {
  const edgePath = `M ${sourceX} ${sourceY} L ${targetX} ${sourceY}`;
  const labelX = (sourceX + targetX) / 2;
  const runtime = data as SequenceMessageRuntimeData | undefined;
  const runtimeRef = useRef(runtime);
  const dragCleanupRef = useRef<(() => void) | undefined>(undefined);
  const [dragging, setDragging] = useState(false);
  runtimeRef.current = runtime;
  useEffect(() => () => dragCleanupRef.current?.(), []);
  return <>
    <BaseEdge
      id={id}
      path={edgePath}
      labelX={labelX}
      labelY={sourceY}
      markerStart={markerStart}
      markerEnd={markerEnd}
      style={style}
      label={label}
      labelStyle={labelStyle}
      labelBgStyle={labelBgStyle}
      labelBgPadding={labelBgPadding}
      labelBgBorderRadius={labelBgBorderRadius}
      interactionWidth={interactionWidth}
    />
    <path
      className={`sequence-edge-drag-zone nodrag nopan ${dragging ? 'dragging' : ''}`}
      d={edgePath}
      onMouseDown={(event) => {
        event.preventDefault();
        event.stopPropagation();
        dragCleanupRef.current?.();
        setDragging(true);
        runtime?.onVerticalMoveStart?.(id);
        const onMouseMove = (moveEvent: MouseEvent) => runtimeRef.current?.onVerticalMove?.(id, moveEvent.clientY);
        const finish = () => {
          window.removeEventListener('mousemove', onMouseMove);
          window.removeEventListener('mouseup', finish);
          dragCleanupRef.current = undefined;
          setDragging(false);
          runtimeRef.current?.onVerticalMoveEnd?.();
        };
        dragCleanupRef.current = () => {
          window.removeEventListener('mousemove', onMouseMove);
          window.removeEventListener('mouseup', finish);
        };
        window.addEventListener('mousemove', onMouseMove);
        window.addEventListener('mouseup', finish, { once: true });
      }}
      onClick={(event) => {
        event.stopPropagation();
        runtime?.onSelect?.(id);
      }}
      aria-label="上下拖动消息线"
      role="button"
      tabIndex={0}
    />
    <rect
      className={`sequence-edge-drag-indicator ${dragging ? 'dragging' : ''}`}
      x={labelX - 14}
      y={sourceY - 2}
      width={28}
      height={4}
      rx={2}
    />
  </>;
}
