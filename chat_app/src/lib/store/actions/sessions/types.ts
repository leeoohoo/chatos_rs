// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
  force?: boolean;
  limit?: number;
  offset?: number;
  append?: boolean;
  silent?: boolean;
};
