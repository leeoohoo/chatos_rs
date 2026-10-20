import { useEffect } from "react";
import { useRunInspectorStore } from "../application/runInspectorStore";
import { graphHasActiveTasks, type RunInspectorSection } from "../domain/runInspector";
import type { RunInspectorLookup, RunInspectorPort } from "../ports/RunInspectorPort";
import { InspectorHeaderActions, InspectorRecoveryActions } from "./RunInspectorActions";
import {
  InspectorState,
  statusLabel,
  TaskDetailSection,
  TaskProcessSection,
  TaskRunSection,
} from "./RunInspectorSections";

const POLL_INTERVAL_MILLISECONDS = 1_500;
const sections: Array<{ id: RunInspectorSection; label: string }> = [
  { id: "detail", label: "任务详情" },
  { id: "process", label: "执行过程" },
  { id: "run", label: "运行详情" },
];

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
  const setSection = useRunInspectorStore((state) => state.setSection);
  const setRetryInstruction = useRunInspectorStore((state) => state.setRetryInstruction);
  const loadOlder = useRunInspectorStore((state) => state.loadOlder);
  const refresh = useRunInspectorStore((state) => state.refresh);
  const retry = useRunInspectorStore((state) => state.retry);
  const cancel = useRunInspectorStore((state) => state.cancel);
  const restart = useRunInspectorStore((state) => state.restart);
  const pollingKey = phase.kind === "open" && graphHasActiveTasks(phase.graph)
    ? `${phase.lookup.userMessageId}:${phase.selectedTaskId}`
    : null;

  useEffect(() => {
    if (!pollingKey) return;
    const timer = window.setInterval(() => {
      void refresh(port);
    }, POLL_INTERVAL_MILLISECONDS);
    return () => window.clearInterval(timer);
  }, [pollingKey, port, refresh]);

  if (phase.kind !== "open") return null;
  const selected = phase.taskDetail?.task;
  return (
    <div className="inspector-backdrop" role="presentation" onMouseDown={close}>
      <section
        aria-label="任务工作区"
        aria-modal="true"
        className="run-inspector"
        onMouseDown={(event) => event.stopPropagation()}
        role="dialog"
      >
        <header className="run-inspector__header">
          <div>
            <p className="eyebrow">TASK WORKSPACE</p>
            <h2>任务工作区</h2>
          </div>
          <div className="run-inspector__header-actions">
            <InspectorHeaderActions phase={phase} onCancel={() => void cancel(port)} />
            <button aria-label="关闭" className="inspector-close" onClick={close} type="button">×</button>
          </div>
        </header>
        <div className="run-inspector__body">
          <nav className="run-task-list" aria-label="任务列表">
            {phase.graph.nodes.map(({ task, depth }) => (
              <button
                className={task.task_id === phase.selectedTaskId ? "run-task run-task--active" : "run-task"}
                key={task.task_id}
                onClick={() => void selectTask(port, task.task_id)}
                style={{ paddingLeft: 10 + Math.min(depth, 4) * 12 }}
                type="button"
              >
                <strong>{task.title}</strong>
                <span>{statusLabel(task.status)}</span>
              </button>
            ))}
          </nav>
          <div className="run-detail">
            <div className="run-detail__summary">
              <div>
                <h3>{selected?.title ?? phase.graph.nodes.find(({ task }) => task.task_id === phase.selectedTaskId)?.task.title ?? "任务"}</h3>
                <p>{statusLabel(selected?.status ?? "loading")}</p>
              </div>
              {phase.taskDetail?.run ? (
                <span className="run-iteration">
                  {phase.taskDetail.run.iteration} / {phase.taskDetail.run.maxIterations}
                </span>
              ) : null}
            </div>
            <div className="inspector-segmented" role="tablist" aria-label="检查内容">
              {sections.map((section) => (
                <button
                  aria-selected={phase.section === section.id}
                  className={phase.section === section.id ? "is-selected" : undefined}
                  key={section.id}
                  onClick={() => void setSection(port, section.id)}
                  role="tab"
                  type="button"
                >
                  {section.label}
                </button>
              ))}
            </div>
            {phase.error ? <div className="inspector-error">{phase.error}</div> : null}
            <div className="run-detail__scroll">
              {phase.isLoadingTask && !phase.taskDetail ? (
                <InspectorState loading message="正在加载任务基础详情…" />
              ) : phase.section === "detail" ? (
                <TaskDetailSection phase={phase} />
              ) : phase.section === "process" ? (
                <TaskProcessSection phase={phase} />
              ) : (
                <TaskRunSection phase={phase} onLoadOlder={() => void loadOlder(port)} />
              )}
              {phase.section === "detail" ? (
                <InspectorRecoveryActions
                  phase={phase}
                  onInstruction={setRetryInstruction}
                  onRestart={() => void restart(port)}
                  onRetry={() => void retry(port)}
                />
              ) : null}
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}
