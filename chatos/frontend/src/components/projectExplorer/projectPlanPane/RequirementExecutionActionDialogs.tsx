// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import {
  AlertTriangle,
  LoaderCircle,
  Play,
  RotateCcw,
  Square,
  X,
  XCircle,
} from 'lucide-react';

import type { MessageTaskRunnerTask } from '../../../lib/api/client/types';
import { readString } from '../../messageTasks/utils';

const RequirementExecutionConfirmationDialog: React.FC<{
  actionLabel: string;
  ariaLabel: string;
  busy: boolean;
  busyActionLabel: string;
  cancelLabel: string;
  children: React.ReactNode;
  icon: React.ReactNode;
  onCancel: () => void;
  onConfirm: () => void;
  title: string;
  tone: 'amber' | 'red';
}> = ({
  actionLabel,
  ariaLabel,
  busy,
  busyActionLabel,
  cancelLabel,
  children,
  icon,
  onCancel,
  onConfirm,
  title,
  tone,
}) => {
  const amber = tone === 'amber';
  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4"
      role="alertdialog"
      aria-modal="true"
      aria-label={ariaLabel}
    >
      <section className="w-full max-w-lg rounded-xl border border-border bg-card p-5 shadow-2xl">
        <div className="flex items-start gap-3">
          <span className={amber
            ? 'mt-0.5 rounded-full bg-amber-100 p-2 text-amber-700 dark:bg-amber-950/40 dark:text-amber-300'
            : 'mt-0.5 rounded-full bg-red-100 p-2 text-red-700 dark:bg-red-950/40 dark:text-red-300'}
          >
            <AlertTriangle className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h3 className="text-base font-semibold text-foreground">{title}</h3>
            {children}
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            className="rounded-md border border-border bg-background px-4 py-2 text-sm font-medium text-foreground hover:bg-accent disabled:opacity-60"
            disabled={busy}
            onClick={onCancel}
          >
            {cancelLabel}
          </button>
          <button
            type="button"
            className={amber
              ? 'inline-flex items-center gap-1.5 rounded-md bg-amber-600 px-4 py-2 text-sm font-semibold text-white hover:bg-amber-700 disabled:opacity-60'
              : 'inline-flex items-center gap-1.5 rounded-md bg-red-600 px-4 py-2 text-sm font-semibold text-white hover:bg-red-700 disabled:opacity-60'}
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? <LoaderCircle className="h-4 w-4 animate-spin" /> : icon}
            {busy ? busyActionLabel : actionLabel}
          </button>
        </div>
      </section>
    </div>
  );
};

const FailedTaskRetryDialog: React.FC<{
  onClose: () => void;
  onRetry: (task: MessageTaskRunnerTask) => void | Promise<void>;
  retryableTasks: MessageTaskRunnerTask[];
  retryingTaskId?: string | null;
}> = ({ onClose, onRetry, retryableTasks, retryingTaskId }) => (
  <div
    className="fixed inset-0 z-[80] flex items-center justify-center bg-black/60 p-4"
    role="dialog"
    aria-modal="true"
    aria-label="失败任务重试"
  >
    <section className="flex max-h-[min(720px,88dvh)] w-full max-w-2xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl">
      <header className="flex shrink-0 items-start justify-between gap-3 border-b border-border px-5 py-4">
        <div className="flex min-w-0 items-start gap-3">
          <span className="mt-0.5 rounded-full bg-red-100 p-2 text-red-700 dark:bg-red-950/40 dark:text-red-300">
            <RotateCcw className="h-5 w-5" />
          </span>
          <div className="min-w-0">
            <h3 className="text-base font-semibold text-foreground">重试失败任务</h3>
            <p className="mt-1 text-sm leading-6 text-muted-foreground">
              下面只列出当前执行批次中可以重试的失败节点。点击“重新开始”后，只重跑该节点；成功后，后续节点会继续按照依赖关系调度。
            </p>
          </div>
        </div>
        <button
          type="button"
          className="rounded-md border border-border bg-background p-1.5 text-muted-foreground hover:bg-accent hover:text-foreground"
          onClick={onClose}
          aria-label="关闭失败任务列表"
        >
          <X className="h-4 w-4" />
        </button>
      </header>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-5">
        {retryableTasks.map((task) => {
          const taskId = readString(task.id) || '';
          const taskTitle = task.title || taskId;
          const retrying = retryingTaskId === taskId;
          const errorMessage = readString(task.last_run?.error_message);
          return (
            <div
              key={taskId}
              role="group"
              aria-label={`失败任务：${taskTitle}`}
              className="rounded-lg border border-red-200 bg-red-50/60 p-4 dark:border-red-900 dark:bg-red-950/20"
            >
              <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="rounded-full border border-red-300 bg-background px-2 py-0.5 text-[11px] font-medium text-red-700 dark:border-red-800 dark:text-red-200">
                      执行失败
                    </span>
                    <span className="text-[11px] text-muted-foreground">
                      运行 {readString(task.last_run_id)}
                    </span>
                  </div>
                  <h4 className="mt-2 break-words text-sm font-semibold text-foreground">
                    {taskTitle}
                  </h4>
                  {(readString(task.objective) || readString(task.description)) ? (
                    <p className="mt-1 line-clamp-3 whitespace-pre-wrap text-xs leading-5 text-muted-foreground">
                      {readString(task.objective) || readString(task.description)}
                    </p>
                  ) : null}
                  {errorMessage ? (
                    <div className="mt-3 rounded-md border border-red-200 bg-background/80 px-3 py-2 text-xs leading-5 text-red-700 dark:border-red-900 dark:text-red-200">
                      {errorMessage}
                    </div>
                  ) : null}
                </div>
                <button
                  type="button"
                  className="inline-flex shrink-0 items-center gap-1.5 rounded-md bg-red-600 px-4 py-2 text-xs font-semibold text-white hover:bg-red-700 disabled:cursor-not-allowed disabled:opacity-60"
                  disabled={Boolean(retryingTaskId)}
                  onClick={() => void onRetry(task)}
                >
                  {retrying
                    ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                    : <Play className="h-3.5 w-3.5" />}
                  {retrying ? '正在重新开始' : '重新开始'}
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/10 px-5 py-3">
        <span className="text-xs text-muted-foreground">
          当前共有 {retryableTasks.length} 个失败任务可以重试
        </span>
        <button
          type="button"
          className="rounded-md border border-border bg-background px-4 py-2 text-xs font-medium text-foreground hover:bg-accent"
          onClick={onClose}
        >
          关闭
        </button>
      </footer>
    </section>
  </div>
);

export const RequirementExecutionActionDialogs: React.FC<{
  cancelConfirmOpen: boolean;
  discardConfirmOpen: boolean;
  failedTaskRetryOpen: boolean;
  onCancelCurrentBatch: () => void;
  onCloseCancelConfirm: () => void;
  onCloseDiscardConfirm: () => void;
  onCloseFailedTaskRetry: () => void;
  onCloseRerunConfirm: () => void;
  onDiscardCurrentPlan: () => void;
  onRerunStoppedBatch: () => void;
  onRetryFailedTask: (task: MessageTaskRunnerTask) => void | Promise<void>;
  rerunConfirmOpen: boolean;
  rerunning: boolean;
  retryableFailedTasks: MessageTaskRunnerTask[];
  retryingTaskId?: string | null;
  stopping: boolean;
}> = ({
  cancelConfirmOpen,
  discardConfirmOpen,
  failedTaskRetryOpen,
  onCancelCurrentBatch,
  onCloseCancelConfirm,
  onCloseDiscardConfirm,
  onCloseFailedTaskRetry,
  onCloseRerunConfirm,
  onDiscardCurrentPlan,
  onRerunStoppedBatch,
  onRetryFailedTask,
  rerunConfirmOpen,
  rerunning,
  retryableFailedTasks,
  retryingTaskId,
  stopping,
}) => (
  <>
    {failedTaskRetryOpen ? (
      <FailedTaskRetryDialog
        onClose={onCloseFailedTaskRetry}
        onRetry={onRetryFailedTask}
        retryableTasks={retryableFailedTasks}
        retryingTaskId={retryingTaskId}
      />
    ) : null}
    {cancelConfirmOpen ? (
      <RequirementExecutionConfirmationDialog
        actionLabel="确认取消本次执行"
        ariaLabel="确认取消本次执行"
        busy={stopping}
        busyActionLabel="正在取消本次执行"
        cancelLabel="返回"
        icon={<XCircle className="h-4 w-4" />}
        onCancel={onCloseCancelConfirm}
        onConfirm={onCancelCurrentBatch}
        title="取消本次执行？"
        tone="red"
      >
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          这会取消正在运行的任务，并阻止所有排队和后续依赖节点继续执行。这个操作不是暂停，取消后不能直接继续当前批次。
        </p>
        <p className="mt-2 text-xs leading-5 text-red-700 dark:text-red-300">
          如需稍后继续，请关闭此提示并使用“暂停后续任务”；当前已运行任务仍会继续完成。
        </p>
      </RequirementExecutionConfirmationDialog>
    ) : null}
    {discardConfirmOpen ? (
      <RequirementExecutionConfirmationDialog
        actionLabel="确认取消并删除"
        ariaLabel="确认取消规划并删除任务"
        busy={stopping}
        busyActionLabel="正在取消并清理"
        cancelLabel="取消"
        icon={<Square className="h-4 w-4" />}
        onCancel={onCloseDiscardConfirm}
        onConfirm={onDiscardCurrentPlan}
        title="取消规划并删除已创建的任务？"
        tone="red"
      >
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          系统会立即停止当前计划生成，并删除这个执行批次已经创建的任务、运行记录和关联链接。
        </p>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          项目需求、项目任务和技术文档不会被删除。之后仍可重新发起规划。
        </p>
      </RequirementExecutionConfirmationDialog>
    ) : null}
    {rerunConfirmOpen ? (
      <RequirementExecutionConfirmationDialog
        actionLabel="确认清理并重新执行"
        ariaLabel="确认重新执行"
        busy={rerunning}
        busyActionLabel="正在清理并重新执行"
        cancelLabel="取消"
        icon={<RotateCcw className="h-4 w-4" />}
        onCancel={onCloseRerunConfirm}
        onConfirm={onRerunStoppedBatch}
        title="确认重新执行整个任务流程？"
        tone="amber"
      >
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          系统会复制当前完整执行流程并立即运行新副本。新批次创建成功后，会删除旧批次的任务、运行记录、执行环境和临时工作区。
        </p>
        <p className="mt-2 text-xs leading-5 text-amber-700 dark:text-amber-300">
          此清理操作不可撤销，但项目需求和流程替换记录会保留。
        </p>
      </RequirementExecutionConfirmationDialog>
    ) : null}
  </>
);
