import React from 'react';

export const CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_OPTIONS = [
  {
    id: 'builtin_code_maintainer_read',
    label: '查看',
    description: '允许未来任务执行只读查看与代码理解。',
  },
  {
    id: 'builtin_code_maintainer_write',
    label: '读写',
    description: '允许未来任务执行写文件与代码修改。',
  },
  {
    id: 'builtin_terminal_controller',
    label: '终端',
    description: '允许未来任务执行终端命令。',
  },
  {
    id: 'builtin_remote_connection_controller',
    label: '远程连接',
    description: '允许未来任务使用远程连接能力。',
  },
  {
    id: 'builtin_notepad',
    label: 'Notepad',
    description: '允许未来任务使用记事本能力。',
  },
  {
    id: 'builtin_agent_builder',
    label: 'Agent Builder',
    description: '允许未来任务使用智能体构建能力。',
  },
  {
    id: 'builtin_ui_prompter',
    label: 'UI Prompter',
    description: '允许未来任务请求用户补充输入或确认信息。',
  },
] as const;

export const CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_IDS = CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_OPTIONS
  .map((item) => item.id);

export const CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_ID_SET = new Set<string>(
  CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_IDS,
);

interface ContactBuiltinMcpGrantsModalProps {
  isOpen: boolean;
  contactName: string;
  selectedIds: string[];
  loading: boolean;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onToggle: (mcpId: string) => void;
  onSave: () => void;
}

export const ContactBuiltinMcpGrantsModal: React.FC<ContactBuiltinMcpGrantsModalProps> = ({
  isOpen,
  contactName,
  selectedIds,
  loading,
  saving,
  error,
  onClose,
  onToggle,
  onSave,
}) => {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4">
      <div className="w-full max-w-xl rounded-xl border border-border bg-card shadow-2xl">
        <div className="border-b border-border px-5 py-4">
          <h3 className="text-lg font-semibold text-foreground">内置 MCP 授权</h3>
          <p className="mt-1 text-sm text-muted-foreground">
            {contactName || '当前联系人'}
            {' '}
            未来创建任务时，只能从这里勾选任务执行期可用的内置能力。
            任务类 MCP 属于系统默认核心能力，不在这里授权；这里仅配置其他可授权的执行能力。
          </p>
        </div>

        <div className="max-h-[60vh] overflow-y-auto px-5 py-4 space-y-3">
          {loading ? (
            <div className="text-sm text-muted-foreground">加载中...</div>
          ) : (
            CONTACT_TASK_AUTHORIZABLE_BUILTIN_MCP_OPTIONS.map((item) => {
              const checked = selectedIds.includes(item.id);
              return (
                <label
                  key={item.id}
                  className="flex items-start gap-3 rounded-lg border border-border px-3 py-3 hover:bg-accent/40"
                >
                  <input
                    type="checkbox"
                    className="mt-1"
                    checked={checked}
                    disabled={saving}
                    onChange={() => onToggle(item.id)}
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground">{item.label}</span>
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">{item.description}</div>
                    <div className="mt-1 text-[11px] text-muted-foreground/80">{item.id}</div>
                  </div>
                </label>
              );
            })
          )}
          {error && (
            <div className="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
          <button
            type="button"
            className="rounded-lg border border-border px-4 py-2 text-sm text-muted-foreground hover:bg-accent"
            onClick={onClose}
            disabled={saving}
          >
            关闭
          </button>
          <button
            type="button"
            className="rounded-lg bg-primary px-4 py-2 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            onClick={onSave}
            disabled={loading || saving}
          >
            {saving ? '保存中...' : '保存'}
          </button>
        </div>
      </div>
    </div>
  );
};
