import { useEffect, useMemo, useState } from 'react';
import ELK from 'elkjs/lib/elk.bundled.js';
import { Background, BackgroundVariant, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, useReactFlow, type NodeProps } from '@xyflow/react';
import type { ExecutionPlan, ExecutionTask, PlanNodePosition } from '../../src/schema';

type TaskNodeData = { task: ExecutionTask; ready: boolean };
const elk = new ELK();

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
  const graph = await elk.layout({
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.spacing.nodeNode': '34',
      'elk.layered.spacing.nodeNodeBetweenLayers': '74',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX'
    },
    children: plan.tasks.map((task) => ({ id: task.id, width: 268, height: 154 })),
    edges: plan.tasks.flatMap((task) => task.dependsOn.map((dependency) => ({ id: `${dependency}->${task.id}`, sources: [dependency], targets: [task.id] })))
  });
  return Object.fromEntries((graph.children ?? []).map((node) => [node.id, { x: node.x ?? 0, y: node.y ?? 0 }]));
}

export function PlanGraph({ plan, selectedTaskId, onSelect, onPositionsChange }: {
  plan: ExecutionPlan;
  selectedTaskId?: string;
  onSelect: (taskId?: string) => void;
  onPositionsChange: (positions: Record<string, PlanNodePosition>) => void;
}) {
  const [layouting, setLayouting] = useState(false);
  const [transientPositions, setTransientPositions] = useState<Record<string, PlanNodePosition>>({});
  const { fitView } = useReactFlow();
  const ready = useMemo(() => readyIds(plan), [plan]);
  const missingPositions = plan.tasks.some((task) => !plan.positions[task.id] && !transientPositions[task.id]);

  function showAutomaticPositions(positions: Record<string, PlanNodePosition>) {
    setTransientPositions(positions);
    window.requestAnimationFrame(() => window.requestAnimationFrame(() => void fitView({ padding: 0.22, duration: 240 })));
  }

  useEffect(() => {
    if (!missingPositions || plan.tasks.length === 0 || layouting) return;
    setLayouting(true);
    void automaticPositions(plan).then(showAutomaticPositions).finally(() => setLayouting(false));
  }, [missingPositions, plan.tasks.length]);

  const nodes = plan.tasks.map((task, index) => ({
    id: task.id,
    type: 'taskNode',
    position: plan.positions[task.id] ?? transientPositions[task.id] ?? { x: (index % 3) * 330, y: Math.floor(index / 3) * 210 },
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

  return <section className="plan-graph-shell">
    <div className="plan-graph-toolbar">
      <div><strong>依赖关系图</strong><span>{ready.size} 个任务可以开始</span></div>
      <button onClick={() => void relayout()} disabled={layouting}>{layouting ? '正在整理…' : '自动整理'}</button>
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
        onNodeDragStop={(_event, node) => onPositionsChange({ ...transientPositions, ...plan.positions, [node.id]: node.position })}
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
}
