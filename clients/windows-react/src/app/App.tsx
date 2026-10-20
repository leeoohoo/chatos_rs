import { Route, Routes } from "react-router";
import { TauriAuthenticationAdapter } from "../features/authentication/adapters/TauriAuthenticationAdapter";
import { useAuthenticationStore } from "../features/authentication/application/authenticationStore";
import { AuthenticationGate } from "../features/authentication/ui/AuthenticationGate";
import { TauriHostLifecycleAdapter } from "../features/host-lifecycle/adapters/TauriHostLifecycleAdapter";
import { useHostLifecycleStore } from "../features/host-lifecycle/application/hostLifecycleStore";
import { TauriWindowChromeAdapter } from "../platform/window/adapters/TauriWindowChromeAdapter";
import { WindowControls } from "../platform/window/ui/WindowControls";
import { TauriWorkspaceIndexAdapter } from "../features/workspace-index/adapters/TauriWorkspaceIndexAdapter";
import { AppShell } from "./AppShell";

const authentication = new TauriAuthenticationAdapter();
const hostLifecycle = new TauriHostLifecycleAdapter();
const windowChrome = new TauriWindowChromeAdapter();
const workspaceIndex = new TauriWorkspaceIndexAdapter();

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
            session={session}
            onLogout={() => logout(authentication, () => stopHost(hostLifecycle))}
          />
        )}
      </AuthenticationGate>
    </main>
  );
}
