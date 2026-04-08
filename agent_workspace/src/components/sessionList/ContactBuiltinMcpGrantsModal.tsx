import React from 'react';
import type { TaskCapabilityResponse } from '../../lib/api/client/types';

interface ContactBuiltinMcpGrantsModalProps {
  isOpen: boolean;
  contactName: string;
  options: TaskCapabilityResponse[];
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
  options,
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
          ) : options.length === 0 ? (
            <div className="rounded-lg border border-dashed border-border px-3 py-6 text-sm text-muted-foreground">
              当前没有可授权的内置 MCP 能力。
            </div>
          ) : (
            options.map((item) => {
              const checked = selectedIds.includes(item.builtin_mcp_id);
              return (
                <label
                  key={item.builtin_mcp_id}
                  className="flex items-start gap-3 rounded-lg border border-border px-3 py-3 hover:bg-accent/40"
                >
                  <input
                    type="checkbox"
                    className="mt-1"
                    checked={checked}
                    disabled={saving}
                    onChange={() => onToggle(item.builtin_mcp_id)}
                  />
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium text-foreground">{item.display_name}</span>
                      <span className="rounded-full bg-accent px-2 py-0.5 text-[11px] text-muted-foreground">
                        {item.token}
                      </span>
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">{item.description}</div>
                    <div className="mt-1 text-[11px] text-muted-foreground/80">{item.builtin_mcp_id}</div>
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
