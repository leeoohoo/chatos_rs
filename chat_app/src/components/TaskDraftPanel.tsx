import React, { useEffect, useMemo, useState } from 'react';

import type { TaskReviewDraft, TaskReviewPanelState } from '../lib/store/types';

interface TaskDraftPanelProps {
  panel: TaskReviewPanelState;
  onConfirm: (drafts: TaskReviewDraft[]) => Promise<void> | void;
  onCancel: () => Promise<void> | void;
}

const createDraftId = () => {
  const randomPart =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID().replace(/-/g, '')
      : Date.now().toString() + '_' + Math.random().toString(36).slice(2, 10);
  return 'draft_ui_' + randomPart;
};

const createEmptyDraft = (): TaskReviewDraft => ({
  id: createDraftId(),
  title: '',
  details: '',
  priority: 'medium',
  status: 'todo',
  tags: [],
  dueAt: null,
});

const normalizeDraft = (draft: TaskReviewDraft): TaskReviewDraft => ({
  ...draft,
  id: draft.id || createDraftId(),
  title: (draft.title || '').trim(),
  details: (draft.details || '').trim(),
  tags: (draft.tags || [])
    .map((tag) => String(tag || '').trim())
    .filter((tag, index, arr) => Boolean(tag) && arr.indexOf(tag) === index),
  dueAt: draft.dueAt ? String(draft.dueAt).trim() : null,
});

export const TaskDraftPanel: React.FC<TaskDraftPanelProps> = ({ panel, onConfirm, onCancel }) => {
  const [drafts, setDrafts] = useState<TaskReviewDraft[]>(() =>
    panel.drafts.length > 0 ? panel.drafts : [createEmptyDraft()]
  );

  useEffect(() => {
    setDrafts(panel.drafts.length > 0 ? panel.drafts : [createEmptyDraft()]);
  }, [panel.reviewId, panel.drafts]);

  const hasEmptyTitle = useMemo(
    () => drafts.some((draft) => !(draft.title || '').trim()),
    [drafts]
  );

  const confirmDisabled = panel.submitting === true || drafts.length === 0 || hasEmptyTitle;

  const updateDraft = (id: string, patch: Partial<TaskReviewDraft>) => {
    setDrafts((prev) => prev.map((draft) => (draft.id === id ? { ...draft, ...patch } : draft)));
  };

  const removeDraft = (id: string) => {
    setDrafts((prev) => {
      const next = prev.filter((draft) => draft.id !== id);
      return next.length > 0 ? next : [createEmptyDraft()];
    });
  };

  const addDraft = () => {
    setDrafts((prev) => [...prev, createEmptyDraft()]);
  };

  const handleConfirm = async () => {
    if (confirmDisabled) {
      return;
    }
    const normalized = drafts.map(normalizeDraft);
    await onConfirm(normalized);
  };

  const handleCancel = async () => {
    if (panel.submitting) {
      return;
    }
    await onCancel();
  };

  return (
    <div className="mx-3 mb-3 rounded-xl border border-amber-300 bg-amber-50 p-3 shadow-sm dark:border-amber-700 dark:bg-amber-900/20">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-semibold text-amber-900 dark:text-amber-100">任务创建确认</div>
          <div className="text-xs text-amber-800/80 dark:text-amber-200/80">
            可编辑任务后点击确定，系统会立即创建并把结果返回给调用方
          </div>
        </div>
        <button
          type="button"
          className="rounded-md border border-amber-300 px-2 py-1 text-xs text-amber-900 hover:bg-amber-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-amber-700 dark:text-amber-100 dark:hover:bg-amber-800/50"
          onClick={addDraft}
          disabled={panel.submitting === true}
        >
          新增任务
        </button>
      </div>

      <div className="max-h-72 space-y-2 overflow-y-auto pr-1">
        {drafts.map((draft, index) => (
          <div key={draft.id} className="rounded-lg border border-amber-200 bg-white p-2 dark:border-amber-800 dark:bg-slate-900/60">
            <div className="mb-2 flex items-center justify-between">
              <div className="text-xs font-medium text-slate-600 dark:text-slate-300">任务 {index + 1}</div>
              <button
                type="button"
                className="text-xs text-red-600 hover:underline disabled:cursor-not-allowed disabled:opacity-60"
                onClick={() => removeDraft(draft.id)}
                disabled={panel.submitting === true}
              >
                删除
              </button>
            </div>

            <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                标题
                <input
                  type="text"
                  value={draft.title}
                  onChange={(event) => updateDraft(draft.id, { title: event.target.value })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="必填"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                截止时间（可选）
                <input
                  type="text"
                  value={draft.dueAt || ''}
                  onChange={(event) => updateDraft(draft.id, { dueAt: event.target.value || null })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="例如: 2026-03-01T10:00:00Z"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                优先级
                <select
                  value={draft.priority}
                  onChange={(event) => updateDraft(draft.id, { priority: event.target.value as TaskReviewDraft['priority'] })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  disabled={panel.submitting === true}
                >
                  <option value="high">high</option>
                  <option value="medium">medium</option>
                  <option value="low">low</option>
                </select>
              </label>

              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                状态
                <select
                  value={draft.status}
                  onChange={(event) => updateDraft(draft.id, { status: event.target.value as TaskReviewDraft['status'] })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  disabled={panel.submitting === true}
                >
                  <option value="todo">todo</option>
                  <option value="doing">doing</option>
                  <option value="blocked">blocked</option>
                  <option value="done">done</option>
                </select>
              </label>

              <label className="md:col-span-2 flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                标签（逗号分隔）
                <input
                  type="text"
                  value={draft.tags.join(', ')}
                  onChange={(event) => {
                    const nextTags = event.target.value
                      .split(',')
                      .map((tag) => tag.trim())
                      .filter((tag, idx, arr) => Boolean(tag) && arr.indexOf(tag) === idx);
                    updateDraft(draft.id, { tags: nextTags });
                  }}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="frontend, urgent"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="md:col-span-2 flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                详情
                <textarea
                  value={draft.details}
                  onChange={(event) => updateDraft(draft.id, { details: event.target.value })}
                  className="min-h-[64px] rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="可选"
                  disabled={panel.submitting === true}
                />
              </label>
            </div>
          </div>
        ))}
      </div>

      {panel.error ? (
        <div className="mt-2 rounded-md border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200">
          {panel.error}
        </div>
      ) : null}

      <div className="mt-3 flex items-center justify-end gap-2">
        <button
          type="button"
          className="rounded-md border border-slate-300 px-3 py-1.5 text-sm text-slate-700 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
          onClick={handleCancel}
          disabled={panel.submitting === true}
        >
          取消
        </button>
        <button
          type="button"
          className="rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-60"
          onClick={handleConfirm}
          disabled={confirmDisabled}
        >
          {panel.submitting ? '提交中...' : '确定并创建任务'}
        </button>
      </div>
    </div>
  );
};

export default TaskDraftPanel;
