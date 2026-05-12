import type { AgentConfig } from '../../types';

interface AgentListProps {
  agents: AgentConfig[];
  onEdit: (agent: AgentConfig) => void;
  onDelete: (agentId: string) => Promise<void>;
  onInspectSessions: (agent: AgentConfig) => Promise<void>;
}

const AgentList = ({ agents, onEdit, onDelete, onInspectSessions }: AgentListProps) => {
  if (!agents.length) {
    return (
      <div className="rounded-xl border border-dashed border-border p-6 text-sm text-muted-foreground">
        还没有智能体，先创建一个吧。
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {agents.map((agent) => (
        <div key={agent.id} className="rounded-xl border border-border bg-background/40 p-4">
          <div className="flex items-start justify-between gap-3">
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <h3 className="text-sm font-semibold text-foreground truncate">{agent.name}</h3>
                <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-[11px] ${
                  agent.enabled !== false
                    ? 'bg-emerald-500/15 text-emerald-600'
                    : 'bg-muted text-muted-foreground'
                }`}>
                  {agent.enabled !== false ? '启用' : '停用'}
                </span>
              </div>
              {agent.category ? (
                <div className="mt-1 text-xs text-muted-foreground">{agent.category}</div>
              ) : null}
              {agent.description ? (
                <div className="mt-2 text-sm text-muted-foreground">{agent.description}</div>
              ) : null}
              <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground">
                <span>插件 {Array.isArray(agent.plugin_sources) ? agent.plugin_sources.length : 0}</span>
                <span>技能 {Array.isArray(agent.skill_ids) ? agent.skill_ids.length : 0}</span>
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <button
                onClick={() => {
                  void onInspectSessions(agent);
                }}
                className="px-2.5 py-1.5 text-xs rounded-md bg-muted hover:bg-accent transition-colors"
              >
                会话
              </button>
              <button
                onClick={() => onEdit(agent)}
                className="px-2.5 py-1.5 text-xs rounded-md bg-muted hover:bg-accent transition-colors"
              >
                编辑
              </button>
              <button
                onClick={() => {
                  void onDelete(agent.id);
                }}
                className="px-2.5 py-1.5 text-xs rounded-md bg-red-500/10 text-red-600 hover:bg-red-500/15 transition-colors"
              >
                删除
              </button>
            </div>
          </div>
        </div>
      ))}
    </div>
  );
};

export default AgentList;
