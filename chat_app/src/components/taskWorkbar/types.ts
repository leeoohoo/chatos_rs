export type TaskWorkbarStatus =
  | 'pending_confirm'
  | 'pending_execute'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface TaskWorkbarItem {
  id: string;
  title: string;
  details: string;
  status: TaskWorkbarStatus;
  priority: 'high' | 'medium' | 'low';
  conversationTurnId: string;
  createdAt: string;
  dueAt?: string | null;
  tags: string[];
}

export interface SessionSummaryWorkbarItem {
  id: string;
  summaryText: string;
  summaryModel: string;
  triggerType: string;
  sourceMessageCount: number;
  sourceEstimatedTokens: number;
  createdAt: string;
  status?: string;
  errorMessage?: string | null;
}

export interface RuntimeGuidanceWorkbarItem {
  guidanceId: string;
  turnId: string | null;
  content: string;
  status: 'queued' | 'applied' | 'dropped';
  createdAt: string;
  appliedAt: string | null;
}

export type HistoryFilter = 'all' | 'processed';
