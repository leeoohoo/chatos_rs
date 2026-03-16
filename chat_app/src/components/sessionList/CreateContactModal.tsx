import React from 'react';

interface CreateContactModalProps {
  isOpen: boolean;
  agents: Array<{
    id: string;
    name: string;
    description?: string;
    enabled?: boolean;
  }>;
  selectedAgentId: string | null;
  error: string | null;
  onClose: () => void;
  onSelectedAgentChange: (agentId: string) => void;
  onCreate: () => void;
}

export const CreateContactModal: React.FC<CreateContactModalProps> = ({
  isOpen,
  agents,
  selectedAgentId,
  error,
  onClose,
  onSelectedAgentChange,
  onCreate,
}) => {
  if (!isOpen) {
    return null;
  }

  const enabledAgents = agents.filter((agent) => agent.enabled !== false);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="fixed inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg shadow-xl w-[520px] p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-foreground">添加联系人</h3>
          <button
            onClick={onClose}
            className="p-2 text-muted-foreground hover:text-foreground hover:bg-accent rounded-lg transition-colors"
          >
            <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="space-y-3">
          {enabledAgents.length === 0 ? (
            <div className="text-sm text-muted-foreground">
              当前没有可用联系人，请先在 Memory 中创建 Agent。
            </div>
          ) : (
            <div className="max-h-72 overflow-y-auto border border-border rounded">
              {enabledAgents.map((agent) => {
                const selected = selectedAgentId === agent.id;
                return (
                  <button
                    key={agent.id}
                    type="button"
                    onClick={() => onSelectedAgentChange(agent.id)}
                    className={[
                      'w-full text-left px-3 py-2 border-b border-border last:border-b-0',
                      selected ? 'bg-accent' : 'hover:bg-accent/60',
                    ].join(' ')}
                  >
                    <div className="text-sm font-medium text-foreground">{agent.name}</div>
                    {agent.description ? (
                      <div className="text-xs text-muted-foreground mt-1 line-clamp-2">
                        {agent.description}
                      </div>
                    ) : null}
                  </button>
                );
              })}
            </div>
          )}
          {error ? (
            <div className="text-xs text-destructive">{error}</div>
          ) : null}
        </div>
        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="px-3 py-2 rounded bg-muted text-muted-foreground hover:bg-accent"
          >
            取消
          </button>
          <button
            onClick={onCreate}
            disabled={!selectedAgentId || enabledAgents.length === 0}
            className="px-4 py-2 rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            添加并开始聊天
          </button>
        </div>
      </div>
    </div>
  );
};
