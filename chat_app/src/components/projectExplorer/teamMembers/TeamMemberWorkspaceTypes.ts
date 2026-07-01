// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { SessionSummaryItem } from '../../../features/sessionSummary/useSessionSummaryPanel';
import type { SendMessageRuntimeOptions } from '../../../lib/store/types';
import type {
  AiModelConfig,
  Message,
  Project,
  RemoteConnection,
  Session,
} from '../../../types';
import type { ContactItem } from './types';

export interface TeamMemberWorkspaceProps {
  project: Project;
  selectedContact: ContactItem | null;
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
  anchorMessageId?: string | null;
  anchorRequestKey?: number;
  onAnchorClear?: () => void;
  selectedModelId: string | null;
  selectedModelName?: string | null;
  selectedThinkingLevel?: string | null;
  aiModelConfigs: AiModelConfig[];
  supportsReasoning: boolean;
  reasoningEnabled: boolean;
  planModeEnabled: boolean;
  availableRemoteConnections: RemoteConnection[];
  currentRemoteConnectionId: string | null;
  onRemoteConnectionChange: (connectionId: string | null) => void;
  onLoadMore: () => void | Promise<void>;
  onClearSummaries: () => void;
  onRefreshSummaries: () => void;
  onCloseSummary: () => void;
  onDeleteSummary: (summaryId: string) => void;
  onSend: (
    content: string,
    attachments?: File[],
    runtimeOptions?: SendMessageRuntimeOptions,
  ) => void | Promise<void>;
  onModelChange: (modelId: string | null) => void;
  onModelNameChange?: (modelName: string | null) => void;
  onThinkingLevelChange?: (level: string | null) => void;
  onModelRuntimeChange?: (selection: {
    selectedModelId?: string | null;
    selectedModelName?: string | null;
    selectedThinkingLevel?: string | null;
  }) => void;
  onReasoningToggle: (enabled: boolean) => void;
  onPlanModeToggle: (enabled: boolean) => void;
}
