import type { OpenInspectorPhase } from "../application/runInspectorStore";
import {
  isTaskExecutionActive,
  taskExecutionPendingMessage,
} from "../domain/runInspector";

export function TaskDetailSection({ phase }: { phase: OpenInspectorPhase }) {
  const detail = phase.taskDetail;
  if (!detail) return <InspectorState loading={phase.isLoadingTask} message="正在加载任务详情…" />;
  const { task, run } = detail;
  const executionActive = isTaskExecutionActive(task.status);
  return (
    <div className="inspector-section">
      <InfoCard title="任务目标">
        <TextValue value={task.objective ?? task.description} fallback="这个任务没有额外目标说明。" />
      </InfoCard>
      {task.description && task.description !== task.objective ? (
        <InfoCard title="说明"><TextValue value={task.description} /></InfoCard>
      ) : null}
      {executionActive ? (
        <InfoCard tone="attention" title={statusLabel(task.status)}>
          <TextValue value={taskExecutionPendingMessage(task.status)} />
        </InfoCard>
      ) : null}
      {!executionActive && run?.reportContent ? (
        <InfoCard title="模型输出"><TextValue value={run.reportContent} /></InfoCard>
      ) : null}
      {!executionActive && task.resultSummary && task.resultSummary.trim() !== run?.reportContent?.trim() ? (
        <InfoCard title="结果摘要"><TextValue value={task.resultSummary} /></InfoCard>
      ) : null}
      <InfoCard title="任务信息">
        <Definition label="状态" value={statusLabel(task.status)} />
        <Definition label="模型配置" value={task.modelConfigRef} mono />
        {task.thinkingLevel ? <Definition label="推理级别" value={task.thinkingLevel} /> : null}
        <Definition label="任务 ID" value={task.taskId} mono />
        <Definition label="创建时间" value={formatDateTime(task.createdAtUnixMs)} />
        <Definition label="更新时间" value={formatDateTime(task.updatedAtUnixMs)} />
      </InfoCard>
      {run?.errorMessage ? (
        <InfoCard tone="error" title="错误"><TextValue value={run.errorMessage} /></InfoCard>
      ) : null}
    </div>
  );
}

export function TaskProcessSection({ phase }: { phase: OpenInspectorPhase }) {
  const detail = phase.taskDetail;
  if (!detail) return <InspectorState loading={phase.isLoadingTask} message="正在加载执行过程…" />;
  if (detail.task.processEntries.length === 0) {
    return <InspectorState message="该任务还没有记录执行过程。" />;
  }
  return (
    <div className="process-timeline">
      {detail.task.processEntries.map((entry) => (
        <article className="process-entry" key={entry.id}>
          <span className="process-entry__marker" />
          <div>
            <header>
              <strong>{entry.heading ?? "执行进展"}</strong>
              <time>{formatDateTime(entry.createdAtUnixMs)}</time>
            </header>
            <p>{entry.content}</p>
          </div>
        </article>
      ))}
    </div>
  );
}

export function TaskRunSection({
  phase,
  onLoadOlder,
}: {
  phase: OpenInspectorPhase;
  onLoadOlder(): void;
}) {
  if (phase.isLoadingSection && !phase.runDetail) {
    return <InspectorState loading message="正在加载最新运行详情…" />;
  }
  const detail = phase.runDetail;
  if (!detail) {
    return <InspectorState message={phase.taskDetail?.task.lastRunId ? "运行详情暂时不可用。" : "该任务还没有 Run。"} />;
  }
  return (
    <div className="inspector-section">
      {detail.run.reportContent ? (
        <InfoCard title="模型输出"><TextValue value={detail.run.reportContent} /></InfoCard>
      ) : null}
      {detail.run.resultSummary
        && detail.run.resultSummary.trim() !== detail.run.reportContent?.trim() ? (
          <InfoCard title="运行结果摘要"><TextValue value={detail.run.resultSummary} /></InfoCard>
        ) : null}
      <InfoCard title="运行信息">
        <Definition label="状态" value={statusLabel(detail.run.status)} />
        <Definition label="迭代" value={`${detail.run.iteration} / ${detail.run.maxIterations}`} />
        <Definition label="Run ID" value={detail.run.runId} mono />
        <Definition label="开始时间" value={formatDateTime(detail.run.createdAtUnixMs)} />
        {detail.run.finishedAtUnixMs != null ? (
          <Definition label="结束时间" value={formatDateTime(detail.run.finishedAtUnixMs)} />
        ) : null}
      </InfoCard>
      {detail.run.errorMessage ? (
        <InfoCard tone="error" title="错误"><TextValue value={detail.run.errorMessage} /></InfoCard>
      ) : null}
      <section className="run-events-card">
        <h4>运行事件</h4>
        {detail.events.length > 0 ? (
          <div className="run-event-list">
            {detail.events.map((event) => (
              <article className="run-event" key={event.eventId}>
                <span className="run-event__dot" />
                <div>
                  <div className="run-event__title">
                    <strong>{eventTitle(event.eventType)}</strong>
                    <time>{formatTime(event.createdAtUnixMs)}</time>
                  </div>
                  {event.summary ? <p>{event.summary}</p> : null}
                  {event.status ? <small>{event.status}</small> : null}
                </div>
              </article>
            ))}
            {detail.eventsHasMore ? (
              <button
                className="history-more"
                disabled={phase.isLoadingOlder}
                onClick={onLoadOlder}
                type="button"
              >
                {phase.isLoadingOlder ? "正在加载…" : "加载更早的运行事件"}
              </button>
            ) : null}
          </div>
        ) : <p className="run-events-empty">该次运行尚未写入诊断事件。</p>}
      </section>
    </div>
  );
}

export function InspectorState({
  loading = false,
  message,
}: {
  loading?: boolean;
  message: string;
}) {
  return (
    <div className="run-detail__state">
      {loading ? <span className="spinner" /> : <span className="empty-symbol">◇</span>}
      <span>{message}</span>
    </div>
  );
}

function InfoCard({
  title,
  tone,
  children,
}: {
  title: string;
  tone?: "attention" | "error";
  children: React.ReactNode;
}) {
  return (
    <section className={tone ? `inspector-card inspector-card--${tone}` : "inspector-card"}>
      <h4>{title}</h4>
      {children}
    </section>
  );
}

function Definition({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="inspector-definition">
      <span>{label}</span>
      <strong className={mono ? "inspector-mono" : undefined}>{value}</strong>
    </div>
  );
}

function TextValue({ value, fallback }: { value: string | null; fallback?: string }) {
  return <p className="inspector-text">{value?.trim() || fallback || "—"}</p>;
}

export function statusLabel(status: string) {
  const labels: Record<string, string> = {
    pending: "等待调度",
    ready: "准备执行",
    queued: "排队中",
    running: "执行中",
    model_ready: "模型准备中",
    model_running: "模型执行中",
    waiting_tool_result: "等待工具",
    continuation_ready: "准备继续",
    waiting_user: "等待用户",
    retry_scheduled: "等待重试",
    paused: "已暂停",
    needs_review: "需要处理",
    reviewing_followup: "审查追加要求",
    waiting_confirmation: "等待确认",
    succeeded: "已完成",
    failed: "任务未完成",
    blocked: "已阻塞",
    cancelled: "已取消",
  };
  return labels[status.trim().toLowerCase()] ?? status;
}

function eventTitle(value: string) {
  return value
    .replaceAll("_", " ")
    .replaceAll(".", " · ")
    .replace(/\b\w/g, (character) => character.toUpperCase());
}

function formatTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(unixMilliseconds));
}

function formatDateTime(unixMilliseconds: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(unixMilliseconds));
}
