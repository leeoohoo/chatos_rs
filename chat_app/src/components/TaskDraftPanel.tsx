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
  status: 'pending_confirm',
  tags: [],
  dueAt: null,
  taskRef: null,
  taskKind: null,
  dependsOnRefs: [],
  verificationOfRefs: [],
  acceptanceCriteria: [],
  plannedBuiltinMcpIds: [],
  plannedContextAssets: [],
  executionResultContract: null,
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
  taskRef: draft.taskRef ? String(draft.taskRef).trim() : null,
  taskKind: draft.taskKind ? String(draft.taskKind).trim() : null,
  dependsOnRefs: (draft.dependsOnRefs || [])
    .map((item) => String(item || '').trim())
    .filter((item, index, arr) => Boolean(item) && arr.indexOf(item) === index),
  verificationOfRefs: (draft.verificationOfRefs || [])
    .map((item) => String(item || '').trim())
    .filter((item, index, arr) => Boolean(item) && arr.indexOf(item) === index),
  acceptanceCriteria: (draft.acceptanceCriteria || [])
    .map((item) => String(item || '').trim())
    .filter((item, index, arr) => Boolean(item) && arr.indexOf(item) === index),
  plannedBuiltinMcpIds: (draft.plannedBuiltinMcpIds || [])
    .map((item) => String(item || '').trim())
    .filter((item, index, arr) => Boolean(item) && arr.indexOf(item) === index),
  plannedContextAssets: (draft.plannedContextAssets || [])
    .map((asset) => ({
      assetType: String(asset.assetType || '').trim(),
      assetId: String(asset.assetId || '').trim(),
      displayName: asset.displayName ? String(asset.displayName).trim() : null,
      sourceType: asset.sourceType ? String(asset.sourceType).trim() : null,
      sourcePath: asset.sourcePath ? String(asset.sourcePath).trim() : null,
    }))
    .filter((asset) => asset.assetType && asset.assetId),
  executionResultContract: draft.executionResultContract
    ? {
      resultRequired: draft.executionResultContract.resultRequired !== false,
      preferredFormat: draft.executionResultContract.preferredFormat
        ? String(draft.executionResultContract.preferredFormat).trim()
        : null,
    }
    : null,
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

  const normalizedDrafts = useMemo(
    () => drafts.map(normalizeDraft),
    [drafts]
  );

  const planGraph = useMemo(() => {
    const refOwners = new Map<string, number>();
    const duplicateRefs = new Set<string>();
    const verificationTargets = new Set<string>();
    let dependencyEdgeCount = 0;
    let verificationEdgeCount = 0;

    normalizedDrafts.forEach((draft, index) => {
      const taskRef = draft.taskRef?.trim();
      if (!taskRef) {
        return;
      }
      if (refOwners.has(taskRef)) {
        duplicateRefs.add(taskRef);
        return;
      }
      refOwners.set(taskRef, index);
    });

    normalizedDrafts.forEach((draft) => {
      (draft.verificationOfRefs || []).forEach((taskRef) => {
        verificationTargets.add(taskRef);
        verificationEdgeCount += 1;
      });
      dependencyEdgeCount += draft.dependsOnRefs?.length || 0;
    });

    const errors: string[] = [];
    const warnings: string[] = [];

    if (duplicateRefs.size > 0) {
      errors.push(`存在重复的 task_ref: ${Array.from(duplicateRefs).join(' / ')}`);
    }

    const rows = normalizedDrafts.map((draft, index) => {
      const taskRef = draft.taskRef?.trim() || `draft_${index + 1}`;
      const dependsOnRefs = draft.dependsOnRefs || [];
      const verificationOfRefs = draft.verificationOfRefs || [];
      const missingDependencies = dependsOnRefs.filter((item) => !refOwners.has(item));
      const missingVerificationTargets = verificationOfRefs.filter((item) => !refOwners.has(item));
      const selfDependent = dependsOnRefs.includes(taskRef);
      const selfVerification = verificationOfRefs.includes(taskRef);
      const normalizedKind = (draft.taskKind || '').trim().toLowerCase();
      const looksLikeImplementation = normalizedKind === 'implementation'
        || (draft.plannedBuiltinMcpIds || []).some((item) =>
          item === 'builtin_code_maintainer_write' || item === 'builtin_terminal_controller');

      if (missingDependencies.length > 0) {
        errors.push(`${taskRef} 缺少前置任务引用: ${missingDependencies.join(' / ')}`);
      }
      if (missingVerificationTargets.length > 0) {
        errors.push(`${taskRef} 的验证对象不存在: ${missingVerificationTargets.join(' / ')}`);
      }
      if (selfDependent) {
        errors.push(`${taskRef} 不能依赖自己`);
      }
      if (selfVerification) {
        errors.push(`${taskRef} 不能把自己作为验证对象`);
      }
      if (looksLikeImplementation && !verificationTargets.has(taskRef)) {
        warnings.push(`${taskRef} 看起来是实现任务，但当前没有任何验证节点引用它`);
      }

      return {
        id: draft.id,
        taskRef,
        title: draft.title || `未命名任务 ${index + 1}`,
        taskKind: draft.taskKind || '未指定',
        status: draft.status,
        dependsOnRefs,
        verificationOfRefs,
        acceptanceCriteriaCount: draft.acceptanceCriteria?.length || 0,
        builtinCount: draft.plannedBuiltinMcpIds?.length || 0,
        assetCount: draft.plannedContextAssets?.length || 0,
      };
    });

    return {
      rows,
      dependencyEdgeCount,
      verificationEdgeCount,
      rootTaskCount: rows.filter((row) => row.dependsOnRefs.length === 0).length,
      verificationTaskCount: rows.filter((row) => row.verificationOfRefs.length > 0).length,
      implementationTaskCount: rows.filter((row) => row.taskKind.toLowerCase() === 'implementation').length,
      errors,
      warnings,
    };
  }, [normalizedDrafts]);

  const confirmDisabled = panel.submitting === true
    || drafts.length === 0
    || hasEmptyTitle
    || planGraph.errors.length > 0;

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

      <div className="mb-3 rounded-lg border border-amber-200 bg-white/80 p-3 dark:border-amber-800 dark:bg-slate-900/50">
        <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
          <span className="rounded-full bg-amber-100 px-2 py-1 text-amber-900 dark:bg-amber-900/40 dark:text-amber-100">
            {`节点 ${planGraph.rows.length}`}
          </span>
          <span className="rounded-full bg-slate-100 px-2 py-1 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
            {`根任务 ${planGraph.rootTaskCount}`}
          </span>
          <span className="rounded-full bg-slate-100 px-2 py-1 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
            {`依赖边 ${planGraph.dependencyEdgeCount}`}
          </span>
          <span className="rounded-full bg-slate-100 px-2 py-1 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
            {`验证边 ${planGraph.verificationEdgeCount}`}
          </span>
          <span className="rounded-full bg-slate-100 px-2 py-1 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
            {`实现节点 ${planGraph.implementationTaskCount}`}
          </span>
          <span className="rounded-full bg-slate-100 px-2 py-1 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
            {`验证节点 ${planGraph.verificationTaskCount}`}
          </span>
        </div>

        <div className="space-y-2">
          {planGraph.rows.map((row, index) => (
            <div
              key={`graph-${row.id}`}
              className="rounded-md border border-slate-200 bg-white px-3 py-2 text-xs dark:border-slate-800 dark:bg-slate-950/60"
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-semibold text-slate-900 dark:text-slate-100">{`#${index + 1} ${row.title}`}</span>
                <span className="rounded-full bg-slate-100 px-2 py-0.5 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
                  {row.taskRef}
                </span>
                <span className="rounded-full bg-slate-100 px-2 py-0.5 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
                  {row.taskKind}
                </span>
                <span className="rounded-full bg-slate-100 px-2 py-0.5 text-slate-700 dark:bg-slate-800 dark:text-slate-200">
                  {row.status}
                </span>
              </div>
              <div className="mt-1 space-y-1 text-slate-600 dark:text-slate-300">
                <div>{`前置: ${row.dependsOnRefs.length > 0 ? row.dependsOnRefs.join(' / ') : '无'}`}</div>
                <div>{`验证对象: ${row.verificationOfRefs.length > 0 ? row.verificationOfRefs.join(' / ') : '无'}`}</div>
                <div>{`验收 ${row.acceptanceCriteriaCount} 条 · MCP ${row.builtinCount} 个 · 资产 ${row.assetCount} 个`}</div>
              </div>
            </div>
          ))}
        </div>

        {planGraph.errors.length > 0 ? (
          <div className="mt-3 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-200">
            <div className="mb-1 font-semibold">确认前需修复的问题</div>
            {planGraph.errors.map((item) => (
              <div key={`plan-error-${item}`}>{item}</div>
            ))}
          </div>
        ) : null}

        {planGraph.warnings.length > 0 ? (
          <div className="mt-3 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-700 dark:bg-amber-950/30 dark:text-amber-200">
            <div className="mb-1 font-semibold">建议再检查</div>
            {planGraph.warnings.map((item) => (
              <div key={`plan-warning-${item}`}>{item}</div>
            ))}
          </div>
        ) : null}
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
                  <option value="pending_confirm">pending_confirm</option>
                  <option value="pending_execute">pending_execute</option>
                  <option value="running">running</option>
                  <option value="paused">paused</option>
                  <option value="blocked">blocked</option>
                  <option value="completed">completed</option>
                  <option value="failed">failed</option>
                  <option value="cancelled">cancelled</option>
                  <option value="skipped">skipped</option>
                </select>
              </label>

              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                task_ref
                <input
                  type="text"
                  value={draft.taskRef || ''}
                  onChange={(event) => updateDraft(draft.id, { taskRef: event.target.value || null })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="例如: impl_api"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                task_kind
                <input
                  type="text"
                  value={draft.taskKind || ''}
                  onChange={(event) => updateDraft(draft.id, { taskKind: event.target.value || null })}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="例如: implementation / verification"
                  disabled={panel.submitting === true}
                />
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
                depends_on_refs（逗号分隔）
                <input
                  type="text"
                  value={(draft.dependsOnRefs || []).join(', ')}
                  onChange={(event) => {
                    const nextValues = event.target.value
                      .split(',')
                      .map((item) => item.trim())
                      .filter((item, idx, arr) => Boolean(item) && arr.indexOf(item) === idx);
                    updateDraft(draft.id, { dependsOnRefs: nextValues });
                  }}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="例如: analyze_api, impl_api"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="md:col-span-2 flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                verification_of_refs（逗号分隔）
                <input
                  type="text"
                  value={(draft.verificationOfRefs || []).join(', ')}
                  onChange={(event) => {
                    const nextValues = event.target.value
                      .split(',')
                      .map((item) => item.trim())
                      .filter((item, idx, arr) => Boolean(item) && arr.indexOf(item) === idx);
                    updateDraft(draft.id, { verificationOfRefs: nextValues });
                  }}
                  className="rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder="例如: impl_api"
                  disabled={panel.submitting === true}
                />
              </label>

              <label className="md:col-span-2 flex flex-col gap-1 text-xs text-slate-600 dark:text-slate-300">
                acceptance_criteria（每行一条）
                <textarea
                  value={(draft.acceptanceCriteria || []).join('\n')}
                  onChange={(event) => {
                    const nextValues = event.target.value
                      .split('\n')
                      .map((item) => item.trim())
                      .filter((item, idx, arr) => Boolean(item) && arr.indexOf(item) === idx);
                    updateDraft(draft.id, { acceptanceCriteria: nextValues });
                  }}
                  className="min-h-[72px] rounded-md border border-slate-300 bg-white px-2 py-1 text-sm text-slate-900 outline-none focus:border-primary dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100"
                  placeholder={'例如:\n接口返回 200\n测试用例通过'}
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
