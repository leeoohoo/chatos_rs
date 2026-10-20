import type { HostLifecycleSnapshot } from "../domain/hostLifecycle";

interface HostReadyGateProps {
  snapshot: HostLifecycleSnapshot;
  onRetry(): void;
}

const workingPhases = new Set([
  "launching",
  "handshaking",
  "synchronizing",
  "restarting",
]);

export function HostReadyGate({ snapshot, onRetry }: HostReadyGateProps) {
  if (snapshot.ready) {
    return (
      <div className="ready-gate">
        <section className="gate-card">
          <div className="gate-icon" aria-hidden="true">✓</div>
          <h2>本地服务已就绪</h2>
          <p>Host 生命周期切片已经通过协议验证。业务页面将按源码事实矩阵逐项开放。</p>
        </section>
      </div>
    );
  }

  const working = workingPhases.has(snapshot.phase);
  return (
    <div className="ready-gate">
      <section className="gate-card" aria-live="polite">
        <div className="gate-icon" aria-hidden="true">
          {working ? <span className="spinner" /> : "⌁"}
        </div>
        <h2>{working ? "正在准备本地服务" : "本地服务尚未就绪"}</h2>
        <p>{snapshot.detail}</p>
        {snapshot.retryable && !working ? (
          <div className="gate-actions">
            <button className="primary-button" type="button" onClick={onRetry}>重新检查</button>
          </div>
        ) : null}
      </section>
    </div>
  );
}
