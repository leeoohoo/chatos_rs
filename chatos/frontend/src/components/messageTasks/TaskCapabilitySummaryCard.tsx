// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { taskCapabilitySummary } from './taskCapabilitySummary';

const CapabilityChips: FC<{ values: string[]; empty: string }> = ({ values, empty }) => (
  <div className="mt-1.5 flex flex-wrap gap-1.5">
    {values.length ? values.map((value) => (
      <span
        key={value}
        className="rounded-full border border-border bg-background px-2 py-0.5 text-[11px] text-foreground"
      >
        {value}
      </span>
    )) : (
      <span className="text-xs text-muted-foreground">{empty}</span>
    )}
  </div>
);

export const TaskCapabilitySummaryCard: FC<{ mcpConfig: unknown }> = ({ mcpConfig }) => {
  const summary = taskCapabilitySummary(mcpConfig);
  return (
    <div className="rounded-md border border-border bg-muted/20 px-3 py-3 text-sm">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <div className="font-medium text-foreground">当前任务可用能力</div>
          <div className="mt-0.5 text-xs text-muted-foreground">
            这里展示任务创建时实际带入运行环境的 builtin MCP 与 Skills。
          </div>
        </div>
        {summary.hasComputerUse ? (
          <span className="rounded-full border border-emerald-300 bg-emerald-50 px-2 py-0.5 text-[11px] text-emerald-700 dark:border-emerald-800 dark:bg-emerald-950/40 dark:text-emerald-200">
            已启用 Computer Use
          </span>
        ) : (
          <span className="rounded-full border border-amber-300 bg-amber-50 px-2 py-0.5 text-[11px] text-amber-700 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
            未启用 Computer Use
          </span>
        )}
      </div>

      <div className="mt-3 grid gap-3 md:grid-cols-3">
        <div>
          <div className="text-xs font-medium text-muted-foreground">Builtin MCP</div>
          <CapabilityChips values={summary.builtinLabels} empty="未选择 builtin MCP" />
        </div>
        <div>
          <div className="text-xs font-medium text-muted-foreground">Skills</div>
          <CapabilityChips
            values={summary.skillLabels}
            empty="未选择 Skill（因此无 Computer Use）"
          />
        </div>
        <div>
          <div className="text-xs font-medium text-muted-foreground">外部 MCP</div>
          <CapabilityChips values={summary.externalMcpIds} empty="未选择外部 MCP" />
        </div>
      </div>

      {!summary.hasAnyCapability ? (
        <div className="mt-3 rounded-md border border-dashed border-border px-2 py-1.5 text-xs text-muted-foreground">
          这个任务没有选择任何额外能力，只能使用默认对话能力。
        </div>
      ) : null}
    </div>
  );
};
