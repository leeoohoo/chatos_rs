// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useEffect, useMemo, useRef, useState, type FC } from 'react';
import { CheckCircle2, LoaderCircle, Play, RefreshCw, X } from 'lucide-react';
import type { Message } from '../../types';
import { useI18n } from '../../i18n/I18nProvider';
import { useApiClient } from '../../lib/api/ApiClientContext';
import { cn } from '../../lib/utils';
import { MessageTaskDetailModal, MessageTaskProcessLogModal } from './MessageTaskDetailModal';
import { MessageTaskChangesModal } from './MessageTaskChangesModal';
import { MessageTaskGraphPanel } from './MessageTaskGraphPanel';
import { MessageTaskRunDetailModal } from './MessageTaskRunDetailModal';
import { formatDateTime, readString } from './utils';
import { useMessageTaskGraph } from './useMessageTaskGraph';
import { resolveProjectExecutionConfirmationState } from './projectExecutionConfirmation';

export {
  resolveProjectExecutionConfirmationState,
  type ProjectExecutionConfirmationState,
} from './projectExecutionConfirmation';

interface MessageTaskDrawerProps {
  open: boolean;
  message: Message;
  onClose: () => void;
}

const MESSAGE_TASK_DRAWER_WIDTH_KEY = 'message_task_drawer_width';
const MESSAGE_TASK_DRAWER_DEFAULT_WIDTH = 760;

const getDrawerWidthBounds = () => {
  if (typeof window === 'undefined') {
    return {
      minWidth: 460,
      maxWidth: 1120,
    };
  }
  const maxWidth = Math.max(360, Math.min(1120, window.innerWidth - 40));
  const minWidth = Math.min(460, maxWidth);
  return { minWidth, maxWidth };
};

const clampDrawerWidth = (value: number): number => {
  const { minWidth, maxWidth } = getDrawerWidthBounds();
  return Math.min(Math.max(value, minWidth), maxWidth);
};

const readInitialDrawerWidth = (): number => {
  if (typeof window === 'undefined') {
    return clampDrawerWidth(MESSAGE_TASK_DRAWER_DEFAULT_WIDTH);
  }
  const saved = Number(window.localStorage.getItem(MESSAGE_TASK_DRAWER_WIDTH_KEY));
  if (Number.isFinite(saved) && saved > 0) {
    return clampDrawerWidth(saved);
  }
  return clampDrawerWidth(MESSAGE_TASK_DRAWER_DEFAULT_WIDTH);
};

export const MessageTaskDrawer: FC<MessageTaskDrawerProps> = ({
  open,
  message,
  onClose,
}) => {
  const { t } = useI18n();
  const apiClient = useApiClient();
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(0);
  const [drawerWidth, setDrawerWidth] = useState(readInitialDrawerWidth);
  const [isResizing, setIsResizing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [stoppingPlan, setStoppingPlan] = useState(false);
  const [planStopped, setPlanStopped] = useState(false);
  const [confirmationError, setConfirmationError] = useState<string | null>(null);
  const [confirmationMessage, setConfirmationMessage] = useState<string | null>(null);

  const taskLookup = useMemo(() => {
    const taskRunnerAsync = message.metadata?.task_runner_async;
    const rawSourceUserMessageId = readString(taskRunnerAsync?.source_user_message_id);
    const sourceUserMessageId = rawSourceUserMessageId?.startsWith('temp_')
      ? null
      : rawSourceUserMessageId;
    return {
      sessionId: message.sessionId,
      turnId: readString(message.metadata?.conversation_turn_id)
        || readString(taskRunnerAsync?.source_turn_id),
      sourceUserMessageId,
    };
  }, [message.metadata, message.sessionId]);

  const {
    graph,
    rootTasks,
    allTasks,
    sourceUserMessageId,
    loading,
    error,
    detailTask,
    processTask,
    processRunDetail,
    runDetail,
    changesTask,
    outputChanges,
    outputDiff,
    selectedChangePath,
    loadingProcessTaskId,
    loadingRunId,
    loadingChangesRunId,
    loadingDiffPath,
    retryingTaskId,
    retryError,
    reloadGraph,
    openDetail,
    openProcessLog,
    openRun,
    openChanges,
    retryTask,
    selectChangeFile,
    loadMoreRunEvents,
    closeDetail,
    closeProcessLog,
    closeRun,
    closeChanges,
  } = useMessageTaskGraph({
    open,
    messageId: message.id,
    lookup: taskLookup,
  });

  const role = message.role === 'user'
    ? t('message.role.user')
    : message.role === 'assistant'
      ? t('message.role.assistant')
      : message.role;
  const messageSummary = `${role} · ${formatDateTime(message.createdAt.toISOString())}`;
  const dependencyTaskCount = Math.max(allTasks.length - rootTasks.length, 0);
  const confirmationState = useMemo(
    () => resolveProjectExecutionConfirmationState({ graph, message, tasks: allTasks }),
    [allTasks, graph, message],
  );

  const confirmExecution = async () => {
    if (
      !confirmationState.canConfirm
      || !confirmationState.projectId
      || !confirmationState.requirementId
      || !confirmationState.executionGroupId
    ) {
      setConfirmationError('执行任务图尚未完整生成，当前不能启动执行');
      return;
    }
    setConfirming(true);
    setConfirmationError(null);
    setConfirmationMessage(null);
    try {
      await apiClient.confirmProjectRequirementExecution(
        confirmationState.projectId,
        confirmationState.requirementId,
        {
          execution_group_id: confirmationState.executionGroupId,
          conversation_id: confirmationState.conversationId,
          ...(confirmationState.contactId ? { contact_id: confirmationState.contactId } : {}),
        },
      );
      setConfirmationMessage('已确认执行，任务正在按流程图从起始节点开始运行。');
      await reloadGraph();
    } catch (err) {
      setConfirmationError(err instanceof Error ? err.message : '确认执行失败');
    } finally {
      setConfirming(false);
    }
  };

  const stopExecutionPlan = async () => {
    if (
      !confirmationState.projectId
      || !confirmationState.requirementId
      || !confirmationState.executionGroupId
      || !confirmationState.conversationId
    ) {
      setConfirmationError('当前消息缺少完整的执行计划标识，无法安全地放弃计划');
      return;
    }
    setStoppingPlan(true);
    setConfirmationError(null);
    try {
      await apiClient.stopProjectRequirementExecution(
        confirmationState.projectId,
        confirmationState.requirementId,
        {
          execution_group_id: confirmationState.executionGroupId,
          conversation_id: confirmationState.conversationId,
          ...(confirmationState.contactId ? { contact_id: confirmationState.contactId } : {}),
        },
      );
      setPlanStopped(true);
      setConfirmationMessage(null);
      await reloadGraph();
    } catch (err) {
      setConfirmationError(err instanceof Error ? err.message : '放弃执行计划失败');
    } finally {
      setStoppingPlan(false);
    }
  };

  useEffect(() => {
    if (!isResizing) {
      return undefined;
    }
    const handleMove = (event: MouseEvent) => {
      const delta = resizeStartX.current - event.clientX;
      setDrawerWidth(clampDrawerWidth(resizeStartWidth.current + delta));
    };
    const handleUp = () => {
      setIsResizing(false);
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }
    window.localStorage.setItem(MESSAGE_TASK_DRAWER_WIDTH_KEY, String(drawerWidth));
  }, [drawerWidth]);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return undefined;
    }
    const handleResize = () => {
      setDrawerWidth((current) => clampDrawerWidth(current));
    };
    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, []);

  useEffect(() => {
    setConfirming(false);
    setStoppingPlan(false);
    setPlanStopped(false);
    setConfirmationError(null);
    setConfirmationMessage(null);
  }, [message.id, open]);

  if (!open) {
    return null;
  }

  return (
    <>
      <div
        className={cn(
          'w-1.5 shrink-0 cursor-col-resize bg-border/60 transition-colors hover:bg-primary/35',
          isResizing && 'bg-primary/45',
        )}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = drawerWidth;
          setIsResizing(true);
        }}
        aria-hidden
      />
      <aside
        className="h-full shrink-0 border-l border-border bg-card shadow-xl"
        style={{
          width: drawerWidth,
          minWidth: drawerWidth,
          maxWidth: drawerWidth,
        }}
      >
        <div className="flex h-full flex-col">
          <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
            <div className="min-w-0">
              <h2 className="text-sm font-semibold text-foreground">任务流程图</h2>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">{messageSummary}</p>
              <p className="mt-0.5 truncate text-xs text-muted-foreground">
                源消息：{sourceUserMessageId || message.id}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
                disabled={loading}
                onClick={() => void reloadGraph()}
                aria-label="刷新任务"
              >
                <RefreshCw className={cn('h-4 w-4', loading && 'animate-spin')} />
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={onClose}
                aria-label="关闭"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
          </div>

          <div className="flex min-h-0 flex-1 flex-col px-4 py-4">
            {confirmationState.isProjectExecution
              && confirmationState.awaitingConfirmation
              && !confirmationState.hasStartedTasks
              && !confirmationMessage
              && !planStopped ? (
              <div className="mb-3 rounded-md border border-amber-300 bg-amber-50 px-3 py-3 text-amber-900 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-100">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <div className="text-sm font-semibold">执行计划已完整生成，尚未开始执行</div>
                    <p className="mt-1 text-xs leading-5 text-amber-800 dark:text-amber-200">
                      请先检查下面的完整任务依赖图。只有点击“确认执行”后，任务才会从起始节点开始运行。
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-2">
                    <button
                      type="button"
                      className="rounded-md border border-amber-400 bg-background px-3 py-2 text-xs font-medium text-amber-800 hover:bg-amber-100 disabled:cursor-wait disabled:opacity-60 dark:text-amber-200"
                      disabled={confirming || stoppingPlan}
                      onClick={() => void stopExecutionPlan()}
                    >
                      {stoppingPlan ? '放弃中' : '放弃计划'}
                    </button>
                    <button
                      type="button"
                      className="inline-flex items-center gap-1.5 rounded-md border border-amber-500/60 bg-amber-600 px-3 py-2 text-xs font-semibold text-white hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-60"
                      disabled={confirming || stoppingPlan || !confirmationState.canConfirm}
                      onClick={() => void confirmExecution()}
                      title={confirmationState.graphReadyForConfirmation
                        ? '确认后开始执行完整任务图'
                        : '任务图状态尚未达到可执行条件'}
                    >
                      {confirming ? (
                        <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Play className="h-3.5 w-3.5" />
                      )}
                      {confirming ? '启动中' : '确认执行'}
                    </button>
                  </div>
                </div>
              </div>
            ) : confirmationState.isProjectExecution
              && ['planning', 'pending', 'processing'].includes(confirmationState.overallStatus) ? (
                <div className="mb-3 flex items-start gap-2 rounded-md border border-sky-200 bg-sky-50 px-3 py-2 text-sm text-sky-800 dark:border-sky-800 dark:bg-sky-950/30 dark:text-sky-200">
                  <LoaderCircle className="mt-0.5 h-4 w-4 shrink-0 animate-spin" />
                  <span>正在生成完整执行任务图。生成完成前不会启动任务。</span>
                </div>
              ) : confirmationState.isProjectExecution
                && (planStopped || confirmationState.overallStatus === 'stopped') ? (
                  <div className="mb-3 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
                    执行计划已放弃，任务未启动。你可以回到需求页面重新生成。
                  </div>
                ) : confirmationState.isProjectExecution
                && ['failed', 'error', 'blocked', 'cancelled', 'canceled']
                  .includes(confirmationState.overallStatus) ? (
                  <div className="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-200">
                    <span>{confirmationState.overallStatus === 'blocked'
                      ? '当前执行批次存在阻塞任务，请查看任务详情并处理后重试。'
                      : confirmationState.hasStartedTasks
                        ? '当前执行批次存在失败或取消任务，请查看任务详情。'
                        : '执行计划没有完整生成，任务未启动。'}</span>
                    <button
                      type="button"
                      className="rounded-md border border-red-300 bg-background px-2.5 py-1.5 text-xs font-medium hover:bg-red-100 disabled:cursor-wait disabled:opacity-60"
                      disabled={stoppingPlan}
                      onClick={() => void stopExecutionPlan()}
                    >
                      {stoppingPlan ? '清理中' : '清理失败计划'}
                    </button>
                  </div>
                ) : confirmationState.isProjectExecution
                && (confirmationState.hasStartedTasks || confirmationMessage) ? (
                  <div className="mb-3 flex items-start gap-2 rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-800 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-200">
                    <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" />
                    <span>{confirmationMessage || '执行计划已确认，任务正在按依赖顺序运行。'}</span>
                  </div>
                ) : null}

            {confirmationError ? (
              <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {confirmationError}
              </div>
            ) : null}

            {error ? (
              <div className="mb-3 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700">
                {error}
              </div>
            ) : null}

            <div className="mb-4 grid grid-cols-3 gap-3">
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">当前消息任务</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{rootTasks.length}</div>
              </div>
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">已展开前置任务</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{dependencyTaskCount}</div>
              </div>
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="text-[11px] text-muted-foreground">依赖连线</div>
                <div className="mt-1 text-lg font-semibold text-foreground">{graph.edges.length}</div>
              </div>
            </div>

            <p className="mb-4 text-xs leading-5 text-muted-foreground">
              这里会把当前消息直接关联的任务和它们的前置依赖一起展开成 DAG。节点上的
              <span className="font-medium text-foreground">执行过程</span>
              可直接查看过程记录，按钮也可查看
              <span className="font-medium text-foreground">详情</span>
              或
              <span className="font-medium text-foreground">运行详情</span>
              。
            </p>

            <div className="min-h-0 flex-1">
              <MessageTaskGraphPanel
                graph={graph}
                loading={loading}
                error={error}
                loadingRunId={loadingRunId}
                loadingChangesRunId={loadingChangesRunId}
                loadingProcessTaskId={loadingProcessTaskId}
                panelWidth={drawerWidth}
                onOpenDetail={openDetail}
                onOpenProcessLog={openProcessLog}
                onOpenRun={openRun}
                onOpenChanges={openChanges}
              />
            </div>
          </div>
        </div>
      </aside>

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
        messageId={message.id}
        taskLookup={taskLookup}
        loadingMoreEvents={Boolean(runDetail && loadingRunId === runDetail.run?.id)}
        onLoadMoreEvents={loadMoreRunEvents}
        onClose={closeRun}
      />
      <MessageTaskChangesModal
        task={changesTask}
        changes={outputChanges}
        diff={outputDiff}
        selectedPath={selectedChangePath}
        loadingChanges={Boolean(changesTask?.last_run_id && loadingChangesRunId === changesTask.last_run_id)}
        loadingDiff={Boolean(selectedChangePath && loadingDiffPath === selectedChangePath)}
        error={error}
        onSelectFile={selectChangeFile}
        onClose={closeChanges}
      />
    </>
  );
};
