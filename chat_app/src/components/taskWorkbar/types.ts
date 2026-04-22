export interface TaskWorkbarItem {
  id: string;
  title: string;
  details: string;
  status: 'todo' | 'doing' | 'blocked' | 'done';
  priority: 'high' | 'medium' | 'low';
  conversationTurnId: string;
  createdAt: string;
  dueAt?: string | null;
  tags: string[];
  outcomeSummary: string;
  outcomeItems: Array<{
    kind: string;
    text: string;
    importance?: 'high' | 'medium' | 'low';
    refs: string[];
  }>;
  resumeHint: string;
  blockerReason: string;
  blockerNeeds: string[];
  blockerKind: string;
  completedAt?: string | null;
  lastOutcomeAt?: string | null;
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
