import React from 'react';
import type { TurnRuntimeSnapshotLookupResponse } from '../../lib/api/client/types';

interface TurnRuntimeContextDrawerProps {
  open: boolean;
  sessionId: string | null;
  loading: boolean;
  error: string | null;
  data: TurnRuntimeSnapshotLookupResponse | null;
  onRefresh: () => void;
  onClose: () => void;
}

const TurnRuntimeContextDrawer: React.FC<TurnRuntimeContextDrawerProps> = ({
  open,
  sessionId,
  loading,
  error,
  data,
  onRefresh,
  onClose,
}) => {
  if (!open) {
    return null;
  }

  const snapshot = data?.snapshot || null;
  const systemMessages = Array.isArray(snapshot?.system_messages)
    ? snapshot?.system_messages
    : [];
  const tools = Array.isArray(snapshot?.tools) ? snapshot?.tools : [];
  const runtime = snapshot?.runtime || null;
  const snapshotSource = data?.snapshot_source || 'missing';
  const status = data?.status || 'unknown';

  return (
    <div className="fixed inset-0 z-50">
      <button
        type="button"
        aria-label="关闭上下文快照抽屉"
        className="absolute inset-0 bg-black/35"
        onClick={onClose}
      />
      <div className="absolute right-0 top-0 h-full w-full max-w-2xl border-l border-border bg-card shadow-xl">
        <div className="flex h-full flex-col">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <div>
              <div className="text-sm font-semibold text-foreground">轮次运行上下文</div>
              <div className="text-xs text-muted-foreground">
                {sessionId ? `会话 ${sessionId}` : '未选择会话'}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent disabled:opacity-60 disabled:cursor-not-allowed"
                disabled={!sessionId || loading}
                onClick={onRefresh}
              >
                {loading ? '刷新中...' : '刷新'}
              </button>
              <button
                type="button"
                className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground hover:bg-accent"
                onClick={onClose}
              >
                关闭
              </button>
            </div>
          </div>

          <div className="custom-scrollbar flex-1 overflow-y-scroll px-4 py-3 [scrollbar-gutter:stable]">
            {error ? (
              <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            <div className="mb-3 rounded-md border border-border bg-background/70 p-3 text-xs text-muted-foreground">
              <div>{`turn_id: ${data?.turn_id || '-'}`}</div>
              <div>{`status: ${status}`}</div>
              <div>{`snapshot_source: ${snapshotSource}`}</div>
              <div>{`captured_at: ${snapshot?.captured_at || '-'}`}</div>
            </div>

            {snapshotSource !== 'captured' || !snapshot ? (
              <div className="rounded-md border border-border bg-background/70 px-3 py-2 text-xs text-muted-foreground">
                当前轮次暂无快照（snapshot_source=missing）
              </div>
            ) : (
              <div className="space-y-4">
                <div>
                  <div className="mb-2 text-sm font-medium text-foreground">System 消息</div>
                  {systemMessages.length === 0 ? (
                    <div className="text-xs text-muted-foreground">无 system 消息</div>
                  ) : (
                    <div className="space-y-2">
                      {systemMessages.map((item) => (
                        <div key={`${item.id}:${item.source}`} className="rounded-md border border-border bg-background/80 p-2">
                          <div className="mb-1 text-[11px] text-muted-foreground">
                            {`${item.id} · ${item.source}`}
                          </div>
                          <pre className="whitespace-pre-wrap break-words text-xs text-foreground">
{item.content}
                          </pre>
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div>
                  <div className="mb-2 text-sm font-medium text-foreground">工具列表</div>
                  {tools.length === 0 ? (
                    <div className="text-xs text-muted-foreground">无工具</div>
                  ) : (
                    <div className="space-y-2">
                      {tools.map((tool) => (
                        <div key={`${tool.server_name}:${tool.name}`} className="rounded-md border border-border bg-background/80 p-2">
                          <div className="text-xs font-medium text-foreground">{tool.name}</div>
                          <div className="text-[11px] text-muted-foreground">
                            {`${tool.server_type} · ${tool.server_name}`}
                          </div>
                          {tool.description ? (
                            <div className="mt-1 text-xs text-muted-foreground">{tool.description}</div>
                          ) : null}
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div>
                  <div className="mb-2 text-sm font-medium text-foreground">运行时</div>
                  <div className="rounded-md border border-border bg-background/80 p-2 text-xs text-muted-foreground">
                    <div>{`model: ${runtime?.model || '-'}`}</div>
                    <div>{`provider: ${runtime?.provider || '-'}`}</div>
                    <div>{`contact_agent_id: ${runtime?.contact_agent_id || '-'}`}</div>
                    <div>{`project_id: ${runtime?.project_id || '-'}`}</div>
                    <div>{`project_root: ${runtime?.project_root || '-'}`}</div>
                    <div>{`mcp_enabled: ${runtime?.mcp_enabled === true ? 'true' : runtime?.mcp_enabled === false ? 'false' : '-'}`}</div>
                    <div>{`enabled_mcp_ids: ${Array.isArray(runtime?.enabled_mcp_ids) ? runtime?.enabled_mcp_ids.join(', ') : '-'}`}</div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default TurnRuntimeContextDrawer;
