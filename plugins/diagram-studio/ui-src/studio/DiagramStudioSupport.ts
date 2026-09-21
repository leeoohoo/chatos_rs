import type { DiagramDocument, DiagramEdge, DiagramKind, DiagramNode } from '../../src/schema';
import {
  parseSequenceActivationHandle,
  parseSequenceSlot,
  sequenceActivationHandleId,
  sequenceActivationSlotCount,
  sequenceActivationSlotPercentage,
  sequenceLifelineSlotCount,
  sequenceSlotPercentage,
  type SequenceActivationSide
} from '../../src/sequence';
import type { PaletteItem } from './TemplateSidebar';
import { mindMapNodeSize } from '../../src/mindmap';

export function kindLabel(kind: DiagramKind): string {
  return ({ architecture: '架构图', flowchart: '流程图', swimlane: '泳道图', topology: '拓扑图', sequence: '时序图', mindmap: '思维导图' })[kind];
}

export function supportsPlantUml(_kind: DiagramKind): boolean {
  return true;
}

export function plantUmlDialectLabel(kind: DiagramKind): string {
  return kind === 'mindmap'
    ? 'MINDMAP'
    : kind === 'sequence'
    ? 'SEQUENCE'
    : kind === 'swimlane'
      ? 'ACTIVITY · PARTITION'
      : kind === 'architecture'
        ? 'COMPONENT'
        : kind === 'topology'
          ? 'DEPLOYMENT'
          : 'ACTIVITY';
}

export function plantUmlDescription(kind: DiagramKind): string {
  return kind === 'mindmap'
    ? 'PlantUML MindMap 与层级分支画布双向转换'
    : kind === 'sequence'
    ? '时序语义与当前画布双向转换'
    : kind === 'swimlane'
      ? 'Activity Partition 与泳道画布双向转换'
      : kind === 'architecture'
        ? 'Component Diagram 与架构画布双向转换'
        : kind === 'topology'
          ? 'Deployment Diagram 与拓扑画布双向转换'
          : 'Activity Diagram 与流程画布双向转换';
}

export function plantUmlEditorHint(kind: DiagramKind): string {
  return kind === 'mindmap'
    ? '使用 @startmindmap、星号层级和 left side 编辑中心主题与左右分支。应用后会按树结构自动排版。'
    : kind === 'sequence'
    ? '修改参与者、消息、激活和组合片段后应用。外部 PlantUML 没有布局信息时会自动排版。'
    : kind === 'swimlane'
      ? '修改泳道、活动和判断分支后应用。partition 或 |泳道| 语法会生成可拖拽的泳道结构。'
      : kind === 'architecture'
        ? '修改 actor、component、interface、database、queue 和依赖关系后应用。外部源码会转换成可拖拽的架构组件。'
        : kind === 'topology'
          ? '修改 node、cloud、database、storage、artifact 和网络关系后应用。外部源码会转换成可编辑的拓扑节点。'
          : '修改活动、判断与分支后应用。start、if/else/endif 和 stop 会生成对应的可编辑流程组件。';
}

export function kindIcon(kind: DiagramKind): 'architecture' | 'flowchart' | 'swimlane' | 'topology' | 'sequence' | 'mindmap' {
  return kind;
}

export function formatUpdatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '最近更新';
  const today = new Date();
  const sameDay = date.getFullYear() === today.getFullYear()
    && date.getMonth() === today.getMonth()
    && date.getDate() === today.getDate();
  return sameDay
    ? `今天 ${date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}`
    : date.toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
}

export function defaultNodeSize(node: DiagramNode): { width: number; height: number } {
  if (node.data.shape === 'lifeline') return { width: 160, height: 560 };
  if (node.data.shape === 'activation') return { width: 14, height: 120 };
  if (node.data.shape === 'fragment') return { width: 620, height: 220 };
  if (node.data.shape === 'container') return { width: 300, height: 180 };
  if (node.data.shape === 'mindmap-root' || node.data.shape === 'mindmap-topic') return mindMapNodeSize(node);
  if (node.data.shape === 'lane') return { width: 900, height: 180 };
  if (node.data.icon && node.data.showLabel === false) return { width: 58, height: 58 };
  if (node.data.shape === 'text') return { width: 120, height: 34 };
  if (node.data.showLabel === false) {
    if (node.data.shape === 'circle') return { width: 72, height: 72 };
    if (node.data.shape === 'diamond') return { width: 96, height: 72 };
    if (node.data.shape === 'cylinder') return { width: 120, height: 58 };
    return { width: 132, height: 56 };
  }
  if (node.data.shape === 'circle') return { width: 104, height: 104 };
  if (node.data.shape === 'diamond') return { width: 138, height: 100 };
  if (node.data.shape === 'cylinder') return { width: 164, height: 82 };
  return { width: 168, height: 68 };
}

export function newComponentSize(item: PaletteItem): { width: number; height: number } {
  if (item.width && item.height) return { width: item.width, height: item.height };
  if (item.icon) return { width: 58, height: 58 };
  if (item.shape === 'text') return { width: 120, height: 34 };
  if (item.shape === 'mindmap-root') return { width: 200, height: 64 };
  if (item.shape === 'mindmap-topic') return { width: 150, height: 46 };
  if (item.shape === 'circle') return { width: 72, height: 72 };
  if (item.shape === 'diamond') return { width: 96, height: 72 };
  if (item.shape === 'cylinder') return { width: 120, height: 58 };
  return { width: 132, height: 56 };
}

export function balancedMindMapSide(document: DiagramDocument, rootId: string): 'left' | 'right' {
  let left = 0;
  let right = 0;
  for (const edge of document.edges.filter((candidate) => candidate.source === rootId)) {
    const child = document.nodes.find((node) => node.id === edge.target);
    if (child?.data.mindmapSide === 'left') left += 1;
    else right += 1;
  }
  return right <= left ? 'right' : 'left';
}

export function connectionEndClientPosition(event: MouseEvent | TouchEvent): { x: number; y: number } | undefined {
  if ('clientX' in event) return { x: event.clientX, y: event.clientY };
  const touch = event.changedTouches[0] ?? event.touches[0];
  return touch ? { x: touch.clientX, y: touch.clientY } : undefined;
}

export function findActivationAt(nodes: DiagramNode[], lifeline: DiagramNode, slot: number): DiagramNode | undefined {
  const lifelinePosition = absoluteNodePosition(nodes, lifeline);
  const lifelineWidth = lifeline.width ?? 160;
  const lifelineHeight = lifeline.height ?? 560;
  const connectionPoint = {
    x: lifelinePosition.x + lifelineWidth / 2,
    y: lifelinePosition.y + lifelineHeight * sequenceSlotPercentage(slot) / 100
  };
  return nodes.find((node) => {
    if (node.data.shape !== 'activation') return false;
    const position = absoluteNodePosition(nodes, node);
    const width = node.width ?? 14;
    const height = node.height ?? 96;
    return connectionPoint.x >= position.x - 4
      && connectionPoint.x <= position.x + width + 4
      && connectionPoint.y >= position.y - 4
      && connectionPoint.y <= position.y + height + 4;
  });
}

export function sequenceEndpointY(nodes: DiagramNode[], nodeId: string, handleId?: string | null): number | undefined {
  const node = nodes.find((candidate) => candidate.id === nodeId);
  if (!node) return undefined;
  const position = absoluteNodePosition(nodes, node);
  const height = node.height ?? defaultNodeSize(node).height;
  if (node.data.shape === 'lifeline') {
    const slot = parseSequenceSlot(handleId);
    return position.y + height * (slot === undefined ? 50 : sequenceSlotPercentage(slot)) / 100;
  }
  if (node.data.shape === 'activation') {
    const handle = parseSequenceActivationHandle(handleId);
    return position.y + height * (handle ? sequenceActivationSlotPercentage(handle.slot, handle.version) : 50) / 100;
  }
  return position.y + height / 2;
}

export function closestActivationHandle(
  activation: DiagramNode,
  nodes: DiagramNode[],
  side: SequenceActivationSide,
  connectionY?: number
): string {
  if (connectionY === undefined) return sequenceActivationHandleId(side, Math.floor(sequenceActivationSlotCount / 2));
  const position = absoluteNodePosition(nodes, activation);
  const height = activation.height ?? 96;
  const percentage = Math.max(0, Math.min(100, (connectionY - position.y) * 100 / height));
  const slot = Math.round(percentage * (sequenceActivationSlotCount - 1) / 100);
  return sequenceActivationHandleId(side, slot);
}

export function closestLifelineSlot(lifeline: DiagramNode, nodes: DiagramNode[], connectionY: number): number {
  const position = absoluteNodePosition(nodes, lifeline);
  const height = lifeline.height ?? 560;
  const percentage = Math.max(12, Math.min(98, (connectionY - position.y) * 100 / height));
  return Math.round((percentage - 12) * (sequenceLifelineSlotCount - 1) / 86);
}

export function closestSequenceHandleAtY(
  node: DiagramNode,
  nodes: DiagramNode[],
  currentHandle: string | undefined,
  connectionY: number
): string | undefined {
  if (node.data.shape === 'lifeline') return `slot-${closestLifelineSlot(node, nodes, connectionY)}`;
  if (node.data.shape === 'activation') {
    const current = parseSequenceActivationHandle(currentHandle);
    return closestActivationHandle(node, nodes, current?.side ?? 'left', connectionY);
  }
  return currentHandle;
}


export function absoluteNodePosition(nodes: DiagramNode[], node: DiagramNode): { x: number; y: number } {
  if (!node.parentId) return node.position;
  const parent = nodes.find((candidate) => candidate.id === node.parentId);
  if (!parent) return node.position;
  const parentPosition = absoluteNodePosition(nodes, parent);
  return { x: parentPosition.x + node.position.x, y: parentPosition.y + node.position.y };
}

export function safeFileName(value: string): string {
  return value.trim().replace(/[\\/:*?"<>|]+/g, '-').replace(/\s+/g, ' ') || 'diagram';
}

export function downloadBlob(blob: Blob, fileName: string) {
  const url = URL.createObjectURL(blob);
  downloadDataUrl(url, fileName);
  window.setTimeout(() => URL.revokeObjectURL(url), 2000);
}

export function downloadDataUrl(url: string, fileName: string) {
  const anchor = window.document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
}

export function resolvedCanvasColor(): string {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? '#13151A' : '#FBFCFF';
}
