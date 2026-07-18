export type ViewMode = 'room' | 'computer' | 'chat' | 'terminal' | 'remote' | 'archive' | 'project' | 'projection' | 'phone';

export type TimeMode = 'day' | 'sunset' | 'night';

export interface DemoProject {
  id: string;
  name: string;
  subtitle: string;
  status: 'running' | 'planning' | 'idle';
  progress: number;
  accent: string;
  updatedAt: string;
  summary: string;
  files: string[];
  rootPath?: string | null;
  gitUrl?: string | null;
  sourceType?: string | null;
  importStatus?: string | null;
  createdAt?: string | null;
  updatedAtExact?: string | null;
  planItems?: Array<{
    title: string;
    status?: string | null;
    kind: 'requirement' | 'work-item' | 'document';
  }>;
  workItemCounts?: {
    total: number;
    done: number;
    blocked: number;
    running: number;
  };
}

export interface DemoTask {
  id: string;
  title: string;
  status: 'doing' | 'todo' | 'blocked' | 'done';
  progress: number;
  detail: string;
  conversationId?: string;
  conversationTitle?: string;
  conversationTurnId?: string;
  sourceUserMessageId?: string;
  priority?: 'high' | 'medium' | 'low' | null;
  createdAt?: string;
  updatedAt?: string;
  completedAt?: string;
}

export interface DemoTaskGraphNode {
  id: string;
  title: string;
  detail: string;
  status: DemoTask['status'];
  progress: number;
  depth: number;
  isRoot: boolean;
  isCurrent: boolean;
  prerequisiteIds: string[];
  creatorName?: string | null;
  updatedAt?: string | null;
  resultSummary?: string | null;
}

export interface DemoTaskGraphEdge {
  id: string;
  source: string;
  target: string;
}

export interface DemoTaskGraph {
  rootTaskIds: string[];
  nodes: DemoTaskGraphNode[];
  edges: DemoTaskGraphEdge[];
  sourceSessionId?: string | null;
  sourceTurnId?: string | null;
  sourceUserMessageId?: string | null;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  time: string;
  attachments?: ChatAttachment[];
  status?: 'sending' | 'complete' | 'error';
}

export interface ChatAttachment {
  id?: string;
  name: string;
  mimeType: string;
  size: number;
  type: 'image' | 'file' | 'audio';
  url?: string;
}

export interface ChatAttachmentPayload extends ChatAttachment {
  dataUrl?: string;
  text?: string;
}

export interface ChatSession {
  id: string;
  title: string;
  projectId: string | null;
  updatedAt: string;
  archived: boolean;
}

export interface ChatModelOption {
  id: string;
  name: string;
  modelName: string;
  thinkingLevel: string | null;
  supportsImages: boolean;
  supportsReasoning: boolean;
  enabled: boolean;
}

export interface ChatContact {
  id: string;
  agentId: string;
  name: string;
  description?: string | null;
  sessionId: string | null;
  projectId: string | null;
  lastActive: string;
}

export interface ChatAgentOption {
  id: string;
  name: string;
  description?: string | null;
  enabled: boolean;
}

export interface ChatRuntimeSettings {
  selectedModelId: string | null;
  selectedModelName: string | null;
  selectedThinkingLevel: string | null;
  reasoningEnabled: boolean;
  planModeEnabled: boolean;
}
