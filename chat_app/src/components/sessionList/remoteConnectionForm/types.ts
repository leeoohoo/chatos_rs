import type {
  RemoteConnectionDraftPayload,
  RemoteConnectionTestResponse,
  RemoteConnectionUpdatePayload,
} from '../../../lib/api/client/types';
import type { TranslateFn } from '../../../i18n/I18nProvider';
import type { RemoteConnection } from '../../../types';

export interface RemoteConnectionApiClient {
  testRemoteConnectionDraft(
    data: RemoteConnectionDraftPayload,
    verificationCode?: string,
  ): Promise<RemoteConnectionTestResponse>;
  testRemoteConnection(
    id: string,
    verificationCode?: string,
  ): Promise<RemoteConnectionTestResponse>;
}

export interface RemoteConnectionTestResult extends RemoteConnectionTestResponse {
  remote_host?: string;
  remoteHost?: string;
}

export interface UseRemoteConnectionFormOptions {
  apiClient: RemoteConnectionApiClient;
  t?: TranslateFn;
  remoteConnections: RemoteConnection[];
  createRemoteConnection: (payload: RemoteConnectionDraftPayload) => Promise<RemoteConnection>;
  updateRemoteConnection: (
    id: string,
    payload: RemoteConnectionUpdatePayload,
  ) => Promise<RemoteConnection | null>;
}
