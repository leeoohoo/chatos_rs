// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { SystemConfigResponse } from '../types';
import { request } from './client';

export const systemApi = {
  config: () => request<SystemConfigResponse>('/api/system/config'),
};
