// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import {
  MessageTaskDetailModal,
  MessageTaskProcessLogModal,
} from '../../messageTasks/MessageTaskDetailModal';
import { MessageTaskRunDetailModal } from '../../messageTasks/MessageTaskRunDetailModal';
import { useMessageTaskGraph } from '../../messageTasks/useMessageTaskGraph';

export const RequirementExecutionTaskModals: React.FC<{
  taskGraph: ReturnType<typeof useMessageTaskGraph>;
}> = ({ taskGraph }) => {
  const {
    allTasks,
    closeDetail,
    closeProcessLog,
    closeRun,
    detailTask,
    loadMoreRunEvents,
    loadingRunId,
    processRunDetail,
    processTask,
    refreshRunDetail,
    refreshingRunDetail,
    retryError,
    retryTask,
    retryTaskIntegration,
    retryingTaskId,
    runDetail,
    waiveTaskIntegration,
  } = taskGraph;

  return (
    <>
      <MessageTaskDetailModal
        task={detailTask}
        relatedTasks={allTasks}
        retrying={Boolean(retryingTaskId)}
        retryError={retryError}
        onRetry={retryTask}
        onRetryIntegration={retryTaskIntegration}
        onWaiveIntegration={waiveTaskIntegration}
        onClose={closeDetail}
      />
      <MessageTaskProcessLogModal
        task={processTask}
        runDetail={processRunDetail}
        onClose={closeProcessLog}
      />
      <MessageTaskRunDetailModal
        detail={runDetail}
        loadingMoreEvents={Boolean(runDetail && loadingRunId === runDetail.run?.id)}
        refreshing={refreshingRunDetail}
        onLoadMoreEvents={loadMoreRunEvents}
        onRefresh={() => void refreshRunDetail(false)}
        onClose={closeRun}
      />
    </>
  );
};
