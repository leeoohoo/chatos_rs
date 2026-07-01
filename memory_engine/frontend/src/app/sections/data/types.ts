// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';

import type {
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
  selectedThread: EngineThread | null;
  onSelectThread: (thread: EngineThread) => void;
  threadDetailLoading: boolean;
  threadRecordsLoading: boolean;
  threadRecords: EngineRecord[];
  threadRecordPage: number;
  threadRecordPageSize: number;
  threadRecordTotal: number;
  onThreadRecordPageChange: (page: number, pageSize: number) => void;
  threadSummaries: EngineSummary[];
  subjectMemories: EngineSubjectMemory[];
  detailTab: DataDetailTab;
  onDetailTabChange: (detailTab: DataDetailTab) => void;
};

export type DataSectionProps = DataFiltersCardProps & ThreadWorkspaceProps;
