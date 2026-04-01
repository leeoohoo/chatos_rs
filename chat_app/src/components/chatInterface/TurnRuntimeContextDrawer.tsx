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

function renderValue(value?: string | null): string {
  const normalized = value?.trim();
  return normalized ? normalized : '-';
}

function renderBoolean(value?: boolean | null): string {
  if (value === true) {
    return 'true';
  }
  if (value === false) {
    return 'false';
  }
  return '-';
}

interface RuntimeFieldProps {
  label: string;
  value: string;
  tone?: 'default' | 'code';
}

const RuntimeField: React.FC<RuntimeFieldProps> = ({
  label,
  value,
  tone = 'default',
}) => (
  <div className="rounded-md border border-border bg-background/70 p-2">
    <div className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</div>
    <div
      className={
        tone === 'code'
          ? 'mt-1 break-all font-mono text-xs text-foreground'
          : 'mt-1 break-words text-xs text-foreground'
      }
    >
      {value}
    </div>
  </div>
);

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
  const selectedCommands = Array.isArray(runtime?.selected_commands)
    ? runtime?.selected_commands
    : [];
  const explicitSelectedCommands = selectedCommands.filter(
    (item) => (item?.trigger || '').toLowerCase() === 'explicit',
  );
  const implicitSelectedCommands = selectedCommands.filter(
    (item) => (item?.trigger || '').toLowerCase() === 'implicit',
  );
  const otherSelectedCommands = selectedCommands.filter((item) => {
    const trigger = (item?.trigger || '').toLowerCase();
    return trigger !== 'explicit' && trigger !== 'implicit';
  });
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

            <div className="mb-3 rounded-md border border-sky-500/30 bg-sky-500/10 p-3 text-xs text-sky-950 dark:text-sky-100">
              这里展示的是最近一轮已经发送到后端并被快照记录的 runtime，不包含输入框里尚未发送的临时目录、工具或 MCP 改动。
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
                  <div className="mb-2 text-sm font-medium text-foreground">命令使用</div>
                  {selectedCommands.length === 0 ? (
                    <div className="text-xs text-muted-foreground">本轮未命中 command</div>
                  ) : (
                    <div className="space-y-3">
                      <div>
                        <div className="mb-1 text-xs font-medium text-foreground">显式触发（/command）</div>
                        {explicitSelectedCommands.length === 0 ? (
                          <div className="text-xs text-muted-foreground">无</div>
                        ) : (
                          <div className="space-y-2">
                            {explicitSelectedCommands.map((item, index) => (
                              <div
                                key={`explicit:${item.command_ref || '-'}:${item.plugin_source}:${item.source_path}:${index}`}
                                className="rounded-md border border-border bg-background/80 p-2"
                              >
                                <div className="text-xs font-medium text-foreground">
                                  {item.command_ref || '-'}{item.name ? ` · ${item.name}` : ''}
                                </div>
                                <div className="mt-0.5 text-[11px] text-muted-foreground">
                                  {`${item.plugin_source} · ${item.source_path}`}
                                </div>
                                {item.arguments ? (
                                  <pre className="mt-1 whitespace-pre-wrap break-words text-xs text-muted-foreground">
{`args: ${item.arguments}`}
                                  </pre>
                                ) : null}
                              </div>
                            ))}
                          </div>
                        )}
                      </div>

                      <div>
                        <div className="mb-1 text-xs font-medium text-foreground">隐式触发（工具读取）</div>
                        {implicitSelectedCommands.length === 0 ? (
                          <div className="text-xs text-muted-foreground">无</div>
                        ) : (
                          <div className="space-y-2">
                            {implicitSelectedCommands.map((item, index) => (
                              <div
                                key={`implicit:${item.command_ref || '-'}:${item.plugin_source}:${item.source_path}:${index}`}
                                className="rounded-md border border-border bg-background/80 p-2"
                              >
                                <div className="text-xs font-medium text-foreground">
                                  {item.command_ref || '-'}{item.name ? ` · ${item.name}` : ''}
                                </div>
                                <div className="mt-0.5 text-[11px] text-muted-foreground">
                                  {`${item.plugin_source} · ${item.source_path}`}
                                </div>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>

                      {otherSelectedCommands.length > 0 ? (
                        <div>
                          <div className="mb-1 text-xs font-medium text-foreground">其他触发</div>
                          <div className="space-y-2">
                            {otherSelectedCommands.map((item, index) => (
                              <div
                                key={`other:${item.command_ref || '-'}:${item.plugin_source}:${item.source_path}:${index}`}
                                className="rounded-md border border-border bg-background/80 p-2"
                              >
                                <div className="text-xs font-medium text-foreground">
                                  {item.command_ref || '-'}{item.name ? ` · ${item.name}` : ''}
                                </div>
                                <div className="mt-0.5 text-[11px] text-muted-foreground">
                                  {`${item.plugin_source} · ${item.source_path}`}
                                </div>
                                <div className="mt-1 text-[11px] text-muted-foreground">
                                  {`trigger: ${item.trigger || '-'}`}
                                </div>
                              </div>
                            ))}
                          </div>
                        </div>
                      ) : null}
                    </div>
                  )}
                </div>

                <div>
                  <div className="mb-2 text-sm font-medium text-foreground">运行时</div>
                  <div className="space-y-3">
                    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                      <RuntimeField label="model" value={renderValue(runtime?.model)} />
                      <RuntimeField label="provider" value={renderValue(runtime?.provider)} />
                      <RuntimeField
                        label="contact_agent_id"
                        value={renderValue(runtime?.contact_agent_id)}
                        tone="code"
                      />
                      <RuntimeField
                        label="remote_connection_id"
                        value={renderValue(runtime?.remote_connection_id)}
                        tone="code"
                      />
                      <RuntimeField
                        label="project_id"
                        value={renderValue(runtime?.project_id)}
                        tone="code"
                      />
                      <RuntimeField
                        label="mcp_enabled"
                        value={renderBoolean(runtime?.mcp_enabled)}
                      />
                    </div>

                    <div className="rounded-md border border-border bg-background/80 p-3">
                      <div className="mb-2 text-xs font-medium text-foreground">目录上下文</div>
                      <div className="space-y-2">
                        <RuntimeField
                          label="本轮执行根目录（后端字段 project_root）"
                          value={renderValue(runtime?.project_root)}
                          tone="code"
                        />
                        <RuntimeField
                          label="workspace_root"
                          value={renderValue(runtime?.workspace_root)}
                          tone="code"
                        />
                      </div>
                    </div>

                    <div className="rounded-md border border-border bg-background/80 p-3">
                      <div className="mb-2 text-xs font-medium text-foreground">MCP</div>
                      {Array.isArray(runtime?.enabled_mcp_ids) && runtime.enabled_mcp_ids.length > 0 ? (
                        <div className="flex flex-wrap gap-2">
                          {runtime.enabled_mcp_ids.map((mcpId) => (
                            <span
                              key={mcpId}
                              className="rounded-full border border-border bg-background px-2 py-1 font-mono text-[11px] text-foreground"
                            >
                              {mcpId}
                            </span>
                          ))}
                        </div>
                      ) : (
                        <div className="text-xs text-muted-foreground">本轮未启用任何 MCP ID</div>
                      )}
                    </div>
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
