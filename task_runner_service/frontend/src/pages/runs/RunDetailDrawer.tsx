import {
  Drawer,
  Empty,
  Space,
} from 'antd';

import type { TranslateFn } from '../../i18n/I18nProvider';
import type { RemoteOperationStats } from '../shared/remoteOperationUtils';
import type {
  TaskRunEventRecord,
  TaskRunRecord,
  TaskSummaryRecord,
  UiPromptRecord,
  UiPromptStatus,
} from '../../types';
import { JsonBlock } from './payloadView';
import type {
  RemoteOperationView,
  ToolCallView,
  ToolResultView,
} from './runEventUtils';
import { RunDetailSummary } from './RunDetailSummary';
import { RunEventsTimeline } from './RunEventsTimeline';
import { RunModelRequestsSection } from './RunModelRequestsSection';
import { RunPromptsSection } from './RunPromptsSection';
import { RunRemoteOperationsSection } from './RunRemoteOperationsSection';
import {
  RunToolCallsSection,
  RunToolResultsSection,
} from './RunToolSections';

type RunStreamStats = {
  chunkCount: number;
  chunkChars: number;
  thinkingCount: number;
  thinkingChars: number;
};

type RunPromptsPage = {
  items: UiPromptRecord[];
  total: number;
};

type RunDetailDrawerProps = {
  t: TranslateFn;
  open: boolean;
  loading: boolean;
  run: TaskRunRecord | null;
  taskMap: Map<string, TaskSummaryRecord>;
  modelNameMap: Map<string, string>;
  toolCalls: ToolCallView[];
  toolResults: ToolResultView[];
  modelRequests: TaskRunEventRecord[];
  streamStats: RunStreamStats;
  remoteOperations: RemoteOperationView[];
  remoteOperationStats: RemoteOperationStats;
  promptsPage?: RunPromptsPage;
  promptsLoading: boolean;
  promptPage: number;
  promptPageSize: number;
  events: TaskRunEventRecord[];
  eventsLoading: boolean;
  canceling: boolean;
  retrying: boolean;
  onClose: () => void;
  onOpenTask: (taskId: string) => void;
  onOpenModel: (modelConfigId: string) => void;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
  onManageServers: () => void;
  onOpenServer: (serverId: string) => void;
  onOpenPrompt: (promptId: string, runId: string) => void;
  onPromptPageChange: (page: number, pageSize: number) => void;
};

export function RunDetailDrawer({
  t,
  open,
  loading,
  run,
  taskMap,
  modelNameMap,
  toolCalls,
  toolResults,
  modelRequests,
  streamStats,
  remoteOperations,
  remoteOperationStats,
  promptsPage,
  promptsLoading,
  promptPage,
  promptPageSize,
  events,
  eventsLoading,
  canceling,
  retrying,
  onClose,
  onOpenTask,
  onOpenModel,
  onCancel,
  onRetry,
  onManageServers,
  onOpenServer,
  onOpenPrompt,
  onPromptPageChange,
}: RunDetailDrawerProps) {
  const promptStatusLabel = (status: UiPromptStatus) => t(`prompts.status.${status}`);

  return (
    <Drawer
      title={t('runs.detail.title')}
      open={open}
      width={760}
      onClose={onClose}
    >
      {run ? (
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <RunDetailSummary
            t={t}
            run={run}
            taskTitle={taskMap.get(run.task_id)?.title || run.task_id}
            modelName={modelNameMap.get(run.model_config_id) || run.model_config_id}
            toolCallCount={toolCalls.length}
            toolResultCount={toolResults.length}
            modelRequestCount={modelRequests.length}
            streamStats={streamStats}
            canceling={canceling}
            retrying={retrying}
            onOpenTask={onOpenTask}
            onOpenModel={onOpenModel}
            onCancel={onCancel}
            onRetry={onRetry}
          />

          <RunRemoteOperationsSection
            t={t}
            operations={remoteOperations}
            stats={remoteOperationStats}
            onManageServers={onManageServers}
            onOpenServer={onOpenServer}
          />

          <RunToolCallsSection t={t} toolCalls={toolCalls} />
          <RunToolResultsSection t={t} toolResults={toolResults} />
          <RunModelRequestsSection t={t} modelRequests={modelRequests} />

          <JsonBlock title={t('runs.snapshot.input')} value={run.input_snapshot} t={t} />
          <JsonBlock
            title={t('runs.snapshot.context')}
            value={run.context_snapshot}
            collapsible
            defaultOpen={false}
            t={t}
          />
          <JsonBlock title={t('runs.snapshot.usage')} value={run.usage} t={t} />
          <JsonBlock title={t('runs.snapshot.report')} value={run.report} t={t} />

          <RunPromptsSection
            t={t}
            prompts={promptsPage?.items || []}
            loading={promptsLoading}
            page={promptPage}
            pageSize={promptPageSize}
            total={promptsPage?.total || 0}
            promptStatusLabel={promptStatusLabel}
            onOpenPrompt={(promptId) => onOpenPrompt(promptId, run.id)}
            onPageChange={onPromptPageChange}
          />

          <RunEventsTimeline
            t={t}
            events={events}
            loading={eventsLoading}
          />
        </Space>
      ) : loading ? null : (
        <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} />
      )}
    </Drawer>
  );
}
