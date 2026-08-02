// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
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
  onSubmitFeedback: () => void;
  phase: RequirementExecutionProcessPhase;
  phaseText: { title: string; detail: string };
  processEntries: ProcessEntry[];
  revising: boolean;
  taskCount: number;
  terminal: boolean;
}> = ({
  canRevise,
  cancellationSettling,
  feedback,
  onFeedbackChange,
  onSubmitFeedback,
  phase,
  phaseText,
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
        <span className="text-[11px] text-muted-foreground">{taskCount} 个任务</span>
      </div>
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

    <div className="shrink-0 border-t border-border bg-background px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
        <MessageSquareText className="h-3.5 w-3.5" />
        调整执行计划
      </div>
      <textarea
        value={feedback}
        disabled={!canRevise || revising}
        onChange={(event) => onFeedbackChange(event.target.value)}
        placeholder={cancellationSettling
          ? '正在取消当前批次，任务状态收敛后即可调整'
          : !canRevise
            ? phase === 'completed'
              ? '当前执行已经完成；新的产品迭代请创建新的项目需求'
              : phase === 'running'
                ? '执行已经开始；如需调整，请先明确停止当前执行批次'
                : '当前批次仍有活动任务，请先停止并等待状态收敛后再调整'
            : '输入你的想法，例如：先补测试，再修改接口；把前端和后端拆开执行……'}
        className="min-h-24 w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground focus:border-primary disabled:cursor-not-allowed disabled:bg-muted/40"
      />
      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="text-[10px] text-muted-foreground">
          发送后会保留历史记录并生成新的 DAG
        </span>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
          disabled={!feedback.trim() || revising || !canRevise}
          onClick={onSubmitFeedback}
        >
          {revising
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <Send className="h-3.5 w-3.5" />}
          {revising ? '重新规划中' : '发送并调整'}
        </button>
      </div>
    </div>
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
          Agent 每创建一个任务节点，右侧 DAG 都会自动更新。
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

export const RequirementExecutionProcessActions: React.FC<{
  actuallyStarted: boolean;
  canRegenerate: boolean;
  canRerun: boolean;
  cancellationSettling: boolean;
  confirming: boolean;
  executionPaused: boolean;
  graphReady: boolean;
  hasActiveRuns: boolean;
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
        : graphReady
        ? '流程图已就绪，当前没有任何任务 run。'
        : phase === 'paused'
          ? (runningTaskCount > 0
            ? `暂停门禁已生效，等待 ${runningTaskCount} 个运行中任务结束。`
            : '执行已暂停，不会启动新的任务节点。')
        : phase === 'completed'
          ? '全部任务已完成，执行结果已同步回项目 Plan。'
        : phase === 'failed'
          ? '当前批次存在失败任务，可查看详情、重试或重新规划。'
        : actuallyStarted
          ? 'Task Runner 已收到执行确认。'
          : '完整 DAG 生成前，执行按钮保持禁用。'}
    </div>
    <div className="flex flex-wrap items-center gap-2">
      {!cancellationSettling && retryableFailedTaskCount > 0 ? (
        <button
          type="button"
          aria-label={`重试失败任务，共 ${retryableFailedTaskCount} 个`}
          className="inline-flex items-center gap-1.5 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs font-semibold text-red-700 hover:bg-red-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200"
          disabled={Boolean(retryingTaskId)}
          onClick={onOpenFailedTaskRetry}
        >
          {retryingTaskId
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <RotateCcw className="h-3.5 w-3.5" />}
          重试失败任务
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
          disabled={!graphReady || confirming || stopping || pausing || revising}
          onClick={onConfirmExecution}
        >
          {confirming
            ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
            : <Play className="h-3.5 w-3.5" />}
          {confirming ? '启动中' : '执行'}
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
