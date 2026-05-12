import type { FormEvent } from 'react';

import type { AgentFormData } from './types';

interface AgentManagerFormProps {
  showForm: boolean;
  editingAgentId: string | null;
  formData: AgentFormData;
  pluginOptions: Array<{ value: string; label: string }>;
  skillOptions: Array<{ value: string; label: string }>;
  onToggleForm: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => Promise<void>;
  onCancel: () => void;
  onFormDataChange: (patch: Partial<AgentFormData>) => void;
  onOpenAiCreate: () => void;
}

const AgentManagerForm = ({
  showForm,
  editingAgentId,
  formData,
  pluginOptions,
  skillOptions,
  onToggleForm,
  onSubmit,
  onCancel,
  onFormDataChange,
  onOpenAiCreate,
}: AgentManagerFormProps) => {
  if (!showForm) {
    return (
      <div className="flex items-center gap-2 pb-4">
        <button
          onClick={onToggleForm}
          className="px-3 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
        >
          新建智能体
        </button>
        <button
          onClick={onOpenAiCreate}
          className="px-3 py-2 text-sm rounded-lg bg-muted hover:bg-accent transition-colors"
        >
          AI 创建
        </button>
      </div>
    );
  }

  return (
    <form onSubmit={(event) => void onSubmit(event)} className="space-y-4 rounded-xl border border-border bg-background/40 p-4">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-foreground">
          {editingAgentId ? '编辑智能体' : '新建智能体'}
        </h3>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onOpenAiCreate}
            className="px-2.5 py-1.5 text-xs rounded-md bg-muted hover:bg-accent transition-colors"
          >
            AI 创建
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="px-2.5 py-1.5 text-xs rounded-md bg-muted hover:bg-accent transition-colors"
          >
            取消
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <label className="space-y-1">
          <span className="text-xs text-muted-foreground">名称</span>
          <input
            value={formData.name}
            onChange={(event) => onFormDataChange({ name: event.target.value })}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            placeholder="输入智能体名称"
          />
        </label>
        <label className="space-y-1">
          <span className="text-xs text-muted-foreground">分类</span>
          <input
            value={formData.category}
            onChange={(event) => onFormDataChange({ category: event.target.value })}
            className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
            placeholder="输入分类"
          />
        </label>
      </div>

      <label className="space-y-1 block">
        <span className="text-xs text-muted-foreground">描述</span>
        <input
          value={formData.description}
          onChange={(event) => onFormDataChange({ description: event.target.value })}
          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
          placeholder="补充用途和边界"
        />
      </label>

      <label className="space-y-1 block">
        <span className="text-xs text-muted-foreground">角色定义</span>
        <textarea
          value={formData.roleDefinition}
          onChange={(event) => onFormDataChange({ roleDefinition: event.target.value })}
          rows={5}
          className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
          placeholder="描述这个智能体的职责、行为边界和输出风格"
        />
      </label>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
        <label className="space-y-1 block">
          <span className="text-xs text-muted-foreground">插件引用</span>
          <select
            multiple
            value={formData.pluginSources}
            onChange={(event) => {
              const values = Array.from(event.currentTarget.selectedOptions).map((option) => option.value);
              onFormDataChange({ pluginSources: values });
            }}
            className="w-full min-h-36 rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
          >
            {pluginOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <label className="space-y-1 block">
          <span className="text-xs text-muted-foreground">技能引用</span>
          <select
            multiple
            value={formData.skillIds}
            onChange={(event) => {
              const values = Array.from(event.currentTarget.selectedOptions).map((option) => option.value);
              onFormDataChange({ skillIds: values });
            }}
            className="w-full min-h-36 rounded-lg border border-border bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-primary/30"
          >
            {skillOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      </div>

      <label className="inline-flex items-center gap-2 text-sm text-foreground">
        <input
          type="checkbox"
          checked={formData.enabled}
          onChange={(event) => onFormDataChange({ enabled: event.target.checked })}
          className="rounded border-border"
        />
        启用
      </label>

      <div className="flex items-center justify-end gap-2">
        <button
          type="button"
          onClick={onCancel}
          className="px-3 py-2 text-sm rounded-lg bg-muted hover:bg-accent transition-colors"
        >
          取消
        </button>
        <button
          type="submit"
          className="px-3 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:opacity-90 transition-opacity"
        >
          {editingAgentId ? '保存修改' : '创建智能体'}
        </button>
      </div>
    </form>
  );
};

export default AgentManagerForm;
