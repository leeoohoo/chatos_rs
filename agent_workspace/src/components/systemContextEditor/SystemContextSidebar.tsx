import { readContextName, readContextUpdatedAt } from './helpers';
import type { SystemContextLike } from './types';

interface SystemContextSidebarProps {
  isLoading: boolean;
  searchKeyword: string;
  selectedContextId: string | null;
  filteredContexts: SystemContextLike[];
  onSearchKeywordChange: (value: string) => void;
  onCreate: () => void;
  onSelectContext: (context: SystemContextLike) => void;
  onDeleteContext: (context: SystemContextLike) => void;
}

const PlusIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
  </svg>
);

const TrashIcon = () => (
  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
  </svg>
);

export default function SystemContextSidebar({
  isLoading,
  searchKeyword,
  selectedContextId,
  filteredContexts,
  onSearchKeywordChange,
  onCreate,
  onSelectContext,
  onDeleteContext,
}: SystemContextSidebarProps) {
  return (
    <aside className="w-80 min-w-80 border-r border-border flex flex-col">
      <div className="p-4 border-b border-border space-y-3">
        <button
          onClick={onCreate}
          className="w-full inline-flex items-center justify-center gap-2 px-3 py-2 text-sm bg-blue-600 text-white rounded-md hover:bg-blue-700"
        >
          <PlusIcon />
          <span>新建提示词</span>
        </button>
        <input
          type="text"
          value={searchKeyword}
          onChange={(event) => onSearchKeywordChange(event.target.value)}
          placeholder="搜索提示词"
          className="w-full px-3 py-2 text-sm border border-input bg-background rounded-md focus:outline-none focus:ring-2 focus:ring-ring"
        />
      </div>

      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="p-4 text-sm text-muted-foreground">加载中...</div>
        ) : filteredContexts.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground">暂无提示词</div>
        ) : (
          <ul className="divide-y divide-border">
            {filteredContexts.map((context) => {
              const active = context.id === selectedContextId;
              return (
                <li key={context.id} className={active ? 'bg-blue-50 dark:bg-blue-950/20' : ''}>
                  <div className="flex items-center justify-between gap-2 px-4 py-3">
                    <button
                      onClick={() => onSelectContext(context)}
                      className="flex-1 text-left"
                    >
                      <p className="text-sm font-medium truncate">{readContextName(context)}</p>
                      <p className="text-xs text-muted-foreground truncate">
                        更新时间：{readContextUpdatedAt(context)}
                      </p>
                    </button>
                    <button
                      onClick={() => onDeleteContext(context)}
                      className="p-1 text-muted-foreground hover:text-red-600"
                      title="删除"
                    >
                      <TrashIcon />
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </aside>
  );
}
