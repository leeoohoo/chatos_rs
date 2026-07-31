// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { pluginRunSnapshotSummary } from './pluginRunSnapshot';

const shortIdentity = (value: string): string => (
  value.length > 28 ? `${value.slice(0, 14)}…${value.slice(-8)}` : value
);

const joinedOrDash = (values: string[]): string => (
  values.length > 0 ? values.join('、') : '-'
);

export const PluginRunSnapshotCard: FC<{ inputSnapshot: unknown }> = ({ inputSnapshot }) => {
  const summary = pluginRunSnapshotSummary(inputSnapshot);
  if (!summary) {
    return null;
  }

  return (
    <div className="rounded-lg border border-border bg-card p-4 text-sm">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="font-medium text-foreground">外挂程式运行快照</div>
          <div className="mt-1 text-xs text-muted-foreground">
            展示任务创建时固定的选择与不可变 Release；Command 参数仅显示 SHA-256 审计，不显示原文。
          </div>
        </div>
        <div className="text-right text-xs text-muted-foreground">
          <div>设备：{shortIdentity(summary.deviceId)}</div>
          <div>工作区：{summary.workspaceId ? shortIdentity(summary.workspaceId) : '-'}</div>
        </div>
      </div>

      <div className="mt-3 grid gap-2 lg:grid-cols-2">
        {summary.plugins.map((plugin) => (
          <div key={plugin.pluginId} className="rounded-md border border-border/70 bg-background p-3">
            <div className="font-medium text-foreground">{plugin.pluginId}</div>
            <div className="mt-1 text-xs text-muted-foreground">
              Release：{plugin.releaseId ? shortIdentity(plugin.releaseId) : '-'}
              {plugin.version ? ` · v${plugin.version}` : ''}
            </div>
            <div className="mt-2 space-y-1 text-xs text-muted-foreground">
              <div>组件：{joinedOrDash(plugin.componentKeys)}</div>
              <div>Skills：{joinedOrDash(plugin.selectedSkillIds)}</div>
              <div>Commands：{joinedOrDash(plugin.selectedCommandIds)}</div>
              <div>Agents：{joinedOrDash(plugin.selectedAgentIds)}</div>
            </div>
          </div>
        ))}
      </div>

      {summary.commands.length > 0 ? (
        <div className="mt-3 space-y-1 rounded-md border border-border/70 bg-background p-3 text-xs">
          <div className="font-medium text-foreground">Command 审计</div>
          {summary.commands.map((command) => (
            <div key={`${command.pluginId}:${command.commandId}`} className="text-muted-foreground">
              {command.pluginId} / {command.commandId}
              {' · '}
              {command.argumentsPresent
                ? `参数 SHA-256：${command.argumentsSha256 || '旧记录未提供'}`
                : '无参数'}
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
};
