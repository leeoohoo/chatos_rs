import { Route, Routes } from "react-router";
import { TauriAuthenticationAdapter } from "../features/authentication/adapters/TauriAuthenticationAdapter";
import { TauriAskUserAdapter } from "../features/ask-user/adapters/TauriAskUserAdapter";
import { useAuthenticationStore } from "../features/authentication/application/authenticationStore";
import { AuthenticationGate } from "../features/authentication/ui/AuthenticationGate";
import { TauriConversationHistoryAdapter } from "../features/conversation-history/adapters/TauriConversationHistoryAdapter";
import { TauriConversationComposerAdapter } from "../features/conversation-composer/adapters/TauriConversationComposerAdapter";
import { TauriConversationAttachmentAdapter } from "../features/conversation-attachments/adapters/TauriConversationAttachmentAdapter";
import { TauriConversationRealtimeAdapter } from "../features/conversation-realtime/adapters/TauriConversationRealtimeAdapter";
import { TauriConversationRuntimeSettingsAdapter } from "../features/conversation-runtime-settings/adapters/TauriConversationRuntimeSettingsAdapter";
import { TauriRunInspectorAdapter } from "../features/run-inspector/adapters/TauriRunInspectorAdapter";
import { TauriRemoteConnectionAdapter } from "../features/remote-connection/adapters/TauriRemoteConnectionAdapter";
import { TauriToolApprovalAdapter } from "../features/tool-approval/adapters/TauriToolApprovalAdapter";
import { TauriHostLifecycleAdapter } from "../features/host-lifecycle/adapters/TauriHostLifecycleAdapter";
import { TauriPluginApplicationAdapter } from "../features/plugin-applications/adapters/TauriPluginApplicationAdapter";
import { TauriGeneralSettingsAdapter } from "../features/general-settings/adapters/TauriGeneralSettingsAdapter";
import { TauriPetPreferencesAdapter } from "../features/pet/adapters/TauriPetPreferencesAdapter";
import { PetOverlaySurface } from "../features/pet/ui/PetOverlaySurface";
import { PetActivitySurface } from "../features/pet/ui/PetActivitySurface";
import { TauriPetPresentationAdapter } from "../features/pet/adapters/TauriPetPresentationAdapter";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriWindowChromeAdapter } from "../platform/window/adapters/TauriWindowChromeAdapter";
import { WindowControls } from "../platform/window/ui/WindowControls";
import { TauriWorkspaceIndexAdapter } from "../features/workspace-index/adapters/TauriWorkspaceIndexAdapter";
import { AppShell } from "./AppShell";

const authentication = new TauriAuthenticationAdapter();
const askUser = new TauriAskUserAdapter();
const hostLifecycle = new TauriHostLifecycleAdapter();
const pluginApplication = new TauriPluginApplicationAdapter();
const generalSettings = new TauriGeneralSettingsAdapter();
const petPreferences = new TauriPetPreferencesAdapter();
const petPresentation = new TauriPetPresentationAdapter();
const windowChrome = new TauriWindowChromeAdapter();
const workspaceIndex = new TauriWorkspaceIndexAdapter();
const conversationHistory = new TauriConversationHistoryAdapter();
const conversationComposer = new TauriConversationComposerAdapter();
const conversationAttachments = new TauriConversationAttachmentAdapter();
const conversationRealtime = new TauriConversationRealtimeAdapter();
const conversationRuntimeSettings = new TauriConversationRuntimeSettingsAdapter();
const runInspector = new TauriRunInspectorAdapter();
const remoteConnection = new TauriRemoteConnectionAdapter();
const toolApproval = new TauriToolApprovalAdapter();

export function App() {
  if (new URLSearchParams(window.location.search).get("surface") === "pet-overlay") {
    return <PetOverlaySurface port={petPresentation} />;
  }
  if (new URLSearchParams(window.location.search).get("surface") === "pet-activity") {
    return <PetActivitySurface port={petPresentation} />;
  }
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
            hostLifecycle={hostLifecycle}
            generalSettings={generalSettings}
            petPreferences={petPreferences}
            petPresentation={petPresentation}
            pluginApplication={pluginApplication}
            workspaceIndex={workspaceIndex}
            remoteConnection={remoteConnection}
            conversationHistory={conversationHistory}
            conversationComposer={conversationComposer}
            conversationAttachments={conversationAttachments}
            conversationRealtime={conversationRealtime}
            conversationRuntimeSettings={conversationRuntimeSettings}
            runInspector={runInspector}
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
