// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  Activity,
  Check,
  CheckCircle2,
  GitBranch,
  LoaderCircle,
  MessageSquareText,
  Pause,
  Play,
  RotateCcw,
  Send,
  Square,
  X,
  XCircle,
} from 'lucide-react';

import { cn } from '../../../lib/utils';
import { MessageTaskGraphPanel } from '../../messageTasks/MessageTaskGraphPanel';
import { RunProcessTimeline } from '../../messageTasks/RunProcessTimeline';
import type { TimelineItem } from '../../userMessages/ConversationProcessTimelineModel';
import type {
  ProcessEntry,
  RequirementExecutionProcessPhase,
} from './requirementExecutionPhase';
import {
  shouldShowCancelRequirementExecution,
  shouldShowDiscardRequirementPlan,
} from './requirementExecutionProcessModel';

export const RequirementExecutionProcessSidebar: React.FC<{
  canRevise: boolean;
  cancellationSettling: boolean;
  feedback: string;
  onFeedbackChange: (value: string) => void;
  onOpenPlannerProcess: () => void;
  onSubmitFeedback: () => void;
  phase: RequirementExecutionProcessPhase;
  phaseText: { title: string; detail: string };
  plannerActive: boolean;
  plannerProcessMessageCount: number;
  processEntries: ProcessEntry[];
  revising: boolean;
  taskCount: number;
  terminal: boolean;
}> = ({
  canRevise,
  cancellationSettling,
  feedback,
  onFeedbackChange,
  onOpenPlannerProcess,
  onSubmitFeedback,
  phase,
  phaseText,
  plannerActive,
  plannerProcessMessageCount,
  processEntries,
  revising,
  taskCount,
  terminal,
}) => (
  <aside className="flex min-h-0 flex-col border-b border-border bg-muted/10 lg:border-b-0 lg:border-r">
    <div className="border-b border-border px-4 py-3">
      <div className="flex items-start gap-2">
        {cancellationSettling ? (
          <LoaderCircle className="mt-0.5 h-4 w-4 shrink-0 animate-spin text-sky-500" />
        ) : phase === 'failed' ? (
          <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-500" />
        ) : phase === 'completed' ? (
          <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-500" />
        ) : phase === 'stopped' ? (
          <Square className="mt-0.5 h-4 w-4 shrink-0 fill-current text-slate-500" />
        ) : phase === 'paused' ? (
          <Pause className="mt-0.5 h-4 w-4 shrink-0 text-amber-500" />
        ) : (
          <LoaderCircle className={cn(
            'mt-0.5 h-4 w-4 shrink-0 text-sky-500',
            !terminal && 'animate-spin',
          )}
          />
        )}
        <div>
          <div className="text-sm font-semibold text-foreground">{phaseText.title}</div>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{phaseText.detail}</p>
        </div>
      </div>
    </div>

    <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
      <div className="mb-3 flex items-center justify-between">
        <div className="text-xs font-semibold text-foreground">规划过程</div>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1 text-[11px] font-medium text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={onOpenPlannerProcess}
        >
          <Activity className="h-3 w-3" />
          详细过程 {plannerProcessMessageCount}
          {plannerActive ? (
            <span className="h-1.5 w-1.5 rounded-full bg-sky-500 motion-safe:animate-pulse" />
          ) : null}
        </button>
      </div>
      <div className="mb-3 text-[11px] text-muted-foreground">已生成 {taskCount} 个任务</div>
      <ol className="space-y-3">
        {processEntries.map((entry, index) => (
          <li key={entry.id} className="relative flex gap-3">
            {index < processEntries.length - 1 ? (
              <span className="absolute left-[9px] top-5 h-[calc(100%+0.75rem)] w-px bg-border" />
            ) : null}
            <span className={cn(
              'relative z-10 mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border bg-background',
              entry.state === 'done' && 'border-emerald-400 text-emerald-600',
              entry.state === 'active' && 'border-sky-400 text-sky-600',
              entry.state === 'error' && 'border-red-400 text-red-600',
              entry.state === 'stopped' && 'border-slate-400 text-slate-500',
              entry.state === 'pending' && 'border-border text-muted-foreground',
            )}
            >
              {entry.state === 'done' ? (
                <Check className="h-3 w-3" />
              ) : entry.state === 'active' ? (
                <LoaderCircle className="h-3 w-3 animate-spin" />
              ) : entry.state === 'error' ? (
                <X className="h-3 w-3" />
              ) : entry.state === 'stopped' ? (
                <Square className="h-2.5 w-2.5 fill-current" />
              ) : (
                <span className="h-1.5 w-1.5 rounded-full bg-current" />
              )}
            </span>
            <div className="min-w-0 pb-1">
              <div className="break-words text-xs font-medium text-foreground">{entry.title}</div>
              {entry.detail ? (
                <div className="mt-1 line-clamp-4 whitespace-pre-wrap text-[11px] leading-5 text-muted-foreground">
                  {entry.detail}
                </div>
              ) : null}
            </div>
          </li>
        ))}
      </ol>
    </div>

    {canRevise ? (
      <div className="shrink-0 border-t border-border bg-background px-4 py-3">
        <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
          <MessageSquareText className="h-3.5 w-3.5" />
          调整执行计划
        </div>
        <textarea
          value={feedback}
          disabled={revising}
          onChange={(event) => onFeedbackChange(event.target.value)}
          placeholder="输入你的想法，例如：先补测试，再修改接口；把前端和后端拆开执行……"
          className="min-h-24 w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground focus:border-primary disabled:cursor-not-allowed disabled:bg-muted/40"
        />
        <div className="mt-2 flex items-center justify-between gap-2">
          <span className="text-[10px] text-muted-foreground">
            发送后会保留历史记录并生成新的任务依赖图
          </span>
          <button
            type="button"
            className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
            disabled={!feedback.trim() || revising}
            onClick={onSubmitFeedback}
          >
            {revising
              ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              : <Send className="h-3.5 w-3.5" />}
            {revising ? '重新规划中' : '发送并调整'}
          </button>
        </div>
      </div>
    ) : (
      <div className="shrink-0 border-t border-border bg-muted/20 px-4 py-3">
        <div className="text-xs font-semibold text-foreground">执行计划已冻结</div>
        <p className="mt-1 text-[11px] leading-5 text-muted-foreground">
          {cancellationSettling
            ? '正在停止当前执行批次；所有任务收敛后才可重新规划。'
            : phase === 'completed'
              ? '当前批次已经完成；新的产品调整请创建新的项目需求。'
              : '任务已开始执行，运行期间不能重新规划。需要调整时请先停止当前执行批次。'}
        </p>
      </div>
    )}
  </aside>
);

export const RequirementExecutionGraphSurface: React.FC<{
  actionError?: string | null;
  actionMessage?: string | null;
  containerRef: React.RefObject<HTMLDivElement | null>;
  dependencyCount: number;
  graphPanelProps: React.ComponentProps<typeof MessageTaskGraphPanel>;
  runRecordCount: number;
  syncError?: string | null;
  taskCount: number;
}> = ({
  actionError,
  actionMessage,
  containerRef,
  dependencyCount,
  graphPanelProps,
  runRecordCount,
  syncError,
  taskCount,
}) => (
  <>
    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-3">
      <div>
        <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
          <GitBranch className="h-4 w-4" />
          实时执行流程图
        </div>
        <p className="mt-1 text-xs text-muted-foreground">
          每创建一个任务节点，右侧任务依赖图都会自动更新。
        </p>
      </div>
      <div className="flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        <span className="rounded-full border border-border bg-background px-2 py-1">节点 {taskCount}</span>
        <span className="rounded-full border border-border bg-background px-2 py-1">依赖 {dependencyCount}</span>
        <span className="rounded-full border border-border bg-background px-2 py-1">
          运行记录 {runRecordCount}
        </span>
      </div>
    </div>

    {(actionError || syncError || actionMessage) ? (
      <div className="shrink-0 px-4 pt-3">
        {actionError || syncError ? (
          <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {actionError || syncError}
          </div>
        ) : (
          <div className="rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/30 dark:text-emerald-200">
            {actionMessage}
          </div>
        )}
      </div>
    ) : null}

    <div ref={containerRef} className="min-h-0 flex-1 p-4">
      <MessageTaskGraphPanel {...graphPanelProps} />
    </div>
  </>
);

export const RequirementExecutionPlannerProcessModal: React.FC<{
  active: boolean;
  error?: string | null;
  items: TimelineItem[];
  loading: boolean;
  onClose: () => void;
  processMessageCount: number;
}> = ({ active, error, items, loading, onClose, processMessageCount }) => (
  <div className="fixed inset-0 z-[70]">
    <button
      type="button"
      aria-label="关闭规划运行过程"
      className="absolute inset-0 bg-black/45"
      onClick={onClose}
    />
    <section className="absolute left-1/2 top-1/2 flex max-h-[86vh] w-[calc(100vw-32px)] max-w-5xl -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-2xl">
      <header className="flex shrink-0 items-start justify-between gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
            <Activity className="h-4 w-4" />
            规划运行过程
            {active ? (
              <span className="inline-flex items-center gap-1 text-xs font-normal text-sky-600">
                <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                实时更新
              </span>
            ) : null}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            已显示 {processMessageCount} 条规划过程；实时输出会自动更新，结束后由 Memory Engine 固化。
          </p>
        </div>
        <button
          type="button"
          aria-label="关闭规划运行过程"
          className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {error ? (
          <div className="mb-3 rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {error}
          </div>
        ) : null}
        {items.length > 0 ? (
          <RunProcessTimeline items={items} />
        ) : (
          <div className="flex min-h-[320px] items-center justify-center rounded-lg border border-dashed border-border bg-muted/10 px-6 py-10 text-center">
            <div className="max-w-md">
              {active || loading ? (
                <LoaderCircle className="mx-auto h-7 w-7 animate-spin text-sky-500" />
              ) : (
                <Activity className="mx-auto h-7 w-7 text-muted-foreground" />
              )}
              <div className="mt-3 text-sm font-medium text-foreground">
                {active || loading ? '规划 Agent 正在运行，等待第一条过程记录' : '暂无规划运行记录'}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                正在等待当前规划 turn 的实时事件；模型请求、思考、输出和工具状态都会显示在这里。
              </p>
            </div>
          </div>
        )}
      </div>
    </section>
  </div>
);

export const RequirementExecutionProcessActions: React.FC<{
  actuallyStarted: boolean;
  canRegenerate: boolean;
  canRerun: boolean;
  cancellationSettling: boolean;
  confirming: boolean;
  executionPaused: boolean;
  graphReady: boolean;
  hasActiveRuns: boolean;
  runtimeEnvironmentReady: boolean;
  runtimeEnvironmentStatus: string;
  onClose: () => void;
  onCancelRequirementExecution: () => void;
  onConfirmExecution: () => void;
  onOpenCancelConfirm: () => void;
  onOpenDiscardConfirm: () => void;
  onOpenFailedTaskRetry: () => void;
  onOpenRerunConfirm: () => void;
  onRegenerate: () => void;
  onTogglePause: () => void;
  pausing: boolean;
  phase: RequirementExecutionProcessPhase;
  isLocalExecution: boolean;
  queuedTaskCount: number;
  rerunSettling: boolean;
  rerunning: boolean;
  retryableFailedTaskCount: number;
  retryingTaskId?: string | null;
  revising: boolean;
  runningTaskCount: number;
  stopping: boolean;
}> = ({
  actuallyStarted,
  canRegenerate,
  canRerun,
  cancellationSettling,
  confirming,
  executionPaused,
  graphReady,
  hasActiveRuns,
  runtimeEnvironmentReady,
  onClose,
  onCancelRequirementExecution,
  onConfirmExecution,
  onOpenCancelConfirm,
  onOpenDiscardConfirm,
  onOpenFailedTaskRetry,
  onOpenRerunConfirm,
  onRegenerate,
  onTogglePause,
  pausing,
  phase,
  isLocalExecution,
  queuedTaskCount,
  rerunSettling,
  rerunning,
  retryableFailedTaskCount,
  retryingTaskId,
  revising,
  runningTaskCount,
  stopping,
}) => (
  <footer className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-border bg-muted/10 px-4 py-3">
    <div className="text-xs text-muted-foreground">
      {cancellationSettling
        ? (runningTaskCount > 0
          ? `正在取消本次执行，等待 ${runningTaskCount} 个运行中任务结束并回写状态。`
          : queuedTaskCount > 0
            ? `正在取消本次执行，等待 ${queuedTaskCount} 个排队任务回写取消状态。`
            : '正在取消本次执行，等待任务回写取消状态。')
        : graphReady && !runtimeEnvironmentReady
        ? (
          isLocalExecution
            ? '流程图已就绪，正在通过 Local Connector 识别本地技术栈与运行条件。'
            : '流程图已就绪，执行环境正在初始化，完成后自动开放执行。'
        )
        : graphReady
        ? '流程图和执行环境均已就绪，当前还没有任务开始运行。'
        : phase === 'paused'
          ? (runningTaskCount > 0
            ? `暂停门禁已生效，等待 ${runningTaskCount} 个运行中任务结束。`
            : '执行已暂停，不会启动新的任务节点。')
        : phase === 'completed'
          ? '全部任务已完成，执行结果已同步回项目 Plan。'
        : phase === 'failed'
          ? '当前批次存在失败或阻塞任务，可查看详情、重试或重新规划。'
        : actuallyStarted
          ? (
            isLocalExecution
              ? '执行确认已收到，云端正在按依赖顺序通过 Local Connector 调度本机任务。'
              : '执行确认已收到，任务正在按依赖顺序启动。'
          )
          : '完整任务依赖图生成前，执行按钮保持禁用。'}
    </div>
    <div className="flex flex-wrap items-center gap-2">
      {!cancellationSettling && retryableFailedTaskCount > 0 ? (
        <button
          type="button"
          aria-label={`重试失败或阻塞任务，共 ${retryableFailedTaskCount} 个`}
          className="inline-flex items-center gap-1.5 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs font-semibold text-red-700 hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200"
          disabled={Boolean(retryingTaskId)}
          onClick={onOpenFailedTaskRetry}
        >
          {retryingTaskId
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <RotateCcw className="h-3.5 w-3.5" />}
          重试失败或阻塞任务
          <span className="rounded-full bg-red-600 px-1.5 py-0.5 text-[10px] leading-none text-white">
            {retryableFailedTaskCount}
          </span>
        </button>
      ) : null}
      {canRegenerate ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
          disabled={revising || rerunning || stopping || confirming}
          onClick={onRegenerate}
        >
          {revising
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <RotateCcw className="h-3.5 w-3.5" />}
          {revising ? '重新生成中' : '重新生成执行流程'}
        </button>
      ) : null}
      {canRerun ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
          disabled={rerunSettling || rerunning || revising || stopping || confirming}
          onClick={onOpenRerunConfirm}
          aria-busy={rerunSettling || rerunning}
        >
          {rerunSettling || rerunning
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <RotateCcw className="h-3.5 w-3.5" />}
          {rerunSettling ? '等待取消完成' : rerunning ? '重新执行中' : '重新执行'}
        </button>
      ) : null}
      {actuallyStarted && phase !== 'stopped' && (hasActiveRuns || phase === 'paused') ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-800 hover:bg-amber-100 disabled:opacity-60 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200"
          disabled={pausing || stopping || confirming || revising || rerunning}
          onClick={onTogglePause}
        >
          {pausing
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : executionPaused
              ? <Play className="h-3.5 w-3.5" />
              : <Pause className="h-3.5 w-3.5" />}
          {pausing
            ? (executionPaused ? '恢复调度中' : '暂停调度中')
            : executionPaused ? '继续调度' : '暂停后续任务'}
        </button>
      ) : null}
      {(cancellationSettling || shouldShowCancelRequirementExecution({ actuallyStarted, phase })) ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs font-medium text-red-700 hover:bg-red-100 disabled:opacity-60 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200"
          disabled={stopping || pausing || confirming || revising || rerunning}
          onClick={cancellationSettling ? onCancelRequirementExecution : onOpenCancelConfirm}
        >
          {stopping || cancellationSettling
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <XCircle className="h-3.5 w-3.5" />}
          {stopping
            ? '取消中'
            : cancellationSettling ? '重发取消请求' : '取消本次执行'}
        </button>
      ) : null}
      {shouldShowDiscardRequirementPlan({ actuallyStarted, phase }) ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-muted-foreground hover:bg-accent hover:text-foreground disabled:opacity-60"
          disabled={stopping || pausing || confirming || revising}
          onClick={onOpenDiscardConfirm}
        >
          {stopping
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <Square className="h-3.5 w-3.5" />}
          {stopping ? '取消并清理中' : '取消规划并删除任务'}
        </button>
      ) : null}
      {!actuallyStarted && phase !== 'failed' && phase !== 'stopped' ? (
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-5 py-2 text-xs font-semibold text-white hover:bg-amber-700 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!graphReady || !runtimeEnvironmentReady || confirming || stopping || pausing || revising}
          onClick={onConfirmExecution}
        >
          {confirming || (graphReady && !runtimeEnvironmentReady)
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <Play className="h-3.5 w-3.5" />}
          {confirming
            ? '启动中'
            : graphReady && !runtimeEnvironmentReady
              ? (isLocalExecution ? '分析本地运行条件' : '初始化环境中')
              : '执行'}
        </button>
      ) : null}
      <button
        type="button"
        className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-accent"
        onClick={onClose}
      >
        关闭
      </button>
    </div>
  </footer>
);
