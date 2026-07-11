/// <reference types="vite/client" />

interface Window {
  chatosLocalConnector?: {
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
  };
}
