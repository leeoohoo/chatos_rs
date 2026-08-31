// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { adminApi } from './api/admin';
import { threadsApi } from './api/threads';

export const api = {
  ...adminApi,
  ...threadsApi,
};
