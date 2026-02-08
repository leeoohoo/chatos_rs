// Mock database for browser environment
export interface Session {
  id: string;
  title: string;
  createdAt: Date;
  updatedAt: Date;
  messageCount: number;
  tokenUsage: number;
  tags?: string | null;
  pinned: boolean;
  archived: boolean;
  metadata?: string | null;
}

export interface Message {
  id: string;
  sessionId: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  rawContent?: string;
  tokensUsed?: number;
  status: 'pending' | 'streaming' | 'completed' | 'error';
  createdAt: Date;
  updatedAt?: Date;
  toolCallId?: string;
  metadata?: {
    attachments?: any[];
    toolCalls?: any[];
    model?: string;
    [key: string]: any;
  };
}

// In-memory storage for development
let sessions: Session[] = [
  {
    id: '1',
    title: 'Welcome Chat',
    createdAt: new Date(),
    updatedAt: new Date(),
    messageCount: 1,
    tokenUsage: 0,
    tags: null,
    pinned: false,
    archived: false,
    metadata: null,
  },
];

let messages: Message[] = [
  {
    id: '1',
    sessionId: '1',
    role: 'assistant',
    content: 'Hello! How can I help you today?',
    createdAt: new Date(),
    rawContent: undefined,
    tokensUsed: undefined,
    updatedAt: undefined,
    metadata: undefined,
    status: 'completed',
  },
];

// Mock database operations
export const mockDb = {
  sessions: {
    findMany: () => Promise.resolve(sessions),
    findFirst: (where: { id: string }) => 
      Promise.resolve(sessions.find(s => s.id === where.id) || null),
    insert: (data: Omit<Session, 'id'>) => {
      const session: Session = {
        ...data,
        id: Date.now().toString(),
        messageCount: data.messageCount || 0,
        tokenUsage: data.tokenUsage || 0,
        tags: data.tags || null,
        pinned: data.pinned || false,
        archived: data.archived || false,
        metadata: data.metadata || null,
      };
      sessions.push(session);
      return Promise.resolve(session);
    },
    update: (where: { id: string }, data: Partial<Session>) => {
      const index = sessions.findIndex(s => s.id === where.id);
      if (index !== -1) {
        sessions[index] = { ...sessions[index], ...data };
        return Promise.resolve(sessions[index]);
      }
      return Promise.resolve(null);
    },
    delete: (where: { id: string }) => {
      const index = sessions.findIndex(s => s.id === where.id);
      if (index !== -1) {
        sessions.splice(index, 1);
        // Also delete related messages
        messages = messages.filter(m => m.sessionId !== where.id);
        return Promise.resolve(true);
      }
      return Promise.resolve(false);
    },
  },
  messages: {
    findMany: (where?: { sessionId?: string }) => {
      if (where?.sessionId) {
        return Promise.resolve(messages.filter(m => m.sessionId === where.sessionId));
      }
      return Promise.resolve(messages);
    },
    insert: (data: Omit<Message, 'id'>) => {
      const message: Message = {
        ...data,
        id: Date.now().toString(),
        rawContent: data.rawContent,
        tokensUsed: data.tokensUsed,
        updatedAt: data.updatedAt,
        metadata: data.metadata,
        status: data.status || 'completed',
      };
      messages.push(message);
      return Promise.resolve(message);
    },
    update: (id: string, updates: Partial<Message>) => {
      const index = messages.findIndex(m => m.id === id);
      if (index !== -1) {
        messages[index] = { ...messages[index], ...updates };
        return Promise.resolve(messages[index]);
      }
      return Promise.resolve(null);
    },
    delete: (id: string) => {
      const index = messages.findIndex(m => m.id === id);
      if (index !== -1) {
        messages.splice(index, 1);
        return Promise.resolve(true);
      }
      return Promise.resolve(false);
    },
  },
};

// Export for compatibility
export const db = mockDb;