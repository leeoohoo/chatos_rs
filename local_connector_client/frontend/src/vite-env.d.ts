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
  runtimeSettings?: () => Promise<{
    developer_mode: boolean;
    browser_full_cdp_access_enabled: boolean;
    developer_cloud_base_url: string;
    developer_user_service_base_url: string;
    developer_chatos_web_url: string;
  }>;
  updateRuntimeSettings?: (payload: {
    developer_mode?: boolean;
    browser_full_cdp_access_enabled?: boolean;
    developer_cloud_base_url?: string;
    developer_user_service_base_url?: string;
    developer_chatos_web_url?: string;
    acknowledge_browser_full_cdp_risk?: boolean;
  }) => Promise<{
    developer_mode: boolean;
    browser_full_cdp_access_enabled: boolean;
    developer_cloud_base_url: string;
    developer_user_service_base_url: string;
    developer_chatos_web_url: string;
  }>;
  showChromeExtensionDirectory?: () => Promise<boolean>;
  copyChromeExtensionInstallPath?: () => Promise<string | false>;
  openChromeExtensionsPage?: () => Promise<boolean>;
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
