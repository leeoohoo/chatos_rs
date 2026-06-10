import React, { useEffect, useMemo, useState } from 'react';

import { useI18n } from '../../i18n/I18nProvider';
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

const blockerKindOptions: Array<{ value: string; labelKey: string }> = [
  { value: 'unknown', labelKey: 'taskOutcome.blockerKind.unknown' },
  { value: 'missing_information', labelKey: 'taskOutcome.blockerKind.missing_information' },
  { value: 'design_decision', labelKey: 'taskOutcome.blockerKind.design_decision' },
  { value: 'external_dependency', labelKey: 'taskOutcome.blockerKind.external_dependency' },
  { value: 'permission', labelKey: 'taskOutcome.blockerKind.permission' },
  { value: 'environment_failure', labelKey: 'taskOutcome.blockerKind.environment_failure' },
  { value: 'upstream_bug', labelKey: 'taskOutcome.blockerKind.upstream_bug' },
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
  const { t } = useI18n();
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
      return t('taskOutcome.completeTitle');
    }
    return t('taskOutcome.editTitle', {
      suffix: task?.title ? ` · ${task.title}` : '',
    });
  }, [isCompleteMode, t, task?.title]);

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
                ? t('taskOutcome.completeSubtitle')
                : t('taskOutcome.editSubtitle')}
            </div>
          </div>
          <button
            type="button"
            className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-50"
            onClick={onClose}
            disabled={submitting}
          >
            {t('common.close')}
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
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.title')}</span>
                  <input
                    type="text"
                    value={draft.title}
                    onChange={(event) => setDraft((prev) => ({ ...prev, title: event.target.value }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.priority')}</span>
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
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.details')}</span>
                  <textarea
                    value={draft.details}
                    onChange={(event) => setDraft((prev) => ({ ...prev, details: event.target.value }))}
                    rows={3}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.status')}</span>
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
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.dueAt')}</span>
                  <input
                    type="text"
                    value={draft.dueAt}
                    onChange={(event) => setDraft((prev) => ({ ...prev, dueAt: event.target.value }))}
                    placeholder={t('taskOutcome.clearDuePlaceholder')}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>
              </>
            ) : null}

            <label className="md:col-span-2 flex flex-col gap-1.5">
              <span className="text-xs font-medium text-foreground">
                {t('taskOutcome.outcomeSummary')}
                {(isCompleteMode || isBlocked) ? ' *' : ''}
              </span>
              <textarea
                value={draft.outcomeSummary}
                onChange={(event) => setDraft((prev) => ({ ...prev, outcomeSummary: event.target.value }))}
                rows={4}
                placeholder={t('taskOutcome.outcomePlaceholder')}
                className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                disabled={submitting}
              />
            </label>

            <label className="md:col-span-2 flex flex-col gap-1.5">
              <span className="text-xs font-medium text-foreground">{t('taskOutcome.resumeHint')}</span>
              <textarea
                value={draft.resumeHint}
                onChange={(event) => setDraft((prev) => ({ ...prev, resumeHint: event.target.value }))}
                rows={2}
                placeholder={t('taskOutcome.resumeHintPlaceholder')}
                className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                disabled={submitting}
              />
            </label>

            {isBlocked ? (
              <>
                <label className="md:col-span-2 flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.blockerReason')} *</span>
                  <textarea
                    value={draft.blockerReason}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerReason: event.target.value }))}
                    rows={3}
                    placeholder={t('taskOutcome.blockerReasonPlaceholder')}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.blockerKind')}</span>
                  <select
                    value={draft.blockerKind || 'unknown'}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerKind: event.target.value }))}
                    className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-2 focus:ring-ring"
                    disabled={submitting}
                  >
                    {blockerKindOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {t(option.labelKey)}
                      </option>
                    ))}
                  </select>
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-xs font-medium text-foreground">{t('taskOutcome.blockerNeeds')}</span>
                  <textarea
                    value={draft.blockerNeedsText}
                    onChange={(event) => setDraft((prev) => ({ ...prev, blockerNeedsText: event.target.value }))}
                    rows={3}
                    placeholder={t('taskOutcome.blockerNeedsPlaceholder')}
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
              ? t('taskOutcome.blockedHelp')
              : (isCompleteMode ? t('taskOutcome.doneHelp') : t('taskOutcome.editHelp'))}
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className="rounded-md border border-border bg-background px-3 py-2 text-sm text-foreground hover:bg-accent disabled:opacity-50"
              onClick={onClose}
              disabled={submitting}
            >
              {t('common.cancel')}
            </button>
            <button
              type="button"
              className="rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              onClick={() => onSubmit(draft)}
              disabled={submitting}
            >
              {submitting
                ? t('taskOutcome.submitting')
                : (isCompleteMode ? t('taskOutcome.submitComplete') : t('taskOutcome.submitEdit'))}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default TaskOutcomeModal;
