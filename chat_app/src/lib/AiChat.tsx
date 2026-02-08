import React from 'react';
import { StandaloneChatInterface } from '../components/StandaloneChatInterface';
import { createChatStoreWithBackend } from './store/createChatStoreWithBackend';
import type { Application } from '../types';
import ApiClient from './api/client';
import { debugLog } from '@/lib/utils';

export interface AiChatConfig {
  userId: string;
  projectId: string;
  configUrl?: string;
  className?: string;
  showMcpManager?: boolean;
  showAiModelManager?: boolean;
  showSystemContextEditor?: boolean;
  showAgentManager?: boolean;
  onApplicationSelect?: (app: Application) => void;
}

/**
 * AiChat 类 - 支持通过构造函数实例化的聊天组件
 * 
 * 使用方式:
 * ```typescript
 *
 * // 在React组件中使用
 * function App() {
 *   return <div>{aiChat.render()}</div>;
 * }
 * ```
 */
export class AiChat {
  private userId: string;
  private projectId: string;
  private configUrl: string;
  private apiClient: ApiClient;
  private store: ReturnType<typeof createChatStoreWithBackend>;
  private className?: string;
  private showMcpManager: boolean;
  private showAiModelManager: boolean;
  private showSystemContextEditor: boolean;
  private showAgentManager: boolean;
  private onApplicationSelect?: (app: Application) => void;

  constructor(
    userId: string,
    projectId: string,
    configUrl?: string,
    className?: string,
    showMcpManager: boolean = true,
    showAiModelManager: boolean = true,
    showSystemContextEditor: boolean = true,
    showAgentManager: boolean = true,
    onApplicationSelect?: (app: Application) => void
  ) {
    this.userId = userId;
    this.projectId = projectId;
    this.configUrl = configUrl || '/api';
    this.className = className;
    this.showMcpManager = showMcpManager;
    this.showAiModelManager = showAiModelManager;
    this.showSystemContextEditor = showSystemContextEditor;
    this.showAgentManager = showAgentManager;
    this.onApplicationSelect = onApplicationSelect;

    debugLog('🔧 AiChat Constructor - configUrl:', this.configUrl);
    debugLog('🔧 AiChat Constructor - Module Controls:', {
      showMcpManager: this.showMcpManager,
      showAiModelManager: this.showAiModelManager,
      showSystemContextEditor: this.showSystemContextEditor,
      showAgentManager: this.showAgentManager,
      hasApplicationSelectCallback: !!this.onApplicationSelect
    });

    // 创建自定义的 API 客户端
    this.apiClient = new ApiClient(this.configUrl);
    
    // 创建自定义的 store，传入 userId、projectId 和 configUrl
    this.store = createChatStoreWithBackend(this.apiClient, {
      userId: this.userId,
      projectId: this.projectId,
      configUrl: this.configUrl
    });
  }

  /**
   * 渲染聊天界面
   * @returns React 元素
   */
  render(): React.ReactElement {
    return React.createElement(AiChatComponent, {
      className: this.className,
      userId: this.userId,
      projectId: this.projectId,
      configUrl: this.configUrl,
      showMcpManager: this.showMcpManager,
      showAiModelManager: this.showAiModelManager,
      showSystemContextEditor: this.showSystemContextEditor,
      showAgentManager: this.showAgentManager,
      onApplicationSelect: this.onApplicationSelect
    });
  }

  /**
   * 获取当前配置
   */
  getConfig(): AiChatConfig {
    return {
      userId: this.userId,
      projectId: this.projectId,
      configUrl: this.configUrl,
      className: this.className,
      showMcpManager: this.showMcpManager,
      showAiModelManager: this.showAiModelManager,
      showSystemContextEditor: this.showSystemContextEditor,
      showAgentManager: this.showAgentManager,
      onApplicationSelect: this.onApplicationSelect
    };
  }

  /**
   * 更新配置
   */
  updateConfig(config: Partial<AiChatConfig>): void {
    if (config.userId) this.userId = config.userId;
    if (config.projectId) this.projectId = config.projectId;
    if (config.configUrl) {
      this.configUrl = config.configUrl;
      this.apiClient = new ApiClient(this.configUrl);
      this.store = createChatStoreWithBackend(this.apiClient, {
        userId: this.userId,
        projectId: this.projectId,
        configUrl: this.configUrl
      });
    }
    if (config.className !== undefined) this.className = config.className;
    if (config.showMcpManager !== undefined) this.showMcpManager = config.showMcpManager;
    if (config.showAiModelManager !== undefined) this.showAiModelManager = config.showAiModelManager;
    if (config.showSystemContextEditor !== undefined) this.showSystemContextEditor = config.showSystemContextEditor;
    if (config.showAgentManager !== undefined) this.showAgentManager = config.showAgentManager;
    if (config.onApplicationSelect !== undefined) this.onApplicationSelect = config.onApplicationSelect;
  }

  /**
   * 获取 store 实例（用于高级用法）
   */
  getStore(): import('./store/createChatStoreWithBackend').ChatStore {
    return this.store;
  }

  /**
   * 获取 API 客户端实例（用于高级用法）
   */
  getApiClient(): ApiClient {
    return this.apiClient;
  }

  /**
   * 获取当前选中的应用对象
   */
  getSelectedApplication(): Application | null {
    const state = this.store.getState();
    const id = state.selectedApplicationId;
    debugLog('[AiChat] getSelectedApplication called:', { id, applicationsCount: state.applications?.length });
    if (!id) return null;
    const app = state.applications.find(app => app.id === id) || null;
    debugLog('[AiChat] getSelectedApplication result:', app);
    return app;
  }

  /**
   * 获取当前选中的应用 URL
   */
  getSelectedApplicationUrl(): string | null {
    const app = this.getSelectedApplication();
    return app ? app.url : null;
  }

  /**
   * 订阅应用选择变化（实时回调当前应用对象），返回取消订阅函数
   */
  subscribeSelectedApplication(
    listener: (app: Application | null) => void
  ): () => void {
    debugLog('[AiChat] subscribeSelectedApplication: 设置订阅');
    debugLog('[AiChat] store 对象:', this.store);
    debugLog('[AiChat] store.subscribe 类型:', typeof this.store.subscribe);

    let previousId = this.store.getState().selectedApplicationId;
    debugLog('[AiChat] 初始 previousId:', previousId);

    // 使用基础的 subscribe，监听整个 state 的变化
    const unsubscribe = this.store.subscribe((state) => {
      const currentId = state.selectedApplicationId;
      debugLog('[AiChat] store.subscribe 被触发, currentId:', currentId, 'previousId:', previousId);

      if (currentId !== previousId) {
        debugLog('[AiChat] selectedApplicationId 变化检测到:', { old: previousId, new: currentId });
        previousId = currentId;
        const app = this.getSelectedApplication();
        debugLog('[AiChat] 准备调用 listener，app:', app);
        listener(app);
      }
    });

    // 立即推送当前值，保证首次订阅就拿到状态
    debugLog('[AiChat] 首次调用 listener');
    listener(this.getSelectedApplication());

    return () => {
      debugLog('[AiChat] 取消订阅');
      unsubscribe();
    };
  }
}

/**
 * 内部组件，用于渲染聊天界面
 */
interface AiChatComponentProps {
  className?: string;
  userId: string;
  projectId: string;
  configUrl: string;
  showMcpManager?: boolean;
  showAiModelManager?: boolean;
  showSystemContextEditor?: boolean;
  showAgentManager?: boolean;
  onApplicationSelect?: (app: Application) => void;
}

const AiChatComponent: React.FC<AiChatComponentProps> = ({
  className,
  userId,
  projectId,
  configUrl,
  showMcpManager,
  showAiModelManager,
  showSystemContextEditor,
  showAgentManager,
  onApplicationSelect
}) => {
  return (
    <StandaloneChatInterface
      className={className}
      apiBaseUrl={configUrl}
      userId={userId}
      projectId={projectId}
      showMcpManager={showMcpManager}
      showAiModelManager={showAiModelManager}
      showSystemContextEditor={showSystemContextEditor}
      showAgentManager={showAgentManager}
      onApplicationSelect={onApplicationSelect}
    />
  );
};

export default AiChat;