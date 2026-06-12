import type { FC } from 'react';
import type { MessageTaskRunnerTask } from '../../lib/api/client/types';
import { CollapsibleSection, CollapsibleText } from './CollapsibleSection';
import { FieldGrid, ModalShell, valueOrDash } from './parts';
import { formatDateTime, isRecord, readStringArray } from './utils';

interface MessageTaskDetailModalProps {
  task: MessageTaskRunnerTask | null;
  onClose: () => void;
}

export const MessageTaskDetailModal: FC<MessageTaskDetailModalProps> = ({
  task,
  onClose,
}) => {
  if (!task) {
    return null;
  }
  const prerequisiteIds = readStringArray(task.prerequisite_task_ids);
  const toolState = isRecord(task.task_tool_state) ? task.task_tool_state : {};
  const outcomeItems = Array.isArray(toolState.outcome_items) ? toolState.outcome_items : [];

  return (
    <ModalShell title="任务详情" subtitle={task.title || task.id} onClose={onClose}>
      <FieldGrid
        items={[
          ['任务 ID', task.id],
          ['状态', task.status],
          ['创建人', task.creator_display_name || task.creator_username || task.creator_user_id],
          ['模型', task.default_model_config_id],
          ['优先级', task.priority],
          ['最近运行', task.last_run_id],
          ['创建时间', formatDateTime(task.created_at)],
          ['更新时间', formatDateTime(task.updated_at)],
        ]}
      />

      <CollapsibleSection title="任务内容" defaultOpen>
        <div className="space-y-3">
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">目标</div>
            <CollapsibleText value={task.objective || '-'} />
          </div>
          <div>
            <div className="mb-1 text-xs font-medium text-muted-foreground">描述</div>
            <CollapsibleText value={task.description || '-'} />
          </div>
        </div>
      </CollapsibleSection>

      {task.result_summary ? (
        <CollapsibleSection title="执行结果" defaultOpen>
          <CollapsibleText value={task.result_summary} />
        </CollapsibleSection>
      ) : null}

      {task.process_log ? (
        <CollapsibleSection title="执行过程">
          <CollapsibleText value={task.process_log} />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection
        title="前置任务"
        summary={prerequisiteIds.length ? `${prerequisiteIds.length} 个前置任务` : '无'}
      >
        <CollapsibleText value={prerequisiteIds.length ? prerequisiteIds : '-'} code />
      </CollapsibleSection>

      <CollapsibleSection
        title="MCP / 工作区 / 服务器"
        summary={valueOrDash(isRecord(task.mcp_config) ? task.mcp_config.workspace_dir : null)}
      >
        <CollapsibleText value={task.mcp_config || '-'} code />
      </CollapsibleSection>

      <CollapsibleSection
        title="过程产物"
        summary={outcomeItems.length ? `${outcomeItems.length} 条` : '无'}
      >
        <CollapsibleText value={task.task_tool_state || '-'} code />
      </CollapsibleSection>

      <CollapsibleSection title="来源信息">
        <FieldGrid
          items={[
            ['会话 ID', task.source_session_id],
            ['轮次 ID', task.source_turn_id],
            ['源消息 ID', task.source_user_message_id],
            ['父任务 ID', task.parent_task_id],
            ['来源运行 ID', task.source_run_id],
          ]}
        />
      </CollapsibleSection>

      <CollapsibleSection title="原始输入">
        <CollapsibleText value={task.input_payload || '-'} code />
      </CollapsibleSection>
    </ModalShell>
  );
};
