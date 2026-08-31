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
  openSettings?: (tab?: string) => Promise<void>;
  closeSettings?: () => Promise<boolean>;
  setApprovalOverlayMode?: (mode: 'hidden' | 'compact' | 'expanded') => Promise<boolean>;
  setVisualPreviewMode?: (mode: 'hidden' | 'collapsed' | 'expanded') => Promise<boolean>;
  onSettingsTabRequested?: (callback: (tab: string) => void) => () => void;
  reloadChatOS?: () => Promise<void>;
  setDeveloperMode?: (enabled: boolean) => Promise<boolean>;
  runtimeSettings?: () => Promise<{
    developer_mode: boolean;
    developer_cloud_base_url: string;
    developer_user_service_base_url: string;
    developer_chatos_web_url: string;
  }>;
  updateRuntimeSettings?: (payload: {
    developer_mode?: boolean;
    developer_cloud_base_url?: string;
    developer_user_service_base_url?: string;
    developer_chatos_web_url?: string;
  }) => Promise<{
    developer_mode: boolean;
    developer_cloud_base_url: string;
    developer_user_service_base_url: string;
    developer_chatos_web_url: string;
  }>;
  selectPluginFiles?: () => Promise<string[]>;
}

interface Window {
  chatosLocalConnector?: ChatosLocalConnectorBridge;
}
