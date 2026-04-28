import type ApiClient from '../../../api/client';
import type {
  ChatStoreGet,
  ChatStoreSet,
} from '../../types';

export interface SessionActionDeps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getSessionParams: () => { userId: string; projectId: string };
  customUserId?: string;
  customProjectId?: string;
}

export type LoadSessionsOptions = {
  limit?: number;
  offset?: number;
  append?: boolean;
  silent?: boolean;
};
