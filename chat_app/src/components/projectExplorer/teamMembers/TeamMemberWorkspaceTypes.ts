import type { SessionSummaryItem } from '../../../features/sessionSummary/useSessionSummaryPanel';
import type {
  SendMessageRuntimeOptions,
  TaskReviewDraft,
  TaskReviewPanelState,
  UiPromptPanelState,
  UiPromptResponsePayload,
} from '../../../lib/store/types';
import type {
  AgentConfig,
  AiModelConfig,
  Message,
  Project,
  RemoteConnection,
  Session,
} from '../../../types';
import type { RuntimeGuidanceWorkbarItem, TaskWorkbarItem } from '../../TaskWorkbar';
import type { TaskOutcomeDraft } from '../../taskWorkbar/TaskOutcomeModal';
import type { ContactItem } from './types';

export interface TeamMemberWorkspaceProps {
  project: Project;
  selectedContact: ContactItem | null;
  currentAgent: AgentConfig | null;
  selectedProjectSession: Session | null;
  isSelectedSessionActive: boolean;
  sessionSummaryPaneVisible: boolean;
  summaryItems: SessionSummaryItem[];
  summaryLoading: boolean;
  summaryError: string | null;
  clearingSummaries: boolean;
  deletingSummaryId: string | null;
  messages: Message[];
  hasMoreMessages: boolean;
  chatIsLoading: boolean;
  chatIsStreaming: boolean;
  chatIsStopping: boolean;
  selectedModelId: string | null;
  aiModelConfigs: AiModelConfig[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  mcpEnabled: boolean;
  enabledMcpIds: string[];
  availableRemoteConnections: RemoteConnection[];
  currentRemoteConnectionId: string | null;
  onRemoteConnectionChange: (connectionId: string | null) => void;
  onLoadMore: () => void;
  onToggleTurnProcess: (userMessageId: string) => void;
  onClearSummaries: () => void;
  onRefreshSummaries: () => void;
  onCloseSummary: () => void;
  onDeleteSummary: (summaryId: string) => void;
  onSend: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => void | Promise<void>;
  onGuide: (content: string) => void | Promise<void>;
  onStop: () => void;
  onModelChange: (modelId: string | null) => void;
  onReasoningToggle: (enabled: boolean) => void;
  onMcpEnabledChange: (enabled: boolean) => void;
  onEnabledMcpIdsChange: (ids: string[]) => void;
  mergedCurrentTurnTasks: TaskWorkbarItem[];
  workbarHistoryTasks: TaskWorkbarItem[];
  taskHistoryOpen?: boolean;
  activeConversationTurnId: string | null;
  workbarLoading: boolean;
  workbarHistoryLoading: boolean;
  workbarError: string | null;
  workbarHistoryError: string | null;
  workbarActionLoadingTaskId: string | null;
  taskModalOpen: boolean;
  taskModalMode: 'complete' | 'edit';
  taskModalTask: TaskWorkbarItem | null;
  taskModalError: string | null;
  reviewRepairRunning: boolean;
  reviewRepairPendingCount: number | null;
  onRefreshWorkbarTasks: () => void;
  onOpenWorkbarHistory: (sessionId: string) => void;
  onTaskHistoryOpenChange?: (value: boolean) => void;
  onRunReviewRepair: (sessionId: string) => Promise<void>;
  onCompleteTask: (task: TaskWorkbarItem) => void;
  onDeleteTask: (task: TaskWorkbarItem) => void;
  onEditTask: (task: TaskWorkbarItem) => void;
  onCloseTaskModal: () => void;
  onSubmitTaskModal: (draft: TaskOutcomeDraft) => void;
  activeUiPromptPanel: UiPromptPanelState | null;
  onUiPromptSubmit: (payload: UiPromptResponsePayload) => void;
  onUiPromptCancel: () => void;
  activeTaskReviewPanel: TaskReviewPanelState | null;
  onTaskReviewConfirm: (payload: TaskReviewDraft[]) => void;
  onTaskReviewCancel: () => void;
  runtimeGuidancePendingCount?: number;
  runtimeGuidanceAppliedCount?: number;
  runtimeGuidanceLastAppliedAt?: string | null;
  runtimeGuidanceItems?: RuntimeGuidanceWorkbarItem[];
}
