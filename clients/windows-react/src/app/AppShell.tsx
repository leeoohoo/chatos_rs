import { useEffect, useState } from "react";
import type { AuthSession } from "../features/authentication/domain/authentication";
import { HostReadyGate } from "../features/host-lifecycle/ui/HostReadyGate";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import type { HostLifecyclePort } from "../features/host-lifecycle/ports/HostLifecyclePort";
import { useProjectConversationStore } from "../features/project-conversation/application/projectConversationStore";
import type { ProjectConversationPort } from "../features/project-conversation/ports/ProjectConversationPort";
import { ProjectConversationPlaceholder } from "../features/project-conversation/ui/ProjectConversationPlaceholder";
import { useWorkspaceIndexStore } from "../features/workspace-index/application/workspaceIndexStore";
import type { WorkspaceSelection } from "../features/workspace-index/domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../features/workspace-index/ports/WorkspaceIndexPort";
import { WorkspaceSidebar } from "../features/workspace-index/ui/WorkspaceSidebar";

interface AppShellProps {
  hostLifecycle: HostLifecyclePort;
  workspaceIndex: WorkspaceIndexPort;
  projectConversation: ProjectConversationPort;
  session: AuthSession;
  onLogout(): Promise<void>;
}

export function AppShell({
  hostLifecycle,
  workspaceIndex,
  projectConversation,
  session,
  onLogout,
}: AppShellProps) {
  const snapshot = useHostLifecycleStore((state) => state.snapshot);
  const diagnostics = useHostLifecycleStore((state) => state.diagnostics);
  const start = useHostLifecycleStore((state) => state.start);
  const retry = useHostLifecycleStore((state) => state.retry);
  const workspaceSnapshot = useWorkspaceIndexStore((state) => state.snapshot);
  const resetProjectConversations = useProjectConversationStore((state) => state.reset);
  const [selection, setSelection] = useState<WorkspaceSelection>({ kind: "workbench" });

  useEffect(() => {
    void start(hostLifecycle, session.user.id);
  }, [hostLifecycle, session.user.id, start]);

  useEffect(() => {
    setSelection({ kind: "workbench" });
    resetProjectConversations(session.user.id);
  }, [resetProjectConversations, session.user.id]);

  useEffect(() => {
    if (selection.kind === "project"
      && !workspaceSnapshot.projects.some((project) => project.id === selection.id)) {
      setSelection({ kind: "workbench" });
    }
    if (selection.kind === "contact"
      && !workspaceSnapshot.contacts.some((contact) => contact.id === selection.id)) {
      setSelection({ kind: "workbench" });
    }
  }, [selection, workspaceSnapshot.contacts, workspaceSnapshot.projects]);

  const selectedProject = selection.kind === "project"
    ? workspaceSnapshot.projects.find((project) => project.id === selection.id)
    : undefined;
  const selectedContact = selection.kind === "contact"
    ? workspaceSnapshot.contacts.find((contact) => contact.id === selection.id)
    : undefined;
  const defaultContactReady = workspaceSnapshot.contacts.some((contact) => contact.id === "jiguli");
  const title = selectedProject?.title ?? selectedContact?.title ?? "工作台";

  return (
    <>
      <aside className="sidebar" aria-label="主导航">
        <div className="brand">
          <span className="brand-mark">C</span>
          <span>ChatOS</span>
        </div>
        <WorkspaceSidebar
          hostReady={snapshot.ready}
          ownerUserId={session.user.id}
          workspaceIndex={workspaceIndex}
          selection={selection}
          onSelectionChange={setSelection}
        />
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
            <h1>{title}</h1>
          </div>
          <span className="protocol-badge">Host v{snapshot.protocolVersion}</span>
        </header>
        {selectedProject ? (
          <ProjectConversationPlaceholder
            ownerUserId={session.user.id}
            project={selectedProject}
            hostReady={snapshot.ready}
            defaultContactReady={defaultContactReady}
            port={projectConversation}
          />
        ) : selectedContact ? (
          <div className="project-state">
            <div className="project-state__card">
              <div className="project-state__icon" aria-hidden="true">◇</div>
              <h2>联系人会话已建立</h2>
              <p>联系人入口来自 macOS 工作区关系。Conversation 时间线尚未通过 React 源码准入。</p>
              {selectedContact.conversationId && <code className="conversation-identity">{selectedContact.conversationId}</code>}
            </div>
          </div>
        ) : (
          <HostReadyGate
            snapshot={snapshot}
            diagnostics={diagnostics}
            onRetry={() => void retry(hostLifecycle)}
          />
        )}
      </section>
    </>
  );
}
