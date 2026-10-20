import { Route, Routes } from "react-router";
import { TauriAuthenticationAdapter } from "../features/authentication/adapters/TauriAuthenticationAdapter";
import { useAuthenticationStore } from "../features/authentication/application/authenticationStore";
import { AuthenticationGate } from "../features/authentication/ui/AuthenticationGate";
import { TauriConversationHistoryAdapter } from "../features/conversation-history/adapters/TauriConversationHistoryAdapter";
import { TauriConversationComposerAdapter } from "../features/conversation-composer/adapters/TauriConversationComposerAdapter";
import { TauriConversationAttachmentAdapter } from "../features/conversation-attachments/adapters/TauriConversationAttachmentAdapter";
import { TauriConversationRealtimeAdapter } from "../features/conversation-realtime/adapters/TauriConversationRealtimeAdapter";
import { TauriConversationRuntimeSettingsAdapter } from "../features/conversation-runtime-settings/adapters/TauriConversationRuntimeSettingsAdapter";
import { TauriRunInspectorAdapter } from "../features/run-inspector/adapters/TauriRunInspectorAdapter";
import { TauriHostLifecycleAdapter } from "../features/host-lifecycle/adapters/TauriHostLifecycleAdapter";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriProjectConversationAdapter } from "../features/project-conversation/adapters/TauriProjectConversationAdapter";
import { TauriWindowChromeAdapter } from "../platform/window/adapters/TauriWindowChromeAdapter";
import { WindowControls } from "../platform/window/ui/WindowControls";
import { TauriWorkspaceIndexAdapter } from "../features/workspace-index/adapters/TauriWorkspaceIndexAdapter";
import { AppShell } from "./AppShell";

const authentication = new TauriAuthenticationAdapter();
const hostLifecycle = new TauriHostLifecycleAdapter();
const windowChrome = new TauriWindowChromeAdapter();
const workspaceIndex = new TauriWorkspaceIndexAdapter();
const projectConversation = new TauriProjectConversationAdapter();
const conversationHistory = new TauriConversationHistoryAdapter();
const conversationComposer = new TauriConversationComposerAdapter();
const conversationAttachments = new TauriConversationAttachmentAdapter();
const conversationRealtime = new TauriConversationRealtimeAdapter();
const conversationRuntimeSettings = new TauriConversationRuntimeSettingsAdapter();
const runInspector = new TauriRunInspectorAdapter();

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
            hostLifecycle={hostLifecycle}
            workspaceIndex={workspaceIndex}
            projectConversation={projectConversation}
            conversationHistory={conversationHistory}
            conversationComposer={conversationComposer}
            conversationAttachments={conversationAttachments}
            conversationRealtime={conversationRealtime}
            conversationRuntimeSettings={conversationRuntimeSettings}
            runInspector={runInspector}
            session={session}
            onLogout={() => logout(authentication, () => stopHost(hostLifecycle))}
          />
        )}
      </AuthenticationGate>
    </main>
  );
}
