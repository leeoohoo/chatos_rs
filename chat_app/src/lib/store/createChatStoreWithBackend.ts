import {create} from 'zustand';
import {immer} from 'zustand/middleware/immer';
import {persist} from 'zustand/middleware';
import type {Message, Session, ChatConfig, Theme, McpConfig, AiModelConfig, SystemContext, AgentConfig, Application, Project} from '../../types';
import {apiClient} from '../api/client';
import type ApiClient from '../api/client';
import {createSendMessageHandler} from './actions/sendMessage';
import { createApplicationActions } from './actions/applications';
import { createAiModelActions } from './actions/aiModels';
import { createMcpActions } from './actions/mcp';
import { createChatConfigActions } from './actions/chatConfig';
import { debugLog } from '@/lib/utils';

// 聊天状态接口
export interface ChatState {
    // 会话相关
    sessions: Session[];
    currentSessionId: string | null;
    currentSession: Session | null;

    // 项目相关
    projects: Project[];
    currentProjectId: string | null;
    currentProject: Project | null;
    activePanel: 'chat' | 'project';

    // 消息相关
    messages: Message[];
    isLoading: boolean;
    isStreaming: boolean;
    streamingMessageId: string | null;
    hasMoreMessages: boolean;
    sessionChatState: Record<string, { isLoading: boolean; isStreaming: boolean; streamingMessageId: string | null }>;

    // UI状态
    sidebarOpen: boolean;
    theme: Theme;

    // 配置相关
    chatConfig: ChatConfig;
    mcpConfigs: McpConfig[];
    aiModelConfigs: AiModelConfig[];
    selectedModelId: string | null;
    agents: AgentConfig[];
    selectedAgentId: string | null;
    systemContexts: SystemContext[];
    activeSystemContext: SystemContext | null;
    // 应用相关
    applications: Application[];
    selectedApplicationId: string | null;

    // 错误处理
    error: string | null;
}

// 聊天操作接口
export interface ChatActions {
    // 会话操作
    loadSessions: (options?: { limit?: number; offset?: number; append?: boolean; silent?: boolean }) => Promise<Session[]>;
    createSession: (title?: string) => Promise<string>;
    selectSession: (sessionId: string) => Promise<void>;
    updateSession: (sessionId: string, updates: Partial<Session>) => Promise<void>;
    deleteSession: (sessionId: string) => Promise<void>;

    // 项目操作
    loadProjects: () => Promise<Project[]>;
    createProject: (name: string, rootPath: string, description?: string) => Promise<Project>;
    updateProject: (projectId: string, updates: Partial<Project>) => Promise<Project | null>;
    deleteProject: (projectId: string) => Promise<void>;
    selectProject: (projectId: string) => Promise<void>;
    setActivePanel: (panel: 'chat' | 'project') => void;

    // 消息操作
    loadMessages: (sessionId: string) => Promise<void>;
    loadMoreMessages: (sessionId: string) => Promise<void>;
    sendMessage: (content: string, attachments?: any[]) => Promise<void>;
    updateMessage: (messageId: string, updates: Partial<Message>) => Promise<void>;
    deleteMessage: (messageId: string) => Promise<void>;

    // 流式消息处理
    startStreaming: (messageId: string) => void;
    updateStreamingMessage: (content: string) => void;
    stopStreaming: () => void;
    abortCurrentConversation: () => void;

    // UI操作
    toggleSidebar: () => void;
    setTheme: (theme: Theme) => void;

    // 配置操作
    updateChatConfig: (config: Partial<ChatConfig>) => Promise<void>;
    loadMcpConfigs: () => Promise<void>;
    updateMcpConfig: (config: McpConfig) => Promise<McpConfig | null>;
    deleteMcpConfig: (id: string) => Promise<void>;
    loadAiModelConfigs: () => Promise<void>;
    updateAiModelConfig: (config: AiModelConfig) => Promise<void>;
    deleteAiModelConfig: (id: string) => Promise<void>;
    setSelectedModel: (modelId: string | null) => void;
    // 智能体
    loadAgents: () => Promise<void>;
    setSelectedAgent: (agentId: string | null) => void;
    loadSystemContexts: () => Promise<void>;
    createSystemContext: (name: string, content: string, appIds?: string[]) => Promise<any>;
    updateSystemContext: (id: string, name: string, content: string, appIds?: string[]) => Promise<any>;
    deleteSystemContext: (id: string) => Promise<void>;
    activateSystemContext: (id: string) => Promise<void>;
    // 应用管理
    loadApplications: () => Promise<void>;
    createApplication: (name: string, url: string, iconUrl?: string) => Promise<void>;
    updateApplication: (id: string, updates: Partial<Application>) => Promise<void>;
    deleteApplication: (id: string) => Promise<void>;
    setSelectedApplication: (appId: string | null) => void;
    setSystemContextAppAssociation: (contextId: string, appIds: string[]) => void;
    setAgentAppAssociation: (agentId: string, appIds: string[]) => void;

    // 错误处理
    setError: (error: string | null) => void;
    clearError: () => void;
}

// 自定义配置接口
export interface ChatStoreConfig {
    userId?: string;
    projectId?: string;
}

/**
 * 创建聊天store的工厂函数（使用后端API版本）
 * @param customApiClient 自定义的API客户端实例，如果不提供则使用默认的apiClient
 * @param config 自定义配置，包含userId和projectId
 * @returns 聊天store hook
 */
export function createChatStoreWithBackend(customApiClient?: ApiClient, config?: ChatStoreConfig) {
    const client = customApiClient || apiClient;
    const customUserId = config?.userId;
    const customProjectId = config?.projectId;
    
    // 使用传入的参数或默认值
    const userId = customUserId || 'default-user';
    const projectId = customProjectId || 'default-project';
    
    // 获取userId的统一函数
    const getUserIdParam = () => userId;
    
    // 获取会话相关参数的统一函数
    const getSessionParams = () => {
        return { userId, projectId };
    };
    
    const normalizeSession = (raw: any): Session => ({
        id: raw?.id,
        title: raw?.title ?? '',
        createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
        updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
        messageCount: raw?.messageCount ?? 0,
        tokenUsage: raw?.tokenUsage ?? 0,
        pinned: raw?.pinned ?? false,
        archived: raw?.archived ?? false,
        tags: raw?.tags ?? null,
        metadata: raw?.metadata ?? null,
    });

    const fetchSession = async (sessionId: string): Promise<Session | null> => {
        try {
            const session = await client.getSession(sessionId);
            if (!session) return null;
            return normalizeSession(session);
        } catch (error) {
            console.warn('Failed to fetch session:', error);
            return null;
        }
    };

    const fetchSessionMessages = async (sessionId: string, options: { limit?: number; offset?: number } = { limit: 10, offset: 0 }): Promise<Message[]> => {
        const limit = options.limit ?? 10;
        const offset = options.offset ?? 0;
        const messages = await client.getSessionMessages(sessionId, { limit, offset });

        const parsedMessages = messages.map((message: any) => {
            let parsedMetadata = undefined;
            if (message.metadata) {
                try {
                    parsedMetadata = typeof message.metadata === 'string' ? JSON.parse(message.metadata) : message.metadata;
                } catch (error) {
                    console.warn('Failed to parse message metadata:', error);
                    parsedMetadata = {};
                }
            }

            let parsedTopLevelToolCalls = undefined;
            if (message.toolCalls) {
                try {
                    parsedTopLevelToolCalls = typeof message.toolCalls === 'string' ? JSON.parse(message.toolCalls) : message.toolCalls;
                } catch (error) {
                    console.warn('Failed to parse top-level toolCalls:', error);
                }
            }

            return {
                id: message.id,
                sessionId: message.session_id,
                role: message.role as 'user' | 'assistant' | 'system' | 'tool',
                content: message.content,
                summary: message.summary,
                toolCallId: message.tool_call_id,
                reasoning: message.reasoning,
                metadata: parsedMetadata,
                topLevelToolCalls: parsedTopLevelToolCalls,
                createdAt: new Date(message.created_at),
                originalMessage: message
            };
        });

        const toolResultsMap = new Map<string, { content: string; error?: string }>();

        parsedMessages.forEach(msg => {
            if (msg.role === 'tool' && msg.toolCallId) {
                const isError = msg.metadata?.isError || false;
                toolResultsMap.set(msg.toolCallId, {
                    content: msg.content,
                    error: isError ? msg.content : undefined
                });
            }
        });

        const normalized = parsedMessages.map(msg => {
            let toolCalls = undefined;

            const sourceToolCalls = (msg as any).topLevelToolCalls || msg.metadata?.toolCalls;

            if (msg.role === 'assistant' && sourceToolCalls && Array.isArray(sourceToolCalls)) {
                debugLog('[Store] 处理工具调用:', { messageId: msg.id, sourceToolCalls });
                toolCalls = sourceToolCalls.map((toolCall: any) => {
                    if (toolCall.function) {
                        let parsedArguments = {};
                        try {
                            parsedArguments = typeof toolCall.function.arguments === 'string'
                                ? JSON.parse(toolCall.function.arguments)
                                : toolCall.function.arguments;
                        } catch (error) {
                            console.warn('Failed to parse tool call arguments:', error);
                            parsedArguments = {};
                        }

                        const toolResult = toolResultsMap.get(toolCall.id);

                        return {
                            id: toolCall.id || `tool_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                            messageId: msg.id,
                            name: toolCall.function.name,
                            arguments: parsedArguments,
                            result: toolResult?.content || undefined,
                            error: toolResult?.error || undefined,
                            createdAt: msg.createdAt
                        };
                    }
                    return null;
                }).filter(Boolean);
            }

            const contentSegments: any[] = [];
            const hasReasoning = typeof msg.reasoning === 'string' && msg.reasoning.trim().length > 0;
            if (msg.role === 'assistant' && hasReasoning) {
                contentSegments.push({ type: 'thinking', content: msg.reasoning });
            }
            if (typeof msg.content === 'string' && msg.content.trim().length > 0) {
                contentSegments.push({ type: 'text', content: msg.content });
            }
            if (toolCalls && Array.isArray(toolCalls) && toolCalls.length > 0) {
                toolCalls.forEach((tc: any) => {
                    contentSegments.push({ type: 'tool_call', toolCallId: tc.id });
                });
            }

            let normalizedAttachments: any[] | undefined = undefined;
            try {
                const rawAtts: any[] = (msg.metadata && (msg.metadata as any).attachments) || [];
                if (Array.isArray(rawAtts) && rawAtts.length > 0) {
                    normalizedAttachments = rawAtts.map((a: any, idx: number) => {
                        const mime = a.mimeType || a.mime || 'application/octet-stream';
                        const hasPreview = Boolean(a.preview || a.url);
                        const baseType = mime.startsWith('image/') ? 'image' : (mime.startsWith('audio/') ? 'audio' : 'file');
                        const type = hasPreview ? (a.type || baseType) : (baseType === 'image' ? 'file' : baseType);
                        return {
                            id: a.id || `${msg.id}_att_${idx}`,
                            messageId: msg.id,
                            type,
                            name: a.name || `attachment-${idx + 1}`,
                            url: a.preview || a.url || '',
                            size: a.size || 0,
                            mimeType: mime,
                            createdAt: msg.createdAt
                        };
                    });
                }
            } catch (_) {}

            return {
                id: msg.id,
                sessionId: msg.sessionId,
                role: msg.role,
                content: msg.content,
                rawContent: msg.summary,
                tokensUsed: undefined,
                status: 'completed' as const,
                createdAt: msg.createdAt,
                updatedAt: undefined,
                toolCallId: msg.toolCallId,
                metadata: {
                    ...msg.metadata,
                    ...(normalizedAttachments ? { attachments: normalizedAttachments } : {}),
                    toolCalls: toolCalls,
                    contentSegments: contentSegments.length > 0 ? contentSegments : msg.metadata?.contentSegments
                }
            };
        });

        const toHide = new Set<string>();
        for (let i = 0; i < normalized.length; i++) {
            const m = normalized[i];
            if (m?.metadata?.type === 'session_summary') {
                const summaryText = (typeof m.rawContent === 'string' && m.rawContent.length > 0)
                    ? m.rawContent
                    : (typeof (m.metadata as any)?.summary === 'string' && (m.metadata as any).summary.length > 0)
                        ? (m.metadata as any).summary
                        : (typeof m.content === 'string' ? m.content : '');

                let targetIdx = -1;
                for (let j = i + 1; j < normalized.length; j++) {
                    if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
                }
                if (targetIdx === -1) {
                    for (let j = i - 1; j >= 0; j--) {
                        if (normalized[j]?.role === 'assistant') { targetIdx = j; break; }
                    }
                }

                if (targetIdx !== -1) {
                    const target = normalized[targetIdx];
                    const header = '【上下文摘要】\n';
                    const segs = (target.metadata?.contentSegments || []).slice();
                    const lastIdx = segs.length - 1;
                    if (lastIdx >= 0 && segs[lastIdx].type === 'thinking' && String((segs[lastIdx] as any).content || '').startsWith(header)) {
                        (segs[lastIdx] as any).content = header + String(summaryText || '');
                    } else {
                        segs.push({ type: 'thinking', content: header + String(summaryText || '') });
                    }
                    target.metadata = target.metadata || {} as any;
                    (target.metadata as any).contentSegments = segs;
                    m.metadata = (m.metadata || {}) as any;
                    (m.metadata as any).hidden = true;
                    toHide.add(m.id);
                }
            }
        }
        return normalized;
    };

    const normalizeProject = (raw: any): Project => ({
        id: raw?.id,
        name: raw?.name ?? '',
        rootPath: raw?.root_path ?? raw?.rootPath ?? '',
        description: raw?.description ?? null,
        userId: raw?.user_id ?? raw?.userId ?? null,
        createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
        updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
    });

    return create<ChatState & ChatActions>()(
        immer(
            persist(
                    (set, get) => ({
                    // 初始状态
                    sessions: [],
                    currentSessionId: null,
                    currentSession: null,
                    projects: [],
                    currentProjectId: null,
                    currentProject: null,
                    activePanel: 'chat',
                    messages: [],
                    isLoading: false,
                    isStreaming: false,
                    streamingMessageId: null,
                    hasMoreMessages: true,
                    sessionChatState: {},
                    sidebarOpen: true,
                    theme: 'light',
                    chatConfig: {
                        model: 'gpt-4',
                        temperature: 0.7,
                        systemPrompt: '',
                        enableMcp: true,
                        reasoningEnabled: false,
                    },
                    mcpConfigs: [],
                    aiModelConfigs: [],
                    selectedModelId: null,
                    agents: [],
                    selectedAgentId: null,
                    systemContexts: [],
                    activeSystemContext: null,
                    applications: [],
                    selectedApplicationId: null,
                    error: null,

                    // 会话操作
                    loadSessions: async (options = {}) => {
                        try {
                            debugLog('🔍 loadSessions 被调用');
                            if (!options.silent) {
                                set((state) => {
                                    state.isLoading = true;
                                    state.error = null;
                                });
                                debugLog('🔍 loadSessions isLoading 设置为 true');
                            }

                            // 使用统一的参数获取逻辑
                            const { userId, projectId } = getSessionParams();
                            
                            debugLog('🔍 loadSessions 调用 client.getSessions', { userId, projectId, customUserId, customProjectId, options });
                            const sessions = await client.getSessions(userId, projectId, { limit: options.limit, offset: options.offset });
                            debugLog('🔍 loadSessions 返回结果:', sessions);

                            const existing = options.append ? (get().sessions || []) : [];
                            const merged = options.append ? [...existing, ...sessions] : sessions;
                            const deduped: Session[] = [];
                            const seen = new Set<string>();
                            for (const s of merged) {
                                if (s && !seen.has(s.id)) {
                                    seen.add(s.id);
                                    deduped.push(s);
                                }
                            }

                            set((state) => {
                                state.sessions = deduped;
                                if (!options.silent) {
                                    state.isLoading = false;
                                }
                                if (state.currentSessionId) {
                                    const matched = deduped.find(s => s.id === state.currentSessionId);
                                    if (matched) {
                                        state.currentSession = matched;
                                    }
                                }
                            });

                            // 会话持久化逻辑：自动选择上次使用的会话或最新的会话
                            const currentState = get();
                            if (deduped.length > 0 && !currentState.currentSessionId) {
                                // 尝试从 localStorage 获取上次使用的会话ID
                                const lastSessionId = localStorage.getItem(`lastSessionId_${userId}_${projectId}`);
                                let sessionToSelect = null;

                                if (lastSessionId) {
                                    // 检查上次使用的会话是否仍然存在
                                    sessionToSelect = deduped.find(s => s.id === lastSessionId);
                                }

                                // 如果上次的会话不存在，选择最新的会话（按创建时间排序）
                                if (!sessionToSelect) {
                                    sessionToSelect = [...deduped].sort((a, b) => 
                                        new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
                                    )[0];
                                }

                                if (sessionToSelect) {
                                    debugLog('🔍 自动选择会话:', sessionToSelect.id);
                                    // 异步选择会话，不阻塞 loadSessions 的完成
                                    setTimeout(() => {
                                        get().selectSession(sessionToSelect.id);
                                    }, 0);
                                }
                            }

                            debugLog('🔍 loadSessions 完成');
                            return sessions;
                        } catch (error) {
                            console.error('🔍 loadSessions 错误:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to load sessions';
                                if (!options.silent) {
                                    state.isLoading = false;
                                }
                            });
                            return [];
                        }
                    },

                    createSession: async (title = 'New Chat') => {
                        try {
                            // 使用统一的参数获取逻辑
                            const { userId, projectId } = getSessionParams();
                    
                            debugLog('🔍 createSession 使用参数:', { userId, projectId, title });
                            debugLog('🔍 createSession 自定义参数:', { customUserId, customProjectId });
                            debugLog('🔍 createSession 最终使用的参数:', { 
                                userId: userId, 
                                projectId: projectId,
                                isCustomUserId: !!customUserId,
                                isCustomProjectId: !!customProjectId
                            });
                            
                            // 直接调用API客户端创建会话
                            const sessionData = {
                                id: crypto.randomUUID(),
                                title,
                                user_id: userId,
                                project_id: projectId
                            };
                            
                            const session = await client.createSession(sessionData);
                            debugLog('✅ createSession API调用成功:', session);
                            
                            // 转换为前端格式
                            const formattedSession = {
                                id: session.id,
                                title: session.title,
                                createdAt: new Date(session.created_at),
                                updatedAt: new Date(session.updated_at),
                                messageCount: 0,
                                tokenUsage: 0,
                                pinned: false,
                                archived: false,
                                tags: null,
                                metadata: null
                            };

                            set((state) => {
                                state.sessions.unshift(formattedSession);
                                state.currentSessionId = formattedSession.id;
                                state.currentSession = formattedSession;
                                state.messages = [];
                                state.activePanel = 'chat';
                                state.error = null;
                            });

                            // 保存新创建的会话ID到 localStorage 以实现持久化
                            localStorage.setItem(`lastSessionId_${userId}_${projectId}`, formattedSession.id);
                            debugLog('🔍 保存新创建的会话ID到 localStorage:', formattedSession.id);

                            return formattedSession.id;
                        } catch (error) {
                            console.error('❌ createSession 失败:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to create session';
                            });
                            throw error;
                        }
                    },

                    selectSession: async (sessionId: string) => {
                        try {
                            set((state) => {
                                state.isLoading = true;
                                state.error = null;
                            });

                            const session = await fetchSession(sessionId);
                            const messages = await fetchSessionMessages(sessionId, { limit: 10, offset: 0 });
                            
                            set((state) => {
                            state.currentSessionId = sessionId;
                            (state as any).currentSession = session; // Type assertion to handle immer WritableDraft issue
                            state.messages = messages;
                            state.activePanel = 'chat';
                            state.isLoading = false;
                            state.hasMoreMessages = messages.length === 10;
                            const chatState = state.sessionChatState[sessionId];
                            state.isStreaming = chatState?.isStreaming ?? false;
                            state.streamingMessageId = chatState?.streamingMessageId ?? null;
                            if (chatState) {
                                state.isLoading = chatState.isLoading;
                            }
                            if (!session) {
                                state.error = 'Session not found';
                            }
                        });

                            // 保存当前会话ID到 localStorage 以实现持久化
                            if (session) {
                                const { userId, projectId } = getSessionParams();
                                localStorage.setItem(`lastSessionId_${userId}_${projectId}`, sessionId);
                                debugLog('🔍 保存会话ID到 localStorage:', sessionId);
                            }
                        } catch (error) {
                            console.error('Failed to select session:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to select session';
                                state.isLoading = false;
                            });
                        }
                    },

                    updateSession: async (sessionId: string, updates: Partial<Session>) => {
                        try {
                            console.warn('updateSession not implemented yet');
                            const updatedSession = null;
                            
                            set((state) => {
                                const index = state.sessions.findIndex(s => s.id === sessionId);
                                if (index !== -1 && updatedSession) {
                                    state.sessions[index] = updatedSession;
                                }
                                if (state.currentSessionId === sessionId) {
                                    state.currentSession = updatedSession;
                                }
                            });
                        } catch (error) {
                            console.error('Failed to update session:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to update session';
                            });
                        }
                    },

                    deleteSession: async (sessionId: string) => {
                        try {
                            await client.deleteSession(sessionId);
                            
                            set((state) => {
                                state.sessions = state.sessions.filter(s => s.id !== sessionId);
                                if (state.currentSessionId === sessionId) {
                                    state.currentSessionId = null;
                                    state.currentSession = null;
                                    state.messages = [];
                                }
                                if (state.activePanel === 'chat' && state.currentSessionId === null) {
                                    state.activePanel = state.currentProjectId ? 'project' : 'chat';
                                }
                            });
                        } catch (error) {
                            console.error('Failed to delete session:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to delete session';
                            });
                        }
                    },

                    // 项目操作
                    loadProjects: async () => {
                        try {
                            const uid = getUserIdParam();
                            const list = await client.listProjects(uid);
                            const formatted = Array.isArray(list) ? list.map(normalizeProject) : [];
                            set((state) => {
                                state.projects = formatted;
                                if (!state.currentProjectId) {
                                    const lastId = localStorage.getItem(`lastProjectId_${uid}`);
                                    if (lastId) {
                                        const matched = formatted.find(p => p.id === lastId);
                                        if (matched) {
                                            state.currentProjectId = matched.id;
                                            state.currentProject = matched;
                                        }
                                    }
                                } else {
                                    const matched = formatted.find(p => p.id === state.currentProjectId);
                                    if (matched) {
                                        state.currentProject = matched;
                                    }
                                }
                            });
                            return formatted;
                        } catch (error) {
                            console.error('Failed to load projects:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to load projects';
                            });
                            return [];
                        }
                    },

                    createProject: async (name: string, rootPath: string, description?: string) => {
                        const uid = getUserIdParam();
                        const payload = {
                            name,
                            root_path: rootPath,
                            description: description?.trim() || undefined,
                            user_id: uid,
                        };
                        const created = await client.createProject(payload);
                        const project = normalizeProject(created);
                        set((state) => {
                            state.projects.unshift(project);
                            state.currentProjectId = project.id;
                            state.currentProject = project;
                            state.activePanel = 'project';
                        });
                        localStorage.setItem(`lastProjectId_${uid}`, project.id);
                        return project;
                    },

                    updateProject: async (projectId: string, updates: Partial<Project>) => {
                        try {
                            const payload: { name?: string; root_path?: string; description?: string } = {};
                            if (updates.name !== undefined) payload.name = updates.name;
                            if (updates.rootPath !== undefined) payload.root_path = updates.rootPath;
                            if (updates.description !== undefined) payload.description = updates.description || undefined;
                            const updated = await client.updateProject(projectId, payload);
                            const project = normalizeProject(updated);
                            set((state) => {
                                const index = state.projects.findIndex(p => p.id === projectId);
                                if (index !== -1) {
                                    state.projects[index] = project;
                                }
                                if (state.currentProjectId === projectId) {
                                    state.currentProject = project;
                                }
                            });
                            return project;
                        } catch (error) {
                            console.error('Failed to update project:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to update project';
                            });
                            return null;
                        }
                    },

                    deleteProject: async (projectId: string) => {
                        try {
                            await client.deleteProject(projectId);
                            set((state) => {
                                state.projects = state.projects.filter(p => p.id !== projectId);
                                if (state.currentProjectId === projectId) {
                                    state.currentProjectId = null;
                                    state.currentProject = null;
                                    if (state.activePanel === 'project') {
                                        state.activePanel = 'chat';
                                    }
                                }
                            });
                        } catch (error) {
                            console.error('Failed to delete project:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to delete project';
                            });
                        }
                    },

                    selectProject: async (projectId: string) => {
                        try {
                            let project = get().projects.find(p => p.id === projectId) || null;
                            if (!project) {
                                const fetched = await client.getProject(projectId);
                                project = normalizeProject(fetched);
                            }
                            const uid = getUserIdParam();
                            set((state) => {
                                state.currentProjectId = projectId;
                                state.currentProject = project;
                                state.activePanel = 'project';
                            });
                            localStorage.setItem(`lastProjectId_${uid}`, projectId);
                        } catch (error) {
                            console.error('Failed to select project:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to select project';
                            });
                        }
                    },

                    setActivePanel: (panel: 'chat' | 'project') => {
                        set((state) => {
                            state.activePanel = panel;
                        });
                    },

                    // 消息操作
                    loadMessages: async (sessionId: string) => {
                        try {
                            set((state) => {
                                state.isLoading = true;
                                state.error = null;
                            });

                            const messages = await fetchSessionMessages(sessionId, { limit: 10, offset: 0 });
                            
                            set((state) => {
                                state.messages = messages;
                                state.isLoading = false;
                                state.hasMoreMessages = messages.length === 10;
                            });
                        } catch (error) {
                            console.error('Failed to load messages:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to load messages';
                                state.isLoading = false;
                            });
                        }
                    },

                    // 加载更多历史消息（向上分页）
                    loadMoreMessages: async (sessionId: string) => {
                        try {
                            const current = get();
                            const offset = current.messages.length;
                            const page = await fetchSessionMessages(sessionId, { limit: 10, offset });
                            set((state) => {
                                const existingIds = new Set(state.messages.map(m => m.id));
                                const older = page.filter(m => !existingIds.has(m.id));
                                state.messages = [...older, ...state.messages];
                                state.hasMoreMessages = page.length === 10;
                            });
                        } catch (error) {
                            console.error('Failed to load more messages:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to load more messages';
                            });
                        }
                    },

                    sendMessage: createSendMessageHandler({ set, get, client, getUserIdParam }),

                    updateMessage: async (messageId: string, updates: Partial<Message>) => {
                        try {
                            console.warn('updateMessage not implemented yet');
                            const updatedMessage = null;
                            
                            set((state) => {
                                const index = state.messages.findIndex(m => m.id === messageId);
                                if (index !== -1 && updatedMessage) {
                                    state.messages[index] = updatedMessage;
                                }
                            });
                        } catch (error) {
                            console.error('Failed to update message:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to update message';
                            });
                        }
                    },

                    deleteMessage: async (messageId: string) => {
                        try {
                            console.warn('deleteMessage not implemented yet');
                            
                            set((state) => {
                                state.messages = state.messages.filter(m => m.id !== messageId);
                            });
                        } catch (error) {
                            console.error('Failed to delete message:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to delete message';
                            });
                        }
                    },

                    // 流式消息处理
                    startStreaming: (messageId: string) => {
                        set((state) => {
                            const sessionId = state.currentSessionId;
                            if (sessionId) {
                                const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
                                state.sessionChatState[sessionId] = { ...prev, isStreaming: true, streamingMessageId: messageId };
                            }
                            state.isStreaming = true;
                            state.streamingMessageId = messageId;
                        });
                    },

                    updateStreamingMessage: (content: string) => {
                        set((state) => {
                            if (state.streamingMessageId) {
                                const messageIndex = state.messages.findIndex(m => m.id === state.streamingMessageId);
                                if (messageIndex !== -1) {
                                    state.messages[messageIndex].content = content;
                                }
                            }
                        });
                    },

                    stopStreaming: () => {
                        set((state) => {
                            const sessionId = state.currentSessionId;
                            if (sessionId) {
                                const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
                                state.sessionChatState[sessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
                            }
                            state.isStreaming = false;
                            state.streamingMessageId = null;
                        });
                    },

                    abortCurrentConversation: async () => {
                        const { currentSessionId } = get();
                        
                        if (currentSessionId) {
                            try {
                                const { selectedModelId, selectedAgentId, aiModelConfigs, agents } = get();
                                let activeModel: any = null;
                                if (selectedAgentId) {
                                    const agent = agents.find((a: any) => a.id === selectedAgentId);
                                    if (agent) {
                                        activeModel = aiModelConfigs.find((m: any) => m.id === agent.ai_model_config_id);
                                    }
                                } else if (selectedModelId) {
                                    activeModel = aiModelConfigs.find((m: any) => m.id === selectedModelId);
                                }
                                const useResponses = activeModel?.supports_responses === true;
                                // 调用后端停止聊天API
                                await client.stopChat(currentSessionId, { useResponses });
                                debugLog('✅ 成功停止当前对话');
                            } catch (error) {
                                console.error('❌ 停止对话失败:', error);
                            }
                        }

                        set((state) => {
                            const sessionId = state.currentSessionId;
                            const streamingId = state.streamingMessageId;
                            if (sessionId) {
                                const prev = state.sessionChatState[sessionId] || { isLoading: false, isStreaming: false, streamingMessageId: null };
                                state.sessionChatState[sessionId] = { ...prev, isLoading: false, isStreaming: false, streamingMessageId: null };
                            }
                            state.isStreaming = false;
                            state.streamingMessageId = null;
                            state.isLoading = false;
                            if (streamingId) {
                                const messageIndex = state.messages.findIndex((m: any) => m.id === streamingId);
                                if (messageIndex !== -1) {
                                    const message = state.messages[messageIndex];
                                    if (message.metadata && message.metadata.toolCalls) {
                                        message.metadata.toolCalls.forEach((tc: any) => {
                                            if (!tc.error) {
                                                const hasResult = tc.result !== undefined && tc.result !== null && String(tc.result).trim() !== '';
                                                if (!hasResult) {
                                                    tc.result = tc.result || '';
                                                }
                                                tc.error = '已取消';
                                            }
                                        });
                                        (message as any).updatedAt = new Date();
                                    }
                                }
                            }
                        });
                    },

                    // UI操作
                    toggleSidebar: () => {
                        set((state) => {
                            state.sidebarOpen = !state.sidebarOpen;
                        });
                    },

                    setTheme: (theme: Theme) => {
                        set((state) => {
                            state.theme = theme;
                        });
                    },

                    // 配置操作（拆分到独立模块）
                    ...createChatConfigActions({ set, get }),

                    // MCP 管理（拆分到独立模块）
                    ...createMcpActions({ set, get, client, getUserIdParam }),

                    // 应用管理（拆分到独立模块）
                    ...createApplicationActions({ set, get, client, getUserIdParam }),

                    // AI模型管理（拆分到独立模块）
                    ...createAiModelActions({ set, get, client, getUserIdParam }),

                    loadAgents: async () => {
                        try {
                            const agents = await client.getAgents(getUserIdParam());
                            debugLog('🔍 [后端返回] loadAgents 返回的数据:', agents);
                            debugLog('🔍 [后端返回] 第一个智能体的 app_ids:', agents?.[0]?.app_ids);
                            set((state) => {
                                state.agents = (agents || []) as any[];
                            });
                        } catch (error) {
                            console.error('Failed to load agents:', error);
                            set((state) => {
                                state.agents = [];
                                state.error = error instanceof Error ? error.message : 'Failed to load agents';
                            });
                        }
                    },

                    setSelectedAgent: (agentId: string | null) => {
                        set((state) => {
                            state.selectedAgentId = agentId;
                            // 选择智能体时清空已选模型
                            if (agentId) {
                                state.selectedModelId = null;
                            }
                        });
                    },

                    loadSystemContexts: async () => {
                        try {
                            const contexts = await client.getSystemContexts(getUserIdParam());
                            const activeContextResponse = await client.getActiveSystemContext(getUserIdParam());
                            set((state) => {
                                // 先将所有上下文的isActive设为false
                                const updatedContexts = (contexts || []).map((ctx: any) => ({
                                    ...ctx,
                                    isActive: false,
                                }));

                                // 处理激活的上下文
                                if (activeContextResponse && activeContextResponse.context) {
                                    const activeContext = activeContextResponse.context;
                                    // 找到对应的上下文并设置为激活状态
                                    const activeIndex = updatedContexts.findIndex(ctx => ctx.id === activeContext.id);
                                    if (activeIndex !== -1) {
                                        updatedContexts[activeIndex].isActive = true;
                                        state.activeSystemContext = { ...updatedContexts[activeIndex] };
                                    } else {
                                        state.activeSystemContext = null;
                                    }
                                } else {
                                    state.activeSystemContext = null;
                                }

                                state.systemContexts = updatedContexts;
                            });
                        } catch (error) {
                            console.error('Failed to load system contexts:', error);
                            set((state) => {
                                state.systemContexts = [];
                                state.activeSystemContext = null;
                            });
                        }
                    },

                    createSystemContext: async (name: string, content: string, appIds?: string[]) => {
                        try {
                            const context = await client.createSystemContext({
                                name,
                                content,
                                user_id: getUserIdParam(),
                                app_ids: Array.isArray(appIds) ? appIds : undefined,
                            });
                            set((state) => {
                                state.systemContexts.push(context);
                            });
                            return context;
                        } catch (error) {
                            console.error('Failed to create system context:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to create system context';
                            });
                            return null;
                        }
                    },

                    updateSystemContext: async (id: string, name: string, content: string, appIds?: string[]) => {
                        try {
                            const updatedContext = await client.updateSystemContext(id, { name, content, app_ids: Array.isArray(appIds) ? appIds : undefined });
                            set((state) => {
                                const index = state.systemContexts.findIndex(c => c.id === id);
                                if (index !== -1) {
                                    state.systemContexts[index] = updatedContext;
                                }
                            });
                            return updatedContext;
                        } catch (error) {
                            console.error('Failed to update system context:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to update system context';
                            });
                            return null;
                        }
                    },

                    deleteSystemContext: async (id: string) => {
                        try {
                            await client.deleteSystemContext(id);
                            set((state) => {
                                state.systemContexts = state.systemContexts.filter(c => c.id !== id);
                                if (state.activeSystemContext?.id === id) {
                                    state.activeSystemContext = null;
                                }
                            });
                        } catch (error) {
                            console.error('Failed to delete system context:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to delete system context';
                            });
                        }
                    },

                    activateSystemContext: async (id: string) => {
                        try {
                            await client.activateSystemContext(id, getUserIdParam());
                            set((state) => {
                                const context = state.systemContexts.find(c => c.id === id);
                                if (context) {
                                    // 更新所有上下文的激活状态
                                    state.systemContexts.forEach(ctx => {
                                        ctx.isActive = ctx.id === id;
                                    });
                                    state.activeSystemContext = { ...context, isActive: true };
                                }
                            });
                        } catch (error) {
                            console.error('Failed to activate system context:', error);
                            set((state) => {
                                state.error = error instanceof Error ? error.message : 'Failed to activate system context';
                            });
                        }
                    },

                    // 错误处理
                    setError: (error: string | null) => {
                        set((state) => {
                            state.error = error;
                        });
                    },

                    clearError: () => {
                        set((state) => {
                            state.error = null;
                        });
                    },
                }),
                {
                    name: 'chat-store-with-backend',
                    partialize: (state) => ({
                        theme: state.theme,
                        sidebarOpen: state.sidebarOpen,
                        chatConfig: state.chatConfig,
                        selectedModelId: state.selectedModelId,
                    }),
                }
            )
    ));
}

// 导出 ChatStore 类型别名，供外部命名使用
export type ChatStore = ReturnType<typeof createChatStoreWithBackend>;
