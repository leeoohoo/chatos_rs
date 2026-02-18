import React, { useMemo } from 'react';

export interface TaskWorkbarItem {
  id: string;
  title: string;
  details: string;
  status: 'todo' | 'doing' | 'blocked' | 'done';
  priority: 'high' | 'medium' | 'low';
  conversationTurnId: string;
  createdAt: string;
  dueAt?: string | null;
  tags: string[];
}

interface TaskWorkbarProps {
  tasks: TaskWorkbarItem[];
  isLoading?: boolean;
  error?: string | null;
  onRefresh?: () => void;
}

const statusStyles: Record<TaskWorkbarItem['status'], string> = {
  todo: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-100',
  doing: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200',
  blocked: 'bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-200',
  done: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200',
};

const priorityStyles: Record<TaskWorkbarItem['priority'], string> = {
  high: 'text-rose-600 dark:text-rose-300',
  medium: 'text-amber-600 dark:text-amber-300',
  low: 'text-slate-600 dark:text-slate-300',
};

export const TaskWorkbar: React.FC<TaskWorkbarProps> = ({ tasks, isLoading = false, error = null, onRefresh }) => {
  const sortedTasks = useMemo(() => {
    return [...tasks].sort((a, b) => {
      const left = Date.parse(a.createdAt) || 0;
      const right = Date.parse(b.createdAt) || 0;
      return right - left;
    });
  }, [tasks]);

  return (
    <div className="mx-3 mt-3 rounded-xl border border-slate-200 bg-slate-50 p-3 dark:border-slate-800 dark:bg-slate-900/50">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-semibold text-slate-900 dark:text-slate-100">Workbar</div>
          <div className="text-xs text-slate-500 dark:text-slate-400">当前会话任务：{sortedTasks.length}</div>
        </div>
        {onRefresh ? (
          <button
            type="button"
            className="rounded-md border border-slate-300 px-2 py-1 text-xs text-slate-700 hover:bg-slate-100 dark:border-slate-700 dark:text-slate-200 dark:hover:bg-slate-800"
            onClick={onRefresh}
            disabled={isLoading}
          >
            {isLoading ? '刷新中...' : '刷新'}
          </button>
        ) : null}
      </div>

      {error ? (
        <div className="mb-2 rounded-md border border-rose-200 bg-rose-50 px-2 py-1 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-200">
          {error}
        </div>
      ) : null}

      {isLoading && sortedTasks.length === 0 ? (
        <div className="text-xs text-slate-500 dark:text-slate-400">任务加载中...</div>
      ) : null}

      {!isLoading && sortedTasks.length === 0 ? (
        <div className="text-xs text-slate-500 dark:text-slate-400">还没有任务，创建任务后会在这里展示。</div>
      ) : null}

      {sortedTasks.length > 0 ? (
        <div className="flex gap-2 overflow-x-auto pb-1">
          {sortedTasks.map((task) => (
            <div
              key={task.id}
              className="min-w-[220px] max-w-[260px] rounded-lg border border-slate-200 bg-white p-2 dark:border-slate-700 dark:bg-slate-900"
            >
              <div className="mb-1 flex items-start justify-between gap-2">
                <div className="line-clamp-2 text-sm font-medium text-slate-900 dark:text-slate-100">{task.title}</div>
                <span className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${statusStyles[task.status]}`}>
                  {task.status}
                </span>
              </div>

              {task.details ? (
                <div className="mb-1 line-clamp-2 text-xs text-slate-600 dark:text-slate-300">{task.details}</div>
              ) : null}

              <div className="mb-1 text-[11px] text-slate-500 dark:text-slate-400">
                <span className={priorityStyles[task.priority]}>优先级 {task.priority}</span>
                <span className="mx-1">·</span>
                <span>轮次 {task.conversationTurnId}</span>
              </div>

              {task.tags.length > 0 ? (
                <div className="mb-1 flex flex-wrap gap-1">
                  {task.tags.map((tag) => (
                    <span
                      key={task.id + '_' + tag}
                      className="rounded bg-slate-100 px-1.5 py-0.5 text-[10px] text-slate-600 dark:bg-slate-800 dark:text-slate-300"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
              ) : null}

              {task.dueAt ? (
                <div className="text-[11px] text-slate-500 dark:text-slate-400">截止 {task.dueAt}</div>
              ) : null}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
};

export default TaskWorkbar;
