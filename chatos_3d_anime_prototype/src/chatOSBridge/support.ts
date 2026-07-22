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
} from '../types';

export const DEFAULT_API_BASE_URL = 'http://127.0.0.1:3997/api';
export const API_BASE_URL = String(import.meta.env.VITE_CHATOS_API_BASE_URL || DEFAULT_API_BASE_URL).replace(/\/$/, '');
export const AUTH_STORAGE_KEY = 'chatos-3d-auth';

export type BridgeStatus = 'demo' | 'connecting' | 'live' | 'error';
export type WebSocketStatus = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';

export interface ChatOSUser {
  id: string;
  username: string;
}

export interface StoredAuth {
  accessToken: string;
  user: ChatOSUser;
}

export interface RawProject {
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

export interface RawSession {
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

export interface RawContact {
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

export interface RawAgent {
  id: string;
  name: string;
  description?: string | null;
  enabled?: boolean;
}

export interface RawProjectContact {
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

export interface RawMessage {
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

export interface RawTask {
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

export interface RawTaskRunnerTask {
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

export interface RawTaskRunnerGraph {
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

export interface RawModelConfig {
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

export interface RawRuntimeSettings {
  selected_model_id?: string | null;
  selected_model_name?: string | null;
  selected_thinking_level?: string | null;
  reasoning_enabled?: boolean;
  plan_mode_enabled?: boolean;
}

export interface RawProjectPlan {
  requirements?: Array<{ title?: string; status?: string }>;
  work_items?: Array<{ title?: string; status?: string }>;
  workItems?: Array<{ title?: string; status?: string }>;
  work_item_counts?: { total?: number; done?: number; blocked?: number };
  workItemCounts?: { total?: number; done?: number; blocked?: number };
}

export interface RealtimeEnvelope {
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

export interface WorkspaceSnapshot {
  projects: RawProject[];
  sessions: RawSession[];
  taskSessions: RawSession[];
  modelConfigs: RawModelConfig[];
  contacts: RawContact[];
  agents: RawAgent[];
}

export const ACCENTS = ['#79543d', '#506d63', '#7d6448', '#695655', '#4f6478', '#76634f'];

export const EMPTY_TASK_GRAPH: DemoTaskGraph = {
  rootTaskIds: [],
  nodes: [],
  edges: [],
  sourceSessionId: null,
  sourceTurnId: null,
  sourceUserMessageId: null,
};

export const readStoredAuth = (): StoredAuth | null => {
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

export const persistAuth = (auth: StoredAuth | null) => {
  if (typeof window === 'undefined') return;
  if (!auth) {
    window.localStorage.removeItem(AUTH_STORAGE_KEY);
    return;
  }
  window.localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(auth));
};

export const apiRequest = async <T,>(
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

export const value = (first?: string | null, second?: string | null): string => String(first || second || '').trim();

export const formatRelativeTime = (raw?: string): string => {
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

export const formatDateTime = (raw?: string): string | null => {
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

export const basename = (path: string): string => path.replace(/[\\/]+$/, '').split(/[\\/]/).filter(Boolean).pop() || path;

export const projectMetadataFiles = (project: RawProject): string[] => {
  const root = value(project.display_root_path, project.displayRootPath) || value(project.root_path, project.rootPath);
  const source = value(project.source_type, project.sourceType);
  const gitUrl = value(project.git_url, project.gitUrl);
  return [root ? basename(root) : '', source ? `来源：${source}` : '', gitUrl ? 'Git 仓库' : '']
    .filter(Boolean)
    .slice(0, 6);
};

export const mapProject = (project: RawProject, index: number, plan?: RawProjectPlan | null): DemoProject => {
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

export const normalizeTaskStatus = (raw?: string | null): DemoTask['status'] => {
  const status = String(raw || '').trim().toLowerCase();
  if (['done', 'completed', 'complete', 'success', 'succeeded'].includes(status)) return 'done';
  if (['doing', 'running', 'in_progress', 'in-progress', 'executing'].includes(status)) return 'doing';
  if (['blocked', 'failed', 'error', 'cancelled', 'canceled'].includes(status)) return 'blocked';
  return 'todo';
};

export const taskProgress = (status: DemoTask['status']): number => (
  status === 'done' ? 100 : status === 'doing' ? 62 : status === 'blocked' ? 35 : 10
);

export const mapTask = (task: RawTask): DemoTask => {
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

export const mapTaskRunnerGraph = (raw: RawTaskRunnerGraph): DemoTaskGraph => {
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

export const rawTaskTime = (task: RawTask): number => {
  const timestamp = new Date(task.updated_at || task.created_at || 0).getTime();
  return Number.isFinite(timestamp) ? timestamp : 0;
};

export const formatMessageTime = (raw?: string | Date): string => {
  const date = raw ? new Date(raw) : new Date();
  if (!Number.isFinite(date.getTime())) return '--:--';
  return new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false }).format(date);
};

export const rawAttachments = (metadata?: Record<string, unknown> | null): ChatAttachment[] => {
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

export const mapMessages = (items: RawMessage[]): ChatMessage[] => items
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

export const metadataObject = (metadata: RawSession['metadata']): Record<string, unknown> => {
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

export const nestedRecord = (record: Record<string, unknown>, key: string): Record<string, unknown> => {
  const candidate = record[key];
  return candidate && typeof candidate === 'object' && !Array.isArray(candidate) ? candidate as Record<string, unknown> : {};
};

export const sessionIdentity = (session: RawSession) => {
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

export const findContactSession = (
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

export const mapAgent = (agent: RawAgent): ChatAgentOption => ({
  id: agent.id,
  name: agent.name || '未命名 Agent',
  description: agent.description || null,
  enabled: agent.enabled !== false,
});

export const mapModel = (model: RawModelConfig): ChatModelOption => ({
  id: model.id,
  name: String(model.name || model.model_name || model.model || '未命名模型'),
  modelName: String(model.model_name || model.model || model.name || ''),
  thinkingLevel: model.thinking_level || null,
  supportsImages: model.supports_images !== false,
  supportsReasoning: model.supports_reasoning !== false,
  enabled: model.enabled !== false,
});

export const normalizeRuntimeSettings = (
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

export const latestSession = (sessions: RawSession[]): RawSession | null => {
  const active = sessions.filter((session) => !session.archived);
  return [...active].sort((a, b) => {
    const left = new Date(value(a.updated_at, a.updatedAt) || value(a.created_at, a.createdAt) || 0).getTime();
    const right = new Date(value(b.updated_at, b.updatedAt) || value(b.created_at, b.createdAt) || 0).getTime();
    return right - left;
  })[0] || null;
};
