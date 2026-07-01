import type { SystemConfigResponse } from '../types';
import { request } from './client';

export const systemApi = {
  config: () => request<SystemConfigResponse>('/api/system/config'),
};
