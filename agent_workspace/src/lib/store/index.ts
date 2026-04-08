import { createChatStoreWithBackend } from './createChatStoreWithBackend';
import type { ChatStore as ChatStoreHook } from './createChatStoreWithBackend';

// 默认的聊天store实例
export const useChatStore: ChatStoreHook = createChatStoreWithBackend();
