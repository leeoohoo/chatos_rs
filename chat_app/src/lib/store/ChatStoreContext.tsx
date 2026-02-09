import React, { createContext, useContext, ReactNode } from 'react';
import { useChatStore } from './index';
import { createChatStoreWithBackend } from './createChatStoreWithBackend';
import type { ChatStore as ChatStoreHook, ChatState, ChatActions } from './createChatStoreWithBackend';
import type ApiClient from '../api/client';
import { debugLog } from '@/lib/utils';

// Store类型定义
type ChatStore = ChatStoreHook;

// Context接口
interface ChatStoreContextType {
  store: ChatStore;
  userId?: string;
  projectId?: string;
  apiClient?: ApiClient;
}

// 创建Context
const ChatStoreContext = createContext<ChatStoreContextType | null>(null);

// Provider Props
interface ChatStoreProviderProps {
  children: ReactNode;
  userId?: string;
  projectId?: string;
  customApiClient?: ApiClient;
}

// Provider组件
export const ChatStoreProvider: React.FC<ChatStoreProviderProps> = ({
  children,
  userId,
  projectId,
  customApiClient
}) => {
  // 根据是否有自定义参数决定使用哪个store
  const store = React.useMemo(() => {
    if (userId || projectId || customApiClient) {
      debugLog('🏪 创建自定义store:', { userId, projectId, hasCustomApiClient: !!customApiClient });
      return createChatStoreWithBackend(customApiClient, {
        userId: userId || 'default-user',
        projectId: projectId || 'default-project',
      });
    } else {
      debugLog('🏪 使用默认store');
      return useChatStore;
    }
  }, [userId, projectId, customApiClient]);

  return (
    <ChatStoreContext.Provider value={{ store, userId, projectId, apiClient: customApiClient }}>
      {children}
    </ChatStoreContext.Provider>
  );
};

// Hook来使用Context
export const useChatStoreContext = (): ChatStore => {
  const context = useContext(ChatStoreContext);
  if (!context) {
    throw new Error('useChatStoreContext must be used within a ChatStoreProvider');
  }
  return context.store;
};

// 为了向后兼容，导出一个hook来获取store的状态和方法
export const useChatStoreFromContext = (): ChatState & ChatActions => {
  const store = useChatStoreContext();
  return store();
};

// 新增：导出当前运行环境（userId、projectId）
export const useChatRuntimeEnv = () => {
  const context = useContext(ChatStoreContext);
  if (!context) {
    return { userId: undefined, projectId: undefined } as const;
  }
  return { userId: context.userId, projectId: context.projectId } as const;
};

// 新增：从Context中获取自定义ApiClient（如果存在）
export const useChatApiClientFromContext = () => {
  const context = useContext(ChatStoreContext);
  return context?.apiClient;
};
