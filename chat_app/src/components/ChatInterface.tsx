import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useChatApiClientFromContext, useChatStoreFromContext } from '../lib/store/ChatStoreContext';
import { MessageList } from './MessageList';
import { InputArea } from './InputArea';
import { SessionList } from './SessionList';
import { ThemeToggle } from './ThemeToggle';
import McpManager from './McpManager';
import AiModelManager from './AiModelManager';
import SystemContextEditor from './SystemContextEditor';
import AgentManager from './AgentManager';
import UserSettingsPanel from './UserSettingsPanel';
import ProjectExplorer from './ProjectExplorer';
import TerminalView from './TerminalView';
// 应用弹窗管理器由 ApplicationsPanel 直接承担
import ApplicationsPanel from './ApplicationsPanel';
import TaskDraftPanel from './TaskDraftPanel';
import TaskWorkbar, { type TaskWorkbarItem } from './TaskWorkbar';
import ApiClient from '../lib/api/client';
import { cn } from '../lib/utils';
import type { ChatInterfaceProps } from '../types';
import type { TaskReviewDraft } from '../lib/store/types';

const SESSION_PAGE_SIZE = 30;

export const ChatInterface: React.FC<ChatInterfaceProps> = ({
  className,
  onMessageSend,
  customRenderer,
}) => {
  const {
    currentSession,
    currentProject,
    currentTerminal,
    projects,
    activePanel,
    messages,
    hasMoreMessages,
    error,
    loadSessions,
    loadProjects,
    // selectSession,
    loadMoreMessages,
    sendMessage,
    clearError,
    sidebarOpen,
    toggleSidebar,
    aiModelConfigs,
    selectedModelId,
    setSelectedModel,
    loadAiModelConfigs,
    agents,
    selectedAgentId,
    setSelectedAgent,
    loadAgents,
    chatConfig,
    updateChatConfig,
    abortCurrentConversation,
    sessionChatState = {},
    taskReviewPanel,
    setTaskReviewPanel,
    // applications,  // 不再在此组件中使用
    // selectedApplicationId,  // 不再用于自动显示
  } = useChatStoreFromContext();

  const apiClientFromContext = useChatApiClientFromContext();
  const apiClient = useMemo(() => apiClientFromContext || new ApiClient(), [apiClientFromContext]);

  const selectedAgent = useMemo(
    () => (selectedAgentId ? agents.find((a: any) => a.id === selectedAgentId) : null),
    [agents, selectedAgentId]
  );
  const activeModelConfig = useMemo(() => (
    selectedAgent
      ? aiModelConfigs.find((m: any) => m.id === selectedAgent.ai_model_config_id)
      : aiModelConfigs.find((m: any) => m.id === selectedModelId)
  ), [aiModelConfigs, selectedAgent, selectedModelId]);
  const supportsImages = activeModelConfig?.supports_images === true;
  const supportsReasoning = activeModelConfig?.supports_reasoning === true;
  const supportedFileTypes = useMemo(() => (
    supportsImages
      ? ['image/*', 'text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
      : ['text/*', 'application/json', 'application/pdf', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document']
  ), [supportsImages]);
  const currentChatState = useMemo(() => (
    currentSession ? sessionChatState[currentSession.id] : undefined
  ), [currentSession, sessionChatState]);
  const chatIsLoading = currentChatState?.isLoading ?? false;
  const chatIsStreaming = currentChatState?.isStreaming ?? false;
  const headerTitle = activePanel === 'project'
    ? (currentProject?.name || '项目')
    : activePanel === 'terminal'
      ? (currentTerminal?.name || '终端')
      : (currentSession?.title || '');

  const [showMcpManager, setShowMcpManager] = useState(false);
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showAgentManager, setShowAgentManager] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);
  const didInitRef = useRef(false);
  const [workbarTasks, setWorkbarTasks] = useState<TaskWorkbarItem[]>([]);
  const [workbarLoading, setWorkbarLoading] = useState(false);
  const [workbarError, setWorkbarError] = useState<string | null>(null);

  const normalizeWorkbarTask = useCallback((raw: any): TaskWorkbarItem => {
    const statusRaw = String(raw?.status || 'todo').toLowerCase();
    const status: TaskWorkbarItem['status'] =
      statusRaw === 'doing' || statusRaw === 'blocked' || statusRaw === 'done'
        ? statusRaw
        : 'todo';

    const priorityRaw = String(raw?.priority || 'medium').toLowerCase();
    const priority: TaskWorkbarItem['priority'] =
      priorityRaw === 'high' || priorityRaw === 'low' ? priorityRaw : 'medium';

    return {
      id: String(raw?.id || ''),
      title: String(raw?.title || ''),
      details: String(raw?.details || ''),
      status,
      priority,
      conversationTurnId: String(raw?.conversation_turn_id || ''),
      createdAt: String(raw?.created_at || ''),
      dueAt: raw?.due_at ? String(raw.due_at) : null,
      tags: Array.isArray(raw?.tags) ? raw.tags.map((tag: any) => String(tag)) : [],
    };
  }, []);

  const loadWorkbarTasks = useCallback(async (sessionId: string, conversationTurnId?: string) => {
    if (!sessionId) {
      setWorkbarTasks([]);
      setWorkbarError(null);
      return;
    }

    setWorkbarLoading(true);
    setWorkbarError(null);
    try {
      const tasks = await apiClient.getTaskManagerTasks(sessionId, {
        conversationTurnId,
        includeDone: false,
        limit: 100,
      });
      setWorkbarTasks(tasks.map(normalizeWorkbarTask));
    } catch (error) {
      setWorkbarError(error instanceof Error ? error.message : '任务加载失败');
    } finally {
      setWorkbarLoading(false);
    }
  }, [apiClient, normalizeWorkbarTask]);

  // 初始化加载会话、AI模型和智能体配置
  useEffect(() => {
    // React 18 在开发模式下会双调用副作用，这里加一次性保护（组件内）
    if (didInitRef.current) return;
    didInitRef.current = true;

    loadSessions({ limit: SESSION_PAGE_SIZE, offset: 0 });
    loadProjects();
    loadAiModelConfigs();
    loadAgents();
  }, [loadSessions, loadProjects, loadAiModelConfigs, loadAgents]);

  useEffect(() => {
    if (!currentSession || activePanel !== 'chat') {
      setWorkbarTasks([]);
      setWorkbarError(null);
      return;
    }

    void loadWorkbarTasks(currentSession.id);
  }, [activePanel, currentSession, loadWorkbarTasks]);

  // 处理消息发送
  const handleMessageSend = useCallback(async (content: string, attachments?: File[]) => {
    try {
      await sendMessage(content, attachments);
      onMessageSend?.(content, attachments);
    } catch (error) {
      console.error('Failed to send message:', error);
    }
  }, [onMessageSend, sendMessage]);

  const handleLoadMore = useCallback(() => {
    if (currentSession) {
      loadMoreMessages(currentSession.id);
    }
  }, [currentSession, loadMoreMessages]);

  const handleTaskReviewConfirm = useCallback(async (drafts: TaskReviewDraft[]) => {
    if (!taskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...taskReviewPanel,
      drafts,
      submitting: true,
      error: null,
    };
    setTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(taskReviewPanel.reviewId, {
        action: 'confirm',
        tasks: drafts.map((draft) => ({
          title: draft.title,
          details: draft.details,
          priority: draft.priority,
          status: draft.status,
          tags: draft.tags,
          due_at: draft.dueAt || undefined,
        })),
      });
      setTaskReviewPanel(null);
      await loadWorkbarTasks(taskReviewPanel.sessionId);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务确认提交失败';
      setTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [apiClient, loadWorkbarTasks, setTaskReviewPanel, taskReviewPanel]);

  const handleTaskReviewCancel = useCallback(async () => {
    if (!taskReviewPanel) {
      return;
    }

    const pendingPanel = {
      ...taskReviewPanel,
      submitting: true,
      error: null,
    };
    setTaskReviewPanel(pendingPanel);

    try {
      await apiClient.submitTaskReviewDecision(taskReviewPanel.reviewId, {
        action: 'cancel',
        reason: 'user_cancelled',
      });
      setTaskReviewPanel(null);
      await loadWorkbarTasks(taskReviewPanel.sessionId);
    } catch (error) {
      const message = error instanceof Error ? error.message : '任务取消提交失败';
      setTaskReviewPanel({
        ...pendingPanel,
        submitting: false,
        error: message,
      });
    }
  }, [apiClient, loadWorkbarTasks, setTaskReviewPanel, taskReviewPanel]);


  return (
    <div className={cn(
      'flex flex-col h-screen bg-background text-foreground',
      className
    )}>
      {/* 头部 - 包含会话按钮和主题切换 */}
      <div className="flex items-center justify-between p-4 bg-card border-b border-border">
        <div className="flex items-center space-x-3">
          <button
            onClick={toggleSidebar}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title={sidebarOpen ? '收起会话列表' : '展开会话列表'}
          >
            <svg className={`w-5 h-5 transition-transform ${sidebarOpen ? '' : 'rotate-180'}`} fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M15 18L9 12l6-6" />
            </svg>
          </button>
          
          {headerTitle && (
            <div className="flex-1 min-w-0">
              <h1 className="text-lg font-semibold text-foreground truncate">
                {headerTitle}
              </h1>
            </div>
          )}
        </div>
        
        <div className="flex items-center space-x-2">
          <button
            onClick={() => setShowApplicationsPanel(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="打开应用列表"
          >
            <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path d="M4 5h6v14H4z" strokeWidth="2" />
              <path d="M12 5h8v14h-8z" strokeWidth="2" />
            </svg>
          </button>
          <button
            onClick={() => setShowMcpManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="MCP 服务器管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
            </svg>
          </button>
          <button
            onClick={() => setShowAgentManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="智能体管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6M9 16h6M6 8h12a2 2 0 012 2v8a2 2 0 01-2 2H6a2 2 0 01-2-2v-8a2 2 0 012-2z" />
            </svg>
          </button>
          <button
            onClick={() => setShowAiModelManager(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="AI 模型管理"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
            </svg>
          </button>
          <button
            onClick={() => setShowSystemContextEditor(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="系统上下文设置"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </button>
          <ThemeToggle />
          {/* 设置按钮放到最右侧 */}
          <button
            onClick={() => setShowUserSettings(true)}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
            title="用户参数设置"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 16v-2m8-6h2M4 12H2m15.364 5.364l1.414 1.414M5.636 6.636L4.222 5.222m12.728 0l1.414 1.414M5.636 17.364l-1.414 1.414" />
            </svg>
          </button>
        </div>
      </div>

          {/* 错误提示 */}
          {error && (
            <div className="mx-4 mt-4 p-3 bg-destructive/10 border border-destructive/20 rounded-lg">
              <div className="flex items-center justify-between">
                <p className="text-sm text-destructive">{error}</p>
                <button
                  onClick={clearError}
                  className="text-destructive hover:text-destructive/80 transition-colors"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            </div>
          )}

        {/* 主区域：左侧会话列表 + 右侧聊天 */}
        <div className="flex flex-1 overflow-hidden">
          <SessionList
            collapsed={!sidebarOpen}
            onToggleCollapse={toggleSidebar}
          />

          {/* 已移除左侧应用抽屉面板，改为弹窗 */}
          {/* 嵌入区域已移除 - 应用选择后只触发事件，不自动显示 */}
          {/* 外部可以通过 subscribeSelectedApplication 监听应用选择事件 */}
          {/* 然后自行决定如何打开/显示应用（Electron 窗口、window.open 等） */}

          <div className="flex-1 flex flex-col overflow-hidden">
            {activePanel === 'project' ? (
              <ProjectExplorer project={currentProject} className="flex-1" />
            ) : activePanel === 'terminal' ? (
              <TerminalView className="flex-1" />
            ) : (
              <div className="flex-1 flex overflow-hidden">
                <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
                  <div className="flex-1 overflow-hidden">
                    {currentSession ? (
                      <MessageList
                        messages={messages}
                        isLoading={chatIsLoading}
                        isStreaming={chatIsStreaming}
                        hasMore={hasMoreMessages}
                        onLoadMore={handleLoadMore}
                        customRenderer={customRenderer}
                      />
                    ) : (
                      <div className="flex items-center justify-center h-full">
                        <div className="text-center">
                          <h2 className="text-xl font-semibold text-muted-foreground mb-2">
                            欢迎使用 AI 聊天
                          </h2>
                          <p className="text-muted-foreground mb-4">
                            点击左上角按钮选择会话，或创建新的会话开始对话
                          </p>
                          <button
                            onClick={toggleSidebar}
                            className="px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors"
                          >
                            展开会话列表
                          </button>
                        </div>
                      </div>
                    )}
                  </div>

                  {/* 输入区域 */}
                  {currentSession && activePanel === 'chat' && (
                    <div className="border-t border-border">
                      <TaskWorkbar
                        tasks={workbarTasks}
                        isLoading={workbarLoading}
                        error={workbarError}
                        onRefresh={() => {
                          void loadWorkbarTasks(currentSession.id);
                        }}
                      />
                      {taskReviewPanel && taskReviewPanel.sessionId === currentSession.id ? (
                        <TaskDraftPanel
                          panel={taskReviewPanel}
                          onConfirm={handleTaskReviewConfirm}
                          onCancel={handleTaskReviewCancel}
                        />
                      ) : null}
                      <InputArea
                        onSend={handleMessageSend}
                        onStop={abortCurrentConversation}
                        disabled={chatIsLoading || chatIsStreaming}
                        isStreaming={chatIsStreaming}
                        placeholder="输入消息..."
                        allowAttachments={true}
                        supportedFileTypes={supportedFileTypes}
                        reasoningSupported={supportsReasoning}
                        reasoningEnabled={chatConfig?.reasoningEnabled === true}
                        onReasoningToggle={(enabled) => updateChatConfig({ reasoningEnabled: enabled })}
                        showModelSelector={true}
                        selectedModelId={selectedModelId}
                        availableModels={aiModelConfigs}
                        onModelChange={setSelectedModel}
                        selectedAgentId={selectedAgentId}
                        availableAgents={agents}
                        onAgentChange={setSelectedAgent}
                        availableProjects={projects}
                        currentProject={currentProject}
                      />
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
        
        {/* MCP管理器 */}
        {showMcpManager && (
          <McpManager onClose={() => setShowMcpManager(false)} />
        )}

        {/* 智能体管理器 */}
        {showAgentManager && (
          <AgentManager onClose={() => setShowAgentManager(false)} />
        )}
        
        {/* AI模型管理器 */}
        {showAiModelManager && (
          <AiModelManager onClose={() => setShowAiModelManager(false)} />
        )}
        
        {/* 系统上下文编辑器 */}
        {showSystemContextEditor && (
          <SystemContextEditor onClose={() => setShowSystemContextEditor(false)} />
        )}

        {showUserSettings && (
          <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
        )}

        {/* 应用列表（弹窗） */}
        <ApplicationsPanel
          isOpen={showApplicationsPanel}
          onClose={() => setShowApplicationsPanel(false)}
          title="应用列表"
          layout="modal"
        />

        {/* 表情助手已移除 */}
    </div>
  );
};

export default ChatInterface;
