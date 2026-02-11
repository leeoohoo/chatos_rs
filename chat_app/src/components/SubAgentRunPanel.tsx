import React, { useMemo } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';

interface SubAgentRunPanelProps {
  toolCall: any;
  onClose: () => void;
}

interface ProgressItem {
  key: string;
  event: string;
  payload: any;
}

const EVENT_LABEL: Record<string, string> = {
  job_started: '任务已启动',
  job_status: '状态更新',
  job_event: '执行事件',
  job_finished: '任务结束',
};

const safeString = (value: unknown): string => {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
};

const tryParseJson = (raw: string): any | null => {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
};

export const SubAgentRunPanel: React.FC<SubAgentRunPanelProps> = ({ toolCall, onClose }) => {
  const toolName = String(toolCall?.name || 'run_sub_agent');
  const streamLog = safeString(toolCall?.streamLog || '');
  const finalRaw = safeString(toolCall?.finalResult || toolCall?.result || '');
  const finalObj = useMemo(() => tryParseJson(finalRaw), [finalRaw]);

  const progressItems = useMemo<ProgressItem[]>(() => {
    const lines = streamLog
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0)
      .slice(-200);

    return lines.map((line, index) => {
      const parsed = tryParseJson(line);
      if (parsed && parsed.kind === 'sub_agent_progress') {
        return {
          key: `${index}-${parsed.event || 'event'}`,
          event: String(parsed.event || 'event'),
          payload: parsed.payload,
        };
      }
      return {
        key: `${index}-raw`,
        event: 'raw',
        payload: line,
      };
    });
  }, [streamLog]);

  const fallbackEvents = useMemo<ProgressItem[]>(() => {
    if (!Array.isArray(finalObj?.job_events)) return [];
    const items = finalObj.job_events.slice(-100);
    return items.map((event: any, idx: number) => {
      let payload: any = event?.payloadJson ?? null;
      if (typeof payload === 'string') {
        const parsed = tryParseJson(payload);
        payload = parsed ?? payload;
      }
      return {
        key: `fallback-${idx}-${event?.id || ''}`,
        event: String(event?.type || 'event'),
        payload,
      };
    });
  }, [finalObj]);

  const visibleEvents = progressItems.length > 0 ? progressItems : fallbackEvents;

  const reasoning = useMemo(() => {
    if (typeof finalObj?.reasoning === 'string' && finalObj.reasoning.trim().length > 0) {
      return finalObj.reasoning.trim();
    }
    for (let i = visibleEvents.length - 1; i >= 0; i -= 1) {
      const event = visibleEvents[i];
      const payload = event.payload;
      const candidate = payload?.payload?.reasoning || payload?.reasoning;
      if (typeof candidate === 'string' && candidate.trim().length > 0) {
        return candidate.trim();
      }
    }
    return '';
  }, [finalObj, visibleEvents]);

  const responseText = useMemo(() => {
    if (typeof finalObj?.response === 'string' && finalObj.response.trim().length > 0) {
      return finalObj.response;
    }
    if (typeof finalRaw === 'string') return finalRaw;
    return safeString(finalObj);
  }, [finalObj, finalRaw]);

  const status = toolCall?.error
    ? 'error'
    : typeof finalObj?.status === 'string'
      ? finalObj.status
      : visibleEvents.length > 0
        ? 'running'
        : 'idle';

  return (
    <div className="w-[420px] max-w-[45vw] bg-card border-l border-border flex flex-col">
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div>
          <div className="text-sm font-semibold text-foreground">Sub-Agent 执行过程</div>
          <div className="text-xs text-muted-foreground break-all">{toolName}</div>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-xs px-2 py-1 rounded border border-border text-muted-foreground hover:text-foreground hover:bg-accent"
        >
          关闭
        </button>
      </div>

      <div className="px-4 py-2 text-xs border-b border-border bg-muted/20">
        <span className="text-muted-foreground">状态：</span>
        <span className={status === 'error' ? 'text-destructive font-medium' : 'text-foreground font-medium'}>{status}</span>
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-4">
        <section>
          <h3 className="text-xs font-semibold text-foreground mb-2">执行事件</h3>
          {visibleEvents.length === 0 ? (
            <div className="text-xs text-muted-foreground">暂无事件输出</div>
          ) : (
            <div className="space-y-2">
              {visibleEvents.map((item) => (
                <div key={item.key} className="rounded border border-border bg-background p-2">
                  <div className="text-xs font-medium text-foreground mb-1">
                    {EVENT_LABEL[item.event] || item.event}
                  </div>
                  <pre className="text-[11px] whitespace-pre-wrap break-words text-muted-foreground max-h-36 overflow-y-auto">
                    {typeof item.payload === 'string'
                      ? item.payload
                      : JSON.stringify(item.payload, null, 2)}
                  </pre>
                </div>
              ))}
            </div>
          )}
        </section>

        <section>
          <h3 className="text-xs font-semibold text-foreground mb-2">模型思考</h3>
          {reasoning ? (
            <div className="rounded border border-border bg-background p-2">
              <MarkdownRenderer content={reasoning} />
            </div>
          ) : (
            <div className="text-xs text-muted-foreground">暂无思考内容</div>
          )}
        </section>

        <section>
          <h3 className="text-xs font-semibold text-foreground mb-2">最终结果</h3>
          {responseText ? (
            <div className="rounded border border-border bg-background p-2">
              <MarkdownRenderer content={responseText} />
            </div>
          ) : (
            <div className="text-xs text-muted-foreground">尚未产出最终结果</div>
          )}
        </section>
      </div>
    </div>
  );
};

export default SubAgentRunPanel;
