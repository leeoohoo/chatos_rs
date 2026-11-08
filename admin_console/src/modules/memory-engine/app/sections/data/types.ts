// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';

import type {
  AgentDirectoryQuery,
  EngineAgentConversation,
  EngineAgentDirectoryItem,
  EngineAgentProject,
  EngineRecord,
  EngineSubjectMemory,
  EngineSummary,
  EngineThread,
  ThreadQuery,
} from '../../../types';
import type { ThreadFilterFormValues } from '../../types';

export type DataDetailTab = 'records' | 'summaries' | 'memories';

export type UserLabelMap = Record<
  string,
  {
    username: string;
    display_name: string;
  }
>;

export type DataFiltersCardProps = {
  form: FormInstance<ThreadFilterFormValues>;
  initialValues: ThreadQuery;
  threadsLoading: boolean;
  onApplyFilters: () => void;
  onResetFilters: () => void;
  onReload: () => void;
};

export type ThreadWorkspaceProps = {
  threads: EngineThread[];
  tenantLabelsById?: UserLabelMap;
  threadsLoading: boolean;
  threadPage: number;
  threadPageSize: number;
  threadHasMore: boolean;
  threadMaxReachablePage: number;
  onThreadPageChange: (page: number, pageSize: number) => void;
  selectedThread: EngineThread | null;
  onSelectThread: (thread: EngineThread) => void;
  threadDetailLoading: boolean;
  threadRecordsLoading: boolean;
  threadRecords: EngineRecord[];
  threadRecordPage: number;
  threadRecordPageSize: number;
  threadRecordTotal: number;
  threadRecordMaxReachablePage: number;
  onThreadRecordPageChange: (page: number, pageSize: number) => void;
  threadSummaries: EngineSummary[];
  threadSummaryPage: number;
  threadSummaryPageSize: number;
  threadSummaryHasMore: boolean;
  threadSummaryMaxReachablePage: number;
  onThreadSummaryPageChange: (page: number, pageSize: number) => void;
  subjectMemories: EngineSubjectMemory[];
  detailTab: DataDetailTab;
  onDetailTabChange: (detailTab: DataDetailTab) => void;
  showMemories?: boolean;
};

export type AgentFiltersCardProps = {
  form: FormInstance<AgentDirectoryQuery>;
  initialValues: AgentDirectoryQuery;
  loading: boolean;
  onApplyFilters: () => void;
  onResetFilters: () => void;
  onReload: () => void;
};

export type AgentWorkspaceProps = ThreadWorkspaceProps & {
  agents: EngineAgentDirectoryItem[];
  agentsLoading: boolean;
  selectedAgent: EngineAgentDirectoryItem | null;
  onSelectAgent: (agent: EngineAgentDirectoryItem) => void;
  projects: EngineAgentProject[];
  conversations: EngineAgentConversation[];
  selectedConversation: EngineAgentConversation | null;
  onSelectConversation: (conversation: EngineAgentConversation) => void;
  selectedProject: EngineAgentProject | null;
  onSelectProject: (project: EngineAgentProject) => void;
  agentMemories: EngineSubjectMemory[];
  projectMemories: EngineSubjectMemory[];
  agentDetailLoading: boolean;
  projectMemoriesLoading: boolean;
};

export type DataSectionProps = AgentFiltersCardProps & AgentWorkspaceProps;
