import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { prepareAttachmentPayloads } from './chatAttachments';
import type {
  ChatAttachment,
  ChatAgentOption,
  ChatContact,
  ChatMessage,
  ChatModelOption,
  ChatRuntimeSettings,
  DemoProject,
  DemoTask,
  DemoTaskGraph,
} from './types';

const DEFAULT_API_BASE_URL = 'http://127.0.0.1:3997/api';
const API_BASE_URL = String(import.meta.env.VITE_CHATOS_API_BASE_URL || DEFAULT_API_BASE_URL).replace(/\/$/, '');
const AUTH_STORAGE_KEY = 'chatos-3d-auth';

type BridgeStatus = 'demo' | 'connecting' | 'live' | 'error';
type WebSocketStatus = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';

interface ChatOSUser {
  id: string;
  username: string;
}

interface StoredAuth {
  accessToken: string;
  user: ChatOSUser;
}

interface RawProject {
  id: string;
  name: string;
  root_path?: string;
  rootPath?: string;
  display_root_path?: string | null;
  displayRootPath?: string | null;
  git_url?: string | null;
  gitUrl?: string | null;
  source_type?: string | null;
  sourceType?: string | null;
  import_status?: string | null;
  importStatus?: string | null;
  description?: string | null;
  latest_session_id?: string | null;
  latestSessionId?: string | null;
  updated_at?: string;
  updatedAt?: string;
  created_at?: string;
  createdAt?: string;
}

interface RawSession {
  id: string;
  title: string;
  project_id?: string | null;
  projectId?: string | null;
  selected_model_id?: string | null;
  updated_at?: string;
  updatedAt?: string;
  created_at?: string;
  createdAt?: string;
  archived?: boolean;
  message_count?: number;
  messageCount?: number;
  metadata?: Record<string, unknown> | string | null;
}

interface RawContact {
  id: string;
  agent_id?: string;
  agentId?: string;
  agent_name_snapshot?: string | null;
  agentNameSnapshot?: string | null;
  status?: string | null;
  updated_at?: string;
  updatedAt?: string;
  created_at?: string;
  createdAt?: string;
}

interface RawAgent {
  id: string;
  name: string;
  description?: string | null;
  enabled?: boolean;
}

interface RawProjectContact {
  contact_id?: string;
  contactId?: string;
  agent_id?: string;
  agentId?: string;
  agent_name_snapshot?: string | null;
  agentNameSnapshot?: string | null;
  latest_session_id?: string | null;
  latestSessionId?: string | null;
  last_message_at?: string | null;
  lastMessageAt?: string | null;
  updated_at?: string | null;
  updatedAt?: string | null;
}

interface RawMessage {
  id: string;
  role: string;
  content?: string;
  rawContent?: string;
  sequence_no?: number;
  created_at?: string;
  createdAt?: string | Date;
  status?: string;
  metadata?: Record<string, unknown> | null;
}

interface RawTask {
  id: string;
  title?: string;
  details?: string | null;
  status?: 'todo' | 'doing' | 'blocked' | 'done' | null;
  priority?: 'high' | 'medium' | 'low' | null;
  blocker_reason?: string | null;
  outcome_summary?: string | null;
  conversation_turn_id?: string | null;
  source_user_message_id?: string | null;
  completed_at?: string | null;
  updated_at?: string;
  created_at?: string;
}

interface RawTaskRunnerTask {
  id: string;
  title?: string | null;
  description?: string | null;
  objective?: string | null;
  status?: string | null;
  priority?: number | null;
  creator_username?: string | null;
  creator_display_name?: string | null;
  result_summary?: string | null;
  prerequisite_task_ids?: string[];
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
  updated_at?: string | null;
  created_at?: string | null;
}

interface RawTaskRunnerGraph {
  root_task_ids?: string[];
  nodes?: Array<{
    task?: RawTaskRunnerTask | null;
    depth?: number;
    is_root?: boolean;
    is_current_message?: boolean;
  }>;
  edges?: Array<{
    id?: string;
    source?: string;
    target?: string;
  }>;
  source_session_id?: string | null;
  source_turn_id?: string | null;
  source_user_message_id?: string | null;
}

interface RawModelConfig {
  id: string;
  name?: string;
  model?: string;
  model_name?: string;
  thinking_level?: string;
  temperature?: number | null;
  enabled?: boolean;
  supports_images?: boolean;
  supports_reasoning?: boolean;
}

interface RawRuntimeSettings {
  selected_model_id?: string | null;
  selected_model_name?: string | null;
  selected_thinking_level?: string | null;
  reasoning_enabled?: boolean;
  plan_mode_enabled?: boolean;
}

interface RawProjectPlan {
  requirements?: Array<{ title?: string; status?: string }>;
  work_items?: Array<{ title?: string; status?: string }>;
  workItems?: Array<{ title?: string; status?: string }>;
  work_item_counts?: { total?: number; done?: number; blocked?: number };
  workItemCounts?: { total?: number; done?: number; blocked?: number };
}

interface RealtimeEnvelope {
  type?: string;
  event?: string;
  conversation_id?: string | null;
  project_id?: string | null;
  payload?: {
    kind?: string;
    conversation_id?: string;
    stream_type?: string;
    action?: string;
    task_id?: string | null;
    task?: RawTask | null;
    raw?: {
      type?: string;
      content?: unknown;
      message?: string;
      result?: Record<string, unknown> | null;
    };
  };
}

interface WorkspaceSnapshot {
  projects: RawProject[];
  sessions: RawSession[];
  taskSessions: RawSession[];
  modelConfigs: RawModelConfig[];
  contacts: RawContact[];
  agents: RawAgent[];
}

const ACCENTS = ['#79543d', '#506d63', '#7d6448', '#695655', '#4f6478', '#76634f'];

const EMPTY_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: [],
  nodes: [],
  edges: [],
  sourceSessionId: null,
  sourceTurnId: null,
  sourceUserMessageId: null,
};

const readStoredAuth = (): StoredAuth | null => {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(AUTH_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<StoredAuth>;
    const token = String(parsed.accessToken || '').trim();
    const id = String(parsed.user?.id || '').trim();
    if (!token || !id) return null;
    return {
      accessToken: token,
      user: {
        id,
        username: String(parsed.user?.username || id),
      },
    };
  } catch {
    return null;
  }
};

const persistAuth = (auth: StoredAuth | null) => {
  if (typeof window === 'undefined') return;
  if (!auth) {
    window.localStorage.removeItem(AUTH_STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(auth));
};

const apiRequest = async <T,>(
  path: string,
  token?: string | null,
  options: RequestInit = {},
): Promise<T> => {
  const headers = new Headers(options.headers || {});
  if (!headers.has('Content-Type') && !(options.body instanceof FormData)) {
    headers.set('Content-Type', 'application/json');
  }
  if (token) headers.set('Authorization', `Bearer ${token}`);
  const response = await fetch(`${API_BASE_URL}${path}`, { ...options, headers });
  const text = await response.text();
  const payload = text ? JSON.parse(text) as unknown : {};
  if (!response.ok) {
    const message = payload && typeof payload === 'object'
      ? String((payload as Record<string, unknown>).error || (payload as Record<string, unknown>).message || `HTTP ${response.status}`)
      : `HTTP ${response.status}`;
    throw new Error(message);
  }
  return payload as T;
};

const value = (first?: string | null, second?: string | null): string => String(first || second || '').trim();

const formatRelativeTime = (raw?: string): string => {
  if (!raw) return '时间未知';
  const timestamp = new Date(raw).getTime();
  if (!Number.isFinite(timestamp)) return raw;
  const deltaMinutes = Math.max(0, Math.round((Date.now() - timestamp) / 60000));
  if (deltaMinutes < 1) return '刚刚';
  if (deltaMinutes < 60) return `${deltaMinutes} 分钟前`;
  const hours = Math.round(deltaMinutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  return `${Math.round(hours / 24)} 天前`;
};

const formatDateTime = (raw?: string): string | null => {
  if (!raw) return null;
  const date = new Date(raw);
  if (!Number.isFinite(date.getTime())) return raw;
  return new Intl.DateTimeFormat('zh-CN', {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  }).format(date);
};

const basename = (path: string): string => path.replace(/[\\/]+$/, '').split(/[\\/]/).filter(Boolean).pop() || path;

const projectMetadataFiles = (project: RawProject): string[] => {
  const root = value(project.display_root_path, project.displayRootPath) || value(project.root_path, project.rootPath);
  const source = value(project.source_type, project.sourceType);
  const gitUrl = value(project.git_url, project.gitUrl);
  return [root ? basename(root) : '', source ? `来源：${source}` : '', gitUrl ? 'Git 仓库' : '']
    .filter(Boolean)
    .slice(0, 6);
};

const mapProject = (project: RawProject, index: number, plan?: RawProjectPlan | null): DemoProject => {
  const workItems = plan?.work_items || plan?.workItems || [];
  const counts = plan?.work_item_counts || plan?.workItemCounts;
  const total = Number(counts?.total ?? workItems.length ?? 0);
  const done = Number(counts?.done ?? workItems.filter((item) => item.status === 'done').length ?? 0);
  const blocked = Number(counts?.blocked ?? workItems.filter((item) => item.status === 'blocked').length ?? 0);
  const runningCount = workItems.filter((item) => item.status === 'in_progress').length;
  const running = workItems.some((item) => item.status === 'in_progress');
  const progress = total > 0 ? Math.round((done / total) * 100) : 0;
  const importedStatus = value(project.import_status, project.importStatus).toLowerCase();
  const status: DemoProject['status'] = running || importedStatus.includes('running') || importedStatus.includes('import')
    ? 'running'
    : total > 0 && done >= total
      ? 'idle'
      : 'planning';
  const root = value(project.display_root_path, project.displayRootPath) || value(project.root_path, project.rootPath);
  const gitUrl = value(project.git_url, project.gitUrl);
  const rawSourceType = value(project.source_type, project.sourceType);
  const sourceType = rawSourceType || (root.startsWith('harness://') ? 'cloud' : gitUrl ? 'git' : root ? 'local' : 'unknown');
  const importStatus = value(project.import_status, project.importStatus);
  const requirementFiles = (plan?.requirements || []).map((item) => String(item.title || '').trim()).filter(Boolean);
  const workItemFiles = workItems.map((item) => String(item.title || '').trim()).filter(Boolean);
  const files = (requirementFiles.length > 0 ? requirementFiles : workItemFiles.length > 0 ? workItemFiles : projectMetadataFiles(project)).slice(0, 6);
  const planItems = [
    ...(plan?.requirements || []).map((item) => ({
      title: String(item.title || '').trim(),
      status: item.status || null,
      kind: 'requirement' as const,
    })),
    ...workItems.map((item) => ({
      title: String(item.title || '').trim(),
      status: item.status || null,
      kind: 'work-item' as const,
    })),
  ]
    .filter((item) => item.title)
    .filter((item, itemIndex, items) => items.findIndex((candidate) => candidate.title === item.title) === itemIndex)
    .slice(0, 6);
  const createdAt = value(project.created_at, project.createdAt);
  const updatedAt = value(project.updated_at, project.updatedAt) || createdAt;
  return {
    id: project.id,
    name: project.name,
    subtitle: project.description?.trim() || (root ? `项目目录 · ${basename(root)}` : 'ChatOS 用户项目'),
    status,
    progress,
    accent: ACCENTS[index % ACCENTS.length],
    updatedAt: formatRelativeTime(updatedAt),
    summary: project.description?.trim() || '来自当前 ChatOS 账号的真实项目，工作区资料与执行计划会持续同步到这份档案。',
    files: files.length > 0 ? files : [blocked > 0 ? `${blocked} 个阻塞事项` : '暂无项目条目'],
    rootPath: root || null,
    gitUrl: gitUrl || null,
    sourceType,
    importStatus: importStatus || null,
    createdAt: formatDateTime(createdAt),
    updatedAtExact: formatDateTime(updatedAt),
    planItems: planItems.length > 0
      ? planItems
      : files.map((title) => ({ title, status: null, kind: 'document' as const })),
    workItemCounts: {
      total,
      done,
      blocked,
      running: runningCount,
    },
  };
};

const normalizeTaskStatus = (raw?: string | null): DemoTask['status'] => {
  const status = String(raw || '').trim().toLowerCase();
  if (['done', 'completed', 'complete', 'success', 'succeeded'].includes(status)) return 'done';
  if (['doing', 'running', 'in_progress', 'in-progress', 'executing'].includes(status)) return 'doing';
  if (['blocked', 'failed', 'error', 'cancelled', 'canceled'].includes(status)) return 'blocked';
  return 'todo';
};

const taskProgress = (status: DemoTask['status']): number => (
  status === 'done' ? 100 : status === 'doing' ? 62 : status === 'blocked' ? 35 : 10
);

const mapTask = (task: RawTask): DemoTask => {
  const status = normalizeTaskStatus(task.status);
  const progress = status === 'done' ? 100 : status === 'doing' ? 62 : status === 'blocked' ? 35 : 10;
  return {
    id: task.id,
    title: String(task.title || '未命名任务'),
    status,
    progress,
    detail: String(task.blocker_reason || task.details || task.outcome_summary || '来自当前 ChatOS 会话的真实任务。'),
    conversationTurnId: String(task.conversation_turn_id || '').trim() || undefined,
    sourceUserMessageId: String(task.source_user_message_id || '').trim() || undefined,
    priority: task.priority || null,
    createdAt: formatDateTime(task.created_at) || undefined,
    completedAt: formatDateTime(task.completed_at || undefined) || undefined,
  };
};

const mapTaskRunnerGraph = (raw: RawTaskRunnerGraph): DemoTaskGraph => {
  const nodes = (raw.nodes || []).flatMap((entry) => {
    const task = entry.task;
    if (!task?.id) return [];
    const status = normalizeTaskStatus(task.status);
    return [{
      id: task.id,
      title: String(task.title || '未命名任务'),
      detail: String(task.description || task.objective || task.result_summary || '暂无任务说明。'),
      status,
      progress: taskProgress(status),
      depth: Number(entry.depth || 0),
      isRoot: entry.is_root === true,
      isCurrent: entry.is_current_message === true,
      prerequisiteIds: Array.isArray(task.prerequisite_task_ids) ? task.prerequisite_task_ids.filter(Boolean) : [],
      creatorName: String(task.creator_display_name || task.creator_username || '').trim() || null,
      updatedAt: formatDateTime(task.updated_at || task.created_at || undefined),
      resultSummary: task.result_summary || null,
    }];
  });
  const nodeIds = new Set(nodes.map((node) => node.id));
  const edgeMap = new Map<string, { id: string; source: string; target: string }>();
  nodes.forEach((node) => {
    node.prerequisiteIds.forEach((source) => {
      if (!nodeIds.has(source) || source === node.id) return;
      const key = `${source}->${node.id}`;
      edgeMap.set(key, { id: key, source, target: node.id });
    });
  });
  if (edgeMap.size === 0) {
    (raw.edges || []).forEach((edge) => {
      const source = String(edge.source || '').trim();
      const target = String(edge.target || '').trim();
      if (!source || !target || source === target || !nodeIds.has(source) || !nodeIds.has(target)) return;
      const key = `${source}->${target}`;
      edgeMap.set(key, { id: String(edge.id || key), source, target });
    });
  }
  return {
    rootTaskIds: Array.isArray(raw.root_task_ids) ? raw.root_task_ids.filter((id) => nodeIds.has(id)) : [],
    nodes,
    edges: Array.from(edgeMap.values()),
    sourceSessionId: raw.source_session_id || null,
    sourceTurnId: raw.source_turn_id || null,
    sourceUserMessageId: raw.source_user_message_id || null,
  };
};

const rawTaskTime = (task: RawTask): number => {
  const timestamp = new Date(task.updated_at || task.created_at || 0).getTime();
  return Number.isFinite(timestamp) ? timestamp : 0;
};

const formatMessageTime = (raw?: string | Date): string => {
  const date = raw ? new Date(raw) : new Date();
  if (!Number.isFinite(date.getTime())) return '--:--';
  return new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false }).format(date);
};

const rawAttachments = (metadata?: Record<string, unknown> | null): ChatAttachment[] => {
  const items = Array.isArray(metadata?.attachments) ? metadata.attachments : [];
  return items.flatMap((item, index) => {
    if (!item || typeof item !== 'object') return [];
    const record = item as Record<string, unknown>;
    const name = String(record.name || '').trim();
    if (!name) return [];
    const rawType = String(record.type || 'file');
    return [{
      id: String(record.id || `${name}-${index}`),
      name,
      mimeType: String(record.mimeType || record.mime_type || 'application/octet-stream'),
      size: Number(record.size || 0),
      type: rawType === 'image' || rawType === 'audio' ? rawType : 'file',
      url: typeof record.viewUrl === 'string' ? record.viewUrl : typeof record.url === 'string' ? record.url : undefined,
    } satisfies ChatAttachment];
  });
};

const mapMessages = (items: RawMessage[]): ChatMessage[] => items
  .filter((item) => item.role === 'user' || item.role === 'assistant')
  .sort((a, b) => {
    if (typeof a.sequence_no === 'number' && typeof b.sequence_no === 'number') return a.sequence_no - b.sequence_no;
    return new Date(a.created_at || String(a.createdAt || 0)).getTime() - new Date(b.created_at || String(b.createdAt || 0)).getTime();
  })
  .map((item) => ({
    id: item.id,
    role: item.role as ChatMessage['role'],
    content: String(item.content || item.rawContent || ''),
    time: formatMessageTime(item.created_at || item.createdAt),
    attachments: rawAttachments(item.metadata),
    status: item.status === 'failed' || item.status === 'error' ? 'error' as const : 'complete' as const,
  }))
  .filter((item) => item.content.trim().length > 0 || (item.attachments?.length || 0) > 0);

const metadataObject = (metadata: RawSession['metadata']): Record<string, unknown> => {
  if (metadata && typeof metadata === 'object' && !Array.isArray(metadata)) return metadata;
  if (typeof metadata === 'string') {
    try {
      const parsed = JSON.parse(metadata) as unknown;
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) return parsed as Record<string, unknown>;
    } catch {
      return {};
    }
  }
  return {};
};

const nestedRecord = (record: Record<string, unknown>, key: string): Record<string, unknown> => {
  const candidate = record[key];
  return candidate && typeof candidate === 'object' && !Array.isArray(candidate) ? candidate as Record<string, unknown> : {};
};

const sessionIdentity = (session: RawSession) => {
  const root = metadataObject(session.metadata);
  const source = Object.keys(nestedRecord(root, 'source_metadata')).length > 0 ? nestedRecord(root, 'source_metadata') : root;
  const runtime = nestedRecord(source, 'chat_runtime');
  const contact = nestedRecord(source, 'contact');
  const rawProjectId = value(session.project_id, session.projectId)
    || String(runtime.project_id || runtime.projectId || '').trim();
  return {
    contactId: String(contact.contact_id || contact.contactId || runtime.contact_id || runtime.contactId || '').trim() || null,
    agentId: String(contact.agent_id || contact.agentId || runtime.contact_agent_id || runtime.contactAgentId || '').trim() || null,
    projectId: !rawProjectId || rawProjectId === '0' || rawProjectId === '-1' ? null : rawProjectId,
  };
};

const findContactSession = (
  sessions: RawSession[],
  contactId: string,
  agentId: string,
  projectId: string | null,
): RawSession | null => {
  const candidates = sessions.filter((session) => {
    if (session.archived) return false;
    const identity = sessionIdentity(session);
    const sameContact = identity.contactId ? identity.contactId === contactId : identity.agentId === agentId;
    return sameContact && identity.projectId === projectId;
  });
  return candidates.sort((left, right) => {
    const leftMessages = Number(left.message_count ?? left.messageCount ?? 0);
    const rightMessages = Number(right.message_count ?? right.messageCount ?? 0);
    if ((leftMessages > 0) !== (rightMessages > 0)) return rightMessages - leftMessages;
    const leftTime = new Date(value(left.updated_at, left.updatedAt) || value(left.created_at, left.createdAt) || 0).getTime();
    const rightTime = new Date(value(right.updated_at, right.updatedAt) || value(right.created_at, right.createdAt) || 0).getTime();
    return rightTime - leftTime;
  })[0] || null;
};

const mapAgent = (agent: RawAgent): ChatAgentOption => ({
  id: agent.id,
  name: agent.name || '未命名 Agent',
  description: agent.description || null,
  enabled: agent.enabled !== false,
});

const mapModel = (model: RawModelConfig): ChatModelOption => ({
  id: model.id,
  name: String(model.name || model.model_name || model.model || '未命名模型'),
  modelName: String(model.model_name || model.model || model.name || ''),
  thinkingLevel: model.thinking_level || null,
  supportsImages: model.supports_images !== false,
  supportsReasoning: model.supports_reasoning !== false,
  enabled: model.enabled !== false,
});

const normalizeRuntimeSettings = (
  raw: RawRuntimeSettings | null | undefined,
  session: RawSession,
  fallbackModel: RawModelConfig | null,
): ChatRuntimeSettings => ({
  selectedModelId: String(raw?.selected_model_id || session.selected_model_id || fallbackModel?.id || '').trim() || null,
  selectedModelName: String(raw?.selected_model_name || fallbackModel?.model_name || fallbackModel?.model || '').trim() || null,
  selectedThinkingLevel: String(raw?.selected_thinking_level || fallbackModel?.thinking_level || '').trim() || null,
  reasoningEnabled: raw?.reasoning_enabled !== false,
  planModeEnabled: raw?.plan_mode_enabled === true,
});

const latestSession = (sessions: RawSession[]): RawSession | null => {
  const active = sessions.filter((session) => !session.archived);
  return [...active].sort((a, b) => {
    const left = new Date(value(a.updated_at, a.updatedAt) || value(a.created_at, a.createdAt) || 0).getTime();
    const right = new Date(value(b.updated_at, b.updatedAt) || value(b.created_at, b.createdAt) || 0).getTime();
    return right - left;
  })[0] || null;
};

export function useChatOSBridge() {
  const [auth, setAuth] = useState<StoredAuth | null>(() => readStoredAuth());
  const [status, setStatus] = useState<BridgeStatus>(auth ? 'connecting' : 'demo');
  const [webSocketStatus, setWebSocketStatus] = useState<WebSocketStatus>('idle');
  const [pageVisible, setPageVisible] = useState(() => typeof document === 'undefined' || document.visibilityState !== 'hidden');
  const [error, setError] = useState<string | null>(null);
  const [rawProjects, setRawProjects] = useState<RawProject[]>([]);
  const [rawSessions, setRawSessions] = useState<RawSession[]>([]);
  const [modelConfigs, setModelConfigs] = useState<RawModelConfig[]>([]);
  const [rawContacts, setRawContacts] = useState<RawContact[]>([]);
  const [rawAgents, setRawAgents] = useState<RawAgent[]>([]);
  const [rawProjectContacts, setRawProjectContacts] = useState<RawProjectContact[]>([]);
  const [projects, setProjects] = useState<DemoProject[]>([]);
  const [tasks, setTasks] = useState<DemoTask[]>([]);
  const [runningTasks, setRunningTasks] = useState<DemoTask[]>([]);
  const [taskGraph, setTaskGraph] = useState<DemoTaskGraph>(EMPTY_TASK_GRAPH);
  const [taskGraphTaskId, setTaskGraphTaskId] = useState<string | null>(null);
  const [taskGraphLoading, setTaskGraphLoading] = useState(false);
  const [taskGraphError, setTaskGraphError] = useState<string | null>(null);
  const [persistedMessages, setPersistedMessages] = useState<ChatMessage[]>([]);
  const [streamingText, setStreamingText] = useState('');
  const [thinking, setThinking] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [loadingMessages, setLoadingMessages] = useState(false);
  const [hasMoreMessages, setHasMoreMessages] = useState(false);
  const [messageLimit, setMessageLimit] = useState(40);
  const [sessionBusy, setSessionBusy] = useState(false);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversationTitle, setConversationTitle] = useState<string | null>(null);
  const [activeModel, setActiveModel] = useState<RawModelConfig | null>(null);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [activeContactId, setActiveContactId] = useState<string | null>(null);
  const [runtimeSettings, setRuntimeSettings] = useState<ChatRuntimeSettings>({
    selectedModelId: null,
    selectedModelName: null,
    selectedThinkingLevel: null,
    reasoningEnabled: true,
    planModeEnabled: false,
  });
  const refreshTimerRef = useRef<number | null>(null);
  const activeTurnIdRef = useRef<string | null>(null);

  const messages = useMemo(() => {
    if (!streamingText) return persistedMessages;
    return [
      ...persistedMessages,
      { id: 'live-stream', role: 'assistant' as const, content: streamingText, time: formatMessageTime() },
    ];
  }, [persistedMessages, streamingText]);
  const models = useMemo(() => modelConfigs.filter((item) => item.enabled !== false).map(mapModel), [modelConfigs]);
  const agents = useMemo(() => rawAgents.filter((item) => item.enabled !== false).map(mapAgent), [rawAgents]);
  const accountContacts = useMemo<ChatContact[]>(() => rawContacts.flatMap((contact) => {
    const agentId = value(contact.agent_id, contact.agentId);
    if (!agentId) return [];
    const agent = rawAgents.find((item) => item.id === agentId);
    const session = findContactSession(rawSessions, contact.id, agentId, null);
    return [{
      id: contact.id,
      agentId,
      name: value(contact.agent_name_snapshot, contact.agentNameSnapshot) || agent?.name || '未命名联系人',
      description: agent?.description || null,
      sessionId: session?.id || null,
      projectId: null,
      lastActive: formatRelativeTime(value(session?.updated_at, session?.updatedAt) || value(contact.updated_at, contact.updatedAt) || value(contact.created_at, contact.createdAt)),
    } satisfies ChatContact];
  }), [rawAgents, rawContacts, rawSessions]);
  const contacts = useMemo<ChatContact[]>(() => {
    if (activeProjectId) {
      return rawProjectContacts.flatMap((link) => {
        const contactId = value(link.contact_id, link.contactId);
        const agentId = value(link.agent_id, link.agentId);
        if (!contactId || !agentId) return [];
        const accountContact = rawContacts.find((item) => item.id === contactId);
        const agent = rawAgents.find((item) => item.id === agentId);
        const preferredSessionId = value(link.latest_session_id, link.latestSessionId);
        const session = rawSessions.find((item) => item.id === preferredSessionId)
          || findContactSession(rawSessions, contactId, agentId, activeProjectId);
        return [{
          id: contactId,
          agentId,
          name: value(link.agent_name_snapshot, link.agentNameSnapshot)
            || value(accountContact?.agent_name_snapshot, accountContact?.agentNameSnapshot)
            || agent?.name
            || '未命名负责人',
          description: agent?.description || null,
          sessionId: session?.id || null,
          projectId: activeProjectId,
          lastActive: formatRelativeTime(value(link.last_message_at, link.lastMessageAt) || value(link.updated_at, link.updatedAt) || value(session?.updated_at, session?.updatedAt)),
        } satisfies ChatContact];
      });
    }
    return accountContacts;
  }, [accountContacts, activeProjectId, rawAgents, rawContacts, rawProjectContacts, rawSessions]);
  const availableAgents = useMemo(() => {
    const existing = new Set(rawContacts.map((item) => value(item.agent_id, item.agentId)).filter(Boolean));
    return agents.filter((agent) => !existing.has(agent.id));
  }, [agents, rawContacts]);

  const loadConversationData = useCallback(async (
    token: string,
    session: RawSession,
    configs: RawModelConfig[],
    limit = 40,
  ) => {
    setLoadingMessages(true);
    try {
      const [messageItems, taskResult, rawRuntime] = await Promise.all([
        apiRequest<RawMessage[]>(`/conversations/${encodeURIComponent(session.id)}/messages?limit=${limit}`, token),
        apiRequest<{ tasks?: RawTask[] } | RawTask[]>(`/task-manager/tasks?conversation_id=${encodeURIComponent(session.id)}&include_done=true&limit=30`, token),
        apiRequest<RawRuntimeSettings>(`/conversations/${encodeURIComponent(session.id)}/runtime-settings`, token)
          .catch((): RawRuntimeSettings => ({})),
      ]);
      const rawTasks = Array.isArray(taskResult) ? taskResult : Array.isArray(taskResult.tasks) ? taskResult.tasks : [];
      const selectedModelId = String(rawRuntime.selected_model_id || session.selected_model_id || '').trim();
      const model = configs.find((item) => item.id === selectedModelId)
        || configs.find((item) => item.enabled !== false)
        || configs[0]
        || null;
      setConversationId(session.id);
      setConversationTitle(session.title);
      const identity = sessionIdentity(session);
      setActiveProjectId(identity.projectId);
      setActiveContactId(identity.contactId);
      setActiveModel(model);
      setRuntimeSettings(normalizeRuntimeSettings(rawRuntime, session, model));
      setPersistedMessages(mapMessages(messageItems));
      setMessageLimit(limit);
      setHasMoreMessages(messageItems.length >= limit);
      setTasks(rawTasks.map(mapTask));
      setStreamingText('');
      setThinking(false);
      setIsStopping(false);
    } finally {
      setLoadingMessages(false);
    }
  }, []);

  const loadSnapshot = useCallback(async (token: string): Promise<WorkspaceSnapshot> => {
    const [projectItems, sessionItems, taskSessionItems, configs, contacts, agents] = await Promise.all([
      apiRequest<RawProject[]>('/projects', token),
      apiRequest<RawSession[]>('/conversations?limit=80&include_archived=false', token),
      apiRequest<RawSession[]>('/conversations?limit=160&include_archived=true', token)
        .catch(() => [] as RawSession[]),
      apiRequest<RawModelConfig[]>('/ai-model-configs', token),
      apiRequest<RawContact[]>('/contacts?limit=200', token),
      apiRequest<RawAgent[]>('/agents?enabled=true&limit=200', token),
    ]);
    return {
      projects: projectItems,
      sessions: sessionItems,
      taskSessions: taskSessionItems.length > 0 ? taskSessionItems : sessionItems,
      modelConfigs: configs,
      contacts,
      agents,
    };
  }, []);

  const loadWorkspaceTasks = useCallback(async (token: string, sessions: RawSession[]) => {
    if (sessions.length === 0) {
      setRunningTasks([]);
      return [] as DemoTask[];
    }

    const rows: Array<{ task: RawTask; session: RawSession }> = [];
    let nextIndex = 0;
    const workerCount = Math.min(6, sessions.length);
    const workers = Array.from({ length: workerCount }, async () => {
      while (nextIndex < sessions.length) {
        const session = sessions[nextIndex];
        nextIndex += 1;
        try {
          const result = await apiRequest<{ tasks?: RawTask[] } | RawTask[]>(
            `/task-manager/tasks?conversation_id=${encodeURIComponent(session.id)}&include_done=true&limit=100`,
            token,
          );
          const sessionTasks = Array.isArray(result) ? result : Array.isArray(result.tasks) ? result.tasks : [];
          sessionTasks.forEach((task) => rows.push({ task, session }));
        } catch {
          // A deleted or temporarily unavailable conversation should not hide history from other sessions.
        }
      }
    });

    await Promise.all(workers);
    const mapped = rows
      .sort((left, right) => rawTaskTime(right.task) - rawTaskTime(left.task))
      .map(({ task, session }) => ({
        ...mapTask(task),
        id: `${session.id}:${task.id}`,
        conversationId: session.id,
        conversationTitle: session.title || '未命名会话',
        updatedAt: formatRelativeTime(task.updated_at || task.created_at),
      }));
    setRunningTasks(mapped);
    return mapped;
  }, []);

  const loadProjectContactRows = useCallback(async (token: string, projectId: string | null) => {
    if (!projectId) {
      setRawProjectContacts([]);
      return [] as RawProjectContact[];
    }
    const rows = await apiRequest<RawProjectContact[]>(`/projects/${encodeURIComponent(projectId)}/contacts?limit=200`, token);
    setRawProjectContacts(rows);
    return rows;
  }, []);

  const refreshSessionList = useCallback(async (token: string) => {
    const items = await apiRequest<RawSession[]>('/conversations?limit=80&include_archived=false', token);
    setRawSessions(items);
    void apiRequest<RawSession[]>('/conversations?limit=160&include_archived=true', token)
      .then((taskSessions) => loadWorkspaceTasks(token, taskSessions.length > 0 ? taskSessions : items))
      .catch(() => loadWorkspaceTasks(token, items));
    return items;
  }, [loadWorkspaceTasks]);

  const refreshProjectList = useCallback(async (token: string) => {
    const items = await apiRequest<RawProject[]>('/projects', token);
    setRawProjects(items);
    setProjects(items.map((project, index) => mapProject(project, index)));
  }, []);

  const refreshContactList = useCallback(async (token: string) => {
    const items = await apiRequest<RawContact[]>('/contacts?limit=200', token);
    setRawContacts(items);
  }, []);

  const refresh = useCallback(async () => {
    if (!auth?.accessToken) {
      setStatus('demo');
      return;
    }
    const preserveLiveWorkspace = status === 'live';
    if (!preserveLiveWorkspace) setStatus('connecting');
    setError(null);
    try {
      const snapshot = await loadSnapshot(auth.accessToken);
      setRawProjects(snapshot.projects);
      setRawSessions(snapshot.sessions);
      setModelConfigs(snapshot.modelConfigs);
      setRawContacts(snapshot.contacts);
      setRawAgents(snapshot.agents);
      setProjects(snapshot.projects.map((project, index) => mapProject(project, index)));
      const workspaceTasksPromise = loadWorkspaceTasks(auth.accessToken, snapshot.taskSessions);
      const preferred = snapshot.sessions.find((session) => session.id === conversationId)
        || latestSession(snapshot.sessions.filter((session) => {
          const identity = sessionIdentity(session);
          return identity.projectId === null && Boolean(identity.contactId || identity.agentId);
      }));
      if (preferred) {
        const projectRows = await loadProjectContactRows(auth.accessToken, sessionIdentity(preferred).projectId);
        await loadConversationData(auth.accessToken, preferred, snapshot.modelConfigs);
        if (!sessionIdentity(preferred).contactId) {
          const matchedRow = projectRows.find((item) => value(item.latest_session_id, item.latestSessionId) === preferred.id);
          if (matchedRow) setActiveContactId(value(matchedRow.contact_id, matchedRow.contactId) || null);
        }
      } else {
        const firstContact = snapshot.contacts[0];
        const firstAgentId = value(firstContact?.agent_id, firstContact?.agentId);
        const firstAgent = snapshot.agents.find((item) => item.id === firstAgentId);
        setConversationId(null);
        setConversationTitle(firstContact ? value(firstContact.agent_name_snapshot, firstContact.agentNameSnapshot) || firstAgent?.name || '联系人' : null);
        setTasks([]);
        setPersistedMessages([]);
        setActiveProjectId(null);
        setActiveContactId(firstContact?.id || null);
        setRawProjectContacts([]);
        setHasMoreMessages(false);
        setRuntimeSettings({
          selectedModelId: null,
          selectedModelName: null,
          selectedThinkingLevel: null,
          reasoningEnabled: true,
          planModeEnabled: false,
        });
      }
      await workspaceTasksPromise;
      setStatus('live');
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      setStatus(preserveLiveWorkspace ? 'live' : 'error');
    }
  }, [auth, conversationId, loadConversationData, loadProjectContactRows, loadSnapshot, loadWorkspaceTasks, status]);

  const refreshConversation = useCallback(async () => {
    if (!auth?.accessToken || !conversationId) return;
    const session = rawSessions.find((item) => item.id === conversationId);
    if (!session) return;
    await loadConversationData(auth.accessToken, session, modelConfigs, messageLimit);
  }, [auth, conversationId, loadConversationData, messageLimit, modelConfigs, rawSessions]);

  const scheduleConversationRefresh = useCallback(() => {
    if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
    refreshTimerRef.current = window.setTimeout(() => {
      refreshTimerRef.current = null;
      void refreshConversation();
    }, 240);
  }, [refreshConversation]);

  useEffect(() => {
    void refresh();
  }, [auth?.accessToken]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => {
    if (refreshTimerRef.current !== null) window.clearTimeout(refreshTimerRef.current);
  }, []);

  useEffect(() => {
    const handleVisibilityChange = () => setPageVisible(document.visibilityState !== 'hidden');
    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => document.removeEventListener('visibilitychange', handleVisibilityChange);
  }, []);

  useEffect(() => {
    if (!auth?.accessToken || status !== 'live' || !conversationId || !pageVisible) {
      setWebSocketStatus(auth?.accessToken ? 'disconnected' : 'idle');
      return undefined;
    }
    let disposed = false;
    let socket: WebSocket | null = null;
    let retryTimer: number | null = null;

    const connect = async () => {
      setWebSocketStatus('connecting');
      try {
        const ticketResponse = await apiRequest<{ ticket?: string }>('/auth/ws-ticket', auth.accessToken, { method: 'POST' });
        const ticket = String(ticketResponse.ticket || '').trim();
        if (!ticket || disposed) return;
        const wsUrl = new URL(`${API_BASE_URL.replace(/^http/, 'ws')}/realtime/ws`);
        wsUrl.searchParams.set('ws_ticket', ticket);
        socket = new WebSocket(wsUrl.toString());
        socket.onopen = () => {
          if (disposed || !socket) return;
          setWebSocketStatus('connected');
          socket.send(JSON.stringify({
            type: 'subscribe',
            topics: [
              { scope: 'projects' },
              { scope: 'sessions' },
              ...rawSessions.filter((session) => !session.archived).map((session) => ({ scope: 'conversation', id: session.id })),
              ...(activeProjectId ? [{ scope: 'project', id: activeProjectId }] : []),
            ],
          }));
        };
        socket.onmessage = (message) => {
          let envelope: RealtimeEnvelope;
          try {
            envelope = JSON.parse(String(message.data || '')) as RealtimeEnvelope;
          } catch {
            return;
          }
          if (envelope.type !== 'event' || !envelope.payload) return;
          const kind = envelope.payload.kind;
          if (kind === 'projects_updated') {
            void refreshProjectList(auth.accessToken);
            return;
          }
          if (kind === 'sessions_updated') {
            void refreshSessionList(auth.accessToken);
            scheduleConversationRefresh();
            return;
          }
          if (kind === 'contacts_updated') {
            void refreshContactList(auth.accessToken);
            return;
          }
          if (kind === 'project_members_updated' || kind === 'project_contacts_updated') {
            void loadProjectContactRows(auth.accessToken, activeProjectId);
            return;
          }
          const eventConversationId = String(envelope.conversation_id || envelope.payload.conversation_id || '').trim();
          if (kind === 'task_board') {
            const rawTask = envelope.payload.task;
            const eventTaskId = String(rawTask?.id || envelope.payload.task_id || '').trim();
            if (eventConversationId && eventTaskId) {
              const session = rawSessions.find((item) => item.id === eventConversationId);
              const mappedId = `${eventConversationId}:${eventTaskId}`;
              setRunningTasks((current) => {
                const remaining = current.filter((task) => task.id !== mappedId);
                if (!rawTask) return remaining;
                return [{
                  ...mapTask(rawTask),
                  id: mappedId,
                  conversationId: eventConversationId,
                  conversationTitle: session?.title || '未命名会话',
                  updatedAt: formatRelativeTime(rawTask.updated_at || rawTask.created_at),
                }, ...remaining];
              });
            } else {
              void loadWorkspaceTasks(auth.accessToken, rawSessions);
            }
            if (!eventConversationId || eventConversationId === conversationId) scheduleConversationRefresh();
            return;
          }
          if (eventConversationId && eventConversationId !== conversationId) return;
          if (kind !== 'chat_stream') return;
          const streamType = String(envelope.payload.raw?.type || envelope.payload.stream_type || envelope.event || '').toLowerCase();
          if (streamType.includes('start')) {
            setThinking(true);
            setStreamingText('');
            return;
          }
          if (streamType.includes('chunk') || streamType.includes('delta')) {
            const chunk = typeof envelope.payload.raw?.content === 'string' ? envelope.payload.raw.content : '';
            if (chunk) {
              setThinking(false);
              setStreamingText((current) => current + chunk);
            }
            return;
          }
          if (streamType.includes('complete') || streamType.includes('error') || streamType.includes('cancel')) {
            setThinking(false);
            setIsStopping(false);
            activeTurnIdRef.current = null;
            if (streamType.includes('error')) {
              setError(String(envelope.payload.raw?.message || 'AI 回复失败'));
            }
            scheduleConversationRefresh();
          }
        };
        socket.onerror = () => setWebSocketStatus('error');
        socket.onclose = () => {
          if (disposed) return;
          setWebSocketStatus('disconnected');
          retryTimer = window.setTimeout(() => void connect(), 1800);
        };
      } catch (cause) {
        if (disposed) return;
        setWebSocketStatus('error');
        setError(cause instanceof Error ? cause.message : String(cause));
        retryTimer = window.setTimeout(() => void connect(), 2500);
      }
    };

    void connect();
    return () => {
      disposed = true;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
      socket?.close();
    };
  }, [activeProjectId, auth, conversationId, loadProjectContactRows, loadWorkspaceTasks, pageVisible, rawSessions, refreshContactList, refreshProjectList, refreshSessionList, scheduleConversationRefresh, status]);

  const loadTaskGraph = useCallback(async (task: DemoTask) => {
    setTaskGraphTaskId(task.id);
    setTaskGraphError(null);
    if (!auth?.accessToken || !task.conversationId || (!task.conversationTurnId && !task.sourceUserMessageId)) {
      setTaskGraph(EMPTY_TASK_GRAPH);
      setTaskGraphLoading(false);
      if (auth?.accessToken) setTaskGraphError('这个任务没有关联的会话轮次，暂时无法读取依赖图。');
      return EMPTY_TASK_GRAPH;
    }

    setTaskGraphLoading(true);
    setTaskGraph(EMPTY_TASK_GRAPH);
    try {
      const query = new URLSearchParams();
      query.set('session_id', task.conversationId);
      if (task.conversationTurnId) query.set('turn_id', task.conversationTurnId);
      if (task.sourceUserMessageId) query.set('source_user_message_id', task.sourceUserMessageId);
      const messageId = task.sourceUserMessageId || `task-source-${task.id}`;
      const response = await apiRequest<RawTaskRunnerGraph>(
        `/messages/${encodeURIComponent(messageId)}/task-runner/graph?${query.toString()}`,
        auth.accessToken,
      );
      const mapped = mapTaskRunnerGraph(response);
      setTaskGraph(mapped);
      if (mapped.nodes.length === 0) {
        setTaskGraphError('该任务轮次没有 Task Runner 依赖图记录。');
      }
      return mapped;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setTaskGraph(EMPTY_TASK_GRAPH);
      setTaskGraphError(message || '读取任务流程图失败');
      return EMPTY_TASK_GRAPH;
    } finally {
      setTaskGraphLoading(false);
    }
  }, [auth]);

  const login = useCallback(async (username: string, password: string) => {
    setStatus('connecting');
    setError(null);
    try {
      const response = await apiRequest<{
        access_token?: string;
        token?: string;
        user?: { id?: string; username?: string } | null;
      }>('/auth/login', null, {
        method: 'POST',
        body: JSON.stringify({ username, password }),
      });
      const accessToken = String(response.access_token || response.token || '').trim();
      const userId = String(response.user?.id || response.user?.username || username).trim();
      if (!accessToken || !userId) throw new Error('登录响应缺少令牌或用户信息');
      const nextAuth: StoredAuth = {
        accessToken,
        user: { id: userId, username: String(response.user?.username || username) },
      };
      persistAuth(nextAuth);
      setAuth(nextAuth);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setStatus('error');
      throw cause;
    }
  }, []);

  const logout = useCallback(() => {
    persistAuth(null);
    setAuth(null);
    setStatus('demo');
    setError(null);
    setRawProjects([]);
    setRawSessions([]);
    setModelConfigs([]);
    setRawContacts([]);
    setRawAgents([]);
    setRawProjectContacts([]);
    setProjects([]);
    setTasks([]);
    setRunningTasks([]);
    setTaskGraph(EMPTY_TASK_GRAPH);
    setTaskGraphTaskId(null);
    setTaskGraphLoading(false);
    setTaskGraphError(null);
    setPersistedMessages([]);
    setConversationId(null);
    setConversationTitle(null);
    setActiveModel(null);
    setActiveProjectId(null);
    setActiveContactId(null);
    setStreamingText('');
    setThinking(false);
    setIsStopping(false);
    setLoadingMessages(false);
    setHasMoreMessages(false);
    setMessageLimit(40);
    setSessionBusy(false);
    setRuntimeSettings({
      selectedModelId: null,
      selectedModelName: null,
      selectedThinkingLevel: null,
      reasoningEnabled: true,
      planModeEnabled: false,
    });
    activeTurnIdRef.current = null;
  }, []);

  const loadMoreMessages = useCallback(async () => {
    if (!auth?.accessToken || !conversationId || loadingMessages || !hasMoreMessages) return;
    const session = rawSessions.find((item) => item.id === conversationId);
    if (!session) return;
    await loadConversationData(auth.accessToken, session, modelConfigs, messageLimit + 40);
  }, [auth, conversationId, hasMoreMessages, loadConversationData, loadingMessages, messageLimit, modelConfigs, rawSessions]);

  const updateRuntimeSettings = useCallback(async (patch: Partial<ChatRuntimeSettings>) => {
    if (!auth?.accessToken) return;
    const body: RawRuntimeSettings = {};
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedModelId')) body.selected_model_id = patch.selectedModelId ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedModelName')) body.selected_model_name = patch.selectedModelName ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'selectedThinkingLevel')) body.selected_thinking_level = patch.selectedThinkingLevel ?? null;
    if (Object.prototype.hasOwnProperty.call(patch, 'reasoningEnabled')) body.reasoning_enabled = patch.reasoningEnabled === true;
    if (Object.prototype.hasOwnProperty.call(patch, 'planModeEnabled')) body.plan_mode_enabled = patch.planModeEnabled === true;
    const previous = runtimeSettings;
    const optimistic = { ...runtimeSettings, ...patch };
    setRuntimeSettings(optimistic);
    if (patch.selectedModelId) {
      const model = modelConfigs.find((item) => item.id === patch.selectedModelId) || null;
      if (model) setActiveModel(model);
    }
    if (!conversationId) return;
    try {
      const saved = await apiRequest<RawRuntimeSettings>(`/conversations/${encodeURIComponent(conversationId)}/runtime-settings`, auth.accessToken, {
        method: 'PUT',
        body: JSON.stringify(body),
      });
      const session = rawSessions.find((item) => item.id === conversationId) || { id: conversationId, title: conversationTitle || '当前会话' };
      const selected = modelConfigs.find((item) => item.id === saved.selected_model_id)
        || modelConfigs.find((item) => item.id === optimistic.selectedModelId)
        || activeModel;
      setActiveModel(selected || null);
      setRuntimeSettings(normalizeRuntimeSettings(saved, session, selected || null));
    } catch (cause) {
      setRuntimeSettings(previous);
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    }
  }, [activeModel, auth, conversationId, conversationTitle, modelConfigs, rawSessions, runtimeSettings]);

  const selectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || thinking || contactId === activeContactId) return;
    const contact = contacts.find((item) => item.id === contactId);
    if (!contact) throw new Error('找不到这个联系人');
    setActiveContactId(contact.id);
    setError(null);
    if (contact.sessionId) {
      const session = rawSessions.find((item) => item.id === contact.sessionId)
        || await apiRequest<RawSession>(`/conversations/${encodeURIComponent(contact.sessionId)}`, auth.accessToken);
      if (!rawSessions.some((item) => item.id === session.id)) setRawSessions((current) => [session, ...current]);
      await loadConversationData(auth.accessToken, session, modelConfigs, 40);
      return;
    }
    const fallbackModel = modelConfigs.find((item) => item.enabled !== false) || modelConfigs[0] || null;
    setConversationId(null);
    setConversationTitle(contact.name);
    setPersistedMessages([]);
    setTasks([]);
    setHasMoreMessages(false);
    setActiveModel(fallbackModel);
    setRuntimeSettings((current) => ({
      ...current,
      selectedModelId: current.selectedModelId || fallbackModel?.id || null,
      selectedModelName: current.selectedModelName || fallbackModel?.model_name || fallbackModel?.model || null,
      selectedThinkingLevel: current.selectedThinkingLevel || fallbackModel?.thinking_level || null,
    }));
  }, [activeContactId, auth, contacts, loadConversationData, modelConfigs, rawSessions, thinking]);

  const selectPersonalContacts = useCallback(async () => {
    if (!auth?.accessToken || thinking) return;
    setActiveProjectId(null);
    setRawProjectContacts([]);
    const first = rawContacts[0];
    if (!first) {
      setActiveContactId(null);
      setConversationId(null);
      setConversationTitle(null);
      setPersistedMessages([]);
      return;
    }
    const agentId = value(first.agent_id, first.agentId);
    const session = findContactSession(rawSessions, first.id, agentId, null);
    setActiveContactId(first.id);
    if (session) {
      await loadConversationData(auth.accessToken, session, modelConfigs, 40);
    } else {
      const agent = rawAgents.find((item) => item.id === agentId);
      setConversationId(null);
      setConversationTitle(value(first.agent_name_snapshot, first.agentNameSnapshot) || agent?.name || '联系人');
      setPersistedMessages([]);
      setTasks([]);
      setHasMoreMessages(false);
    }
  }, [auth, loadConversationData, modelConfigs, rawAgents, rawContacts, rawSessions, thinking]);

  const addContact = useCallback(async (agentId: string) => {
    if (!auth?.accessToken || sessionBusy) return null;
    const agent = rawAgents.find((item) => item.id === agentId);
    if (!agent) throw new Error('Agent 不存在或已停用');
    setSessionBusy(true);
    try {
      const result = await apiRequest<RawContact | { contact?: RawContact }>('/contacts', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({ agent_id: agent.id, agent_name_snapshot: agent.name, user_id: auth.user.id }),
      });
      const created = 'contact' in result && result.contact ? result.contact : result as RawContact;
      setRawContacts((current) => [created, ...current.filter((item) => item.id !== created.id)]);
      setActiveProjectId(null);
      setActiveContactId(created.id);
      setConversationId(null);
      setConversationTitle(agent.name);
      setPersistedMessages([]);
      setRawProjectContacts([]);
      return created.id;
    } finally {
      setSessionBusy(false);
    }
  }, [auth, rawAgents, sessionBusy]);

  const deleteContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || sessionBusy || thinking) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/contacts/${encodeURIComponent(contactId)}`, auth.accessToken, { method: 'DELETE' });
      const remaining = rawContacts.filter((item) => item.id !== contactId);
      setRawContacts(remaining);
      if (activeContactId === contactId) {
        setActiveContactId(null);
        setConversationId(null);
        setConversationTitle(null);
        setPersistedMessages([]);
      }
    } finally {
      setSessionBusy(false);
    }
  }, [activeContactId, auth, rawContacts, sessionBusy, thinking]);

  const assignProjectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || !activeProjectId || sessionBusy) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/projects/${encodeURIComponent(activeProjectId)}/contacts`, auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({ contact_id: contactId }),
      });
      await loadProjectContactRows(auth.accessToken, activeProjectId);
      setActiveContactId(contactId);
      setConversationId(null);
      const contact = rawContacts.find((item) => item.id === contactId);
      const agent = rawAgents.find((item) => item.id === value(contact?.agent_id, contact?.agentId));
      setConversationTitle(value(contact?.agent_name_snapshot, contact?.agentNameSnapshot) || agent?.name || '项目负责人');
      setPersistedMessages([]);
    } finally {
      setSessionBusy(false);
    }
  }, [activeProjectId, auth, loadProjectContactRows, rawAgents, rawContacts, sessionBusy]);

  const removeProjectContact = useCallback(async (contactId: string) => {
    if (!auth?.accessToken || !activeProjectId || sessionBusy || thinking) return;
    setSessionBusy(true);
    try {
      await apiRequest(`/projects/${encodeURIComponent(activeProjectId)}/contacts/${encodeURIComponent(contactId)}`, auth.accessToken, { method: 'DELETE' });
      const rows = await loadProjectContactRows(auth.accessToken, activeProjectId);
      if (activeContactId === contactId) {
        const nextContactId = rows[0] ? value(rows[0].contact_id, rows[0].contactId) : '';
        setActiveContactId(nextContactId || null);
        setConversationId(null);
        setConversationTitle(rows[0] ? value(rows[0].agent_name_snapshot, rows[0].agentNameSnapshot) || '项目负责人' : null);
        setPersistedMessages([]);
      }
    } finally {
      setSessionBusy(false);
    }
  }, [activeContactId, activeProjectId, auth, loadProjectContactRows, sessionBusy, thinking]);

  const ensureContactConversation = useCallback(async (): Promise<string> => {
    if (conversationId) return conversationId;
    if (!auth?.accessToken || !activeContactId || !activeModel) throw new Error('请先选择联系人或项目负责人');
    const contact = contacts.find((item) => item.id === activeContactId);
    if (!contact) throw new Error('当前联系人不可用');
    const projectScope = activeProjectId || '0';
    const id = `conv_${typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : Date.now()}`;
    const metadata = {
      chat_runtime: {
        selected_model_id: runtimeSettings.selectedModelId || activeModel.id,
        selected_model_name: runtimeSettings.selectedModelName,
        selected_thinking_level: runtimeSettings.selectedThinkingLevel,
        contact_agent_id: contact.agentId,
        project_id: projectScope,
        reasoning_enabled: runtimeSettings.reasoningEnabled,
        plan_mode_enabled: runtimeSettings.planModeEnabled,
      },
      contact: { type: 'memory_agent', agent_id: contact.agentId, contact_id: contact.id },
      ui_chat_selection: { selected_model_id: runtimeSettings.selectedModelId || activeModel.id, selected_agent_id: contact.agentId },
      ui_contact: { type: 'memory_agent', agent_id: contact.agentId, contact_id: contact.id },
    };
    const created = await apiRequest<RawSession>('/conversations', auth.accessToken, {
      method: 'POST',
      body: JSON.stringify({ id, title: contact.name, user_id: auth.user.id, project_id: projectScope, metadata }),
    });
    setRawSessions((current) => [created, ...current.filter((item) => item.id !== created.id)]);
    setConversationId(created.id);
    setConversationTitle(contact.name);
    return created.id;
  }, [activeContactId, activeModel, activeProjectId, auth, contacts, conversationId, runtimeSettings]);

  const selectProject = useCallback(async (projectId: string) => {
    if (!auth?.accessToken) return;
    setActiveProjectId(projectId);
    const projectContactRows = await loadProjectContactRows(auth.accessToken, projectId);
    const rawProject = rawProjects.find((project) => project.id === projectId);
    if (rawProject) {
      const plan = await apiRequest<RawProjectPlan>(`/projects/${encodeURIComponent(projectId)}/plan?include_work_items=true`, auth.accessToken)
        .catch(() => null);
      setProjects((current) => current.map((project, index) => (
        project.id === projectId ? mapProject(rawProject, index, plan) : project
      )));
    }
    const preferredSessionId = projectContactRows.map((item) => value(item.latest_session_id, item.latestSessionId)).find(Boolean)
      || (rawProject ? value(rawProject.latest_session_id, rawProject.latestSessionId) : '');
    const session = rawSessions.find((item) => item.id === preferredSessionId)
      || latestSession(rawSessions.filter((item) => value(item.project_id, item.projectId) === projectId));
    if (session) {
      await loadConversationData(auth.accessToken, session, modelConfigs);
      const matchedContact = projectContactRows.find((item) => value(item.latest_session_id, item.latestSessionId) === session.id)
        || projectContactRows.find((item) => value(item.agent_id, item.agentId) === sessionIdentity(session).agentId);
      if (matchedContact) setActiveContactId(value(matchedContact.contact_id, matchedContact.contactId) || null);
    } else {
      const firstContact = projectContactRows[0];
      setActiveContactId(firstContact ? value(firstContact.contact_id, firstContact.contactId) || null : null);
      setConversationId(null);
      setConversationTitle(firstContact ? value(firstContact.agent_name_snapshot, firstContact.agentNameSnapshot) || '项目负责人' : null);
      setPersistedMessages([]);
      setTasks([]);
      setHasMoreMessages(false);
    }
  }, [auth, loadConversationData, loadProjectContactRows, modelConfigs, rawProjects, rawSessions]);

  const sendMessage = useCallback(async (content: string, files: File[] = []) => {
    const normalized = content.trim();
    if ((!normalized && files.length === 0) || !auth?.accessToken || !activeModel) {
      throw new Error('真实聊天尚未准备好：需要联系人和模型配置');
    }
    const targetConversationId = await ensureContactConversation();
    const optimisticId = `optimistic-${Date.now()}`;
    const userMessage: ChatMessage = {
      id: optimisticId,
      role: 'user',
      content: normalized,
      time: formatMessageTime(),
      status: 'sending',
      attachments: files.map((file, index) => ({
        id: `${optimisticId}-${index}`,
        name: file.name,
        mimeType: file.type || 'application/octet-stream',
        size: file.size,
        type: file.type.startsWith('image/') ? 'image' : file.type.startsWith('audio/') ? 'audio' : 'file',
      })),
    };
    setPersistedMessages((current) => [...current, userMessage]);
    setStreamingText('');
    setThinking(true);
    setError(null);
    const activeProject = rawProjects.find((project) => project.id === activeProjectId);
    const projectRoot = activeProject
      ? value(activeProject.display_root_path, activeProject.displayRootPath) || value(activeProject.root_path, activeProject.rootPath)
      : '';
    const turnId = `turn_${typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : Date.now()}`;
    activeTurnIdRef.current = turnId;
    try {
      const attachments = await prepareAttachmentPayloads(files);
      await apiRequest('/agent/chat/send', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: targetConversationId,
          content: normalized,
          user_id: auth.user.id,
          attachments,
          reasoning_enabled: runtimeSettings.reasoningEnabled,
          turn_id: turnId,
          project_id: activeProjectId || undefined,
          project_root: projectRoot || undefined,
          workspace_root: projectRoot || undefined,
          plan_mode: runtimeSettings.planModeEnabled,
          model_config_id: activeModel.id,
          ai_model_config: {
            temperature: activeModel.temperature ?? 0.7,
            model_name: activeModel.model_name || activeModel.model || '',
            thinking_level: runtimeSettings.selectedThinkingLevel || activeModel.thinking_level || null,
          },
        }),
      });
      setPersistedMessages((current) => current.map((message) => (
        message.id === optimisticId ? { ...message, status: 'complete' } : message
      )));
    } catch (cause) {
      setThinking(false);
      activeTurnIdRef.current = null;
      setPersistedMessages((current) => current.map((message) => (
        message.id === optimisticId ? { ...message, status: 'error' } : message
      )));
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    }
  }, [activeModel, activeProjectId, auth, ensureContactConversation, rawProjects, runtimeSettings.planModeEnabled, runtimeSettings.reasoningEnabled, runtimeSettings.selectedThinkingLevel]);

  const stopMessage = useCallback(async () => {
    if (!auth?.accessToken || !conversationId || (!thinking && !activeTurnIdRef.current) || isStopping) return;
    setIsStopping(true);
    setError(null);
    try {
      await apiRequest('/agent/chat/stop', auth.accessToken, {
        method: 'POST',
        body: JSON.stringify({
          conversation_id: conversationId,
          turn_id: activeTurnIdRef.current || undefined,
        }),
      });
      setThinking(false);
      activeTurnIdRef.current = null;
      scheduleConversationRefresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      throw cause;
    } finally {
      setIsStopping(false);
    }
  }, [auth, conversationId, isStopping, scheduleConversationRefresh, thinking]);

  return {
    apiBaseUrl: API_BASE_URL,
    status,
    webSocketStatus,
    error,
    user: auth?.user || null,
    projects,
    tasks,
    runningTasks,
    taskGraph,
    taskGraphTaskId,
    taskGraphLoading,
    taskGraphError,
    messages,
    models,
    contacts,
    accountContacts,
    agents,
    availableAgents,
    runtimeSettings,
    activeProjectId,
    activeContactId,
    thinking,
    isStopping,
    loadingMessages,
    hasMoreMessages,
    sessionBusy,
    conversationId,
    conversationTitle,
    login,
    logout,
    refresh,
    loadTaskGraph,
    selectContact,
    selectPersonalContacts,
    addContact,
    deleteContact,
    assignProjectContact,
    removeProjectContact,
    loadMoreMessages,
    updateRuntimeSettings,
    selectProject,
    sendMessage,
    stopMessage,
  };
}
