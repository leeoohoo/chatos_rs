import { useEffect } from "react";
import { HostReadyGate } from "../features/host-lifecycle/ui/HostReadyGate";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriHostLifecycleAdapter } from "../features/host-lifecycle/adapters/TauriHostLifecycleAdapter";

const hostLifecycle = new TauriHostLifecycleAdapter();

export function AppShell() {
  const snapshot = useHostLifecycleStore((state) => state.snapshot);
  const diagnostics = useHostLifecycleStore((state) => state.diagnostics);
  const refresh = useHostLifecycleStore((state) => state.refresh);

  useEffect(() => {
    void refresh(hostLifecycle);
  }, [refresh]);

  return (
    <main className="app-window">
      <aside className="sidebar" aria-label="主导航">
        <div className="window-controls" aria-hidden="true">
          <span className="window-dot window-dot--close" />
          <span className="window-dot window-dot--minimize" />
          <span className="window-dot window-dot--zoom" />
        </div>
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
      </aside>
      <section className="content-surface">
        <header className="titlebar">
          <div>
            <p className="eyebrow">LOCAL WORKSPACE</p>
            <h1>工作台</h1>
          </div>
          <span className="protocol-badge">Host v{snapshot.protocolVersion}</span>
        </header>
        <HostReadyGate
          snapshot={snapshot}
          diagnostics={diagnostics}
          onRetry={() => void refresh(hostLifecycle)}
        />
      </section>
    </main>
  );
}
