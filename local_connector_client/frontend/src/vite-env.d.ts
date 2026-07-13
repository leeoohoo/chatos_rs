/// <reference types="vite/client" />

interface ChatosLocalConnectorBridge {
  apiRequest?: (request: {
    method?: string;
    endpoint: string;
    headers?: Record<string, string>;
    body?: string | null;
  }) => Promise<{
    status: number;
    ok: boolean;
    headers: Record<string, string | string[] | undefined>;
    body: string;
  }>;
  openSettings?: () => Promise<void>;
  closeSettings?: () => Promise<boolean>;
  reloadChatOS?: () => Promise<void>;
  setDeveloperMode?: (enabled: boolean) => Promise<boolean>;
  getDesktopSystemPermissions?: () => Promise<Record<string, {
    status: string;
    status_label: string;
    last_error?: string | null;
  }>>;
  requestDesktopSystemPermission?: (permissionId: string) => Promise<Record<string, {
    status: string;
    status_label: string;
    last_error?: string | null;
  }>>;
}

interface Window {
  chatosLocalConnector?: ChatosLocalConnectorBridge;
}
