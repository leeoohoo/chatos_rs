import { useState } from "react";
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
import { TauriGlobalUtilityPreferencesAdapter } from "../features/global-utilities/adapters/TauriGlobalUtilityPreferencesAdapter";
import { PetOverlaySurface } from "../features/pet/ui/PetOverlaySurface";
import { PetActivitySurface } from "../features/pet/ui/PetActivitySurface";
import { TauriPetPresentationAdapter } from "../features/pet/adapters/TauriPetPresentationAdapter";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriWindowChromeAdapter } from "../platform/window/adapters/TauriWindowChromeAdapter";
import { WindowControls } from "../platform/window/ui/WindowControls";
import { TauriWorkspaceIndexAdapter } from "../features/workspace-index/adapters/TauriWorkspaceIndexAdapter";
import { AppShell } from "./AppShell";
import { TauriClipboardHistoryAdapter } from "../features/clipboard-history/adapters/TauriClipboardHistoryAdapter";
import { ClipboardHistorySurface } from "../features/clipboard-history/ui/ClipboardHistorySurface";
import { TauriQuickSearchAdapter } from "../features/quick-search/adapters/TauriQuickSearchAdapter";
import { QuickSearchSurface } from "../features/quick-search/ui/QuickSearchSurface";

const authentication = new TauriAuthenticationAdapter();
const askUser = new TauriAskUserAdapter();
const hostLifecycle = new TauriHostLifecycleAdapter();
const pluginApplication = new TauriPluginApplicationAdapter();
const generalSettings = new TauriGeneralSettingsAdapter();
const petPreferences = new TauriPetPreferencesAdapter();
const globalUtilities = new TauriGlobalUtilityPreferencesAdapter();
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
const clipboardHistory = new TauriClipboardHistoryAdapter();
const quickSearch = new TauriQuickSearchAdapter();

export function App() {
  if (new URLSearchParams(window.location.search).get("surface") === "pet-overlay") {
    return <PetOverlaySurface port={petPresentation} />;
  }
  if (new URLSearchParams(window.location.search).get("surface") === "pet-activity") {
    return <PetActivitySurface port={petPresentation} />;
  }
  if (new URLSearchParams(window.location.search).get("surface") === "clipboard-history") {
    return <ClipboardHistorySurface port={clipboardHistory} />;
  }
  if (new URLSearchParams(window.location.search).get("surface") === "quick-search") {
    return <QuickSearchSurface port={quickSearch} />;
  }
  return (
    <Routes>
      <Route path="*" element={<ApplicationRoot />} />
    </Routes>
  );
}

function ApplicationRoot() {
  const logout = useAuthenticationStore((state) => state.logout);
  const session = useAuthenticationStore((state) => state.session);
  const stopHost = useHostLifecycleStore((state) => state.stop);
  const [notepadOpen, setNotepadOpen] = useState(false);

  return (
    <main className="app-window">
      <header
        className="app-titlebar"
        data-tauri-drag-region
        onDoubleClick={() => void windowChrome.toggleMaximize()}
      >
        <WindowControls windowChrome={windowChrome} />
        <span className="app-titlebar__title" data-tauri-drag-region>ChatOS</span>
        <span className="app-titlebar__actions">
          {session && (
            <button
              aria-label="打开记事本"
              className="app-titlebar__notepad"
              onClick={(event) => {
                event.stopPropagation();
                setNotepadOpen(true);
              }}
              title="打开记事本"
              type="button"
            >▤</button>
          )}
        </span>
      </header>
      <AuthenticationGate authentication={authentication}>
        {(session) => (
          <AppShell
            hostLifecycle={hostLifecycle}
            generalSettings={generalSettings}
            petPreferences={petPreferences}
            globalUtilities={globalUtilities}
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
            quickSearchNavigation={quickSearch}
            session={session}
            notepadOpen={notepadOpen}
            onNotepadOpenChange={setNotepadOpen}
            onLogout={async () => {
              setNotepadOpen(false);
              await logout(authentication, () => stopHost(hostLifecycle));
            }}
          />
        )}
      </AuthenticationGate>
    </main>
  );
}
