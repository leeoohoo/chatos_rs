import React, { useEffect, useMemo, useState } from 'react';

import type { TaskWorkbarItem } from './types';

type TaskModalMode = 'complete' | 'edit';

export interface TaskOutcomeDraft {
  title: string;
  details: string;
  priority: TaskWorkbarItem['priority'];
  status: TaskWorkbarItem['status'];
  dueAt: string;
  outcomeSummary: string;
  resumeHint: string;
  blockerReason: string;
  blockerNeedsText: string;
  blockerKind: string;
}

interface TaskOutcomeModalProps {
  open: boolean;
  mode: TaskModalMode;
  task: TaskWorkbarItem | null;
  submitting?: boolean;
  error?: string | null;
  onClose: () => void;
  onSubmit: (draft: TaskOutcomeDraft) => void;
}

const buildDraft = (task: TaskWorkbarItem | null): TaskOutcomeDraft => ({
  title: task?.title || '',
  details: task?.details || '',
  priority: task?.priority || 'medium',
  status: task?.status || 'todo',
  dueAt: task?.dueAt || '',
  outcomeSummary: task?.outcomeSummary || '',
  resumeHint: task?.resumeHint || '',
  blockerReason: task?.blockerReason || '',
  blockerNeedsText: (task?.blockerNeeds || []).join('\n'),
  blockerKind: task?.blockerKind || 'unknown',
});

const blockerKindOptions: Array<{ value: string; label: string }> = [
  { value: 'unknown', label: '未分类' },
  { value: 'missing_information', label: '缺信息' },
  { value: 'design_decision', label: '待决策' },
  { value: 'external_dependency', label: '外部依赖' },
  { value: 'permission', label: '权限限制' },
  { value: 'environment_failure', label: '环境故障' },
  { value: 'upstream_bug', label: '上游缺陷' },
];

const TaskOutcomeModal: React.FC<TaskOutcomeModalProps> = ({
  open,
  mode,
  task,
  submitting = false,
  error = null,
  onClose,
  onSubmit,
}) => {
  const [draft, setDraft] = useState<TaskOutcomeDraft>(() => buildDraft(task));

  useEffect(() => {
    if (!open) {
      return;
    }
    setDraft(buildDraft(task));
  }, [open, task]);

  const isBlocked = draft.status === 'blocked';
  const isCompleteMode = mode === 'complete';
  const title = useMemo(() => {
    if (isCompleteMode) {
      return '完成任务';
    }
    return `编辑任务${task?.title ? ` · ${task.title}` : ''}`;
  }, [isCompleteMode, task?.title]);

  if (!open || !task) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center p-4">
      <div className="fixed inset-0 bg-black/45" onClick={submitting ? undefined : onClose} />
      <div className="relative w-full max-w-2xl rounded-xl border border-border bg-card shadow-2xl">
        <div className="flex items-center justify-between border-b border-border px-5 py-4">
          <div>
            <div className="text-base font-semibold text-foreground">{title}</div>
            <div className="mt-1 text-xs text-muted-foreground">
              {isCompleteMode
                ? '沉淀本次任务成果，避免后续任务重复探索。'
                : '编辑任务状态与上下文，确保看板里保留可复用信息。'}
            </div>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
            onClick={onClose}
            disabled={submitting}
          >
            关闭
          </button>
        </div>

        <div className="max-h-[80vh] overflow-y-auto px-5 py-4">
          {error ? (
            <div className="mb-4 rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
              {error}
            </div>
          ) : null}

          <div className="grid gap-4 md:grid-cols-2">
            {!isCompleteMode ? (
              <>
                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">标题</span>
                  <input
                    type="text"
                    value={draft.title}
                    onChange={(event) => setDraft((prev) => ({ ...prev, title: event.target.value }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">优先级</span>
                  <select
                    value={draft.priority}
                    onChange={(event) => setDraft((prev) => ({ ...prev, priority: event.target.value as TaskWorkbarItem['priority'] }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  >
                    <option value="high">high</option>
                    <option value="medium">medium</option>
                    <option value="low">low</option>
                  </select>
                </label>

                <label className="md:col-span-2 flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">详情</span>
                  <textarea
                    value={draft.details}
                    onChange={(event) => setDraft((prev) => ({ ...prev, details: event.target.value }))}
                    rows={3}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">状态</span>
                  <select
                    value={draft.status}
                    onChange={(event) => setDraft((prev) => ({ ...prev, status: event.target.value as TaskWorkbarItem['status'] }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  >
                    <option value="todo">todo</option>
                    <option value="doing">doing</option>
                    <option value="blocked">blocked</option>
                    <option value="done">done</option>
                  </select>
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">截止时间</span>
                  <input
                    type="text"
                    value={draft.dueAt}
                    onChange={(event) => setDraft((prev) => ({ ...prev, dueAt: event.target.value }))}
                    placeholder="留空表示清空"
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>
              </>
            ) : null}

            <label className="md:col-span-2 flex flex-col gap-1.5">
              <span className="text-xs font-medium text-foreground">
                成果摘要
                {(isCompleteMode || isBlocked) ? ' *' : ''}
              </span>
              <textarea
                value={draft.outcomeSummary}
                onChange={(event) => setDraft((prev) => ({ ...prev, outcomeSummary: event.target.value }))}
                rows={4}
                placeholder="写清这次做了什么、得出了什么结论、产出了什么结果。"
                className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                disabled={submitting}
              />
            </label>

            <label className="md:col-span-2 flex flex-col gap-1.5">
              <span className="text-xs font-medium text-foreground">接手提示</span>
              <textarea
                value={draft.resumeHint}
                onChange={(event) => setDraft((prev) => ({ ...prev, resumeHint: event.target.value }))}
                rows={2}
                placeholder="给下一个接手的人一句上下文提示，可选。"
                className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                disabled={submitting}
              />
            </label>

            {isBlocked ? (
              <>
                <label className="md:col-span-2 flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">阻塞原因 *</span>
                  <textarea
                    value={draft.blockerReason}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerReason: event.target.value }))}
                    rows={3}
                    placeholder="写清为什么卡住，最好是事实性描述。"
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">阻塞类型</span>
                  <select
                    value={draft.blockerKind || 'unknown'}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerKind: event.target.value }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  >
                    {blockerKindOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">继续所需条件</span>
                  <textarea
                    value={draft.blockerNeedsText}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerNeedsText: event.target.value }))}
                    rows={3}
                    placeholder="每行一项，写清还缺什么才能继续。"
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>
              </>
            ) : null}
          </div>
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-4">
          <div className="text-xs text-muted-foreground">
            {isBlocked
              ? 'blocked 任务会记录已做尝试、阻塞原因和继续条件。'
              : (isCompleteMode ? 'done 任务需要沉淀本次成果。' : '建议为重要任务补充成果，后续上下文会直接复用。')}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground hover:bg-accent disabled:opacity-50"
              onClick={onClose}
              disabled={submitting}
            >
              取消
            </button>
            <button
              type="button"
              className="rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              onClick={() => onSubmit(draft)}
              disabled={submitting}
            >
              {submitting ? '提交中...' : (isCompleteMode ? '标记完成' : '保存任务')}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default TaskOutcomeModal;
