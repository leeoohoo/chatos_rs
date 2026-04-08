import { useState } from 'react';

import {
  priorityStyles,
  priorityText,
  statusText,
  statusStyles,
} from './helpers';
import type { TaskWorkbarItem } from './types';

interface TaskCardProps {
  task: TaskWorkbarItem;
  compact?: boolean;
  onConfirmTask?: (task: TaskWorkbarItem) => void;
  onPauseTask?: (task: TaskWorkbarItem) => void;
  onResumeTask?: (task: TaskWorkbarItem) => void;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  isMutating?: boolean;
}

const formatTaskTime = (value?: string | null): string | null => {
  if (!value) {
    return null;
  }
  const parsed = Date.parse(value);
  if (!Number.isFinite(parsed)) {
    return value;
  }
  return new Date(parsed).toLocaleString();
};

const TaskCard = ({
  task,
  compact = false,
  onConfirmTask,
  onPauseTask,
  onResumeTask,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  isMutating = false,
}: TaskCardProps) => {
  const [expanded, setExpanded] = useState(false);
  const cardClass = compact
    ? 'min-w-0 overflow-hidden rounded-xl border border-border bg-card/90 p-3 shadow-sm'
    : 'min-w-0 overflow-hidden rounded-xl border border-border bg-card/90 p-3 shadow-sm';
  const actionClass = 'rounded-md border border-border bg-background px-2.5 py-1 text-xs text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50';
  const primaryActionClass = 'rounded-md bg-amber-500 px-2.5 py-1 text-xs font-medium text-white hover:bg-amber-600 disabled:cursor-not-allowed disabled:opacity-50';
  const isTerminal = task.status === 'completed'
    || task.status === 'failed'
    || task.status === 'cancelled'
    || task.status === 'skipped';
  const isPendingConfirm = task.status === 'pending_confirm';
  const isRunning = task.status === 'running';
  const isPaused = task.status === 'paused';
  const hasExecutionManifest = (task.plannedBuiltinMcpIds?.length || 0) > 0
    || !!task.taskPlanId
    || !!task.taskRef
    || !!task.taskKind
    || (task.dependsOnTaskIds?.length || 0) > 0
    || (task.verificationOfTaskIds?.length || 0) > 0
    || (task.acceptanceCriteria?.length || 0) > 0
    || !!task.blockedReason
    || !!task.handoffPayload?.summary
    || (task.plannedContextAssets?.length || 0) > 0
    || !!task.projectRoot
    || !!task.remoteConnectionId
    || !!task.executionResultContract
    || !!task.taskResultBrief
    || !!task.planningSnapshot?.sourceUserGoalSummary
    || !!task.planningSnapshot?.sourceConstraintsSummary
    || !!task.resultSummary
    || !!task.lastError;
  const startedAt = formatTaskTime(task.startedAt);
  const finishedAt = formatTaskTime(task.finishedAt);
  const confirmedAt = formatTaskTime(task.confirmedAt);
  const dueAt = formatTaskTime(task.dueAt);
  const metaPills = [
    `优先级 ${priorityText[task.priority]}`,
    task.taskKind ? `类型 ${task.taskKind}` : null,
    task.dependsOnTaskIds && task.dependsOnTaskIds.length > 0 ? `依赖 ${task.dependsOnTaskIds.length}` : null,
    task.verificationOfTaskIds && task.verificationOfTaskIds.length > 0 ? `验证 ${task.verificationOfTaskIds.length}` : null,
    task.plannedBuiltinMcpIds && task.plannedBuiltinMcpIds.length > 0 ? `MCP ${task.plannedBuiltinMcpIds.length}` : null,
    task.plannedContextAssets && task.plannedContextAssets.length > 0 ? `资源 ${task.plannedContextAssets.length}` : null,
  ].filter(Boolean) as string[];

  return (
    <div className={cardClass}>
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <div className="min-w-0 break-words text-sm font-semibold text-foreground">{task.title}</div>
            {task.taskRef ? (
              <span className="rounded-full border border-border px-2 py-0.5 text-[11px] text-muted-foreground">
                {task.taskRef}
              </span>
            ) : null}
          </div>
          {task.details ? (
            <div className="mt-1 line-clamp-3 break-words text-xs text-muted-foreground">
              {task.details}
            </div>
          ) : null}
        </div>
        <span className={`shrink-0 rounded-full px-2 py-1 text-[11px] font-medium ${statusStyles[task.status]}`}>
          {statusText[task.status]}
        </span>
      </div>

      <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-muted-foreground">
        {task.taskPlanId ? (
          <span className="rounded-full border border-border bg-background px-2 py-1">
            {`计划 ${task.taskPlanId}`}
          </span>
        ) : null}
        {metaPills.map((item) => (
          <span key={`${task.id}-${item}`} className={`rounded-full border border-border bg-background px-2 py-1 ${item.startsWith('优先级') ? priorityStyles[task.priority] : ''}`}>
            {item}
          </span>
        ))}
        <span className="rounded-full border border-border bg-background px-2 py-1" title={task.conversationTurnId}>
          {`轮次 ${task.conversationTurnId}`}
        </span>
        {dueAt ? (
          <span className="rounded-full border border-border bg-background px-2 py-1">
            {`截止 ${dueAt}`}
          </span>
        ) : null}
      </div>

      {isPendingConfirm ? (
        <div className="mt-3 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800 dark:border-amber-900/60 dark:bg-amber-950/30 dark:text-amber-100">
          这个任务还在等待确认。确认后会进入待执行队列。
        </div>
      ) : null}

      {task.blockedReason ? (
        <div className="mt-3 rounded-lg border border-orange-200 bg-orange-50 px-3 py-2 text-xs text-orange-800 dark:border-orange-900/60 dark:bg-orange-950/30 dark:text-orange-100">
          {`阻塞原因：${task.blockedReason}`}
        </div>
      ) : null}

      {!task.blockedReason && task.lastError ? (
        <div className="mt-3 rounded-lg border border-rose-200 bg-rose-50 px-3 py-2 text-xs text-rose-800 dark:border-rose-900/60 dark:bg-rose-950/30 dark:text-rose-100">
          {`失败原因：${task.lastError}`}
        </div>
      ) : null}

      {!task.lastError && task.resultSummary ? (
        <div className="mt-3 rounded-lg border border-emerald-200 bg-emerald-50 px-3 py-2 text-xs text-emerald-800 dark:border-emerald-900/60 dark:bg-emerald-950/30 dark:text-emerald-100">
          {task.resultSummary}
        </div>
      ) : null}

      {(onConfirmTask || onPauseTask || onResumeTask || onCompleteTask || onEditTask || onDeleteTask || hasExecutionManifest) ? (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {onConfirmTask && isPendingConfirm ? (
            <button type="button" className={primaryActionClass} onClick={() => onConfirmTask(task)} disabled={isMutating}>
              确认执行
            </button>
          ) : null}
          {onPauseTask && isRunning ? (
            <button type="button" className={actionClass} onClick={() => onPauseTask(task)} disabled={isMutating}>
              暂停
            </button>
          ) : null}
          {onResumeTask && isPaused ? (
            <button type="button" className={actionClass} onClick={() => onResumeTask(task)} disabled={isMutating}>
              开始
            </button>
          ) : null}
          {onCompleteTask && !isTerminal ? (
            <button type="button" className={actionClass} onClick={() => onCompleteTask(task)} disabled={isMutating}>
              完成
            </button>
          ) : null}
          {onEditTask ? (
            <button type="button" className={actionClass} onClick={() => onEditTask(task)} disabled={isMutating}>
              编辑
            </button>
          ) : null}
          {onDeleteTask ? (
            <button type="button" className={actionClass} onClick={() => onDeleteTask(task)} disabled={isMutating}>
              删除
            </button>
          ) : null}
          {hasExecutionManifest ? (
            <button type="button" className={actionClass} onClick={() => setExpanded((value) => !value)}>
              {expanded ? '收起清单' : '执行清单'}
            </button>
          ) : null}
          {isMutating ? (
            <span className="text-xs text-muted-foreground">
              处理中...
            </span>
          ) : null}
        </div>
      ) : null}

      {expanded ? (
        <div className="mt-4 space-y-3 border-t border-border pt-3 text-xs text-muted-foreground">
          <div className="grid gap-2 md:grid-cols-2">
            {confirmedAt ? (
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="font-medium text-foreground/90">确认时间</div>
                <div>{confirmedAt}</div>
              </div>
            ) : null}
            {startedAt ? (
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="font-medium text-foreground/90">开始时间</div>
                <div>{startedAt}</div>
              </div>
            ) : null}
            {finishedAt ? (
              <div className="rounded-lg border border-border bg-background px-3 py-2">
                <div className="font-medium text-foreground/90">结束时间</div>
                <div>{finishedAt}</div>
              </div>
            ) : null}
            {task.projectRoot ? (
              <div className="rounded-lg border border-border bg-background px-3 py-2 md:col-span-2">
                <div className="font-medium text-foreground/90">项目路径</div>
                <div className="break-all whitespace-pre-wrap">{task.projectRoot}</div>
              </div>
            ) : null}
          </div>
          {task.taskPlanId ? (
            <div>
              <div className="font-medium text-foreground/90">任务计划</div>
              <div className="break-all">{task.taskPlanId}</div>
            </div>
          ) : null}
          {task.taskRef || task.taskKind ? (
            <div>
              <div className="font-medium text-foreground/90">任务图谱</div>
              {task.taskRef ? <div>{`task_ref: ${task.taskRef}`}</div> : null}
              {task.taskKind ? <div>{`task_kind: ${task.taskKind}`}</div> : null}
              {task.dependsOnTaskIds && task.dependsOnTaskIds.length > 0 ? (
                <div className="break-all">{`依赖任务: ${task.dependsOnTaskIds.join(', ')}`}</div>
              ) : null}
              {task.verificationOfTaskIds && task.verificationOfTaskIds.length > 0 ? (
                <div className="break-all">{`验证对象: ${task.verificationOfTaskIds.join(', ')}`}</div>
              ) : null}
              {task.acceptanceCriteria && task.acceptanceCriteria.length > 0 ? (
                <div className="break-all whitespace-pre-wrap">
                  {`验收标准: ${task.acceptanceCriteria.join('\n')}`}
                </div>
              ) : null}
              {task.blockedReason ? (
                <div className="break-all text-orange-700 dark:text-orange-200">{`阻塞原因: ${task.blockedReason}`}</div>
              ) : null}
            </div>
          ) : null}
          {task.plannedBuiltinMcpIds && task.plannedBuiltinMcpIds.length > 0 ? (
            <div>
              <div className="font-medium text-foreground/90">内置 MCP</div>
              <div className="break-all">{task.plannedBuiltinMcpIds.join(', ')}</div>
            </div>
          ) : null}
          {task.plannedContextAssets && task.plannedContextAssets.length > 0 ? (
            <div>
              <div className="font-medium text-foreground/90">上下文资产</div>
              <div className="space-y-1">
                {task.plannedContextAssets.map((asset) => (
                  <div key={`${asset.assetType}:${asset.assetId}`} className="break-all">
                    {`${asset.assetType} · ${asset.displayName || asset.assetId}`}
                  </div>
                ))}
              </div>
            </div>
          ) : null}
          {task.remoteConnectionId ? (
            <div>
              <div className="font-medium text-foreground/90">远程连接</div>
              <div className="break-all">{task.remoteConnectionId}</div>
            </div>
          ) : null}
          {task.executionResultContract ? (
            <div>
              <div className="font-medium text-foreground/90">结果契约</div>
              <div>{`必填结果: ${task.executionResultContract.resultRequired ? '是' : '否'}`}</div>
              {task.executionResultContract.preferredFormat ? (
                <div>{`格式: ${task.executionResultContract.preferredFormat}`}</div>
              ) : null}
            </div>
          ) : null}
          {task.planningSnapshot ? (
            <div>
              <div className="font-medium text-foreground/90">规划快照</div>
              {task.planningSnapshot.selectedModelConfigId ? (
                <div>{`模型配置: ${task.planningSnapshot.selectedModelConfigId}`}</div>
              ) : null}
              {task.planningSnapshot.sourceUserGoalSummary ? (
                <div className="break-all whitespace-pre-wrap">
                  {`来源目标: ${task.planningSnapshot.sourceUserGoalSummary}`}
                </div>
              ) : null}
              {task.planningSnapshot.sourceConstraintsSummary ? (
                <div className="break-all whitespace-pre-wrap">
                  {`来源约束: ${task.planningSnapshot.sourceConstraintsSummary}`}
                </div>
              ) : null}
              {task.planningSnapshot.contactAuthorizedBuiltinMcpIds.length > 0 ? (
                <div className="break-all">
                  {`联系人授权: ${task.planningSnapshot.contactAuthorizedBuiltinMcpIds.join(', ')}`}
                </div>
              ) : null}
              {task.planningSnapshot.plannedAt ? (
                <div>{`规划时间: ${task.planningSnapshot.plannedAt}`}</div>
              ) : null}
            </div>
          ) : null}
          {task.taskResultBrief ? (
            <div>
              <div className="font-medium text-foreground/90">结果桥接摘要</div>
              {task.taskResultBrief.taskStatus ? (
                <div>{`桥接状态: ${task.taskResultBrief.taskStatus}`}</div>
              ) : null}
              <div className="break-all whitespace-pre-wrap">{task.taskResultBrief.resultSummary}</div>
              {task.taskResultBrief.resultFormat ? (
                <div>{`结果格式: ${task.taskResultBrief.resultFormat}`}</div>
              ) : null}
              {task.taskResultBrief.sourceSessionId ? (
                <div className="break-all">{`来源会话: ${task.taskResultBrief.sourceSessionId}`}</div>
              ) : null}
              {task.taskResultBrief.sourceTurnId ? (
                <div className="break-all">{`来源轮次: ${task.taskResultBrief.sourceTurnId}`}</div>
              ) : null}
              {task.taskResultBrief.finishedAt ? (
                <div>{`桥接完成时间: ${task.taskResultBrief.finishedAt}`}</div>
              ) : null}
            </div>
          ) : null}
          {task.handoffPayload ? (
            <div>
              <div className="font-medium text-foreground/90">任务交接</div>
              {task.handoffPayload.handoffKind ? (
                <div>{`类型: ${task.handoffPayload.handoffKind}`}</div>
              ) : null}
              <div className="break-all whitespace-pre-wrap">{task.handoffPayload.summary}</div>
              {task.handoffPayload.resultSummary ? (
                <div className="break-all whitespace-pre-wrap">
                  {`结果摘要: ${task.handoffPayload.resultSummary}`}
                </div>
              ) : null}
              {task.handoffPayload.keyChanges && task.handoffPayload.keyChanges.length > 0 ? (
                <div className="space-y-1">
                  <div className="font-medium text-foreground/90">关键变化</div>
                  {task.handoffPayload.keyChanges.map((item, index) => (
                    <div key={`${task.id}-handoff-change-${index}`} className="break-all">{`- ${item}`}</div>
                  ))}
                </div>
              ) : null}
              {task.handoffPayload.verificationSuggestions && task.handoffPayload.verificationSuggestions.length > 0 ? (
                <div className="space-y-1">
                  <div className="font-medium text-foreground/90">验证建议</div>
                  {task.handoffPayload.verificationSuggestions.map((item, index) => (
                    <div key={`${task.id}-handoff-verify-${index}`} className="break-all">{`- ${item}`}</div>
                  ))}
                </div>
              ) : null}
              {task.handoffPayload.openRisks && task.handoffPayload.openRisks.length > 0 ? (
                <div className="space-y-1">
                  <div className="font-medium text-rose-600 dark:text-rose-300">遗留风险</div>
                  {task.handoffPayload.openRisks.map((item, index) => (
                    <div key={`${task.id}-handoff-risk-${index}`} className="break-all text-rose-700 dark:text-rose-200">{`- ${item}`}</div>
                  ))}
                </div>
              ) : null}
              {task.handoffPayload.artifactRefs && task.handoffPayload.artifactRefs.length > 0 ? (
                <div className="break-all">
                  {`关联引用: ${task.handoffPayload.artifactRefs.join(', ')}`}
                </div>
              ) : null}
              {task.handoffPayload.generatedAt ? (
                <div>{`生成时间: ${task.handoffPayload.generatedAt}`}</div>
              ) : null}
            </div>
          ) : null}
          {task.resultSummary ? (
            <div>
              <div className="font-medium text-foreground/90">执行结果</div>
              <div className="break-all whitespace-pre-wrap">{task.resultSummary}</div>
            </div>
          ) : null}
          {task.lastError ? (
            <div>
              <div className="font-medium text-rose-600 dark:text-rose-300">失败原因</div>
              <div className="break-all whitespace-pre-wrap text-rose-700 dark:text-rose-200">{task.lastError}</div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
};

export default TaskCard;
