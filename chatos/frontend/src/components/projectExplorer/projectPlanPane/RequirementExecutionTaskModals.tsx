// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { MessageTaskChangesModal } from '../../messageTasks/MessageTaskChangesModal';
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
    changesTask,
    closeChanges,
    closeDetail,
    closeProcessLog,
    closeRun,
    detailTask,
    error: graphError,
    loadMoreRunEvents,
    loadingChangesRunId,
    loadingDiffPath,
    loadingRunId,
    outputChanges,
    outputDiff,
    processRunDetail,
    processTask,
    retryError,
    retryTask,
    retryingTaskId,
    runDetail,
    selectChangeFile,
    selectedChangePath,
  } = taskGraph;

  return (
    <>
      <MessageTaskDetailModal
        task={detailTask}
        relatedTasks={allTasks}
        retrying={Boolean(retryingTaskId)}
        retryError={retryError}
        onRetry={retryTask}
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
        onLoadMoreEvents={loadMoreRunEvents}
        onClose={closeRun}
      />
      <MessageTaskChangesModal
        task={changesTask}
        changes={outputChanges}
        diff={outputDiff}
        selectedPath={selectedChangePath}
        loadingChanges={Boolean(
          changesTask?.last_run_id && loadingChangesRunId === changesTask.last_run_id
        )}
        loadingDiff={Boolean(selectedChangePath && loadingDiffPath === selectedChangePath)}
        error={graphError}
        onSelectFile={selectChangeFile}
        onClose={closeChanges}
      />
    </>
  );
};
