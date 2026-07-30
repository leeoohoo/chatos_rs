// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useState } from 'react';
import {
  GitBranch,
  LoaderCircle,
  MessageSquareText,
  Play,
} from 'lucide-react';

import type { ProjectRequirementResponse } from '../../../lib/api/client/types';
import { cn } from '../../../lib/utils';
import { RequirementExecutionModalFrame } from './RequirementExecutionModalShell';
import { readText } from './model';

export const RequirementExecutionStartingModal: React.FC<{
  requirement: ProjectRequirementResponse;
  executionPlane?: string | null;
  starting: boolean;
  onClose: () => void;
  onStart: (planningFeedback: string) => void;
}> = ({ requirement, executionPlane, starting, onClose, onStart }) => {
  const isLocalExecution = (executionPlane || '').toLowerCase() === 'local_connector';
  const [fullscreen, setFullscreen] = useState(false);
  const [planningFeedback, setPlanningFeedback] = useState('');
  return (
    <RequirementExecutionModalFrame
      fullscreen={fullscreen}
      isLocalExecution={isLocalExecution}
      onClose={onClose}
      onToggleFullscreen={() => setFullscreen((current) => !current)}
      requirementTitle={readText(requirement.title) || requirement.id}
    >
      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden lg:grid-cols-[minmax(320px,0.78fr)_minmax(0,1.72fr)]">
          <aside className="flex min-h-0 flex-col border-b border-border bg-muted/10 lg:border-b-0 lg:border-r">
            <div className="shrink-0 border-b border-border px-4 py-3">
              <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
                {starting
                  ? <LoaderCircle className="h-4 w-4 animate-spin" />
                  : <Play className="h-4 w-4" />}
                {starting ? '正在建立规划上下文' : '等待用户开始生成执行流程'}
              </div>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {starting
                  ? '规划 Agent 正在创建会话并读取需求、项目任务和技术文档，Task Runner 尚未启动。'
                  : '点击“开始生成执行流程”后只会启动规划 Agent；Task Runner 仍需在完整 DAG 生成后由你点击“执行”。'}
              </p>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
              <ol className="space-y-4">
                <li className="flex gap-3">
                  <span className={cn(
                    'mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border',
                    starting
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border text-muted-foreground',
                  )}
                  >
                    {starting
                      ? <LoaderCircle className="h-3 w-3 animate-spin" />
                      : <span className="h-1.5 w-1.5 rounded-full bg-current" />}
                  </span>
                  <div>
                    <div className="text-xs font-medium text-foreground">
                      {starting ? '执行规划请求已接受' : '等待启动规划 Agent'}
                    </div>
                    <div className="mt-1 text-[11px] leading-5 text-muted-foreground">
                      {starting ? '正在建立规划批次标识' : '尚未创建规划会话或执行任务'}
                    </div>
                  </div>
                </li>
                <li className="flex gap-3">
                  <span className="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-border text-muted-foreground">
                    <span className="h-1.5 w-1.5 rounded-full bg-current" />
                  </span>
                  <div>
                    <div className="text-xs font-medium text-foreground">读取任务与文档</div>
                    <div className="mt-1 text-[11px] leading-5 text-muted-foreground">上下文就绪后会逐个创建执行任务</div>
                  </div>
                </li>
              </ol>
            </div>
            <div className="shrink-0 border-t border-border bg-background px-4 py-3">
              <div className="mb-2 flex items-center gap-2 text-xs font-semibold text-foreground">
                <MessageSquareText className="h-3.5 w-3.5" />
                调整执行计划
              </div>
              <textarea
                value={planningFeedback}
                disabled={starting}
                onChange={(event) => setPlanningFeedback(event.target.value)}
                placeholder="输入希望项目 Agent 遵循的规划要求，例如：先补测试，再拆分接口；把部署放到最后……"
                className="min-h-24 w-full resize-none rounded-md border border-border bg-background px-3 py-2 text-xs leading-5 text-foreground outline-none placeholder:text-muted-foreground focus:border-primary focus:ring-1 focus:ring-primary/20 disabled:cursor-wait disabled:bg-muted/40"
              />
              <div className="mt-1 text-[11px] leading-5 text-muted-foreground">
                这段内容会随首次规划请求一起发送给项目 Agent；留空也可以开始。
              </div>
            </div>
          </aside>

          <main className="flex min-h-0 min-w-0 flex-col">
            <div className="shrink-0 border-b border-border px-4 py-3">
              <div className="flex items-center gap-2 text-sm font-semibold text-foreground">
                <GitBranch className="h-4 w-4" />
                实时执行流程图
              </div>
              <p className="mt-1 text-xs text-muted-foreground">Agent 创建第一个任务后，流程图会立即在这里更新。</p>
            </div>
            <div className="flex min-h-0 flex-1 items-center justify-center p-6">
              <div className="max-w-sm rounded-lg border border-dashed border-border bg-muted/10 px-6 py-8 text-center">
                {starting
                  ? <LoaderCircle className="mx-auto h-7 w-7 animate-spin text-primary" />
                  : <GitBranch className="mx-auto h-7 w-7 text-muted-foreground" />}
                <div className="mt-3 text-sm font-medium text-foreground">
                  {starting ? '等待第一个任务节点' : '执行流程尚未开始生成'}
                </div>
                <div className="mt-1 text-xs leading-5 text-muted-foreground">
                  {starting
                    ? '这里只展示规划结果，不会提前启动任何 task run。'
                    : '用户明确开始后，规划 Agent 创建的任务节点才会显示在这里。'}
                </div>
              </div>
            </div>
            <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-border bg-muted/10 px-4 py-3">
              <span className="text-xs text-muted-foreground">
                {starting
                  ? '正在生成 DAG；此阶段不会启动 Task Runner。'
                  : '开始生成与最终执行是两个独立操作。'}
              </span>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  disabled={starting}
                  className="inline-flex items-center gap-1.5 rounded-md bg-primary px-5 py-2 text-xs font-semibold text-primary-foreground hover:bg-primary/90 disabled:cursor-wait disabled:opacity-60"
                  onClick={() => onStart(planningFeedback.trim())}
                >
                  {starting
                    ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                    : <Play className="h-3.5 w-3.5" />}
                  {starting ? '正在生成执行流程' : '开始生成执行流程'}
                </button>
                <button
                  type="button"
                  className="rounded-md border border-border bg-background px-3 py-2 text-xs font-medium text-foreground hover:bg-accent"
                  onClick={onClose}
                >
                  关闭
                </button>
              </div>
            </footer>
          </main>
      </div>
    </RequirementExecutionModalFrame>
  );
};
