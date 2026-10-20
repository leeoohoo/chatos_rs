import { useState } from "react";
import {
  isCancellableTaskStatus,
  isRecoverableTaskStatus,
  isRetryableTaskStatus,
} from "../domain/runInspector";
import type { OpenInspectorPhase } from "../application/runInspectorStore";

export function InspectorHeaderActions({
  phase,
  onCancel,
}: {
  phase: OpenInspectorPhase;
  onCancel(): void;
}) {
  const [confirming, setConfirming] = useState(false);
  const task = phase.taskDetail?.task;
  if (!task || !isCancellableTaskStatus(task.status)) return null;
  return confirming ? (
    <div className="inspector-confirm">
      <span>确定停止这个任务？</span>
      <button disabled={phase.action != null} onClick={() => setConfirming(false)} type="button">返回</button>
      <button
        className="inspector-danger-button"
        disabled={phase.action != null}
        onClick={() => { setConfirming(false); onCancel(); }}
        type="button"
      >
        {phase.action === "cancel" ? "正在停止…" : "停止"}
      </button>
    </div>
  ) : (
    <button
      className="inspector-stop-button"
      disabled={phase.action != null}
      onClick={() => setConfirming(true)}
      type="button"
    >
      ◼ 停止
    </button>
  );
}

export function InspectorRecoveryActions({
  phase,
  onInstruction,
  onRetry,
  onRestart,
}: {
  phase: OpenInspectorPhase;
  onInstruction(value: string): void;
  onRetry(): void;
  onRestart(): void;
}) {
  const task = phase.taskDetail?.task;
  if (!task) return null;
  const retryable = isRetryableTaskStatus(task.status);
  const recoverable = isRecoverableTaskStatus(task.status);
  if (!retryable && !recoverable) return null;
  return (
    <section className="inspector-recovery">
      <div>
        <strong>{retryable ? "处理阻塞" : "恢复任务"}</strong>
        <p>
          {retryable
            ? "补充希望本次重试遵循的说明。留空也可以直接重新处理。"
            : "这个任务已经终止，可以创建新的 Run 从当前任务重新开始。"}
        </p>
      </div>
      {retryable ? (
        <textarea
          aria-label="重试说明"
          disabled={phase.action != null}
          maxLength={8_000}
          onChange={(event) => onInstruction(event.target.value)}
          placeholder="例如：使用当前日期重新查询，并汇总已确认的信息"
          value={phase.retryInstruction}
        />
      ) : null}
      <button
        className="primary-button inspector-action-button"
        disabled={phase.action != null}
        onClick={retryable ? onRetry : onRestart}
        type="button"
      >
        {phase.action === "retry" || phase.action === "restart"
          ? "正在提交…"
          : retryable ? "重新运行" : "恢复并重新运行"}
      </button>
    </section>
  );
}
