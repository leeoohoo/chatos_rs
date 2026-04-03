export type TaskWorkbarStatus =
  | 'pending_confirm'
  | 'pending_execute'
  | 'running'
  | 'completed'
  | 'failed'
  | 'cancelled';

export interface TaskWorkbarContextAsset {
  assetType: string;
  assetId: string;
  displayName?: string | null;
  sourceType?: string | null;
  sourcePath?: string | null;
}

export interface TaskWorkbarExecutionResultContract {
  resultRequired: boolean;
  preferredFormat?: string | null;
}

export interface TaskWorkbarPlanningSnapshot {
  contactAuthorizedBuiltinMcpIds: string[];
  selectedModelConfigId?: string | null;
  sourceUserGoalSummary?: string | null;
  sourceConstraintsSummary?: string | null;
  plannedAt?: string | null;
}

export interface TaskWorkbarResultBrief {
  taskId: string;
  taskStatus?: string | null;
  resultSummary: string;
  resultFormat?: string | null;
  resultMessageId?: string | null;
  sourceSessionId?: string | null;
  sourceTurnId?: string | null;
  finishedAt?: string | null;
  updatedAt?: string | null;
}

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
  plannedBuiltinMcpIds?: string[];
  plannedContextAssets?: TaskWorkbarContextAsset[];
  projectRoot?: string | null;
  remoteConnectionId?: string | null;
  executionResultContract?: TaskWorkbarExecutionResultContract | null;
  planningSnapshot?: TaskWorkbarPlanningSnapshot | null;
  taskResultBrief?: TaskWorkbarResultBrief | null;
  resultSummary?: string | null;
  lastError?: string | null;
  confirmedAt?: string | null;
  startedAt?: string | null;
  finishedAt?: string | null;
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
