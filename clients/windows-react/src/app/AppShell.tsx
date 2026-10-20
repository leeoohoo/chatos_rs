import { useEffect, useState } from "react";
import type { AuthSession } from "../features/authentication/domain/authentication";
import type { AskUserPort } from "../features/ask-user/ports/AskUserPort";
import type { AccessControlPort } from "../features/access-control/ports/AccessControlPort";
import { AccessControlView } from "../features/access-control/ui/AccessControlView";
import type { ConversationHistoryPort } from "../features/conversation-history/ports/ConversationHistoryPort";
import { ContactConversationView } from "../features/contact-conversation/ui/ContactConversationView";
import type { ConversationComposerPort } from "../features/conversation-composer/ports/ConversationComposerPort";
import type { ConversationAttachmentPort } from "../features/conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationRealtimePort } from "../features/conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../features/conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../features/run-inspector/ports/RunInspectorPort";
import type { SystemPermissionsPort } from "../features/runtime-permissions/ports/SystemPermissionsPort";
import { SystemPermissionsView } from "../features/runtime-permissions/ui/SystemPermissionsView";
import type { ToolApprovalPort } from "../features/tool-approval/ports/ToolApprovalPort";
import type { ToolApprovalSettingsPort } from "../features/tool-approval/ports/ToolApprovalSettingsPort";
import { GlobalToolApprovalOverlay } from "../features/tool-approval/ui/GlobalToolApprovalOverlay";
import { ToolApprovalSettingsView } from "../features/tool-approval/ui/ToolApprovalSettingsView";
import { HostReadyGate } from "../features/host-lifecycle/ui/HostReadyGate";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import type { HostLifecyclePort } from "../features/host-lifecycle/ports/HostLifecyclePort";
import { useProjectConversationStore } from "../features/project-conversation/application/projectConversationStore";
import type { ProjectConversationPort } from "../features/project-conversation/ports/ProjectConversationPort";
import { ProjectConversationPlaceholder } from "../features/project-conversation/ui/ProjectConversationPlaceholder";
import type { ProjectCreationPort } from "../features/project-creation/ports/ProjectCreationPort";
import { CreateProjectSheet } from "../features/project-creation/ui/CreateProjectSheet";
import type { ProjectDirectoryPort } from "../features/project-directory/ports/ProjectDirectoryPort";
import { ProjectWorkspaceSurface } from "../features/project-directory/ui/ProjectWorkspaceSurface";
import type { ProjectGitPort } from "../features/project-git/ports/ProjectGitPort";
import type { ProjectRunPort } from "../features/project-run/ports/ProjectRunPort";
import { useRemoteConnectionStore } from "../features/remote-connection/application/remoteConnectionStore";
import type { RemoteConnectionPort } from "../features/remote-connection/ports/RemoteConnectionPort";
import { RemoteConnectionDetailView } from "../features/remote-connection/ui/RemoteConnectionDetailView";
import { RemoteConnectionEditorSheet } from "../features/remote-connection/ui/RemoteConnectionEditorSheet";
import type { RemoteFilePort } from "../features/remote-files/ports/RemoteFilePort";
import type { RemoteTerminalPort } from "../features/remote-terminal/ports/RemoteTerminalPort";
import { useWorkspaceIndexStore } from "../features/workspace-index/application/workspaceIndexStore";
import type { WorkspaceSelection } from "../features/workspace-index/domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../features/workspace-index/ports/WorkspaceIndexPort";
import { WorkspaceSidebar } from "../features/workspace-index/ui/WorkspaceSidebar";

interface AppShellProps {
  accessControl: AccessControlPort;
  hostLifecycle: HostLifecyclePort;
  workspaceIndex: WorkspaceIndexPort;
  projectConversation: ProjectConversationPort;
  projectCreation: ProjectCreationPort;
  projectDirectory: ProjectDirectoryPort;
  projectGit: ProjectGitPort;
  projectRun: ProjectRunPort;
  remoteConnection: RemoteConnectionPort;
  remoteFile: RemoteFilePort;
  remoteTerminal: RemoteTerminalPort;
  conversationHistory: ConversationHistoryPort;
  conversationComposer: ConversationComposerPort;
  conversationAttachments: ConversationAttachmentPort;
  conversationRealtime: ConversationRealtimePort;
  conversationRuntimeSettings: ConversationRuntimeSettingsPort;
  runInspector: RunInspectorPort;
  systemPermissions: SystemPermissionsPort;
  askUser: AskUserPort;
  toolApproval: ToolApprovalPort & ToolApprovalSettingsPort;
  session: AuthSession;
  onLogout(): Promise<void>;
}

export function AppShell({
  accessControl,
  hostLifecycle,
  workspaceIndex,
  projectConversation,
  projectCreation,
  projectDirectory,
  projectGit,
  projectRun,
  remoteConnection,
  remoteFile,
  remoteTerminal,
  conversationHistory,
  conversationComposer,
  conversationAttachments,
  conversationRealtime,
  conversationRuntimeSettings,
  runInspector,
  systemPermissions,
  askUser,
  toolApproval,
  session,
  onLogout,
}: AppShellProps) {
  const snapshot = useHostLifecycleStore((state) => state.snapshot);
  const diagnostics = useHostLifecycleStore((state) => state.diagnostics);
  const start = useHostLifecycleStore((state) => state.start);
  const retry = useHostLifecycleStore((state) => state.retry);
  const workspaceSnapshot = useWorkspaceIndexStore((state) => state.snapshot);
  const acceptCreatedProject = useWorkspaceIndexStore((state) => state.acceptCreatedProject);
  const acceptRemovedProject = useWorkspaceIndexStore((state) => state.acceptRemovedProject);
  const resetProjectConversations = useProjectConversationStore((state) => state.reset);
  const removeProjectConversation = useProjectConversationStore((state) => state.remove);
  const remoteConnections = useRemoteConnectionStore((state) => state.connections);
  const remoteOwnerUserId = useRemoteConnectionStore((state) => state.ownerUserId);
  const remoteHostAvailable = useRemoteConnectionStore((state) => state.hostAvailable);
  const remoteActiveLoadRequest = useRemoteConnectionStore((state) => state.activeLoadRequest);
  const acceptSavedRemoteConnection = useRemoteConnectionStore((state) => state.acceptSaved);
  const acceptDeletedRemoteConnection = useRemoteConnectionStore((state) => state.acceptDeleted);
  const [selection, setSelection] = useState<WorkspaceSelection>({ kind: "workbench" });
  const [activeSurface, setActiveSurface] = useState<"workspace" | "tool-approval" | "runtime-permissions" | "access-control">("workspace");
  const [showCreateProject, setShowCreateProject] = useState(false);
  const [remoteEditorTarget, setRemoteEditorTarget] = useState<"create" | string | null>(null);

  useEffect(() => {
    void start(hostLifecycle, session.user.id);
  }, [hostLifecycle, session.user.id, start]);

  useEffect(() => {
    setSelection({ kind: "workbench" });
    setActiveSurface("workspace");
    setShowCreateProject(false);
    setRemoteEditorTarget(null);
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

  useEffect(() => {
    if (selection.kind !== "remote"
      || remoteOwnerUserId !== session.user.id
      || !remoteHostAvailable
      || remoteActiveLoadRequest != null
      || remoteConnections.some((connection) => connection.id === selection.id)) return;
    setSelection({ kind: "workbench" });
    setRemoteEditorTarget(null);
  }, [
    remoteActiveLoadRequest,
    remoteConnections,
    remoteHostAvailable,
    remoteOwnerUserId,
    selection,
    session.user.id,
  ]);

  const selectedProject = selection.kind === "project"
    ? workspaceSnapshot.projects.find((project) => project.id === selection.id)
    : undefined;
  const selectedContact = selection.kind === "contact"
    ? workspaceSnapshot.contacts.find((contact) => contact.id === selection.id)
    : undefined;
  const selectedRemoteConnection = selection.kind === "remote"
    ? remoteConnections.find((connection) => connection.id === selection.id)
    : undefined;
  const editingRemoteConnection = remoteEditorTarget && remoteEditorTarget !== "create"
    ? remoteConnections.find((connection) => connection.id === remoteEditorTarget) ?? null
    : null;
  const defaultContactReady = workspaceSnapshot.contacts.some((contact) => contact.id === "jiguli");
  const title = activeSurface === "tool-approval"
    ? "命令审批"
    : activeSurface === "runtime-permissions"
      ? "运行与权限"
      : activeSurface === "access-control"
        ? "访问控制"
      : selectedProject?.title ?? selectedContact?.title ?? selectedRemoteConnection?.name ?? "工作台";
  const selectWorkspace = (nextSelection: WorkspaceSelection) => {
    setSelection(nextSelection);
    setActiveSurface("workspace");
  };

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
          remoteConnection={remoteConnection}
          workspaceIndex={workspaceIndex}
          selection={selection}
          onSelectionChange={selectWorkspace}
          onCreateProject={() => setShowCreateProject(true)}
          onCreateRemoteConnection={() => setRemoteEditorTarget("create")}
        />
        <div className="sidebar-settings-group">
          <p>设置</p>
          <button
            aria-current={activeSurface === "tool-approval" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "tool-approval" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("tool-approval")}
            type="button"
          >
            <span aria-hidden="true">✓</span>
            <span><strong>命令审批</strong><small>本机安全策略</small></span>
          </button>
          <button
            aria-current={activeSurface === "runtime-permissions" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "runtime-permissions" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("runtime-permissions")}
            type="button"
          >
            <span aria-hidden="true">⌁</span>
            <span><strong>运行与权限</strong><small>Windows 系统能力</small></span>
          </button>
          <button
            aria-current={activeSurface === "access-control" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "access-control" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("access-control")}
            type="button"
          >
            <span aria-hidden="true">▣</span>
            <span><strong>访问控制</strong><small>任务执行边界</small></span>
          </button>
        </div>
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
            <p className="eyebrow">{
              activeSurface === "tool-approval"
                ? "LOCAL SECURITY"
                : activeSurface === "runtime-permissions"
                  ? "RUNTIME & PERMISSIONS"
                  : activeSurface === "access-control"
                    ? "ACCESS CONTROL"
                  : "LOCAL WORKSPACE"
            }</p>
            <h1>{title}</h1>
          </div>
          <span className="protocol-badge">Host v{snapshot.protocolVersion}</span>
        </header>
        {activeSurface === "tool-approval" ? (
          <ToolApprovalSettingsView
            approvalPort={toolApproval}
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            settingsPort={toolApproval}
          />
        ) : activeSurface === "runtime-permissions" ? (
          <SystemPermissionsView
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            port={systemPermissions}
          />
        ) : activeSurface === "access-control" ? (
          <AccessControlView
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            port={accessControl}
          />
        ) : selectedRemoteConnection ? (
          <RemoteConnectionDetailView
            connection={selectedRemoteConnection}
            connections={remoteConnections}
            ownerUserId={session.user.id}
            port={remoteConnection}
            remoteFilePort={remoteFile}
            remoteTerminalPort={remoteTerminal}
            onDeleted={(connectionId) => {
              acceptDeletedRemoteConnection(session.user.id, connectionId);
              setRemoteEditorTarget(null);
              setSelection({ kind: "workbench" });
            }}
            onEdit={() => setRemoteEditorTarget(selectedRemoteConnection.id)}
          />
        ) : selectedProject ? (
          <ProjectWorkspaceSurface
            directoryPort={projectDirectory}
            gitPort={projectGit}
            runPort={projectRun}
            ownerUserId={session.user.id}
            projectId={selectedProject.id}
            projectName={selectedProject.title}
            onProjectRemoved={(projectId) => {
              removeProjectConversation(projectId);
              acceptRemovedProject(session.user.id, projectId);
              setSelection({ kind: "workbench" });
            }}
            messages={(
              <ProjectConversationPlaceholder
                ownerUserId={session.user.id}
                project={selectedProject}
                hostReady={snapshot.ready}
                defaultContactReady={defaultContactReady}
                port={projectConversation}
                historyPort={conversationHistory}
                composerPort={conversationComposer}
                attachmentPort={conversationAttachments}
                realtimePort={conversationRealtime}
                runtimeSettingsPort={conversationRuntimeSettings}
                runInspectorPort={runInspector}
                askUserPort={askUser}
              />
            )}
          />
        ) : selectedContact ? (
          <ContactConversationView
            askUserPort={askUser}
            attachmentPort={conversationAttachments}
            composerPort={conversationComposer}
            contact={selectedContact}
            historyPort={conversationHistory}
            ownerUserId={session.user.id}
            realtimePort={conversationRealtime}
            runInspectorPort={runInspector}
            runtimeSettingsPort={conversationRuntimeSettings}
          />
        ) : (
          <HostReadyGate
            snapshot={snapshot}
            diagnostics={diagnostics}
            onRetry={() => void retry(hostLifecycle)}
          />
        )}
      </section>
      <GlobalToolApprovalOverlay
        hostReady={snapshot.ready}
        ownerUserId={session.user.id}
        port={toolApproval}
      />
      {showCreateProject && (
        <CreateProjectSheet
          ownerUserId={session.user.id}
          port={projectCreation}
          onDismiss={() => setShowCreateProject(false)}
          onCreated={(project) => {
            acceptCreatedProject(session.user.id, project);
            selectWorkspace({ kind: "project", id: project.id });
          }}
        />
      )}
      {remoteEditorTarget && (
        <RemoteConnectionEditorSheet
          connections={remoteConnections}
          editingConnection={editingRemoteConnection}
          key={remoteEditorTarget}
          ownerUserId={session.user.id}
          port={remoteConnection}
          onDismiss={() => setRemoteEditorTarget(null)}
          onSaved={(connection) => {
            acceptSavedRemoteConnection(session.user.id, connection);
            selectWorkspace({ kind: "remote", id: connection.id });
          }}
        />
      )}
    </>
  );
}
