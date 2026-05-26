import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Message } from '../../../types';
import type ApiClient from '../../api/client';
import type { ChatStoreDraft, ChatStoreShape } from '../types';
import { createStreamingActions } from './streaming';

const buildStreamingAssistantDraft = (
  overrides: Partial<Message> = {},
): Message => ({
  id: 'assistant_temp_1',
  sessionId: 'session_1',
  role: 'assistant',
  content: 'partial answer',
  status: 'streaming',
  createdAt: new Date('2026-05-25T10:00:01.000Z'),
  metadata: {
    conversation_turn_id: 'turn_1',
    historyFinalForUserMessageId: 'temp_user_1',
    historyDraftUserMessage: {
      id: 'temp_user_1',
      content: 'hello',
      createdAt: '2026-05-25T10:00:00.000Z',
    },
    contentSegments: [{ type: 'text', content: 'partial answer' }],
    toolCalls: [],
  },
  ...overrides,
});

const createStreamingState = (): ChatStoreShape => ({
  currentSessionId: 'session_1',
  currentSession: null,
  sessions: [],
  contacts: [],
  projects: [],
  currentProjectId: null,
  currentProject: null,
  activePanel: 'chat',
  terminals: [],
  currentTerminalId: null,
  currentTerminal: null,
  remoteConnections: [],
  currentRemoteConnectionId: null,
  currentRemoteConnection: null,
  messages: [
    {
      id: 'temp_user_1',
      sessionId: 'session_1',
      role: 'user',
      content: 'hello',
      status: 'completed',
      createdAt: new Date('2026-05-25T10:00:00.000Z'),
      metadata: {
        conversation_turn_id: 'turn_1',
      },
    },
    buildStreamingAssistantDraft(),
  ],
  isLoading: true,
  isStreaming: true,
  streamingMessageId: 'assistant_temp_1',
  hasMoreMessages: true,
  sessionChatState: {
    session_1: {
      isLoading: true,
      isStreaming: true,
      isStopping: false,
      streamingPhase: 'thinking',
      streamingMessageId: 'assistant_temp_1',
      activeTurnId: 'turn_1',
      streamingPreviewText: 'partial answer',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    },
  },
  sessionMessagePaginationState: {},
  sessionMessagesCache: {},
  sessionMessagesCacheOrder: [],
  sessionRuntimeGuidanceState: {},
  sessionStreamingMessageDrafts: {
    session_1: buildStreamingAssistantDraft(),
  },
  sessionTurnProcessCache: {},
  turnProcessViewer: {
    open: false,
    sessionId: null,
    userMessageId: null,
    turnId: null,
  },
  taskReviewPanel: null,
  taskReviewPanelsBySession: {},
  uiPromptPanel: null,
  uiPromptPanelsBySession: {},
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
  aiModelConfigs: [
    {
      id: 'model_1',
      name: 'model_1',
      provider: 'openai',
      base_url: 'https://api.openai.com/v1',
      api_key: 'test-key',
      model_name: 'gpt-4.1',
      supports_responses: true,
      supports_images: false,
      supports_reasoning: true,
      enabled: true,
      createdAt: new Date('2026-05-25T10:00:00.000Z'),
      updatedAt: new Date('2026-05-25T10:00:00.000Z'),
    },
  ],
  selectedModelId: 'model_1',
  agents: [],
  selectedAgentId: null,
  sessionAiSelectionBySession: {},
  systemContexts: [],
  activeSystemContext: null,
  applications: [],
  selectedApplicationId: null,
  error: null,
  loadContacts: vi.fn(),
  createContact: vi.fn(),
  deleteContact: vi.fn(),
  getContactByAgentId: vi.fn(),
  markContactsStale: vi.fn(),
  removeContactLocally: vi.fn(),
  applyRealtimeContactSnapshot: vi.fn(),
  refreshContactById: vi.fn(),
  loadSessions: vi.fn(),
  createSession: vi.fn(),
  selectSession: vi.fn(),
  updateSession: vi.fn(),
  deleteSession: vi.fn(),
  markSessionsStale: vi.fn(),
  removeSessionLocally: vi.fn(),
  applyRealtimeSessionSnapshot: vi.fn(),
  refreshSessionById: vi.fn(),
  loadProjects: vi.fn(),
  createProject: vi.fn(),
  updateProject: vi.fn(),
  deleteProject: vi.fn(),
  selectProject: vi.fn(),
  markProjectsStale: vi.fn(),
  removeProjectLocally: vi.fn(),
  applyRealtimeProjectSnapshot: vi.fn(),
  refreshProjectById: vi.fn(),
  setActivePanel: vi.fn(),
  loadTerminals: vi.fn(),
  createTerminal: vi.fn(),
  deleteTerminal: vi.fn(),
  selectTerminal: vi.fn(),
  markTerminalsStale: vi.fn(),
  removeTerminalLocally: vi.fn(),
  applyRealtimeTerminalSnapshot: vi.fn(),
  refreshTerminalById: vi.fn(),
  loadRemoteConnections: vi.fn(),
  createRemoteConnection: vi.fn(),
  updateRemoteConnection: vi.fn(),
  deleteRemoteConnection: vi.fn(),
  selectRemoteConnection: vi.fn(),
  openRemoteSftp: vi.fn(),
  markRemoteConnectionsStale: vi.fn(),
  removeRemoteConnectionLocally: vi.fn(),
  applyRealtimeRemoteConnectionSnapshot: vi.fn(),
  refreshRemoteConnectionById: vi.fn(),
  loadMessages: vi.fn(),
  syncSessionMessagesInBackground: vi.fn(async (sessionId: string) => {
    if (sessionId !== 'session_1') {
      return;
    }
    const finalAssistant = buildStreamingAssistantDraft({
      id: 'assistant_final_1',
      content: 'final answer',
      status: 'completed',
      metadata: {
        conversation_turn_id: 'turn_1',
        historyFinalForTurnId: 'turn_1',
        historyFinalForUserMessageId: 'temp_user_1',
        contentSegments: [{ type: 'text', content: 'final answer' }],
        toolCalls: [],
      },
    });
    state.messages = [
      state.messages[0] as Message,
      finalAssistant,
    ];
    state.sessionStreamingMessageDrafts.session_1 = null;
    state.sessionChatState.session_1 = {
      ...state.sessionChatState.session_1,
      isLoading: false,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId: null,
      streamingPreviewText: '',
      streamingTransport: null,
    };
    state.isLoading = false;
    state.isStreaming = false;
    state.streamingMessageId = null;
  }),
  loadMoreMessages: vi.fn(),
  openTurnProcessViewer: vi.fn(),
  closeTurnProcessViewer: vi.fn(),
  sendMessage: vi.fn(),
  submitRuntimeGuidance: vi.fn(),
  updateMessage: vi.fn(),
  deleteMessage: vi.fn(),
  startStreaming: vi.fn(),
  updateStreamingMessage: vi.fn(),
  stopStreaming: vi.fn(),
  abortCurrentConversation: vi.fn(),
  setTaskReviewPanel: vi.fn(),
  upsertTaskReviewPanel: vi.fn(),
  removeTaskReviewPanel: vi.fn(),
  setUiPromptPanel: vi.fn(),
  upsertUiPromptPanel: vi.fn(),
  removeUiPromptPanel: vi.fn(),
  toggleSidebar: vi.fn(),
  setTheme: vi.fn(),
  updateChatConfig: vi.fn(),
  loadMcpConfigs: vi.fn(),
  updateMcpConfig: vi.fn(),
  deleteMcpConfig: vi.fn(),
  loadAiModelConfigs: vi.fn(),
  updateAiModelConfig: vi.fn(),
  deleteAiModelConfig: vi.fn(),
  setSelectedModel: vi.fn(),
  loadAgents: vi.fn(),
  createAgent: vi.fn(),
  updateAgent: vi.fn(),
  deleteAgent: vi.fn(),
  aiCreateAgent: vi.fn(),
  setSelectedAgent: vi.fn(),
  loadSystemContexts: vi.fn(),
  createSystemContext: vi.fn(),
  updateSystemContext: vi.fn(),
  deleteSystemContext: vi.fn(),
  activateSystemContext: vi.fn(),
  generateSystemContextDraft: vi.fn(),
  optimizeSystemContextDraft: vi.fn(),
  evaluateSystemContextDraft: vi.fn(),
  loadApplications: vi.fn(),
  createApplication: vi.fn(),
  updateApplication: vi.fn(),
  deleteApplication: vi.fn(),
  setSelectedApplication: vi.fn(),
  setSystemContextAppAssociation: vi.fn(),
  setAgentAppAssociation: vi.fn(),
  setError: vi.fn(),
  clearError: vi.fn(),
});

let state: ChatStoreShape;

describe('createStreamingActions abort recovery', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    state = createStreamingState();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('recovers a stuck stopping session when no realtime terminal event arrives', async () => {
    const client = {
      stopChat: vi.fn().mockResolvedValue({ success: true }),
      getConversationTurnRuntimeContextByTurn: vi.fn().mockResolvedValue({
        conversation_id: 'session_1',
        turn_id: 'turn_1',
        status: 'cancelled',
        snapshot_source: 'runtime',
        snapshot: null,
      }),
      getConversationLatestTurnRuntimeContext: vi.fn().mockResolvedValue({
        conversation_id: 'session_1',
        turn_id: 'turn_1',
        status: 'cancelled',
        snapshot_source: 'runtime',
        snapshot: null,
      }),
      getConversationTurnMessagesByTurn: vi.fn().mockResolvedValue([]),
      getConversationTurnMessages: vi.fn().mockResolvedValue([]),
    } as unknown as ApiClient;
    const set = vi.fn((updater: (draft: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;
    const actions = createStreamingActions({ set, get, client });

    await actions.abortCurrentConversation();

    expect(state.sessionChatState.session_1.isStopping).toBe(true);
    await vi.advanceTimersByTimeAsync(4000);
    await vi.runAllTimersAsync();

    expect(client.stopChat).toHaveBeenCalledWith('session_1');
    expect(client.getConversationTurnRuntimeContextByTurn).toHaveBeenCalledWith('session_1', 'turn_1');
    expect(state.sessionChatState.session_1.isStopping).toBe(false);
    expect(state.sessionChatState.session_1.isStreaming).toBe(false);
    expect(state.sessionStreamingMessageDrafts.session_1).toBeNull();
    expect(state.streamingMessageId).toBeNull();
  });

  it('does not recover stale stop timers after a new turn replaces the active stream', async () => {
    const client = {
      stopChat: vi.fn().mockResolvedValue({ success: true }),
      getConversationTurnRuntimeContextByTurn: vi.fn(),
      getConversationLatestTurnRuntimeContext: vi.fn(),
      getConversationTurnMessagesByTurn: vi.fn(),
      getConversationTurnMessages: vi.fn(),
    } as unknown as ApiClient;
    const set = vi.fn((updater: (draft: ChatStoreDraft) => void) => {
      updater(state as unknown as ChatStoreDraft);
    });
    const get = () => state;
    const actions = createStreamingActions({ set, get, client });

    await actions.abortCurrentConversation();

    state.sessionChatState.session_1 = {
      ...state.sessionChatState.session_1,
      isLoading: true,
      isStreaming: true,
      isStopping: false,
      streamingPhase: 'thinking',
      streamingMessageId: 'assistant_temp_2',
      activeTurnId: 'turn_2',
      streamingPreviewText: 'new answer',
      streamingTransport: 'realtime',
    };
    state.sessionStreamingMessageDrafts.session_1 = buildStreamingAssistantDraft({
      id: 'assistant_temp_2',
      content: 'new answer',
      metadata: {
        conversation_turn_id: 'turn_2',
        historyFinalForUserMessageId: 'temp_user_2',
        historyDraftUserMessage: {
          id: 'temp_user_2',
          content: 'second question',
          createdAt: '2026-05-25T10:01:00.000Z',
        },
        contentSegments: [{ type: 'text', content: 'new answer' }],
        toolCalls: [],
      },
    });
    state.streamingMessageId = 'assistant_temp_2';

    await vi.advanceTimersByTimeAsync(4000);
    await vi.runAllTimersAsync();

    expect(client.getConversationTurnRuntimeContextByTurn).not.toHaveBeenCalled();
    expect(state.sessionChatState.session_1.activeTurnId).toBe('turn_2');
    expect(state.sessionChatState.session_1.isStreaming).toBe(true);
    expect(state.sessionChatState.session_1.isStopping).toBe(false);
    expect(state.sessionStreamingMessageDrafts.session_1?.id).toBe('assistant_temp_2');
  });
});
