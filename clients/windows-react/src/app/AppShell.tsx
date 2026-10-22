import { lazy, Suspense, useEffect, useState, type ComponentProps } from "react";
import type { AuthSession } from "../features/authentication/domain/authentication";
import type { AskUserPort } from "../features/ask-user/ports/AskUserPort";
import type { ConversationHistoryPort } from "../features/conversation-history/ports/ConversationHistoryPort";
import type { ConversationComposerPort } from "../features/conversation-composer/ports/ConversationComposerPort";
import type { ConversationAttachmentPort } from "../features/conversation-attachments/ports/ConversationAttachmentPort";
import type { ConversationRealtimePort } from "../features/conversation-realtime/ports/ConversationRealtimePort";
import type { ConversationRuntimeSettingsPort } from "../features/conversation-runtime-settings/ports/ConversationRuntimeSettingsPort";
import type { RunInspectorPort } from "../features/run-inspector/ports/RunInspectorPort";
import type { ToolApprovalPort } from "../features/tool-approval/ports/ToolApprovalPort";
import type { ToolApprovalSettingsPort } from "../features/tool-approval/ports/ToolApprovalSettingsPort";
import { GlobalToolApprovalOverlay } from "../features/tool-approval/ui/GlobalToolApprovalOverlay";
import { HostReadyGate } from "../features/host-lifecycle/ui/HostReadyGate";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import type { HostLifecyclePort } from "../features/host-lifecycle/ports/HostLifecyclePort";
import { useGeneralSettingsStore } from "../features/general-settings/application/generalSettingsStore";
import type { GeneralSettingsPort } from "../features/general-settings/ports/GeneralSettingsPort";
import { usePetPreferencesStore } from "../features/pet/application/petPreferencesStore";
import type { PetPreferencesPort } from "../features/pet/ports/PetPreferencesPort";
import type { PetPresentationPort } from "../features/pet/ports/PetPresentationPort";
import { startPetPresentationPolling } from "../features/pet/application/petPresentationPolling";
import { useGlobalUtilityPreferencesStore } from "../features/global-utilities/application/globalUtilityPreferencesStore";
import type { GlobalUtilityPreferencesPort } from "../features/global-utilities/ports/GlobalUtilityPreferencesPort";
import { usePluginApplicationStore } from "../features/plugin-applications/application/pluginApplicationStore";
import type { PluginApplicationPort } from "../features/plugin-applications/ports/PluginApplicationPort";
import { useProjectConversationStore } from "../features/project-conversation/application/projectConversationStore";
import { useRemoteConnectionStore } from "../features/remote-connection/application/remoteConnectionStore";
import type { RemoteConnectionPort } from "../features/remote-connection/ports/RemoteConnectionPort";
import { useWorkspaceIndexStore } from "../features/workspace-index/application/workspaceIndexStore";
import type { WorkspaceSelection } from "../features/workspace-index/domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../features/workspace-index/ports/WorkspaceIndexPort";
import type { QuickSearchNavigationPort } from "../features/quick-search/ports/QuickSearchPort";
import { WorkspaceSidebar } from "../features/workspace-index/ui/WorkspaceSidebar";
import { interfaceText } from "../shared/interfaceLocale";

const LocalTerminalWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/local-terminal/adapters/TauriLocalTerminalAdapter"),
    import("../features/local-terminal/ui/LocalTerminalWorkspace"),
  ]);
  const port = new adapterModule.TauriLocalTerminalAdapter();
  return {
    default: ({ hostReady, ownerUserId }: { hostReady: boolean; ownerUserId: string }) => (
      <viewModule.LocalTerminalWorkspace
        hostReady={hostReady}
        ownerUserId={ownerUserId}
        port={port}
      />
    ),
  };
});

const AgentManagementWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/agent-management/adapters/TauriAgentManagementAdapter"),
    import("../features/agent-management/ui/AgentManagementView"),
  ]);
  const port = new adapterModule.TauriAgentManagementAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.AgentManagementView>, "port">;
  return {
    default: (props: Props) => (
      <viewModule.AgentManagementView {...props} port={port} />
    ),
  };
});

const RequirementSurveyWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/requirement-surveys/adapters/TauriRequirementSurveyAdapter"),
    import("../features/requirement-surveys/ui/RequirementSurveyCenterView"),
  ]);
  const port = new adapterModule.TauriRequirementSurveyAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.RequirementSurveyCenterView>, "port">;
  return {
    default: (props: Props) => (
      <viewModule.RequirementSurveyCenterView {...props} port={port} />
    ),
  };
});

const NotepadWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/notepad/adapters/TauriNotepadAdapter"),
    import("../features/notepad/ui/NotepadView"),
  ]);
  const port = new adapterModule.TauriNotepadAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.NotepadView>, "port">;
  return {
    default: (props: Props) => <viewModule.NotepadView {...props} port={port} />,
  };
});

const MediaStudioWorkspace = lazy(async () => {
  const [mediaAdapterModule, storyAdapterModule, viewModule] = await Promise.all([
    import("../features/media-studio/adapters/TauriMediaStudioAdapter"),
    import("../features/story-studio/adapters/TauriStoryStudioAdapter"),
    import("../features/media-studio/ui/MediaStudioView"),
  ]);
  const mediaPort = new mediaAdapterModule.TauriMediaStudioAdapter();
  const storyPort = new storyAdapterModule.TauriStoryStudioAdapter();
  return {
    default: ({ ownerUserId }: { ownerUserId: string }) => (
      <viewModule.MediaStudioView
        ownerUserId={ownerUserId}
        port={mediaPort}
        storyPort={storyPort}
      />
    ),
  };
});

const AccessControlWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/access-control/adapters/TauriAccessControlAdapter"),
    import("../features/access-control/ui/AccessControlView"),
  ]);
  const port = new adapterModule.TauriAccessControlAdapter();
  return {
    default: ({ hostReady, ownerUserId }: { hostReady: boolean; ownerUserId: string }) => (
      <viewModule.AccessControlView
        hostReady={hostReady}
        ownerUserId={ownerUserId}
        port={port}
      />
    ),
  };
});

const GeneralSettingsWorkspace = lazy(async () => {
  const viewModule = await import("../features/general-settings/ui/GeneralSettingsView");
  return { default: viewModule.GeneralSettingsView };
});

const WeChatCompanionWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/wechat-companion/adapters/TauriWeChatCompanionAdapter"),
    import("../features/wechat-companion/ui/WeChatCompanionView"),
  ]);
  const port = new adapterModule.TauriWeChatCompanionAdapter();
  return {
    default: ({ ownerUserId }: { ownerUserId: string }) => (
      <viewModule.WeChatCompanionView ownerUserId={ownerUserId} port={port} />
    ),
  };
});

const PetSettingsWorkspace = lazy(async () => {
  const viewModule = await import("../features/pet/ui/PetSettingsView");
  return { default: viewModule.PetSettingsView };
});

const GlobalUtilitiesSettingsWorkspace = lazy(async () => {
  const viewModule = await import("../features/global-utilities/ui/GlobalUtilitiesSettingsView");
  return { default: viewModule.GlobalUtilitiesSettingsView };
});

const AiModelSettingsWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/ai-model-settings/adapters/TauriAiModelSettingsAdapter"),
    import("../features/ai-model-settings/ui/AiModelSettingsView"),
  ]);
  const port = new adapterModule.TauriAiModelSettingsAdapter();
  return {
    default: ({ ownerUserId }: { ownerUserId: string }) => (
      <viewModule.AiModelSettingsView ownerUserId={ownerUserId} port={port} />
    ),
  };
});

const SystemPermissionsWorkspace = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/runtime-permissions/adapters/TauriSystemPermissionsAdapter"),
    import("../features/runtime-permissions/ui/SystemPermissionsView"),
  ]);
  const port = new adapterModule.TauriSystemPermissionsAdapter();
  return {
    default: ({ hostReady, ownerUserId }: { hostReady: boolean; ownerUserId: string }) => (
      <viewModule.SystemPermissionsView
        hostReady={hostReady}
        ownerUserId={ownerUserId}
        port={port}
      />
    ),
  };
});

const ToolApprovalSettingsView = lazy(async () => {
  const module = await import("../features/tool-approval/ui/ToolApprovalSettingsView");
  return { default: module.ToolApprovalSettingsView };
});

const PluginApplicationsView = lazy(async () => {
  const module = await import("../features/plugin-applications/ui/PluginApplicationsView");
  return { default: module.PluginApplicationsView };
});

const PluginApplicationHostView = lazy(async () => {
  const module = await import("../features/plugin-applications/ui/PluginApplicationsView");
  return { default: module.PluginApplicationHostView };
});

const RemoteConnectionDetailWorkspace = lazy(async () => {
  const [fileAdapterModule, terminalAdapterModule, viewModule] = await Promise.all([
    import("../features/remote-files/adapters/TauriRemoteFileAdapter"),
    import("../features/remote-terminal/adapters/TauriRemoteTerminalAdapter"),
    import("../features/remote-connection/ui/RemoteConnectionDetailView"),
  ]);
  const remoteFilePort = new fileAdapterModule.TauriRemoteFileAdapter();
  const remoteTerminalPort = new terminalAdapterModule.TauriRemoteTerminalAdapter();
  type Props = Omit<
    ComponentProps<typeof viewModule.RemoteConnectionDetailView>,
    "remoteFilePort" | "remoteTerminalPort"
  >;
  return {
    default: (props: Props) => (
      <viewModule.RemoteConnectionDetailView
        {...props}
        remoteFilePort={remoteFilePort}
        remoteTerminalPort={remoteTerminalPort}
      />
    ),
  };
});

const ProjectWorkspaceSurface = lazy(async () => {
  const [directoryAdapterModule, gitAdapterModule, runAdapterModule, viewModule] = await Promise.all([
    import("../features/project-directory/adapters/TauriProjectDirectoryAdapter"),
    import("../features/project-git/adapters/TauriProjectGitAdapter"),
    import("../features/project-run/adapters/TauriProjectRunAdapter"),
    import("../features/project-directory/ui/ProjectWorkspaceSurface"),
  ]);
  const directoryPort = new directoryAdapterModule.TauriProjectDirectoryAdapter();
  const gitPort = new gitAdapterModule.TauriProjectGitAdapter();
  const runPort = new runAdapterModule.TauriProjectRunAdapter();
  type Props = Omit<
    ComponentProps<typeof viewModule.ProjectWorkspaceSurface>,
    "directoryPort" | "gitPort" | "runPort"
  >;
  return {
    default: (props: Props) => (
      <viewModule.ProjectWorkspaceSurface
        {...props}
        directoryPort={directoryPort}
        gitPort={gitPort}
        runPort={runPort}
      />
    ),
  };
});

const ProjectConversationPlaceholder = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/project-conversation/adapters/TauriProjectConversationAdapter"),
    import("../features/project-conversation/ui/ProjectConversationPlaceholder"),
  ]);
  const port = new adapterModule.TauriProjectConversationAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.ProjectConversationPlaceholder>, "port">;
  return {
    default: (props: Props) => (
      <viewModule.ProjectConversationPlaceholder {...props} port={port} />
    ),
  };
});

const ContactConversationView = lazy(async () => {
  const module = await import("../features/contact-conversation/ui/ContactConversationView");
  return { default: module.ContactConversationView };
});

const CreateProjectSheet = lazy(async () => {
  const [adapterModule, viewModule] = await Promise.all([
    import("../features/project-creation/adapters/TauriProjectCreationAdapter"),
    import("../features/project-creation/ui/CreateProjectSheet"),
  ]);
  const port = new adapterModule.TauriProjectCreationAdapter();
  type Props = Omit<ComponentProps<typeof viewModule.CreateProjectSheet>, "port">;
  return {
    default: (props: Props) => <viewModule.CreateProjectSheet {...props} port={port} />,
  };
});

const RemoteConnectionEditorSheet = lazy(async () => {
  const module = await import("../features/remote-connection/ui/RemoteConnectionEditorSheet");
  return { default: module.RemoteConnectionEditorSheet };
});

interface AppShellProps {
  hostLifecycle: HostLifecyclePort;
  generalSettings: GeneralSettingsPort;
  petPreferences: PetPreferencesPort;
  globalUtilities: GlobalUtilityPreferencesPort;
  petPresentation: PetPresentationPort;
  pluginApplication: PluginApplicationPort;
  workspaceIndex: WorkspaceIndexPort;
  remoteConnection: RemoteConnectionPort;
  conversationHistory: ConversationHistoryPort;
  conversationComposer: ConversationComposerPort;
  conversationAttachments: ConversationAttachmentPort;
  conversationRealtime: ConversationRealtimePort;
  conversationRuntimeSettings: ConversationRuntimeSettingsPort;
  runInspector: RunInspectorPort;
  askUser: AskUserPort;
  toolApproval: ToolApprovalPort & ToolApprovalSettingsPort;
  quickSearchNavigation: QuickSearchNavigationPort;
  session: AuthSession;
  onLogout(): Promise<void>;
}

export function AppShell({
  hostLifecycle,
  generalSettings,
  petPreferences,
  globalUtilities,
  petPresentation,
  pluginApplication,
  workspaceIndex,
  remoteConnection,
  conversationHistory,
  conversationComposer,
  conversationAttachments,
  conversationRealtime,
  conversationRuntimeSettings,
  runInspector,
  askUser,
  toolApproval,
  quickSearchNavigation,
  session,
  onLogout,
}: AppShellProps) {
  const snapshot = useHostLifecycleStore((state) => state.snapshot);
  const activateGeneralSettings = useGeneralSettingsStore((state) => state.activate);
  const activatePetPreferences = usePetPreferencesStore((state) => state.activate);
  const activateGlobalUtilities = useGlobalUtilityPreferencesStore((state) => state.activate);
  const resetGlobalUtilities = useGlobalUtilityPreferencesStore((state) => state.reset);
  const interfaceLanguage = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const contextLanguage = useGeneralSettingsStore((state) => state.settings.contextLanguage);
  const updateGeneralSettingsDraft = useGeneralSettingsStore((state) => state.updateDraft);
  const saveGeneralSettings = useGeneralSettingsStore((state) => state.save);
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
  const pluginApplications = usePluginApplicationStore((state) => state.applications);
  const resetPluginApplications = usePluginApplicationStore((state) => state.reset);
  const [selection, setSelection] = useState<WorkspaceSelection>({ kind: "workbench" });
  const [activeSurface, setActiveSurface] = useState<"workspace" | "general-settings" | "wechat-companion" | "pet-settings" | "global-utilities" | "ai-models" | "tool-approval" | "runtime-permissions" | "access-control">("workspace");
  const [showCreateProject, setShowCreateProject] = useState(false);
  const [remoteEditorTarget, setRemoteEditorTarget] = useState<"create" | string | null>(null);
  const text = (key: Parameters<typeof interfaceText>[1]) => interfaceText(interfaceLanguage, key);

  useEffect(() => {
    void start(hostLifecycle, session.user.id);
  }, [hostLifecycle, session.user.id, start]);

  useEffect(() => {
    void activateGeneralSettings(generalSettings, session.user.id);
  }, [activateGeneralSettings, generalSettings, session.user.id]);

  useEffect(() => {
    void activatePetPreferences(petPreferences, session.user.id);
  }, [activatePetPreferences, petPreferences, session.user.id]);

  useEffect(() => {
    void activateGlobalUtilities(globalUtilities, session.user.id);
  }, [activateGlobalUtilities, globalUtilities, session.user.id]);

  useEffect(() => {
    if (!snapshot.ready) {
      void petPresentation.refresh(session.user.id).catch(() => undefined);
      return undefined;
    }
    return startPetPresentationPolling(petPresentation, session.user.id);
  }, [petPresentation, session.user.id, snapshot.ready]);

  useEffect(() => {
    setSelection({ kind: "workbench" });
    setActiveSurface("workspace");
    setShowCreateProject(false);
    setRemoteEditorTarget(null);
    resetProjectConversations(session.user.id);
    resetPluginApplications(pluginApplication);
  }, [pluginApplication, resetPluginApplications, resetProjectConversations, session.user.id]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void quickSearchNavigation.subscribeNavigation((payload) => {
      if (payload.ownerUserId !== session.user.id) return;
      if (payload.destination === "project" && payload.resourceId) {
        setSelection({ kind: "project", id: payload.resourceId });
        setActiveSurface("workspace");
      } else if (payload.destination === "contact" && payload.resourceId) {
        setSelection({ kind: "contact", id: payload.resourceId });
        setActiveSurface("workspace");
      } else if (payload.destination === "settings") {
        setActiveSurface("general-settings");
      } else if (payload.destination === "runtimePermissions") {
        setActiveSurface("runtime-permissions");
      }
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [quickSearchNavigation, session.user.id]);

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
  const selectedPluginApplication = selection.kind === "application"
    ? pluginApplications.find((application) => (
      application.pluginId === selection.pluginId
      && application.componentKey === selection.componentKey
    ))
    : undefined;
  const editingRemoteConnection = remoteEditorTarget && remoteEditorTarget !== "create"
    ? remoteConnections.find((connection) => connection.id === remoteEditorTarget) ?? null
    : null;
  const defaultContactReady = workspaceSnapshot.contacts.some((contact) => contact.id === "jiguli");
  const title = activeSurface === "general-settings"
    ? text("generalAccount")
    : activeSurface === "wechat-companion"
    ? text("wechatCompanion")
    : activeSurface === "pet-settings"
    ? text("petSettings")
    : activeSurface === "global-utilities"
    ? text("globalUtilities")
    : activeSurface === "ai-models"
      ? text("aiModels")
    : activeSurface === "tool-approval"
    ? text("commandApproval")
    : activeSurface === "runtime-permissions"
      ? text("runtimePermissions")
      : activeSurface === "access-control"
        ? text("accessControl")
      : selection.kind === "terminal"
        ? text("localTerminal")
      : selection.kind === "agentManagement"
        ? text("agentManagement")
      : selection.kind === "requirementSurveys"
        ? text("requirementSurveys")
      : selection.kind === "notepad"
        ? text("notepad")
      : selection.kind === "mediaStudio"
        ? text("mediaStudio")
      : selection.kind === "applications"
        ? text("applications")
      : selection.kind === "application"
        ? selectedPluginApplication?.displayName ?? text("applications")
      : selectedProject?.title ?? selectedContact?.title ?? selectedRemoteConnection?.name ?? text("workbench");
  const selectWorkspace = (nextSelection: WorkspaceSelection) => {
    setSelection(nextSelection);
    setActiveSurface("workspace");
  };
  const handleLogout = async () => {
    await globalUtilities.deactivate(session.user.id).catch(() => undefined);
    resetGlobalUtilities();
    await onLogout();
  };

  return (
    <>
      <aside className="sidebar" aria-label={text("mainNavigation")}>
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
          <p>{text("settings")}</p>
          <button
            aria-current={activeSurface === "general-settings" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "general-settings" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("general-settings")}
            type="button"
          >
            <span aria-hidden="true">⚙</span>
            <span><strong>{text("generalAccount")}</strong><small>{text("generalAccountDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "wechat-companion" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "wechat-companion" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("wechat-companion")}
            type="button"
          >
            <span aria-hidden="true">⌗</span>
            <span><strong>{text("wechatCompanion")}</strong><small>{text("wechatCompanionDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "pet-settings" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "pet-settings" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("pet-settings")}
            type="button"
          >
            <span aria-hidden="true">🐾</span>
            <span><strong>{text("petSettings")}</strong><small>{text("petSettingsDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "global-utilities" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "global-utilities" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("global-utilities")}
            type="button"
          >
            <span aria-hidden="true">⌘</span>
            <span><strong>{text("globalUtilities")}</strong><small>{text("globalUtilitiesDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "ai-models" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "ai-models" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("ai-models")}
            type="button"
          >
            <span aria-hidden="true">✦</span>
            <span><strong>{text("aiModels")}</strong><small>{text("aiModelsDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "tool-approval" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "tool-approval" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("tool-approval")}
            type="button"
          >
            <span aria-hidden="true">✓</span>
            <span><strong>{text("commandApproval")}</strong><small>{text("commandApprovalDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "runtime-permissions" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "runtime-permissions" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("runtime-permissions")}
            type="button"
          >
            <span aria-hidden="true">⌁</span>
            <span><strong>{text("runtimePermissions")}</strong><small>{text("runtimePermissionsDetail")}</small></span>
          </button>
          <button
            aria-current={activeSurface === "access-control" ? "page" : undefined}
            className={`sidebar-settings-button ${activeSurface === "access-control" ? "sidebar-settings-button--active" : ""}`}
            onClick={() => setActiveSurface("access-control")}
            type="button"
          >
            <span aria-hidden="true">▣</span>
            <span><strong>{text("accessControl")}</strong><small>{text("accessControlDetail")}</small></span>
          </button>
        </div>
        <div className="sidebar-status">
          <span className={`status-light status-light--${snapshot.phase}`} />
          <span>{snapshot.label}</span>
        </div>
        <button className="account-button" onClick={() => void handleLogout()} type="button">
          <span className="account-avatar">{(session.user.displayName ?? session.user.username).slice(0, 1).toUpperCase()}</span>
          <span className="account-copy">
            <strong>{session.user.displayName ?? session.user.username}</strong>
            <small>{text("signOut")}</small>
          </span>
        </button>
      </aside>
      <section className="content-surface">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">{
              activeSurface === "general-settings"
                ? "SETTINGS"
                : activeSurface === "wechat-companion"
                ? "WECHAT COMPANION"
                : activeSurface === "pet-settings"
                ? "DESKTOP PET"
                : activeSurface === "global-utilities"
                ? "GLOBAL UTILITIES"
                : activeSurface === "ai-models"
                ? "AI MODELS"
                : activeSurface === "tool-approval"
                ? "LOCAL SECURITY"
                : activeSurface === "runtime-permissions"
                  ? "RUNTIME & PERMISSIONS"
                  : activeSurface === "access-control"
                    ? "ACCESS CONTROL"
                  : selection.kind === "mediaStudio"
                    ? "MEDIA STUDIO"
                  : selection.kind === "agentManagement"
                    ? "AGENT MANAGEMENT"
                  : selection.kind === "requirementSurveys"
                    ? "REQUIREMENT SURVEYS"
                  : selection.kind === "notepad"
                    ? "LOCAL NOTEBOOK"
                  : selection.kind === "applications" || selection.kind === "application"
                    ? "PLUGIN APPLICATIONS"
                  : "LOCAL WORKSPACE"
            }</p>
            <h1>{title}</h1>
          </div>
          <span className="protocol-badge">Host v{snapshot.protocolVersion}</span>
        </header>
        <Suspense fallback={<div className="workspace-loading"><span className="mini-spinner" />{text("loadingWorkspace")}</div>}>
          {activeSurface === "general-settings" ? (
          <GeneralSettingsWorkspace
            displayName={session.user.displayName}
            onLogout={handleLogout}
            ownerUserId={session.user.id}
            port={generalSettings}
            role={session.user.role}
            username={session.user.username}
          />
        ) : activeSurface === "wechat-companion" ? (
          <WeChatCompanionWorkspace ownerUserId={session.user.id} />
        ) : activeSurface === "pet-settings" ? (
          <PetSettingsWorkspace ownerUserId={session.user.id} port={petPreferences} />
        ) : activeSurface === "global-utilities" ? (
          <GlobalUtilitiesSettingsWorkspace ownerUserId={session.user.id} port={globalUtilities} />
        ) : activeSurface === "ai-models" ? (
          <AiModelSettingsWorkspace ownerUserId={session.user.id} />
        ) : activeSurface === "tool-approval" ? (
          <ToolApprovalSettingsView
            approvalPort={toolApproval}
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            settingsPort={toolApproval}
          />
        ) : activeSurface === "runtime-permissions" ? (
          <SystemPermissionsWorkspace
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
          />
        ) : activeSurface === "access-control" ? (
          <AccessControlWorkspace
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
          />
        ) : selection.kind === "terminal" ? (
          <LocalTerminalWorkspace
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
          />
        ) : selection.kind === "agentManagement" ? (
          <AgentManagementWorkspace
            askUserPort={askUser}
            attachmentPort={conversationAttachments}
            historyPort={conversationHistory}
            ownerUserId={session.user.id}
            contextLanguage={contextLanguage}
            onContextLanguageChange={async (language) => {
              updateGeneralSettingsDraft({ contextLanguage: language });
              await saveGeneralSettings(generalSettings);
            }}
            projects={workspaceSnapshot.projects}
            realtimePort={conversationRealtime}
            runInspectorPort={runInspector}
            runtimeSettingsPort={conversationRuntimeSettings}
          />
        ) : selection.kind === "requirementSurveys" ? (
          <RequirementSurveyWorkspace
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            projects={workspaceSnapshot.projects}
          />
        ) : selection.kind === "notepad" ? (
          <NotepadWorkspace
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
          />
        ) : selection.kind === "mediaStudio" ? (
          <MediaStudioWorkspace ownerUserId={session.user.id} />
        ) : selection.kind === "applications" || (selection.kind === "application" && !selectedPluginApplication) ? (
          <PluginApplicationsView
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            port={pluginApplication}
            onOpen={(application) => setSelection({
              kind: "application",
              pluginId: application.pluginId,
              componentKey: application.componentKey,
            })}
          />
        ) : selectedPluginApplication ? (
          <PluginApplicationHostView
            application={selectedPluginApplication}
            hostReady={snapshot.ready}
            ownerUserId={session.user.id}
            port={pluginApplication}
            projects={workspaceSnapshot.projects}
            onBack={() => setSelection({ kind: "applications" })}
          />
        ) : selectedRemoteConnection ? (
          <RemoteConnectionDetailWorkspace
            connection={selectedRemoteConnection}
            connections={remoteConnections}
            ownerUserId={session.user.id}
            port={remoteConnection}
            onDeleted={(connectionId) => {
              acceptDeletedRemoteConnection(session.user.id, connectionId);
              setRemoteEditorTarget(null);
              setSelection({ kind: "workbench" });
            }}
            onEdit={() => setRemoteEditorTarget(selectedRemoteConnection.id)}
          />
        ) : selectedProject ? (
          <ProjectWorkspaceSurface
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
        </Suspense>
      </section>
      <GlobalToolApprovalOverlay
        hostReady={snapshot.ready}
        ownerUserId={session.user.id}
        port={toolApproval}
      />
      {showCreateProject && (
        <Suspense fallback={null}>
          <CreateProjectSheet
            ownerUserId={session.user.id}
            onDismiss={() => setShowCreateProject(false)}
            onCreated={(project) => {
              acceptCreatedProject(session.user.id, project);
              selectWorkspace({ kind: "project", id: project.id });
            }}
          />
        </Suspense>
      )}
      {remoteEditorTarget && (
        <Suspense fallback={null}>
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
        </Suspense>
      )}
    </>
  );
}
