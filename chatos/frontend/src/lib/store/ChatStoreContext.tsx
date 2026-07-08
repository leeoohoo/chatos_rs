// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { createContext, useContext, ReactNode } from 'react';
import { useStoreWithEqualityFn } from 'zustand/traditional';
import { useApiClientContext } from '../api/ApiClientContext';
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
  const resolvedApiClient = useApiClientContext();
  const effectiveApiClient = customApiClient || resolvedApiClient;

  const store = React.useMemo(() => {
    debugLog('🏪 创建上下文 store:', {
      userId,
      projectId,
      hasCustomApiClient: Boolean(customApiClient),
    });
    return createChatStoreWithBackend(effectiveApiClient, {
      userId: userId || undefined,
      projectId: projectId || undefined,
    });
  }, [customApiClient, effectiveApiClient, projectId, userId]);

  return (
    <ChatStoreContext.Provider value={{ store, userId, projectId }}>
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

export const useChatStoreSelector = <T,>(
  selector: (state: ChatState & ChatActions) => T,
  equalityFn?: (left: T, right: T) => boolean,
): T => {
  const store = useChatStoreContext();
  return useStoreWithEqualityFn(store, selector, equalityFn);
};

export const useOptionalChatStoreContext = (): ChatStore | null => {
  const context = useContext(ChatStoreContext);
  return context?.store ?? null;
};

export const useChatStoreResolved = (): ChatState & ChatActions => {
  const store = useChatStoreContext();
  return useStoreWithEqualityFn(store, (state) => state);
};

// 新增：导出当前运行环境（userId、projectId）
export const useChatRuntimeEnv = () => {
  const context = useContext(ChatStoreContext);
  if (!context) {
    return { userId: undefined, projectId: undefined } as const;
  }
  return { userId: context.userId, projectId: context.projectId } as const;
};
