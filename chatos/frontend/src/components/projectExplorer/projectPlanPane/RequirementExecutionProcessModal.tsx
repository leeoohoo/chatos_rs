// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { RefreshCw } from 'lucide-react';

import type { MessageTaskRunnerTask } from '../../../lib/api/client/types';
import { useApiClient } from '../../../lib/api/ApiClientContext';
import { useChatStore } from '../../../lib/store';
import { cn } from '../../../lib/utils';
import type { Message } from '../../../types';
import { resolveProjectExecutionConfirmationState } from '../../messageTasks/projectExecutionConfirmation';
import { useMessageTaskGraph } from '../../messageTasks/useMessageTaskGraph';
import { readString } from '../../messageTasks/utils';
import {
  RequirementExecutionModalFrame,
} from './RequirementExecutionModalShell';
import { RequirementExecutionActionDialogs } from './RequirementExecutionActionDialogs';
import { RequirementExecutionTaskModals } from './RequirementExecutionTaskModals';
import {
  RequirementExecutionGraphSurface,
  RequirementExecutionProcessActions,
  RequirementExecutionProcessSidebar,
} from './RequirementExecutionProcessView';
import { readText } from './model';
import {
  buildRequirementExecutionProcessEntries,
  createFallbackMessage,
  isRequirementExecutionCancellationSettling,
  isStoppedExecutionStatus,
  resolveRequirementExecutionPhaseCopy,
  resolveRequirementExecutionProcessPhase,
  resolveRequirementExecutionRecoveryActions,
  withProcessStatus,
} from './requirementExecutionPhase';
import {
  buildRequirementExecutionProcess,
  isPendingRequirementExecutionPlanError,
  isRequirementExecutionRerunCancellationSettlingError,
  REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS,
  shouldReplaceRequirementExecutionBatch,
  shouldStopRequirementExecutionBeforeReplacement,
  type RequirementExecutionProcess,
} from './requirementExecutionProcessModel';
import {
  taskHasActiveRun,
  taskHasQueuedRun,
  taskHasRunningRun,
} from './requirementExecutionTaskRuntime';

export * from './requirementExecutionProcessPublic';

export const RequirementExecutionProcessModal: React.FC<{
  process: RequirementExecutionProcess;
  onClose: () => void;
  onProcessChange: (process: RequirementExecutionProcess) => void;
}> = ({ process, onClose, onProcessChange }) => {
  const apiClient = useApiClient();
  const refreshSessionById = useChatStore((state) => state.refreshSessionById);
  const syncSessionMessagesInBackground = useChatStore(
    (state) => state.syncSessionMessagesInBackground,
  );
  const [liveProcess, setLiveProcess] = useState(process);
  const [message, setMessage] = useState<Message>(
    withProcessStatus(process.initialMessage || createFallbackMessage(process), process),
  );
  const [feedback, setFeedback] = useState('');
  const [syncError, setSyncError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [pausing, setPausing] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [revising, setRevising] = useState(false);
  const [rerunning, setRerunning] = useState(false);
  const [rerunCancellationSettling, setRerunCancellationSettling] = useState(false);
  const [rerunConfirmOpen, setRerunConfirmOpen] = useState(false);
  const [failedTaskRetryOpen, setFailedTaskRetryOpen] = useState(false);
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [cancelConfirmOpen, setCancelConfirmOpen] = useState(false);
  const [planStopped, setPlanStopped] = useState(
    isStoppedExecutionStatus(process.serverStatus),
  );
  const [planDiscarded, setPlanDiscarded] = useState(Boolean(process.tasksDiscarded));
  const [executionConfirmed, setExecutionConfirmed] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [panelWidth, setPanelWidth] = useState(900);
  const graphContainerRef = useRef<HTMLDivElement>(null);
  const pollingRef = useRef(false);
  const rerunningRef = useRef(false);
  const activeExecutionGroupIdRef = useRef(process.executionGroupId);

  useEffect(() => {
    const stopped = isStoppedExecutionStatus(process.serverStatus);
    activeExecutionGroupIdRef.current = process.executionGroupId;
    setLiveProcess(process);
    setMessage(withProcessStatus(
      process.initialMessage || createFallbackMessage(process),
      process,
    ));
    setFeedback('');
    setPlanStopped(stopped);
    setPlanDiscarded(Boolean(process.tasksDiscarded));
    setExecutionConfirmed(Boolean(process.hasStartedRuns));
    setActionError(null);
    setActionMessage(null);
    setSyncError(null);
    setRerunCancellationSettling(false);
    setRerunConfirmOpen(false);
    setFailedTaskRetryOpen(false);
    setDiscardConfirmOpen(false);
    setCancelConfirmOpen(false);
  }, [process.executionGroupId]);

  const taskLookup = useMemo(() => ({
    sessionId: liveProcess.conversationId,
    turnId: liveProcess.executionGroupId,
    sourceUserMessageId: liveProcess.messageId,
  }), [liveProcess.conversationId, liveProcess.executionGroupId, liveProcess.messageId]);

  const taskGraph = useMessageTaskGraph({
    open: true,
    messageId: liveProcess.messageId,
    lookup: taskLookup,
    isTransientError: isPendingRequirementExecutionPlanError,
  });
  const {
    graph, allTasks, loading, error: graphError,
    loadingProcessTaskId, loadingRunId, loadingChangesRunId,
    retryingTaskId, reloadGraph,
    openDetail, openProcessLog, openRun, openChanges, retryTask,
  } = taskGraph;

  const confirmationState = useMemo(
    () => resolveProjectExecutionConfirmationState({ graph, message, tasks: allTasks }),
    [allTasks, graph, message],
  );
  const taskStatuses = useMemo(
    () => allTasks.map((task) => readString(task.status)?.toLowerCase() || ''),
    [allTasks],
  );
  const phase = useMemo(() => resolveRequirementExecutionProcessPhase({
    confirmationState,
    executionConfirmed,
    executionPaused: Boolean(liveProcess.executionPaused),
    failureDetected: false,
    hasStartedRuns: Boolean(liveProcess.hasStartedRuns),
    planStopped,
    serverStatus: liveProcess.serverStatus || '',
    taskStatuses,
  }), [
    confirmationState,
    executionConfirmed,
    liveProcess.executionPaused,
    liveProcess.hasStartedRuns,
    liveProcess.serverStatus,
    planStopped,
    taskStatuses,
  ]);
  const actuallyStarted = Boolean(
    liveProcess.hasStartedRuns || confirmationState.hasStartedTasks || executionConfirmed,
  );
  const hasActiveRuns = allTasks.some(taskHasActiveRun);
  const runningTaskCount = allTasks.filter(taskHasRunningRun).length;
  const queuedTaskCount = allTasks.filter(taskHasQueuedRun).length;
  const retryableFailedTasks = useMemo(() => allTasks.filter((task) => (
    readString(task.status)?.toLowerCase() === 'failed'
    && Boolean(readString(task.last_run_id))
  )), [allTasks]);
  const graphReady = confirmationState.graphReadyForConfirmation
    && allTasks.length > 0
    && !actuallyStarted
    && phase !== 'stopped';
  const recoveryActions = resolveRequirementExecutionRecoveryActions({
    actuallyStarted,
    hasActiveRuns,
    phase,
    recoveryAction: liveProcess.recoveryAction,
  });
  const graphCancellationSettling = isRequirementExecutionCancellationSettling({
    hasActiveRuns,
    phase,
  });
  const cancellationSettling = graphCancellationSettling || rerunCancellationSettling;
  const rerunBusy = rerunning || rerunCancellationSettling;
  const canRegenerate = recoveryActions.canRegenerate && !cancellationSettling;
  const canRevise = recoveryActions.canRevise;
  const canRerun = recoveryActions.canRerun
    && !cancellationSettling;
  const showRerunAction = canRerun || rerunBusy;
  const terminal = ['completed', 'failed', 'stopped'].includes(phase)
    && !hasActiveRuns
    && !rerunCancellationSettling;
  const isLocalExecution = (liveProcess.executionPlane || '').toLowerCase() === 'local_connector'
    || liveProcess.conversationId.startsWith('lc_');
  const phaseText = resolveRequirementExecutionPhaseCopy({
    cancellationSettling,
    phase,
    queuedTaskCount,
    runningTaskCount,
  });

  const processEntries = useMemo(() => buildRequirementExecutionProcessEntries({
    actuallyStarted,
    allTasks,
    cancellationSettling,
    graphReady,
    isLocalExecution,
    phase,
    process: liveProcess,
  }), [
    actuallyStarted,
    allTasks,
    cancellationSettling,
    graphReady,
    isLocalExecution,
    liveProcess,
    phase,
  ]);

  const refreshPlanStatus = useCallback(async (silent = false) => {
    if (!silent) setSyncing(true);
    setSyncError(null);
    const requestedExecutionGroupId = liveProcess.executionGroupId;
    try {
      const response = await apiClient.getProjectRequirementExecutionPlan(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          conversationId: liveProcess.conversationId,
          executionGroupId: liveProcess.executionGroupId,
        },
      );
      const next = buildRequirementExecutionProcess({
        fallback: liveProcess,
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (
        activeExecutionGroupIdRef.current !== requestedExecutionGroupId
      ) {
        return;
      }
      if (next) {
        setLiveProcess(next);
        setMessage(withProcessStatus(next.initialMessage || message, next));
      }
    } catch (err) {
      if (
        activeExecutionGroupIdRef.current === requestedExecutionGroupId
        && !isPendingRequirementExecutionPlanError(err)
      ) {
        setSyncError(err instanceof Error ? err.message : '读取规划批次状态失败');
      }
    } finally {
      if (!silent) setSyncing(false);
    }
  }, [apiClient, liveProcess, message]);

  const refreshAll = useCallback(async (silent = false) => {
    if (pollingRef.current) return;
    pollingRef.current = true;
    try {
      await Promise.all([
        reloadGraph(silent ? { silent: true } : undefined),
        refreshPlanStatus(silent),
      ]);
    } finally {
      pollingRef.current = false;
    }
  }, [refreshPlanStatus, reloadGraph]);

  const retryFailedTask = useCallback(async (task: MessageTaskRunnerTask) => {
    setActionError(null);
    setActionMessage(null);
    const retried = await retryTask(task);
    if (retried) {
      setActionMessage(liveProcess.executionPaused
        ? `任务“${task.title || task.id}”已重新进入暂停队列，将在继续执行后启动。`
        : `任务“${task.title || task.id}”已重新进入执行队列。`);
    }
  }, [
    liveProcess.executionPaused,
    retryTask,
  ]);

  useEffect(() => {
    if (
      failedTaskRetryOpen
      && retryableFailedTasks.length === 0
      && !retryingTaskId
    ) {
      setFailedTaskRetryOpen(false);
    }
  }, [failedTaskRetryOpen, retryableFailedTasks.length, retryingTaskId]);

  useEffect(() => {
    void refreshPlanStatus(true);
  }, [liveProcess.executionGroupId]);

  useEffect(() => {
    if (terminal) return undefined;
    const intervalId = window.setInterval(() => {
      void refreshAll(true);
    }, REQUIREMENT_EXECUTION_REFRESH_INTERVAL_MS);
    return () => window.clearInterval(intervalId);
  }, [refreshAll, terminal]);

  useEffect(() => {
    if (terminal) {
      setActionMessage(null);
    }
  }, [terminal]);

  useEffect(() => {
    if (
      rerunCancellationSettling
      && !graphCancellationSettling
      && !hasActiveRuns
      && !syncing
      && !loading
      && (liveProcess.serverStatus || '').trim().toLowerCase() !== 'stopping'
    ) {
      setRerunCancellationSettling(false);
      setActionMessage((current) => (
        current?.includes('旧批次') || current?.includes('取消')
          ? '旧批次取消状态已收敛，可以重新执行。'
          : current
      ));
    }
  }, [
    graphCancellationSettling,
    hasActiveRuns,
    liveProcess.serverStatus,
    loading,
    rerunCancellationSettling,
    syncing,
  ]);

  useEffect(() => {
    const element = graphContainerRef.current;
    if (!element) return undefined;
    const updateWidth = () => setPanelWidth(Math.max(360, element.clientWidth));
    updateWidth();
    if (typeof ResizeObserver === 'undefined') {
      window.addEventListener('resize', updateWidth);
      return () => window.removeEventListener('resize', updateWidth);
    }
    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const confirmExecution = async () => {
    if (!graphReady || !confirmationState.canConfirm) {
      setActionError('完整流程图尚未生成，或者当前批次已经存在运行记录');
      return;
    }
    setConfirming(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const result = await apiClient.confirmProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
        },
      );
      const startedRuns = result.started_runs || result.startedRuns || [];
      const next = {
        ...liveProcess,
        serverStatus: readText(result.status) || 'execution_started',
        confirmationStatus: 'confirmed',
        hasStartedRuns: startedRuns.length > 0 || liveProcess.hasStartedRuns,
        executionPaused: false,
      };
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      setExecutionConfirmed(true);
      onProcessChange(next);
      setActionMessage('已确认执行。正式用户消息现在才会显示到会话，Task Runner 开始运行。');
      await refreshSessionById(liveProcess.conversationId);
      await syncSessionMessagesInBackground(liveProcess.conversationId);
      await refreshAll(false);
    } catch (err) {
      setActionError(err instanceof Error ? err.message : '确认执行失败');
    } finally {
      setConfirming(false);
    }
  };

  const setExecutionPause = async (paused: boolean) => {
    if (!actuallyStarted || pausing || stopping) return;
    setPausing(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const payload = {
        execution_group_id: liveProcess.executionGroupId,
        conversation_id: liveProcess.conversationId,
        ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
      };
      const result = paused
        ? await apiClient.pauseProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          payload,
        )
        : await apiClient.resumeProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          payload,
        );
      const next = {
        ...liveProcess,
        serverStatus: readText(result.status) || (paused ? 'paused' : 'execution_started'),
        confirmationStatus: 'confirmed',
        executionPaused: paused,
      };
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      onProcessChange(next);
      setActionMessage(paused
        ? (runningTaskCount > 0
          ? `已暂停后续调度；${runningTaskCount} 个已运行任务仍会继续完成。`
          : '已暂停后续调度，不会启动新的任务节点。')
        : '已继续调度，Task Runner 将按照依赖顺序启动后续任务。');
      await refreshAll(false);
    } catch (err) {
      setActionError(err instanceof Error
        ? err.message
        : paused ? '暂停后续调度失败' : '继续调度失败');
    } finally {
      setPausing(false);
    }
  };

  const stopCurrentBatch = async (discardTasks = false) => {
    const previousProcess = liveProcess;
    const stoppingProcess = {
      ...liveProcess,
      serverStatus: 'stopping',
      executionPaused: false,
      tasksDiscarded: discardTasks || liveProcess.tasksDiscarded,
    };
    setStopping(true);
    setActionError(null);
    setActionMessage(discardTasks
      ? '已提交取消并清理请求，正在等待任务停止。'
      : '已提交取消请求，正在等待任务停止。');
    setLiveProcess(stoppingProcess);
    setMessage(withProcessStatus(message, stoppingProcess));
    onProcessChange(stoppingProcess);
    try {
      const response = await apiClient.stopProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          ...(discardTasks ? { discard_tasks: true } : {}),
        },
      );
      const next = buildRequirementExecutionProcess({
        fallback: liveProcess,
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      }) || {
        ...liveProcess,
        serverStatus: 'stopped',
        executionPaused: false,
        tasksDiscarded: discardTasks,
      };
      setPlanStopped(true);
      setPlanDiscarded(discardTasks);
      setLiveProcess(next);
      setMessage(withProcessStatus(message, next));
      setSyncError(null);
      onProcessChange(next);
      setActionMessage(discardTasks
        ? '规划已停止，本批次创建的 Task Runner 任务和关联记录已删除。'
        : '当前执行已整体取消。');
      await reloadGraph();
    } catch (err) {
      if (!isRequirementExecutionCancellationSettling({
        hasActiveRuns,
        phase: resolveRequirementExecutionProcessPhase({
          confirmationState,
          executionConfirmed,
          executionPaused: false,
          failureDetected: false,
          hasStartedRuns: Boolean(previousProcess.hasStartedRuns),
          planStopped,
          serverStatus: previousProcess.serverStatus || '',
          taskStatuses,
        }),
      })) {
        setLiveProcess(previousProcess);
        setMessage(withProcessStatus(message, previousProcess));
        onProcessChange(previousProcess);
      }
      setActionError(err instanceof Error
        ? err.message
        : discardTasks ? '取消规划并删除任务失败' : '取消本次执行失败');
      await refreshAll(false);
    } finally {
      setDiscardConfirmOpen(false);
      setCancelConfirmOpen(false);
      setStopping(false);
    }
  };

  const replaceExecutionPlan = async (planningFeedback?: string) => {
    const normalizedFeedback = planningFeedback?.trim() || '';
    setRevising(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const replacePreviousBatch = shouldReplaceRequirementExecutionBatch({
        planDiscarded,
        replacePreviousBatch: liveProcess.replacePreviousBatch,
      });
      if (shouldStopRequirementExecutionBeforeReplacement({
        phase,
        replacePreviousBatch,
      })) {
        await apiClient.stopProjectRequirementExecution(
          liveProcess.projectId,
          liveProcess.requirement.id,
          {
            execution_group_id: liveProcess.executionGroupId,
            conversation_id: liveProcess.conversationId,
            ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          },
        );
      }
      const response = await apiClient.executeProjectRequirement(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
          ...(liveProcess.selectedModelId
            ? { model_config_id: liveProcess.selectedModelId }
            : {}),
          include_prerequisite_dependents: Boolean(
            liveProcess.includePrerequisiteDependents,
          ),
          ...(normalizedFeedback ? { planning_feedback: normalizedFeedback } : {}),
          ...(replacePreviousBatch ? {
            replaces_execution_group_id: liveProcess.executionGroupId,
            replaces_conversation_id: liveProcess.conversationId,
          } : {}),
        },
      );
      const next = buildRequirementExecutionProcess({
        fallback: liveProcess,
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (!next) {
        throw new Error('后端没有返回新的规划批次标识');
      }
      next.executionPaused = false;
      setFeedback('');
      activeExecutionGroupIdRef.current = next.executionGroupId;
      setLiveProcess(next);
      setMessage(withProcessStatus(next.initialMessage || createFallbackMessage(next), next));
      setPlanStopped(false);
      setPlanDiscarded(false);
      setExecutionConfirmed(false);
      setSyncError(null);
      setActionError(null);
      setActionMessage(normalizedFeedback
        ? '已接收你的意见，正在重新生成执行流程。'
        : '已重新启动规划 Agent，正在生成新的执行流程。');
      onProcessChange(next);
    } catch (err) {
      setActionError(err instanceof Error
        ? err.message
        : normalizedFeedback ? '根据意见重新规划失败' : '重新生成执行流程失败');
    } finally {
      setRevising(false);
    }
  };

  const submitFeedback = async () => {
    const planningFeedback = feedback.trim();
    if (!planningFeedback || revising || !canRevise) return;
    await replaceExecutionPlan(planningFeedback);
  };

  const regenerateFailedPlan = async () => {
    if (!canRegenerate || revising) return;
    await replaceExecutionPlan();
  };

  const rerunStoppedBatch = async () => {
    if (!canRerun || rerunningRef.current || rerunCancellationSettling) return;
    rerunningRef.current = true;
    setRerunning(true);
    setActionError(null);
    setActionMessage(null);
    try {
      const response = await apiClient.rerunProjectRequirementExecution(
        liveProcess.projectId,
        liveProcess.requirement.id,
        {
          execution_group_id: liveProcess.executionGroupId,
          conversation_id: liveProcess.conversationId,
          ...(liveProcess.contactId ? { contact_id: liveProcess.contactId } : {}),
        },
      );
      const next = buildRequirementExecutionProcess({
        projectId: liveProcess.projectId,
        requirement: liveProcess.requirement,
        response,
      });
      if (!next) {
        throw new Error('后端没有返回新的重新执行批次标识');
      }
      next.executionPaused = false;
      setRerunCancellationSettling(false);
      setRerunConfirmOpen(false);
      setFeedback('');
      activeExecutionGroupIdRef.current = next.executionGroupId;
      setLiveProcess(next);
      setMessage(withProcessStatus(next.initialMessage || createFallbackMessage(next), next));
      setPlanStopped(false);
      setPlanDiscarded(false);
      setExecutionConfirmed(Boolean(next.hasStartedRuns));
      setSyncError(null);
      setActionError(null);
      setActionMessage(next.hasStartedRuns
        ? '旧批次资源已清理，新的任务副本已经开始执行。'
        : '旧批次资源已清理，新任务图已经生成；自动启动未成功，请点击“执行”继续。');
      onProcessChange(next);
      await refreshSessionById(next.conversationId);
      await syncSessionMessagesInBackground(next.conversationId);
      await reloadGraph();
    } catch (err) {
      if (isRequirementExecutionRerunCancellationSettlingError(err)) {
        const settlingProcess = {
          ...liveProcess,
          serverStatus: 'stopping',
          executionPaused: false,
          hasStartedRuns: true,
        };
        setRerunConfirmOpen(false);
        setRerunCancellationSettling(true);
        setPlanStopped(true);
        setLiveProcess(settlingProcess);
        setMessage(withProcessStatus(message, settlingProcess));
        onProcessChange(settlingProcess);
        setActionMessage('旧批次仍有任务正在取消，已重新发送取消请求；前端会继续刷新状态，收敛前不会重复重新执行。');
        await refreshAll(false);
        return;
      }
      setActionError(err instanceof Error ? err.message : '重新执行失败');
    } finally {
      setRerunning(false);
      rerunningRef.current = false;
    }
  };

  return (
    <>
      <RequirementExecutionModalFrame
        fullscreen={fullscreen}
        headerActions={(
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
            disabled={loading || syncing}
            onClick={() => void refreshAll(false)}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', (loading || syncing) && 'animate-spin')} />
            刷新
          </button>
        )}
        isLocalExecution={isLocalExecution}
        onClose={onClose}
        onToggleFullscreen={() => setFullscreen((current) => !current)}
        requirementTitle={readText(liveProcess.requirement.title) || liveProcess.requirement.id}
      >
        <div className="grid min-h-0 flex-1 lg:grid-cols-[390px_minmax(0,1fr)]">
          <RequirementExecutionProcessSidebar
            canRevise={canRevise}
            cancellationSettling={cancellationSettling}
            feedback={feedback}
            onFeedbackChange={setFeedback}
            onSubmitFeedback={() => void submitFeedback()}
            phase={phase}
            phaseText={phaseText}
            processEntries={processEntries}
            revising={revising}
            taskCount={allTasks.length}
            terminal={terminal}
          />

          <main className="flex min-h-0 min-w-0 flex-col">
            <RequirementExecutionGraphSurface
              actionError={actionError}
              actionMessage={actionMessage}
              containerRef={graphContainerRef}
              dependencyCount={graph.edges.length}
              graphPanelProps={{
                graph,
                loading,
                error: graphError,
                loadingRunId,
                loadingChangesRunId,
                panelWidth,
                loadingProcessTaskId,
                onOpenDetail: openDetail,
                onOpenProcessLog: openProcessLog,
                onOpenRun: openRun,
                onOpenChanges: openChanges,
              }}
              runRecordCount={allTasks.filter(
                (task) => Boolean(readString(task.last_run_id)),
              ).length}
              syncError={syncError}
              taskCount={allTasks.length}
            />
            <RequirementExecutionProcessActions
              actuallyStarted={actuallyStarted}
              canRegenerate={canRegenerate}
              canRerun={showRerunAction}
              cancellationSettling={cancellationSettling}
              confirming={confirming}
              executionPaused={Boolean(liveProcess.executionPaused)}
              graphReady={graphReady}
              hasActiveRuns={hasActiveRuns}
              onClose={onClose}
              onCancelRequirementExecution={() => void stopCurrentBatch(false)}
              onConfirmExecution={() => void confirmExecution()}
              onOpenCancelConfirm={() => setCancelConfirmOpen(true)}
              onOpenDiscardConfirm={() => setDiscardConfirmOpen(true)}
              onOpenFailedTaskRetry={() => setFailedTaskRetryOpen(true)}
              onOpenRerunConfirm={() => {
                if (!rerunBusy) setRerunConfirmOpen(true);
              }}
              onRegenerate={() => void regenerateFailedPlan()}
              onTogglePause={() => void setExecutionPause(!liveProcess.executionPaused)}
              pausing={pausing}
              phase={phase}
              queuedTaskCount={queuedTaskCount}
              rerunSettling={rerunCancellationSettling}
              rerunning={rerunning}
              retryableFailedTaskCount={retryableFailedTasks.length}
              retryingTaskId={retryingTaskId}
              revising={revising}
              runningTaskCount={runningTaskCount}
              stopping={stopping}
            />
          </main>
        </div>
      </RequirementExecutionModalFrame>

      <RequirementExecutionActionDialogs
        cancelConfirmOpen={cancelConfirmOpen}
        discardConfirmOpen={discardConfirmOpen}
        failedTaskRetryOpen={failedTaskRetryOpen}
        onCancelCurrentBatch={() => void stopCurrentBatch(false)}
        onCloseCancelConfirm={() => setCancelConfirmOpen(false)}
        onCloseDiscardConfirm={() => setDiscardConfirmOpen(false)}
        onCloseFailedTaskRetry={() => setFailedTaskRetryOpen(false)}
        onCloseRerunConfirm={() => setRerunConfirmOpen(false)}
        onDiscardCurrentPlan={() => void stopCurrentBatch(true)}
        onRerunStoppedBatch={() => void rerunStoppedBatch()}
        onRetryFailedTask={retryFailedTask}
        rerunConfirmOpen={rerunConfirmOpen}
        rerunning={rerunning}
        retryableFailedTasks={retryableFailedTasks}
        retryingTaskId={retryingTaskId}
        stopping={stopping}
      />

      <RequirementExecutionTaskModals taskGraph={taskGraph} />
    </>
  );
};
