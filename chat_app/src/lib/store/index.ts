// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { apiClient } from '../api/client';
import { createChatStoreWithBackend } from './createChatStoreWithBackend';
import type { ChatStore as ChatStoreHook } from './createChatStoreWithBackend';

// 默认的聊天store实例
export const useChatStore: ChatStoreHook = createChatStoreWithBackend(apiClient);
