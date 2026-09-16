import { useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import ELK from 'elkjs/lib/elk.bundled.js';
import { Background, BackgroundVariant, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, useReactFlow, type NodeProps } from '@xyflow/react';
import type { ExecutionPlan, ExecutionTask, PlanNodePosition } from '../../src/schema';

type TaskNodeData = { task: ExecutionTask; ready: boolean };
const elk = new ELK();
const NODE_WIDTH = 288;
const NODE_HEIGHT = 176;
const COLLISION_GAP = 20;

function statusLabel(task: ExecutionTask, ready: boolean) {
  if (ready) return '可以开始';
  return ({ planned: '等待前置', in_progress: '进行中', blocked: '已阻塞', done: '已完成', cancelled: '已取消' } as const)[task.status];
}

function TaskNode({ data, selected }: NodeProps) {
  const { task, ready } = data as unknown as TaskNodeData;
  return <article className={`plan-node status-${task.status} ${ready ? 'is-ready' : ''} ${selected ? 'selected' : ''}`}>
    <Handle type="target" position={Position.Left} className="plan-handle" />
    <header><span>{task.id}</span><em>{statusLabel(task, ready)}</em></header>
    <strong>{task.title}</strong>
    <p>{task.description || '尚未补充任务说明'}</p>
    <footer><span>{task.phase || '未分阶段'}</span><span>{task.dependsOn.length} 个前置</span><span>{task.acceptanceCriteria.length} 条验收</span></footer>
    <Handle type="source" position={Position.Right} className="plan-handle" />
  </article>;
}

const nodeTypes = { taskNode: TaskNode };

function readyIds(plan: ExecutionPlan): Set<string> {
  const done = new Set(plan.tasks.filter((task) => task.status === 'done').map((task) => task.id));
  return new Set(plan.tasks.filter((task) => task.status === 'planned' && task.dependsOn.every((id) => done.has(id))).map((task) => task.id));
}

async function automaticPositions(plan: ExecutionPlan): Promise<Record<string, PlanNodePosition>> {
  const taskIds = new Set(plan.tasks.map((task) => task.id));
  const graph = await elk.layout({
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.padding': '[top=40,left=40,bottom=40,right=40]',
      'elk.spacing.nodeNode': '72',
      'elk.spacing.edgeNode': '28',
      'elk.layered.spacing.nodeNodeBetweenLayers': '126',
      'elk.layered.spacing.edgeNodeBetweenLayers': '34',
      'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
      'elk.layered.nodePlacement.favorStraightEdges': 'true'
    },
    children: plan.tasks.map((task) => ({ id: task.id, width: NODE_WIDTH, height: NODE_HEIGHT })),
    edges: plan.tasks.flatMap((task) => task.dependsOn.filter((dependency) => taskIds.has(dependency)).map((dependency) => ({ id: `${dependency}->${task.id}`, sources: [dependency], targets: [task.id] })))
  });
  return Object.fromEntries((graph.children ?? []).map((node) => [node.id, { x: node.x ?? 0, y: node.y ?? 0 }]));
}

function positionsOverlap(plan: ExecutionPlan, transientPositions: Record<string, PlanNodePosition>): boolean {
  const positioned = plan.tasks.flatMap((task) => {
    const position = transientPositions[task.id] ?? plan.positions[task.id];
    return position ? [{ id: task.id, ...position }] : [];
  });
  for (let index = 0; index < positioned.length; index += 1) {
    for (let candidateIndex = index + 1; candidateIndex < positioned.length; candidateIndex += 1) {
      const first = positioned[index];
      const second = positioned[candidateIndex];
      if (Math.abs(first.x - second.x) < NODE_WIDTH + COLLISION_GAP && Math.abs(first.y - second.y) < NODE_HEIGHT + COLLISION_GAP) return true;
    }
  }
  return false;
}

export function PlanGraph({ plan, selectedTaskId, onSelect, onPositionsChange }: {
  plan: ExecutionPlan;
  selectedTaskId?: string;
  onSelect: (taskId?: string) => void;
  onPositionsChange: (positions: Record<string, PlanNodePosition>) => void;
}) {
  const [layouting, setLayouting] = useState(false);
  const [transientPositions, setTransientPositions] = useState<Record<string, PlanNodePosition>>({});
  const [expanded, setExpanded] = useState(false);
  const { fitView } = useReactFlow();
  const ready = useMemo(() => readyIds(plan), [plan]);
  const missingPositions = plan.tasks.some((task) => !plan.positions[task.id] && !transientPositions[task.id]);
  const overlappingPositions = positionsOverlap(plan, transientPositions);
  const taskSignature = plan.tasks.map((task) => task.id).sort().join('|');

  function showAutomaticPositions(positions: Record<string, PlanNodePosition>) {
    setTransientPositions(positions);
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => void fitView({ padding: 0.2, duration: 280 })));
  }

  useEffect(() => setTransientPositions({}), [taskSignature]);

  useEffect(() => {
    if (!expanded) return;
    function closeOnEscape(event: KeyboardEvent) { if (event.key === 'Escape') setExpanded(false); }
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [expanded]);

  useEffect(() => {
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => void fitView({ padding: expanded ? 0.12 : 0.2, duration: 260 })));
  }, [expanded, fitView]);

  useEffect(() => {
    if ((!missingPositions && !overlappingPositions) || plan.tasks.length === 0 || layouting) return;
    setLayouting(true);
    void automaticPositions(plan).then((positions) => {
      showAutomaticPositions(positions);
      if (overlappingPositions) onPositionsChange(positions);
    }).finally(() => setLayouting(false));
  }, [missingPositions, overlappingPositions, taskSignature]);

  const nodes = plan.tasks.map((task, index) => ({
    id: task.id,
    type: 'taskNode',
    position: transientPositions[task.id] ?? plan.positions[task.id] ?? { x: (index % 3) * (NODE_WIDTH + 90), y: Math.floor(index / 3) * (NODE_HEIGHT + 72) },
    selected: task.id === selectedTaskId,
    data: { task, ready: ready.has(task.id) }
  }));
  const edges = plan.tasks.flatMap((task) => task.dependsOn.map((dependency) => ({
    id: `${dependency}->${task.id}`,
    source: dependency,
    target: task.id,
    type: 'smoothstep',
    markerEnd: { type: MarkerType.ArrowClosed, width: 17, height: 17 },
    className: 'plan-edge'
  })));

  async function relayout() {
    if (layouting || plan.tasks.length === 0) return;
    setLayouting(true);
    try {
      const positions = await automaticPositions(plan);
      showAutomaticPositions(positions);
      onPositionsChange(positions);
    }
    finally { setLayouting(false); }
  }

  const graph = <section className={`plan-graph-shell ${expanded ? 'is-expanded' : ''}`}>
    <div className="plan-graph-toolbar">
      <div><strong>依赖关系图</strong><span>{ready.size} 个任务可以开始</span></div>
      <div className="plan-graph-actions"><button onClick={() => void relayout()} disabled={layouting}>{layouting ? '正在整理…' : '自动整理'}</button><button onClick={() => setExpanded((value) => !value)}>{expanded ? '退出全屏' : '全屏'}</button></div>
    </div>
    <div className="plan-graph-canvas">
      {plan.tasks.length === 0 ? <div className="empty-canvas"><span>⌘</span><strong>还没有执行任务</strong><p>添加任务后，前置关系会自动渲染成可浏览的流程图。</p></div> : <ReactFlow
        nodes={nodes as never}
        edges={edges as never}
        nodeTypes={nodeTypes as never}
        fitView
        fitViewOptions={{ padding: 0.22 }}
        minZoom={0.25}
        maxZoom={1.8}
        nodesConnectable={false}
        onPaneClick={() => onSelect(undefined)}
        onNodeClick={(_event, node) => onSelect(node.id)}
        onNodeDragStop={(_event, node) => {
          const positions = { ...plan.positions, ...transientPositions, [node.id]: node.position };
          setTransientPositions(positions);
          onPositionsChange(positions);
        }}
      >
        <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} color="var(--grid-dot)" />
        <MiniMap pannable zoomable className="plan-minimap" nodeColor={(node) => {
          const task = (node.data as unknown as TaskNodeData).task;
          return task.status === 'done' ? '#34C759' : task.status === 'blocked' ? '#FF9F0A' : task.status === 'in_progress' ? '#0A84FF' : ready.has(task.id) ? '#5E5CE6' : '#A7ADB7';
        }} />
        <Controls showInteractive={false} />
      </ReactFlow>}
    </div>
  </section>;
  return expanded ? createPortal(graph, document.body) : graph;
}
