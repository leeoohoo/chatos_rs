import { useState } from 'react';

import {
  priorityStyles,
  priorityText,
  statusStyles,
  statusText,
} from './helpers';
import type { TaskWorkbarItem } from './types';

interface TaskCardProps {
  task: TaskWorkbarItem;
  compact?: boolean;
  onCompleteTask?: (task: TaskWorkbarItem) => void;
  onDeleteTask?: (task: TaskWorkbarItem) => void;
  onEditTask?: (task: TaskWorkbarItem) => void;
  isMutating?: boolean;
}

const TaskCard = ({
  task,
  compact = false,
  onCompleteTask,
  onDeleteTask,
  onEditTask,
  isMutating = false,
}: TaskCardProps) => {
  const [expanded, setExpanded] = useState(false);
  const cardClass = compact
    ? 'min-w-[160px] max-w-[190px] min-w-0 overflow-hidden rounded-md border border-border bg-background p-2'
    : 'min-w-0 overflow-hidden rounded-lg border border-border bg-background p-2.5';

  const titleClass = compact
    ? 'min-w-0 line-clamp-2 break-words text-xs font-medium text-foreground'
    : 'min-w-0 line-clamp-2 break-words text-sm font-medium text-foreground';

  const detailsClass = compact
    ? 'mb-1 line-clamp-1 break-all text-[11px] text-muted-foreground'
    : 'mb-1 line-clamp-2 break-all text-xs text-muted-foreground';

  const metaClass = compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground';
  const actionClass = compact
    ? 'rounded border border-border bg-background px-1.5 py-0.5 text-[10px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50'
    : 'rounded border border-border bg-background px-2 py-0.5 text-[11px] text-foreground hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50';
  const isTerminal = task.status === 'completed'
    || task.status === 'failed'
    || task.status === 'cancelled'
    || task.status === 'skipped';
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

  return (
    <div className={cardClass}>
      <div className="mb-1 flex min-w-0 items-start justify-between gap-2">
        <div className={titleClass}>{task.title}</div>
        <span className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${statusStyles[task.status]}`}>
          {statusText[task.status]}
        </span>
      </div>

      {task.details ? <div className={detailsClass}>{task.details}</div> : null}

      <div className={metaClass}>
        {task.taskPlanId ? (
          <div className="truncate" title={task.taskPlanId}>
            计划 {task.taskPlanId}
          </div>
        ) : null}
        <div>
          <span className={priorityStyles[task.priority]}>优先级 {priorityText[task.priority]}</span>
        </div>
        {task.taskKind ? <div>{`类型 ${task.taskKind}`}</div> : null}
        {task.taskRef ? <div className="truncate" title={task.taskRef}>{`引用 ${task.taskRef}`}</div> : null}
        <div className="truncate" title={task.conversationTurnId}>
          轮次 {task.conversationTurnId}
        </div>
        {task.startedAt ? <div>开始 {task.startedAt}</div> : null}
        {task.finishedAt ? <div>结束 {task.finishedAt}</div> : null}
      </div>

      {(onCompleteTask || onEditTask || onDeleteTask) ? (
        <div className={compact ? 'mt-1 flex items-center gap-1' : 'mt-2 flex items-center gap-1'}>
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
            <span className={compact ? 'text-[10px] text-muted-foreground' : 'text-[11px] text-muted-foreground'}>
              处理中...
            </span>
          ) : null}
        </div>
      ) : null}

      {task.dueAt ? (
        <div className={compact ? 'mt-1 truncate text-[10px] text-muted-foreground' : 'mt-1 truncate text-[11px] text-muted-foreground'} title={task.dueAt}>
          截止 {task.dueAt}
        </div>
      ) : null}

      {expanded ? (
        <div className={compact ? 'mt-1 space-y-1 text-[10px] text-muted-foreground' : 'mt-2 space-y-1.5 text-[11px] text-muted-foreground'}>
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
          {task.projectRoot ? (
            <div>
              <div className="font-medium text-foreground/90">项目路径</div>
              <div className="break-all whitespace-pre-wrap">{task.projectRoot}</div>
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
