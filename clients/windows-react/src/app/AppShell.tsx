import { useEffect } from "react";
import type { AuthSession } from "../features/authentication/domain/authentication";
import { HostReadyGate } from "../features/host-lifecycle/ui/HostReadyGate";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import type { HostLifecyclePort } from "../features/host-lifecycle/ports/HostLifecyclePort";

interface AppShellProps {
  hostLifecycle: HostLifecyclePort;
  session: AuthSession;
  onLogout(): Promise<void>;
}

export function AppShell({ hostLifecycle, session, onLogout }: AppShellProps) {
  const snapshot = useHostLifecycleStore((state) => state.snapshot);
  const diagnostics = useHostLifecycleStore((state) => state.diagnostics);
  const start = useHostLifecycleStore((state) => state.start);
  const retry = useHostLifecycleStore((state) => state.retry);

  useEffect(() => {
    void start(hostLifecycle, session.user.id);
  }, [hostLifecycle, session.user.id, start]);

  return (
    <>
      <aside className="sidebar" aria-label="主导航">
        <div className="brand">
          <span className="brand-mark">C</span>
          <span>ChatOS</span>
        </div>
        <nav className="nav-list">
          <button className="nav-item nav-item--active" type="button">
            <span className="nav-symbol">⌂</span>
            工作台
          </button>
        </nav>
        <div className="sidebar-status">
          <span className={`status-light status-light--${snapshot.phase}`} />
          <span>{snapshot.label}</span>
        </div>
        <button className="account-button" onClick={() => void onLogout()} type="button">
          <span className="account-avatar">{(session.user.displayName ?? session.user.username).slice(0, 1).toUpperCase()}</span>
          <span className="account-copy">
            <strong>{session.user.displayName ?? session.user.username}</strong>
            <small>退出登录</small>
          </span>
        </button>
      </aside>
      <section className="content-surface">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">LOCAL WORKSPACE</p>
            <h1>工作台</h1>
          </div>
          <span className="protocol-badge">Host v{snapshot.protocolVersion}</span>
        </header>
        <HostReadyGate
          snapshot={snapshot}
          diagnostics={diagnostics}
          onRetry={() => void retry(hostLifecycle)}
        />
      </section>
    </>
  );
}
