import { CircleAlert, RefreshCw, X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import type { DemoTask, DemoTaskGraph, TimeMode } from '../types';
import { STATUS_LABELS, TASK_EMPTY_IMAGES } from './constants';

type TaskHistoryFilter = 'all' | DemoTask['status'];

const TASK_HISTORY_FILTERS: Array<{ id: TaskHistoryFilter; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'doing', label: '执行中' },
  { id: 'todo', label: '待处理' },
  { id: 'done', label: '已完成' },
  { id: 'blocked', label: '异常' },
];

const taskMatchesHistoryFilter = (task: DemoTask, filter: TaskHistoryFilter) => (
  filter === 'all' || task.status === filter
);

const buildTaskGraphLayout = (graph: DemoTaskGraph) => {
  const nodeWidth = 250;
  const nodeHeight = 118;
  const horizontalGap = 34;
  const verticalGap = 78;
  const padding = 44;
  const nodeIds = new Set(graph.nodes.map((node) => node.id));
  const indegree = new Map(graph.nodes.map((node) => [node.id, 0]));
  const outgoing = new Map<string, string[]>();
  graph.edges.forEach((edge) => {
    if (!nodeIds.has(edge.source) || !nodeIds.has(edge.target)) return;
    indegree.set(edge.target, (indegree.get(edge.target) || 0) + 1);
    outgoing.set(edge.source, [...(outgoing.get(edge.source) || []), edge.target]);
  });
  const ranks = new Map(graph.nodes.map((node) => [node.id, 0]));
  const queue = graph.nodes.filter((node) => (indegree.get(node.id) || 0) === 0).map((node) => node.id);
  const processed = new Set<string>();
  while (queue.length > 0) {
    const source = queue.shift();
    if (!source || processed.has(source)) continue;
    processed.add(source);
    (outgoing.get(source) || []).forEach((target) => {
      ranks.set(target, Math.max(ranks.get(target) || 0, (ranks.get(source) || 0) + 1));
      const remaining = (indegree.get(target) || 0) - 1;
      indegree.set(target, remaining);
      if (remaining <= 0) queue.push(target);
    });
  }
  graph.nodes.forEach((node) => {
    if (!processed.has(node.id)) ranks.set(node.id, Math.max(0, node.depth));
  });
  const rows = new Map<number, typeof graph.nodes>();
  graph.nodes.forEach((node) => {
    const rank = ranks.get(node.id) || 0;
    rows.set(rank, [...(rows.get(rank) || []), node]);
  });
  const maxColumns = Math.max(1, ...Array.from(rows.values()).map((nodes) => nodes.length));
  const maxRank = Math.max(0, ...Array.from(rows.keys()));
  const contentWidth = padding * 2 + maxColumns * nodeWidth + Math.max(0, maxColumns - 1) * horizontalGap;
  const contentHeight = padding * 2 + (maxRank + 1) * nodeHeight + maxRank * verticalGap;
  const positions = new Map<string, { x: number; y: number }>();
  rows.forEach((nodes, rank) => {
    const rowWidth = nodes.length * nodeWidth + Math.max(0, nodes.length - 1) * horizontalGap;
    const startX = (contentWidth - rowWidth) / 2;
    nodes.forEach((node, index) => {
      positions.set(node.id, {
        x: startX + index * (nodeWidth + horizontalGap),
        y: padding + rank * (nodeHeight + verticalGap),
      });
    });
  });
  return { nodeWidth, nodeHeight, contentWidth, contentHeight, positions };
};

function TaskDependencyGraph({
  graph,
  loading,
  error,
}: {
  graph: DemoTaskGraph;
  loading: boolean;
  error: string | null;
}) {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const layout = useMemo(() => buildTaskGraphLayout(graph), [graph]);
  const focusedNode = graph.nodes.find((node) => node.id === focusedNodeId) || null;

  useEffect(() => {
    setFocusedNodeId(null);
  }, [graph.sourceSessionId, graph.sourceTurnId, graph.sourceUserMessageId]);

  if (loading) {
    return (
      <div className="task-graph-state is-loading">
        <RefreshCw size={30} />
        <b>正在读取真实任务依赖图</b>
        <span>同步旧版任务看板中的节点和前置关系…</span>
      </div>
    );
  }

  if (graph.nodes.length === 0) {
    return (
      <div className="task-graph-state">
        <CircleAlert size={32} />
        <b>这个任务暂时没有流程图记录</b>
        <span>{error || '旧版 Task Runner 没有为这个会话轮次保存依赖关系。'}</span>
      </div>
    );
  }

  return (
    <div className="task-dependency-graph">
      <div className="task-graph-legend">
        <span><i className="is-doing" />执行中</span>
        <span><i className="is-done" />已完成</span>
        <span><i className="is-blocked" />异常</span>
        <em>{graph.nodes.length} 个节点 · {graph.edges.length} 条依赖</em>
      </div>
      {focusedNode ? (
        <div className="task-graph-focus-card">
          <button type="button" aria-label="关闭节点详情" onClick={() => setFocusedNodeId(null)}><X size={13} /></button>
          <span>当前节点</span>
          <b>{focusedNode.title}</b>
          <small>{focusedNode.detail}</small>
        </div>
      ) : null}
      <div className="task-graph-scroll">
        <div className="task-graph-canvas" style={{ width: layout.contentWidth, height: layout.contentHeight }}>
          <svg width={layout.contentWidth} height={layout.contentHeight} aria-hidden>
            <defs>
              <marker id="workspace-task-arrow" markerWidth="10" markerHeight="10" refX="8" refY="5" orient="auto">
                <path d="M0,0 L10,5 L0,10 z" fill="context-stroke" />
              </marker>
            </defs>
            {graph.edges.map((edge) => {
              const source = layout.positions.get(edge.source);
              const target = layout.positions.get(edge.target);
              const sourceNode = graph.nodes.find((node) => node.id === edge.source);
              const targetNode = graph.nodes.find((node) => node.id === edge.target);
              if (!source || !target) return null;
              const startX = source.x + layout.nodeWidth / 2;
              const startY = source.y + layout.nodeHeight;
              const endX = target.x + layout.nodeWidth / 2;
              const endY = target.y;
              const middleY = startY + (endY - startY) / 2;
              const active = sourceNode?.status === 'doing' || targetNode?.status === 'doing';
              return (
                <path
                  key={edge.id}
                  className={active ? 'is-running' : ''}
                  d={`M ${startX} ${startY} C ${startX} ${middleY}, ${endX} ${middleY}, ${endX} ${endY}`}
                  fill="none"
                  markerEnd="url(#workspace-task-arrow)"
                />
              );
            })}
          </svg>
          {graph.nodes.map((node) => {
            const position = layout.positions.get(node.id);
            if (!position) return null;
            return (
              <button
                type="button"
                key={node.id}
                className={`task-graph-node is-${node.status}${focusedNodeId === node.id ? ' is-focused' : ''}${node.isCurrent ? ' is-current-message' : ''}`}
                style={{ left: position.x, top: position.y, width: layout.nodeWidth, height: layout.nodeHeight }}
                onClick={() => setFocusedNodeId((current) => current === node.id ? null : node.id)}
              >
                <span><i />{STATUS_LABELS[node.status]}{node.isRoot ? ' · 根任务' : ''}</span>
                <b>{node.title}</b>
                <small>{node.detail}</small>
                <em>{node.creatorName || 'Agent'} · {node.updatedAt || '时间未知'}</em>
              </button>
            );
          })}
        </div>
      </div>
    </div>
  );
}

export function InWorldTaskWall({
  tasks,
  selectedTask,
  onSelect,
  timeMode,
  graph,
  graphLoading,
  graphError,
  onRefresh,
  onClose,
}: {
  tasks: DemoTask[];
  selectedTask: DemoTask | null;
  onSelect: (task: DemoTask) => void;
  timeMode: TimeMode;
  graph: DemoTaskGraph;
  graphLoading: boolean;
  graphError: string | null;
  onRefresh: () => void;
  onClose: () => void;
}) {
  const [filter, setFilter] = useState<TaskHistoryFilter>('all');
  const filteredTasks = tasks.filter((task) => taskMatchesHistoryFilter(task, filter));
  const activeTask = tasks.find((task) => task.id === selectedTask?.id) || filteredTasks[0] || tasks[0] || null;
  const counts = useMemo(() => ({
    all: tasks.length,
    doing: tasks.filter((task) => task.status === 'doing').length,
    todo: tasks.filter((task) => task.status === 'todo').length,
    done: tasks.filter((task) => task.status === 'done').length,
    blocked: tasks.filter((task) => task.status === 'blocked').length,
  }), [tasks]);

  return (
    <section className={`inworld-task-wall is-focus is-${timeMode}`}>
      <header className="task-center-header">
        <div>
          <span>WORKSPACE TASK CENTER</span>
          <h2>任务运行中心</h2>
          <p>正在执行与历史任务统一查看，流程图来自原 ChatOS Task Runner 依赖关系。</p>
        </div>
        <div className="task-center-header__actions">
          <div className="projection-live"><i /> {counts.doing} RUNNING</div>
          <button type="button" onClick={onRefresh}><RefreshCw size={16} /><span>刷新</span></button>
          <button type="button" onClick={onClose}><X size={17} /><span>返回房间</span></button>
        </div>
      </header>

      <div className="task-center-stats">
        <div><span>全部任务</span><b>{counts.all}</b><small>包含历史记录</small></div>
        <div className="is-doing"><span>正在执行</span><b>{counts.doing}</b><small>实时状态</small></div>
        <div><span>等待处理</span><b>{counts.todo}</b><small>尚未开始</small></div>
        <div className="is-done"><span>已经完成</span><b>{counts.done}</b><small>历史交付</small></div>
        <div className="is-blocked"><span>异常任务</span><b>{counts.blocked}</b><small>阻塞或失败</small></div>
      </div>

      <div className="task-center-body">
        <aside className="task-history-panel">
          <div className="task-history-heading">
            <div><span>TASK HISTORY</span><b>任务记录</b></div>
            <em>{filteredTasks.length} / {tasks.length}</em>
          </div>
          <nav aria-label="任务状态筛选">
            {TASK_HISTORY_FILTERS.map((item) => (
              <button
                type="button"
                key={item.id}
                className={filter === item.id ? 'is-active' : ''}
                onClick={() => {
                  setFilter(item.id);
                  const next = tasks.find((task) => taskMatchesHistoryFilter(task, item.id));
                  if (next) onSelect(next);
                }}
              >
                <span>{item.label}</span>
                <em>{counts[item.id]}</em>
              </button>
            ))}
          </nav>
          <div className="projection-task-list">
            {filteredTasks.map((task) => (
              <button
                type="button"
                key={task.id}
                className={`is-${task.status}${activeTask?.id === task.id ? ' is-active' : ''}`}
                onClick={() => onSelect(task)}
              >
                <i />
                <span>
                  <b>{task.title}</b>
                  <small>{task.detail}</small>
                  <small className="projection-task-meta">{task.conversationTitle || '当前会话'} · {task.updatedAt || task.completedAt || '时间未知'}</small>
                </span>
                <em>{STATUS_LABELS[task.status]}</em>
              </button>
            ))}
            {filteredTasks.length === 0 ? <div className="task-history-empty">这个分类下暂时没有任务</div> : null}
          </div>
        </aside>

        <main className="task-flow-workspace">
          {activeTask ? (
            <>
              <div className="task-flow-heading">
                <div>
                  <span>DEPENDENCY FLOW</span>
                  <h3>{activeTask.title}</h3>
                  <p>{activeTask.detail}</p>
                </div>
                <div className={`task-flow-status is-${activeTask.status}`}>
                  <span>{STATUS_LABELS[activeTask.status]}</span>
                  <b>{activeTask.progress}%</b>
                </div>
              </div>
              <div className="task-flow-meta">
                <span>会话：{activeTask.conversationTitle || '当前会话'}</span>
                <span>创建：{activeTask.createdAt || '时间未知'}</span>
                <span>更新：{activeTask.updatedAt || '时间未知'}</span>
                <span>轮次：{activeTask.conversationTurnId || '无关联轮次'}</span>
              </div>
              <TaskDependencyGraph graph={graph} loading={graphLoading} error={graphError} />
            </>
          ) : (
            <div className={`task-center-empty is-${timeMode}`}>
              <img src={TASK_EMPTY_IMAGES[timeMode]} alt="暂无任务时的默认风景" />
              <div><span>WORKSPACE STANDBY</span><h3>还没有任务记录</h3><p>新任务和已完成的历史任务都会出现在左侧。</p></div>
            </div>
          )}
        </main>
      </div>
    </section>
  );
}
