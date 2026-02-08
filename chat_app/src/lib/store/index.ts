import { createChatStoreWithBackend } from './createChatStoreWithBackend';
import type { ChatStore as ChatStoreHook } from './createChatStoreWithBackend';
import type ApiClient from '../api/client';

// 默认的聊天store实例
export const useChatStore: ChatStoreHook = createChatStoreWithBackend();

// 创建带配置的聊天store
export function createChatStoreWithConfig(userId: string, projectId: string, customApiClient?: ApiClient): ChatStoreHook {
  return createChatStoreWithBackend(customApiClient, { userId, projectId });
}

// 导出选择器hooks
export const useCurrentSession = () => useChatStore((state) => state.currentSession);
export const useMessages = () => useChatStore((state) => state.messages);
export const useSessions = () => useChatStore((state) => state.sessions);
export const useIsLoading = () => useChatStore((state) => state.isLoading);
export const useIsStreaming = () => useChatStore((state) => state.isStreaming);
export const useTheme = () => useChatStore((state) => state.theme);
export const useSidebarOpen = () => useChatStore((state) => state.sidebarOpen);
export const useError = () => useChatStore((state) => state.error);
export const useAiModelConfigs = () => useChatStore((state) => state.aiModelConfigs);
export const useSelectedModelId = () => useChatStore((state) => state.selectedModelId);