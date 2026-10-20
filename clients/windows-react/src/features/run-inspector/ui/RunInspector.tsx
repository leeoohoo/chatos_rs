import { useEffect } from "react";
import { useRunInspectorStore } from "../application/runInspectorStore";
import type { RunInspectorLookup, RunInspectorPort } from "../ports/RunInspectorPort";

export function RunInspectorLauncher({
  lookup,
  port,
}: {
  lookup: RunInspectorLookup;
  port: RunInspectorPort;
}) {
  const phase = useRunInspectorStore((state) => state.probes[lookup.userMessageId]);
  const probe = useRunInspectorStore((state) => state.probe);
  const open = useRunInspectorStore((state) => state.open);

  useEffect(() => {
    void probe(port, lookup);
  }, [lookup, port, probe]);

  if (phase?.kind !== "available") return null;
  return (
    <button
      className="run-inspector-launcher"
      onClick={() => void open(port, lookup)}
      type="button"
    >
      <span aria-hidden="true">⌁</span>
      运行详情
    </button>
  );
}

export function RunInspectorSheet({ port }: { port: RunInspectorPort }) {
  const phase = useRunInspectorStore((state) => state.phase);
  const close = useRunInspectorStore((state) => state.close);
  const selectTask = useRunInspectorStore((state) => state.selectTask);
  const loadOlder = useRunInspectorStore((state) => state.loadOlder);
  if (phase.kind !== "open") return null;
  const selectedTask = phase.graph.nodes.find(
    (node) => node.task.task_id === phase.selectedTaskId,
  )?.task;

  return (
    <div className="inspector-backdrop" role="presentation" onMouseDown={close}>
      <section
        aria-label="运行详情"
        aria-modal="true"
        className="run-inspector"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header className="run-inspector__header">
          <div>
            <p className="eyebrow">TASK EXECUTION</p>
            <h2>运行详情</h2>
          </div>
          <button aria-label="关闭" className="inspector-close" onClick={close} type="button">×</button>
        </header>
        <div className="run-inspector__body">
          <nav className="run-task-list" aria-label="任务列表">
            {phase.graph.nodes.map(({ task }) => (
              <button
                className={task.task_id === phase.selectedTaskId ? "run-task run-task--active" : "run-task"}
                key={task.task_id}
                onClick={() => void selectTask(port, task.task_id)}
                type="button"
              >
                <strong>{task.title}</strong>
                <span>{task.status}</span>
              </button>
            ))}
          </nav>
          <div className="run-detail">
            <div className="run-detail__summary">
              <div>
                <h3>{selectedTask?.title ?? "任务"}</h3>
                <p>{phase.detail ? statusLabel(phase.detail.run.status) : selectedTask?.status}</p>
              </div>
              {phase.detail && (
                <span className="run-iteration">
                  {phase.detail.run.iteration} / {phase.detail.run.maxIterations}
                </span>
              )}
            </div>
            {phase.isLoading ? (
              <div className="run-detail__state"><span className="spinner" /><span>正在加载最新执行记录…</span></div>
            ) : phase.error ? (
              <div className="run-detail__state run-detail__state--error">{phase.error}</div>
            ) : phase.detail ? (
              <div className="run-event-list">
                {phase.detail.events.map((event) => (
                  <article className="run-event" key={event.eventId}>
                    <span className="run-event__dot" />
                    <div>
                      <div className="run-event__title">
                        <strong>{eventTitle(event.eventType)}</strong>
                        <time>{formatTime(event.createdAtUnixMs)}</time>
                      </div>
                      {event.summary && <p>{event.summary}</p>}
                      {event.status && <small>{event.status}</small>}
                    </div>
                  </article>
                ))}
                {phase.detail.events.length === 0 && <p className="run-events-empty">还没有执行记录</p>}
                {phase.detail.eventsHasMore && (
                  <button
                    className="history-more"
                    disabled={phase.isLoadingOlder}
                    onClick={() => void loadOlder(port)}
                    type="button"
                  >
                    {phase.isLoadingOlder ? "正在加载…" : "加载更早记录"}
                  </button>
                )}
              </div>
            ) : (
              <div className="run-detail__state">该任务还没有 Run。</div>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}

function eventTitle(value: string) {
  return value
    .replaceAll("_", " ")
    .replaceAll(".", " · ")
    .replace(/\b\w/g, (character) => character.toUpperCase());
}

function statusLabel(status: string) {
  const labels: Record<string, string> = {
    queued: "等待执行",
    model_running: "模型执行中",
    tool_running: "工具执行中",
    waiting_user: "等待用户",
    retry_scheduled: "等待重试",
    paused: "已暂停",
    needs_review: "需要处理",
    succeeded: "已完成",
    failed: "失败",
    cancelled: "已取消",
  };
  return labels[status] ?? status;
}

function formatTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(unixMilliseconds));
}
