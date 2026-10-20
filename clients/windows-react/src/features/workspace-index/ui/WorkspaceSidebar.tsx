import { useEffect, useState, type ReactNode } from "react";
import { useWorkspaceIndexStore } from "../application/workspaceIndexStore";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";

interface WorkspaceSidebarProps {
  ownerUserId: string;
  hostReady: boolean;
  workspaceIndex: WorkspaceIndexPort;
}

export function WorkspaceSidebar({
  ownerUserId,
  hostReady,
  workspaceIndex,
}: WorkspaceSidebarProps) {
  const snapshot = useWorkspaceIndexStore((state) => state.snapshot);
  const isLoading = useWorkspaceIndexStore((state) => state.isLoading);
  const errorMessage = useWorkspaceIndexStore((state) => state.errorMessage);
  const load = useWorkspaceIndexStore((state) => state.load);
  const [selection, setSelection] = useState<string>("workbench");

  useEffect(() => {
    void load(workspaceIndex, ownerUserId, hostReady);
  }, [hostReady, load, ownerUserId, workspaceIndex]);

  return (
    <nav className="workspace-navigation" aria-label="资源导航">
      <button
        className={`nav-item ${selection === "workbench" ? "nav-item--active" : ""}`}
        onClick={() => setSelection("workbench")}
        type="button"
      >
        <span className="nav-symbol">⌂</span>
        工作台
      </button>

      <SidebarSection title="联系人">
        {isLoading && snapshot.contacts.length === 0 && <LoadingRow title="正在加载联系人…" />}
        {snapshot.contacts.map((contact) => (
          <ResourceButton
            key={contact.id}
            active={selection === `contact:${contact.id}`}
            icon={contact.id === "plugin-installer" ? "◇" : "●"}
            item={contact}
            onSelect={() => setSelection(`contact:${contact.id}`)}
          />
        ))}
      </SidebarSection>

      <SidebarSection title="项目">
        {isLoading && snapshot.projects.length === 0 ? (
          <LoadingRow title="正在加载项目…" />
        ) : snapshot.projects.length === 0 ? (
          <p className="sidebar-empty">项目保存在本机。创建能力将在项目切片验收后开放。</p>
        ) : (
          snapshot.projects.map((project) => (
            <ResourceButton
              key={project.id}
              active={selection === `project:${project.id}`}
              icon="▱"
              item={project}
              onSelect={() => setSelection(`project:${project.id}`)}
            />
          ))
        )}
      </SidebarSection>

      {errorMessage && (
        <div className="sidebar-error" role="status">
          <strong>资源同步失败</strong>
          <span>{errorMessage}</span>
          <button onClick={() => void load(workspaceIndex, ownerUserId, hostReady)} type="button">
            重试
          </button>
        </div>
      )}
    </nav>
  );
}

function SidebarSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="sidebar-section">
      <h2>{title}</h2>
      <div className="sidebar-section__items">{children}</div>
    </section>
  );
}

function ResourceButton({
  item,
  icon,
  active,
  onSelect,
}: {
  item: { title: string; subtitle: string | null };
  icon: string;
  active: boolean;
  onSelect(): void;
}) {
  return (
    <button
      className={`resource-item ${active ? "resource-item--active" : ""}`}
      onClick={onSelect}
      type="button"
    >
      <span className="resource-item__icon" aria-hidden="true">{icon}</span>
      <span className="resource-item__copy">
        <strong>{item.title}</strong>
        {item.subtitle && <small>{item.subtitle}</small>}
      </span>
    </button>
  );
}

function LoadingRow({ title }: { title: string }) {
  return (
    <div className="sidebar-loading">
      <span className="mini-spinner" aria-hidden="true" />
      <span>{title}</span>
    </div>
  );
}

