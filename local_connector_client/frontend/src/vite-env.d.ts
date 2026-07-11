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
  reloadChatOS?: () => Promise<void>;
}

interface Window {
  chatosLocalConnector?: ChatosLocalConnectorBridge;
}
