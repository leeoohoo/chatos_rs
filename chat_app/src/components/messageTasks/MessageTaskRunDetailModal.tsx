import type { FC } from 'react';
import type { MessageTaskRunnerRunDetailResponse } from '../../lib/api/client/types';
import { CollapsibleSection, CollapsibleText } from './CollapsibleSection';
import { FieldGrid, ModalShell } from './parts';
import { extractReportContent, formatDateTime } from './utils';

interface MessageTaskRunDetailModalProps {
  detail: MessageTaskRunnerRunDetailResponse | null;
  onClose: () => void;
}

export const MessageTaskRunDetailModal: FC<MessageTaskRunDetailModalProps> = ({
  detail,
  onClose,
}) => {
  if (!detail) {
    return null;
  }
  const { run, task, events } = detail;
  const reportContent = extractReportContent(run.report);
  const modelRequestCount = events.filter((event) => event.event_type === 'model_request').length;
  const toolEventCount = events.filter((event) => event.event_type.includes('tool')).length;

  return (
    <ModalShell title="运行详情" subtitle={task.title || run.task_id} onClose={onClose}>
      <FieldGrid
        items={[
          ['运行 ID', run.id],
          ['任务', task.title || run.task_id],
          ['状态', run.status],
          ['模型', run.model_config_id],
          ['开始时间', formatDateTime(run.started_at)],
          ['结束时间', formatDateTime(run.finished_at)],
          ['模型请求', modelRequestCount],
          ['工具事件', toolEventCount],
        ]}
      />

      {run.result_summary ? (
        <CollapsibleSection title="最终结果" defaultOpen>
          <CollapsibleText value={run.result_summary} />
        </CollapsibleSection>
      ) : null}

      {run.error_message ? (
        <CollapsibleSection title="错误信息" defaultOpen>
          <CollapsibleText value={run.error_message} />
        </CollapsibleSection>
      ) : null}

      {reportContent ? (
        <CollapsibleSection title="Report 内容" defaultOpen>
          <CollapsibleText value={reportContent} />
        </CollapsibleSection>
      ) : null}

      <CollapsibleSection title="运行事件" summary={`${events.length} 条事件`}>
        <div className="space-y-2">
          {events.length ? events.map((event) => (
            <div key={event.id} className="rounded-md border border-border bg-muted/30 p-2">
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <span className="font-medium text-foreground">{event.event_type}</span>
                <span className="text-muted-foreground">{formatDateTime(event.created_at)}</span>
              </div>
              {event.message ? (
                <p className="mt-1 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                  {event.message}
                </p>
              ) : null}
              {event.payload ? (
                <div className="mt-2">
                  <CollapsibleText value={event.payload} code maxHeightClassName="max-h-48" />
                </div>
              ) : null}
            </div>
          )) : (
            <p className="text-sm text-muted-foreground">暂无事件</p>
          )}
        </div>
      </CollapsibleSection>

      <CollapsibleSection title="运行快照">
        <div className="space-y-3">
          <CollapsibleSection title="输入快照">
            <CollapsibleText value={run.input_snapshot || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="上下文快照">
            <CollapsibleText value={run.context_snapshot || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="用量">
            <CollapsibleText value={run.usage || '-'} code />
          </CollapsibleSection>
          <CollapsibleSection title="完整 Report">
            <CollapsibleText value={run.report || '-'} code />
          </CollapsibleSection>
        </div>
      </CollapsibleSection>
    </ModalShell>
  );
};
