import { useEffect, type ReactNode } from "react";
import { useWorkspaceIndexStore } from "../application/workspaceIndexStore";
import type { WorkspaceSelection } from "../domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";
import { useRemoteConnectionStore } from "../../remote-connection/application/remoteConnectionStore";
import type { RemoteConnectionPort } from "../../remote-connection/ports/RemoteConnectionPort";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { interfaceText } from "../../../shared/interfaceLocale";

interface WorkspaceSidebarProps {
  ownerUserId: string;
  hostReady: boolean;
  workspaceIndex: WorkspaceIndexPort;
  remoteConnection: RemoteConnectionPort;
  selection: WorkspaceSelection;
  onSelectionChange(selection: WorkspaceSelection): void;
  onCreateProject(): void;
  onCreateRemoteConnection(): void;
}

export function WorkspaceSidebar({
  ownerUserId,
  hostReady,
  workspaceIndex,
  remoteConnection,
  selection,
  onSelectionChange,
  onCreateProject,
  onCreateRemoteConnection,
}: WorkspaceSidebarProps) {
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const text = (key: Parameters<typeof interfaceText>[1]) => interfaceText(language, key);
  const snapshot = useWorkspaceIndexStore((state) => state.snapshot);
  const isLoading = useWorkspaceIndexStore((state) => state.isLoading);
  const errorMessage = useWorkspaceIndexStore((state) => state.errorMessage);
  const load = useWorkspaceIndexStore((state) => state.load);
  const remoteConnections = useRemoteConnectionStore((state) => state.connections);
  const remoteIsLoading = useRemoteConnectionStore((state) => state.isLoading);
  const remoteError = useRemoteConnectionStore((state) => state.errorMessage);
  const activateRemoteConnections = useRemoteConnectionStore((state) => state.activate);
  const suspendRemoteConnections = useRemoteConnectionStore((state) => state.suspend);
  const resetRemoteConnections = useRemoteConnectionStore((state) => state.reset);
  const refreshRemoteConnections = useRemoteConnectionStore((state) => state.refresh);

  useEffect(() => {
    void load(workspaceIndex, ownerUserId, hostReady);
  }, [hostReady, load, ownerUserId, workspaceIndex]);

  useEffect(() => {
    if (hostReady) {
      void activateRemoteConnections(remoteConnection, ownerUserId);
    } else {
      const activeOwner = useRemoteConnectionStore.getState().ownerUserId;
      if (activeOwner && activeOwner !== ownerUserId) resetRemoteConnections();
      else suspendRemoteConnections(ownerUserId);
    }
  }, [
    activateRemoteConnections,
    hostReady,
    ownerUserId,
    remoteConnection,
    resetRemoteConnections,
    suspendRemoteConnections,
  ]);

  return (
    <nav className="workspace-navigation" aria-label={text("resourceNavigation")}>
      <div className="workspace-navigation__toolbar">
        <span>{text("resourceNavigation")}</span>
        <button
          aria-label={text("refreshResources")}
          className="workspace-navigation__refresh"
          disabled={isLoading || remoteIsLoading}
          onClick={() => {
            void load(workspaceIndex, ownerUserId, hostReady);
            if (hostReady) void refreshRemoteConnections(remoteConnection);
          }}
          title={text("refreshResources")}
          type="button"
        >↻</button>
      </div>
      <button
        className={`nav-item ${selection.kind === "workbench" ? "nav-item--active" : ""}`}
        onClick={() => onSelectionChange({ kind: "workbench" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">⌂</span>
        {text("workbench")}
      </button>
      <button
        className={`nav-item ${selection.kind === "agentManagement" ? "nav-item--active" : ""}`}
        onClick={() => onSelectionChange({ kind: "agentManagement" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">◎</span>
        {text("agentManagement")}
      </button>
      <button
        className={`nav-item ${selection.kind === "requirementSurveys" ? "nav-item--active" : ""}`}
        disabled={!hostReady}
        onClick={() => onSelectionChange({ kind: "requirementSurveys" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">☷</span>
        {text("requirementSurveys")}
      </button>
      <button
        className={`nav-item ${selection.kind === "notepad" ? "nav-item--active" : ""}`}
        disabled={!hostReady}
        onClick={() => onSelectionChange({ kind: "notepad" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">▤</span>
        {text("notepad")}
      </button>
      <button
        className={`nav-item ${selection.kind === "terminal" ? "nav-item--active" : ""}`}
        disabled={!hostReady}
        onClick={() => onSelectionChange({ kind: "terminal" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">⌘</span>
        {text("localTerminal")}
      </button>
      <button
        className={`nav-item ${selection.kind === "mediaStudio" ? "nav-item--active" : ""}`}
        onClick={() => onSelectionChange({ kind: "mediaStudio" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">✦</span>
        {text("mediaStudio")}
      </button>
      <button
        className={`nav-item ${selection.kind === "applications" || selection.kind === "application" ? "nav-item--active" : ""}`}
        disabled={!hostReady}
        onClick={() => onSelectionChange({ kind: "applications" })}
        type="button"
      >
        <span aria-hidden="true" className="nav-symbol">▦</span>
        {text("applications")}
      </button>

      <SidebarSection title={text("contacts")}>
        {isLoading && snapshot.contacts.length === 0 && <LoadingRow title={text("loadingContacts")} />}
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
        title={text("remoteConnections")}
        action={(
          <button
            aria-label={text("newRemoteConnection")}
            className="sidebar-section__action"
            disabled={!hostReady}
            onClick={onCreateRemoteConnection}
            title={text("newRemoteConnection")}
            type="button"
          >＋</button>
        )}
      >
        {remoteIsLoading && remoteConnections.length === 0 ? (
          <LoadingRow title={text("loadingRemoteConnections")} />
        ) : remoteConnections.length === 0 ? (
          <p className="sidebar-empty">{text("emptyRemoteConnections")}</p>
        ) : remoteConnections.map((connection) => (
          <ResourceButton
            key={connection.id}
            active={selection.kind === "remote" && selection.id === connection.id}
            icon="⌘"
            item={{
              title: connection.name,
              subtitle: `${connection.username}@${connection.host}:${connection.port}`,
            }}
            onSelect={() => onSelectionChange({ kind: "remote", id: connection.id })}
          />
        ))}
      </SidebarSection>

      <SidebarSection
        title={text("projects")}
        action={(
          <button aria-label={text("newProject")} className="sidebar-section__action" onClick={onCreateProject} title={text("newProject")} type="button">＋</button>
        )}
      >
        {isLoading && snapshot.projects.length === 0 ? (
          <LoadingRow title={text("loadingProjects")} />
        ) : snapshot.projects.length === 0 ? (
          <p className="sidebar-empty">{text("emptyProjects")}</p>
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
          <strong>{text("resourceSyncFailed")}</strong>
          <span>{errorMessage}</span>
          <button onClick={() => void load(workspaceIndex, ownerUserId, hostReady)} type="button">
            {text("retry")}
          </button>
        </div>
      )}
      {remoteError && (
        <div className="sidebar-error" role="status">
          <strong>{text("remoteLoadFailed")}</strong>
          <span>{remoteError}</span>
          <button onClick={() => void refreshRemoteConnections(remoteConnection)} type="button">
            {text("retry")}
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

