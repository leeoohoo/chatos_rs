import { useEffect, type ReactNode } from "react";
import { useWorkspaceIndexStore } from "../application/workspaceIndexStore";
import type { WorkspaceSelection } from "../domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";

interface WorkspaceSidebarProps {
  ownerUserId: string;
  hostReady: boolean;
  workspaceIndex: WorkspaceIndexPort;
  selection: WorkspaceSelection;
  onSelectionChange(selection: WorkspaceSelection): void;
  onCreateProject(): void;
}

export function WorkspaceSidebar({
  ownerUserId,
  hostReady,
  workspaceIndex,
  selection,
  onSelectionChange,
  onCreateProject,
}: WorkspaceSidebarProps) {
  const snapshot = useWorkspaceIndexStore((state) => state.snapshot);
  const isLoading = useWorkspaceIndexStore((state) => state.isLoading);
  const errorMessage = useWorkspaceIndexStore((state) => state.errorMessage);
  const load = useWorkspaceIndexStore((state) => state.load);

  useEffect(() => {
    void load(workspaceIndex, ownerUserId, hostReady);
  }, [hostReady, load, ownerUserId, workspaceIndex]);

  return (
    <nav className="workspace-navigation" aria-label="资源导航">
      <button
        className={`nav-item ${selection.kind === "workbench" ? "nav-item--active" : ""}`}
        onClick={() => onSelectionChange({ kind: "workbench" })}
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
            active={selection.kind === "contact" && selection.id === contact.id}
            icon={contact.id === "plugin-installer" ? "◇" : "●"}
            item={contact}
            onSelect={() => onSelectionChange({ kind: "contact", id: contact.id })}
          />
        ))}
      </SidebarSection>

      <SidebarSection
        title="项目"
        action={(
          <button aria-label="新建项目" className="sidebar-section__action" onClick={onCreateProject} title="新建项目" type="button">＋</button>
        )}
      >
        {isLoading && snapshot.projects.length === 0 ? (
          <LoadingRow title="正在加载项目…" />
        ) : snapshot.projects.length === 0 ? (
          <p className="sidebar-empty">项目保存在本机。点击“＋”选择文件夹并创建项目。</p>
        ) : (
          snapshot.projects.map((project) => (
            <ResourceButton
              key={project.id}
              active={selection.kind === "project" && selection.id === project.id}
              icon="▱"
              item={project}
              onSelect={() => onSelectionChange({ kind: "project", id: project.id })}
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

function SidebarSection({
  title,
  action,
  children,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="sidebar-section">
      <header><h2>{title}</h2>{action}</header>
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

