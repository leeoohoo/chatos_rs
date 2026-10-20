import { Route, Routes } from "react-router";
import { TauriAuthenticationAdapter } from "../features/authentication/adapters/TauriAuthenticationAdapter";
import { TauriAskUserAdapter } from "../features/ask-user/adapters/TauriAskUserAdapter";
import { TauriAccessControlAdapter } from "../features/access-control/adapters/TauriAccessControlAdapter";
import { useAuthenticationStore } from "../features/authentication/application/authenticationStore";
import { AuthenticationGate } from "../features/authentication/ui/AuthenticationGate";
import { TauriConversationHistoryAdapter } from "../features/conversation-history/adapters/TauriConversationHistoryAdapter";
import { TauriConversationComposerAdapter } from "../features/conversation-composer/adapters/TauriConversationComposerAdapter";
import { TauriConversationAttachmentAdapter } from "../features/conversation-attachments/adapters/TauriConversationAttachmentAdapter";
import { TauriConversationRealtimeAdapter } from "../features/conversation-realtime/adapters/TauriConversationRealtimeAdapter";
import { TauriConversationRuntimeSettingsAdapter } from "../features/conversation-runtime-settings/adapters/TauriConversationRuntimeSettingsAdapter";
import { TauriRunInspectorAdapter } from "../features/run-inspector/adapters/TauriRunInspectorAdapter";
import { TauriProjectDirectoryAdapter } from "../features/project-directory/adapters/TauriProjectDirectoryAdapter";
import { TauriProjectGitAdapter } from "../features/project-git/adapters/TauriProjectGitAdapter";
import { TauriProjectRunAdapter } from "../features/project-run/adapters/TauriProjectRunAdapter";
import { TauriProjectCreationAdapter } from "../features/project-creation/adapters/TauriProjectCreationAdapter";
import { TauriRemoteConnectionAdapter } from "../features/remote-connection/adapters/TauriRemoteConnectionAdapter";
import { TauriRemoteFileAdapter } from "../features/remote-files/adapters/TauriRemoteFileAdapter";
import { TauriRemoteTerminalAdapter } from "../features/remote-terminal/adapters/TauriRemoteTerminalAdapter";
import { TauriSystemPermissionsAdapter } from "../features/runtime-permissions/adapters/TauriSystemPermissionsAdapter";
import { TauriToolApprovalAdapter } from "../features/tool-approval/adapters/TauriToolApprovalAdapter";
import { TauriHostLifecycleAdapter } from "../features/host-lifecycle/adapters/TauriHostLifecycleAdapter";
import { TauriLocalTerminalAdapter } from "../features/local-terminal/adapters/TauriLocalTerminalAdapter";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriProjectConversationAdapter } from "../features/project-conversation/adapters/TauriProjectConversationAdapter";
import { TauriWindowChromeAdapter } from "../platform/window/adapters/TauriWindowChromeAdapter";
import { WindowControls } from "../platform/window/ui/WindowControls";
import { TauriWorkspaceIndexAdapter } from "../features/workspace-index/adapters/TauriWorkspaceIndexAdapter";
import { AppShell } from "./AppShell";

const authentication = new TauriAuthenticationAdapter();
const askUser = new TauriAskUserAdapter();
const accessControl = new TauriAccessControlAdapter();
const hostLifecycle = new TauriHostLifecycleAdapter();
const localTerminal = new TauriLocalTerminalAdapter();
const windowChrome = new TauriWindowChromeAdapter();
const workspaceIndex = new TauriWorkspaceIndexAdapter();
const projectConversation = new TauriProjectConversationAdapter();
const conversationHistory = new TauriConversationHistoryAdapter();
const conversationComposer = new TauriConversationComposerAdapter();
const conversationAttachments = new TauriConversationAttachmentAdapter();
const conversationRealtime = new TauriConversationRealtimeAdapter();
const conversationRuntimeSettings = new TauriConversationRuntimeSettingsAdapter();
const runInspector = new TauriRunInspectorAdapter();
const projectDirectory = new TauriProjectDirectoryAdapter();
const projectGit = new TauriProjectGitAdapter();
const projectRun = new TauriProjectRunAdapter();
const projectCreation = new TauriProjectCreationAdapter();
const remoteConnection = new TauriRemoteConnectionAdapter();
const remoteFile = new TauriRemoteFileAdapter();
const remoteTerminal = new TauriRemoteTerminalAdapter();
const systemPermissions = new TauriSystemPermissionsAdapter();
const toolApproval = new TauriToolApprovalAdapter();

export function App() {
  return (
    <Routes>
      <Route path="*" element={<ApplicationRoot />} />
    </Routes>
  );
}

function ApplicationRoot() {
  const logout = useAuthenticationStore((state) => state.logout);
  const stopHost = useHostLifecycleStore((state) => state.stop);

  return (
    <main className="app-window">
      <header
        className="app-titlebar"
        data-tauri-drag-region
        onDoubleClick={() => void windowChrome.toggleMaximize()}
      >
        <WindowControls windowChrome={windowChrome} />
        <span className="app-titlebar__title" data-tauri-drag-region>ChatOS</span>
        <span className="app-titlebar__spacer" data-tauri-drag-region />
      </header>
      <AuthenticationGate authentication={authentication}>
        {(session) => (
          <AppShell
            accessControl={accessControl}
            hostLifecycle={hostLifecycle}
            localTerminal={localTerminal}
            workspaceIndex={workspaceIndex}
            projectConversation={projectConversation}
            projectCreation={projectCreation}
            remoteConnection={remoteConnection}
            remoteFile={remoteFile}
            remoteTerminal={remoteTerminal}
            projectDirectory={projectDirectory}
            projectGit={projectGit}
            projectRun={projectRun}
            conversationHistory={conversationHistory}
            conversationComposer={conversationComposer}
            conversationAttachments={conversationAttachments}
            conversationRealtime={conversationRealtime}
            conversationRuntimeSettings={conversationRuntimeSettings}
            runInspector={runInspector}
            systemPermissions={systemPermissions}
            askUser={askUser}
            toolApproval={toolApproval}
            session={session}
            onLogout={() => logout(authentication, () => stopHost(hostLifecycle))}
          />
        )}
      </AuthenticationGate>
    </main>
  );
}
