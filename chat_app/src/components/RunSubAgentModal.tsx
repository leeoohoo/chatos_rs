import React, { useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { MarkdownRenderer } from './MarkdownRenderer';
import './RunSubAgentModal.css';

interface RunSubAgentModalProps {
  toolCall: any;
  onClose: () => void;
}

interface ProgressEvent {
  key: string;
  event: string;
  payload: any;
  createdAt: string;
}



const safeString = (value: unknown): string => {
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return '';
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);
  try {
    return JSON.stringify(value);
  } catch {
    return '';
  }
};

const tryParseJson = (raw: string): any | null => {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
};

const parseMaybeJsonValue = (value: any): any => {
  if (typeof value !== 'string') return value;
  const trimmed = value.trim();
  if (!trimmed) return value;
  const parsed = tryParseJson(trimmed);
  return parsed ?? value;
};

const parseTask = (rawArguments: unknown): string => {
  if (!rawArguments) return '';

  if (typeof rawArguments === 'object') {
    const args = rawArguments as Record<string, unknown>;
    const task = args.task;
    return typeof task === 'string' ? task.trim() : '';
  }

  if (typeof rawArguments !== 'string') return '';

  try {
    const parsed = JSON.parse(rawArguments);
    if (parsed && typeof parsed.task === 'string') {
      return parsed.task.trim();
    }
  } catch {
    return '';
  }

  return '';
};

const toStringList = (value: unknown): string[] => {
  const values: string[] = [];

  const append = (raw: unknown) => {
    const normalized = safeString(raw).trim();
    if (normalized) values.push(normalized);
  };

  if (Array.isArray(value)) {
    value.forEach((item) => {
      if (typeof item === 'string') {
        append(item);
        return;
      }
      if (item && typeof item === 'object') {
        const objectItem = item as Record<string, unknown>;
        append(objectItem.id ?? objectItem.skill_id ?? objectItem.skillId ?? objectItem.name);
        return;
      }
      append(item);
    });
  } else if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!trimmed) return [];

    const parsed = tryParseJson(trimmed);
    if (Array.isArray(parsed)) {
      return toStringList(parsed);
    }

    if (trimmed.includes(',')) {
      trimmed.split(',').forEach((item) => append(item));
    } else {
      append(trimmed);
    }
  }

  const seen = new Set<string>();
  return values.filter((item) => {
    if (seen.has(item)) return false;
    seen.add(item);
    return true;
  });
};

const parseSkillsFromArguments = (rawArguments: unknown): string[] => {
  if (!rawArguments) return [];

  if (typeof rawArguments === 'object') {
    const args = rawArguments as Record<string, unknown>;
    return toStringList(args.skills);
  }

  if (typeof rawArguments !== 'string') return [];

  const parsed = tryParseJson(rawArguments);
  if (parsed && typeof parsed === 'object') {
    return toStringList((parsed as Record<string, unknown>).skills);
  }

  return [];
};

const extractSkillIds = (payload: any): string[] => toStringList(
  payload?.skills
  ?? payload?.selected_skill_ids
  ?? payload?.selected_skills
  ?? payload?.used_skills
  ?? payload?.skill_ids
  ?? payload?.resolved_skills,
);

const toPayloadPreview = (payload: any, max = 180): string => {
  if (payload === null || payload === undefined) return '';

  let raw = '';
  if (typeof payload === 'string') {
    raw = payload;
  } else if (typeof payload === 'object') {
    raw = safeString(
      payload.preview
      || payload.content_preview
      || payload.response_preview
      || payload.reasoning_preview
      || payload.chunk
      || payload.content,
    );
    if (!raw) raw = safeString(payload);
  } else {
    raw = String(payload);
  }

  const normalized = raw.replace(/\s+/g, ' ').trim();
  if (!normalized) return '';
  return normalized.length > max ? `${normalized.slice(0, max)}...` : normalized;
};

const normalizedReasoningKey = (value: string): string => (
  value
    .toLowerCase()
    .replace(/\*+/g, '')
    .replace(/[`~]/g, '')
    .replace(/[，。！？!?,.:;；、'\"“”‘’(){}\[\]-]/g, '')
    .replace(/\s+/g, '')
);

const joinStreamText = (current: string, chunk: string): string => {
  if (!chunk) return current;
  if (!current) return chunk;

  // Some providers stream cumulative snapshots, others stream deltas.
  if (chunk.startsWith(current)) return chunk;
  if (current.startsWith(chunk)) return current;
  if (current.includes(chunk)) return current;
  if (chunk.includes(current)) return chunk;

  const maxOverlap = Math.min(current.length, chunk.length);
  for (let overlap = maxOverlap; overlap >= 8; overlap -= 1) {
    if (current.slice(-overlap) === chunk.slice(0, overlap)) {
      return `${current}${chunk.slice(overlap)}`;
    }
  }

  return `${current}${chunk}`;
};

const collapseRepeatedReasoningLines = (value: string): string => {
  const lines = value
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);

  const compact: string[] = [];
  let previousKey = '';

  lines.forEach((line) => {
    const key = normalizedReasoningKey(line);
    if (!key) return;
    if (key === previousKey) return;
    compact.push(line);
    previousKey = key;
  });

  return compact.join('\n');
};

const formatReasoningForDisplay = (value: string): string => {
  const normalized = value
    .replace(/\r\n?/g, '\n')
    .replace(/\*{2,}/g, '')
    .replace(/\u200b/g, '')
    .trim();
  if (!normalized) return '';

  let output = normalized;
  // Split common stream-merge artifacts like `inspectionStarting`.
  output = output.replace(/([a-z0-9])([A-Z])/g, '$1\n$2');
  // Break long reasoning text by sentence punctuation.
  output = output.replace(/([。！？!?])\s*/g, '$1\n');
  // Also break known reasoning verbs even if no punctuation was emitted.
  output = output.replace(/\b(Starting|Planning|Inspecting|Reading|Summarizing|Preparing|Confirming|Verifying|Scanning|Analyzing|Reviewing|Drafting|Checking|Investigating)\b/g, '\n$1');
  output = output.replace(/\n{3,}/g, '\n\n');
  output = collapseRepeatedReasoningLines(output);

  return output.trim();
};

const normalizeToolCalls = (payload: any): any[] => {
  if (!payload) return [];
  const raw = payload.tool_calls ?? payload.calls ?? payload;
  return Array.isArray(raw) ? raw : (raw ? [raw] : []);
};

const normalizeToolResults = (payload: any): any[] => {
  if (!payload) return [];
  const raw = payload.tool_results ?? payload.results ?? payload;
  return Array.isArray(raw) ? raw : (raw ? [raw] : []);
};

const isStreamTextEvent = (event: string): boolean => (
  event === 'ai_content_stream' || event === 'ai_reasoning_stream'
);

const compareEventByCreatedAt = (a: ProgressEvent, b: ProgressEvent): number => {
  const at = Date.parse(a.createdAt);
  const bt = Date.parse(b.createdAt);
  if (Number.isNaN(at) && Number.isNaN(bt)) return 0;
  if (Number.isNaN(at)) return -1;
  if (Number.isNaN(bt)) return 1;
  return at - bt;
};


const parseStreamEvents = (streamLog: string): ProgressEvent[] => {
  if (!streamLog.trim()) return [];

  const lines = streamLog
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  const parsedEvents = lines.map((line, index) => {
    const normalizedLine = line.startsWith('data:') ? line.slice(5).trim() : line;
    let parsed: any = tryParseJson(normalizedLine);

    if (typeof parsed === 'string') {
      const nested = tryParseJson(parsed);
      if (nested && typeof nested === 'object') {
        parsed = nested;
      }
    }

    if (parsed && typeof parsed === 'object') {
      const eventName = safeString((parsed as any).event || (parsed as any).type);
      const kind = safeString((parsed as any).kind);
      if (kind === 'sub_agent_progress' || eventName) {
        return {
          key: `stream-${index}-${eventName || 'event'}`,
          event: eventName || 'event',
          payload: parseMaybeJsonValue((parsed as any).payload ?? (parsed as any).data ?? null),
          createdAt: safeString((parsed as any).created_at || (parsed as any).createdAt) || new Date().toISOString(),
        };
      }
    }

    return {
      key: `stream-${index}-raw`,
      event: 'raw',
      payload: normalizedLine,
      createdAt: new Date().toISOString(),
    };
  });

  if (parsedEvents.length <= 2_500) return parsedEvents;

  const important = parsedEvents.filter((item) => !isStreamTextEvent(item.event));
  const streamOnly = parsedEvents.filter((item) => isStreamTextEvent(item.event)).slice(-1_800);
  return [...important, ...streamOnly];
};

const parseFinalEvents = (finalObj: any): ProgressEvent[] => {
  if (!Array.isArray(finalObj?.job_events)) return [];

  return finalObj.job_events
    .slice(-800)
    .map((item: any, index: number) => {
      let payload: any = item?.payloadJson ?? item?.payload_json ?? item?.payload ?? item?.data ?? null;
      payload = parseMaybeJsonValue(payload);

      const event = safeString(item?.type || item?.event || 'event') || 'event';
      const createdAt = safeString(item?.created_at || item?.createdAt) || new Date().toISOString();

      return {
        key: `final-${index}-${item?.id || event}`,
        event,
        payload,
        createdAt,
      };
    });
};

const normalizeStatus = (status: string): string => {
  const s = status.trim().toLowerCase();
  if (!s) return '';
  if (s === 'ok' || s === 'done' || s === 'completed' || s === 'success') return 'completed';
  if (s === 'cancelled' || s === 'canceled') return 'cancelled';
  if (s === 'error' || s === 'failed' || s === 'fail') return 'error';
  if (s === 'running' || s === 'in_progress') return 'running';
  return s;
};

export const RunSubAgentModal: React.FC<RunSubAgentModalProps> = ({ toolCall, onClose }) => {
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const reasoningCacheRef = useRef<string>('');

  const task = useMemo(() => parseTask(toolCall?.arguments), [toolCall?.arguments]);
  const requestedSkillIds = useMemo(() => parseSkillsFromArguments(toolCall?.arguments), [toolCall?.arguments]);
  const streamLog = safeString(toolCall?.streamLog);
  const finalText = safeString(toolCall?.finalResult);
  const fallbackText = safeString(toolCall?.result);
  const persistedText = safeString((toolCall as any)?.persistedResult || (toolCall as any)?.toolMessageContent);
  const explicitError = safeString(toolCall?.error);

  const streamEvents = useMemo(() => parseStreamEvents(streamLog), [streamLog]);

  const finalRawText = useMemo(() => {
    if (finalText.trim()) return finalText;
    if (persistedText.trim()) return persistedText;
    if (fallbackText.trim() && fallbackText !== streamLog) return fallbackText;
    return '';
  }, [finalText, persistedText, fallbackText, streamLog]);

  const finalObj = useMemo(() => {
    if (!finalRawText.trim()) return null;
    return tryParseJson(finalRawText);
  }, [finalRawText]);

  const fallbackEvents = useMemo(() => parseFinalEvents(finalObj), [finalObj]);
  const events = useMemo(() => {
    const merged = [...fallbackEvents, ...streamEvents];
    if (merged.length === 0) return [];

    const deduped = new Map<string, ProgressEvent>();
    merged.forEach((item) => {
      const key = [
        item.event,
        item.createdAt,
        safeString(item.payload?.tool_call_id || item.payload?.toolCallId || item.payload?.id),
        toPayloadPreview(item.payload, 120),
      ].join('|');
      if (!deduped.has(key)) {
        deduped.set(key, item);
      }
    });

    const ordered = Array.from(deduped.values()).sort(compareEventByCreatedAt);
    if (ordered.length <= 2_500) return ordered;

    const important = ordered.filter((item) => !isStreamTextEvent(item.event));
    const streamOnly = ordered.filter((item) => isStreamTextEvent(item.event)).slice(-1_800);
    return [...important, ...streamOnly].sort(compareEventByCreatedAt);
  }, [fallbackEvents, streamEvents]);

  const hasStream = streamLog.trim().length > 0;
  const hasFinal = finalText.trim().length > 0;
  const hasPersisted = persistedText.trim().length > 0;
  const hasFallback = fallbackText.trim().length > 0;
  const markedCompleted = (toolCall as any)?.completed === true;
  const finalStatus = normalizeStatus(safeString(finalObj?.status));
  const inferredCompleted = hasStream && (hasFinal || hasPersisted || (hasFallback && fallbackText !== streamLog));
  const completedByStatus = finalStatus === 'completed' || finalStatus === 'error' || finalStatus === 'cancelled';
  const isCompleted = markedCompleted || completedByStatus || inferredCompleted;

  const mergedErrorText = useMemo(() => {
    if (explicitError.trim()) return explicitError;
    const err = safeString(finalObj?.error);
    return err.trim();
  }, [explicitError, finalObj]);

  const status = mergedErrorText
    ? 'error'
    : finalStatus === 'cancelled'
      ? 'cancelled'
      : isCompleted
        ? 'completed'
        : 'running';

  const streamedResponseText = useMemo(() => {
    let content = '';
    for (const item of events) {
      if (item.event !== 'ai_content_stream') continue;
      const chunk = safeString(item.payload?.chunk ?? item.payload?.content ?? item.payload);
      if (chunk) content += chunk;
    }
    return content;
  }, [events]);

  const streamedReasoningText = useMemo(() => {
    let content = '';
    for (const item of events) {
      if (item.event !== 'ai_reasoning_stream') continue;
      const chunk = safeString(item.payload?.chunk ?? item.payload?.content ?? item.payload);
      if (chunk) content = joinStreamText(content, chunk);
    }
    return content;
  }, [events]);

  useEffect(() => {
    const normalized = streamedReasoningText.trim();
    if (!normalized) return;
    reasoningCacheRef.current = normalized;
  }, [streamedReasoningText]);

  const finalResponseText = useMemo(() => {
    const fromFinal = safeString(finalObj?.response);
    if (fromFinal.trim()) return fromFinal;

    if (finalRawText.trim()) {
      const parsed = tryParseJson(finalRawText);
      if (parsed && typeof parsed === 'object') {
        const alt = safeString((parsed as any).response || (parsed as any).result || (parsed as any).content);
        if (alt.trim()) return alt;
      }
    }

    const lastPreviewEvent = [...events].reverse().find((item) => (
      item.event === 'ai_content_ready'
      || item.event === 'ai_finish'
      || item.event === 'ai_response_received'
    ));
    if (lastPreviewEvent) {
      const preview = safeString(
        lastPreviewEvent.payload?.preview
        || lastPreviewEvent.payload?.response_preview
        || lastPreviewEvent.payload?.content_preview,
      );
      if (preview.trim()) return preview;
    }

    if (hasFinal) return finalText;
    if (hasPersisted) return persistedText;
    if (hasFallback && fallbackText !== streamLog) return fallbackText;

    return '';
  }, [finalObj, finalRawText, events, hasFinal, hasPersisted, hasFallback, finalText, persistedText, fallbackText, streamLog]);

  const reasoningText = useMemo(() => {
    const streamRaw = safeString(
      streamedReasoningText.trim()
        ? streamedReasoningText
        : reasoningCacheRef.current,
    );
    const streamReasoning = formatReasoningForDisplay(streamRaw);
    const finalReasoning = formatReasoningForDisplay(safeString(finalObj?.reasoning));

    if (streamReasoning && finalReasoning) {
      const streamKey = normalizedReasoningKey(streamReasoning);
      const finalKey = normalizedReasoningKey(finalReasoning);

      if (!streamKey) return finalReasoning;
      if (!finalKey) return streamReasoning;
      if (streamKey === finalKey) {
        return streamReasoning.length >= finalReasoning.length
          ? streamReasoning
          : finalReasoning;
      }
      if (streamKey.includes(finalKey)) return streamReasoning;
      if (finalKey.includes(streamKey)) return finalReasoning;
      return collapseRepeatedReasoningLines(`${streamReasoning}
${finalReasoning}`);
    }

    return streamReasoning || finalReasoning;
  }, [finalObj, streamedReasoningText]);

  const assistantText = useMemo(() => {
    if (status === 'error') return mergedErrorText;
    if (status === 'running') {
      if (streamedResponseText.trim()) return streamedResponseText;
      if (finalResponseText.trim()) return finalResponseText;
      if (streamEvents.length === 0 && hasStream) return streamLog;
      return '';
    }

    if (finalResponseText.trim()) return finalResponseText;
    if (streamedResponseText.trim()) return streamedResponseText;
    return '';
  }, [status, mergedErrorText, streamedResponseText, finalResponseText, streamEvents.length, hasStream, streamLog]);

  const toolTags = useMemo(() => {
    const byId = new Map<string, {
      key: string;
      name: string;
      status: 'running' | 'done' | 'error';
      preview: string;
      argsPreview: string;
      resultPreview: string;
      startAt: string;
      finishAt: string;
      lastAt: string;
    }>();
    const order: string[] = [];

    const upsert = (id: string, name: string) => {
      if (!byId.has(id)) {
        byId.set(id, {
          key: id,
          name: name || id,
          status: 'running',
          preview: '',
          argsPreview: '',
          resultPreview: '',
          startAt: '',
          finishAt: '',
          lastAt: '',
        });
        order.push(id);
      }
      const current = byId.get(id)!;
      if (name && (!current.name || current.name.startsWith('tool_'))) {
        current.name = name;
      }
      return current;
    };

    events.forEach((item, eventIndex) => {
      if (item.event === 'ai_tools_start') {
        normalizeToolCalls(item.payload?.tool_calls ?? item.payload).forEach((call: any, idx: number) => {
          const id = safeString(call?.tool_call_id || call?.id || call?.toolCallId || `${item.event}-${eventIndex}-${idx}`);
          const name = safeString(call?.name || call?.function?.name || call?.tool_name || call?.toolName || id || `tool_${idx}`);
          const argsPreview = toPayloadPreview(call?.arguments_preview || call?.arguments || call?.function?.arguments || call?.args, 1_600);
          const tag = upsert(id, name);
          if (argsPreview) {
            tag.preview = argsPreview;
            tag.argsPreview = argsPreview;
          }
          tag.status = 'running';
          tag.startAt = tag.startAt || item.createdAt;
          tag.lastAt = item.createdAt;
        });
      }

      if (item.event === 'ai_tools_stream' || item.event === 'ai_tools_end') {
        normalizeToolResults(item.payload?.tool_results ?? item.payload).forEach((result: any, idx: number) => {
          const id = safeString(result?.tool_call_id || result?.id || result?.toolCallId || `${item.event}-${eventIndex}-${idx}`);
          const name = safeString(result?.name || result?.tool_name || result?.toolName || id || `tool_${idx}`);
          const tag = upsert(id, name);
          const preview = toPayloadPreview(
            result?.content_preview || result?.content || result?.result || result?.output || result,
            2_400,
          );
          if (preview) {
            tag.preview = preview;
            tag.resultPreview = preview;
          }
          const isError = result?.is_error === true || result?.success === false;
          tag.status = isError ? 'error' : (item.event === 'ai_tools_end' ? 'done' : 'running');
          if (item.event === 'ai_tools_end') {
            tag.finishAt = item.createdAt;
          }
          tag.lastAt = item.createdAt;
        });
      }
    });

    return order
      .map((id) => byId.get(id))
      .filter((item): item is {
        key: string;
        name: string;
        status: 'running' | 'done' | 'error';
        preview: string;
        argsPreview: string;
        resultPreview: string;
        startAt: string;
        finishAt: string;
        lastAt: string;
      } => Boolean(item));
  }, [events]);

  const [selectedToolKey, setSelectedToolKey] = useState<string>('');

  useEffect(() => {
    if (toolTags.length === 0) {
      if (selectedToolKey) {
        setSelectedToolKey('');
      }
      return;
    }

    if (!selectedToolKey || !toolTags.some((tool) => tool.key === selectedToolKey)) {
      setSelectedToolKey(toolTags[toolTags.length - 1].key);
    }
  }, [toolTags, selectedToolKey]);

  const jobId = useMemo(() => {
    const fromFinal = safeString(finalObj?.job_id);
    if (fromFinal.trim()) return fromFinal;
    const fromEvent = events.find((item) => safeString((item as any)?.job_id).trim());
    return fromEvent ? safeString((fromEvent as any).job_id) : '';
  }, [events, finalObj]);

  const agentId = useMemo(() => {
    const fromFinal = safeString(finalObj?.agent_id);
    if (fromFinal.trim()) return fromFinal;
    const fromEvent = events.find((item) => safeString(item.payload?.agent_id).trim());
    return fromEvent ? safeString(fromEvent.payload?.agent_id) : '';
  }, [events, finalObj]);


  const usedSkillIds = useMemo(() => {
    const fromFinal = extractSkillIds(finalObj);
    if (fromFinal.length > 0) return fromFinal;

    for (let index = events.length - 1; index >= 0; index -= 1) {
      const fromEvent = extractSkillIds(events[index]?.payload);
      if (fromEvent.length > 0) return fromEvent;
    }

    const fromToolCall = extractSkillIds(toolCall);
    if (fromToolCall.length > 0) return fromToolCall;

    return requestedSkillIds;
  }, [finalObj, events, toolCall, requestedSkillIds]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  useEffect(() => {
    if (status !== 'running') return;
    const body = bodyRef.current;
    if (!body) return;
    body.scrollTo({ top: body.scrollHeight, behavior: 'smooth' });
  }, [assistantText, toolTags.length, status]);

  const badgeClass = status === 'error'
    ? 'bg-red-500/10 text-red-500 border-red-500/30'
    : status === 'cancelled'
      ? 'bg-slate-500/10 text-slate-400 border-slate-500/30'
      : status === 'completed'
        ? 'bg-emerald-500/10 text-emerald-500 border-emerald-500/30'
        : 'bg-amber-500/10 text-amber-500 border-amber-500/30';

  const badgeText = status === 'error'
    ? '错误'
    : status === 'cancelled'
      ? '已取消'
      : status === 'completed'
        ? '已完成'
        : '运行中';

  const modalContent = (
    <>
      <div
        className="fixed inset-0 bg-black/60 backdrop-blur-sm z-[70]"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-[71] flex items-center justify-center p-4">
        <div className="w-full max-w-6xl h-[82vh] min-h-[620px] bg-card border border-border rounded-xl shadow-2xl flex flex-col overflow-hidden">
          <div className="flex items-center justify-between px-5 py-4 border-b border-border bg-muted/20">
            <div className="min-w-0">
              <div className="text-sm font-semibold text-foreground">Run Sub-Agent</div>
              <div className="text-xs text-muted-foreground truncate">{toolCall?.name || 'run_sub_agent'}</div>
              {(jobId || agentId) && (
                <div className="text-[11px] text-muted-foreground mt-1 truncate">
                  {jobId ? `job: ${jobId}` : ''}
                  {jobId && agentId ? ' · ' : ''}
                  {agentId ? `agent: ${agentId}` : ''}
                </div>
              )}
              {usedSkillIds.length > 0 && (
                <div className="mt-1 flex items-start gap-1.5">
                  <span className="pt-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">skills</span>
                  <div className="flex flex-wrap gap-1.5">
                    {usedSkillIds.map((skillId) => (
                      <span
                        key={`skill-${skillId}`}
                        className="inline-flex items-center rounded-full border border-blue-500/25 bg-blue-500/10 px-2 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-300"
                      >
                        {skillId}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
            <div className="flex items-center gap-2">
              <span className={`inline-flex items-center rounded-full px-2.5 py-1 text-xs font-medium border ${badgeClass}`}>
                {badgeText}
              </span>
              <button
                type="button"
                onClick={onClose}
                className="h-8 w-8 rounded-md border border-border text-muted-foreground hover:text-foreground hover:bg-accent"
                aria-label="关闭弹窗"
              >
                <svg className="w-4 h-4 mx-auto" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18" strokeLinecap="round" />
                  <path d="M6 6l12 12" strokeLinecap="round" />
                </svg>
              </button>
            </div>
          </div>

          <div className="flex-1 overflow-hidden grid grid-cols-1 lg:grid-cols-[380px_minmax(0,1fr)]">
            <div className="border-b lg:border-b-0 lg:border-r border-border bg-muted/10 overflow-y-auto overflow-x-hidden px-4 py-4">
              <div className="rounded-lg border border-border bg-card px-4 py-3 space-y-3">
                <div className="text-xs font-medium text-muted-foreground">工具调用（点击标签查看）</div>

                {toolTags.length === 0 ? (
                  <div className="text-xs text-muted-foreground">暂无工具调用</div>
                ) : (
                  <div className="space-y-2">
                    {toolTags.map((tool) => {
                      const isActive = selectedToolKey === tool.key;
                      const statusClass = tool.status === 'error'
                        ? 'text-red-500 border-red-500/30 bg-red-500/10'
                        : tool.status === 'done'
                          ? 'text-emerald-500 border-emerald-500/30 bg-emerald-500/10'
                          : 'text-amber-500 border-amber-500/30 bg-amber-500/10';
                      const statusText = tool.status === 'error' ? 'ERROR' : (tool.status === 'done' ? 'DONE' : 'RUNNING');
                      const argsText = (tool.argsPreview || '').trim();

                      const rawResult = (tool.resultPreview || tool.preview || '').trim();
                      let resultText = rawResult;
                      const parsedResult = tryParseJson(rawResult);
                      if (parsedResult && typeof parsedResult === 'object') {
                        const pretty = safeString(JSON.stringify(parsedResult, null, 2));
                        if (pretty.trim()) {
                          resultText = pretty;
                        }
                      }

                      return (
                        <div
                          key={`tool-tag-${tool.key}`}
                          className={`rounded-md border bg-background/80 px-2.5 py-2 ${isActive ? 'border-blue-500/40' : 'border-border'}`}
                        >
                          <button
                            type="button"
                            onClick={() => setSelectedToolKey(tool.key)}
                            className="flex w-full min-w-0 items-center gap-1.5 text-left"
                            title={tool.preview || undefined}
                          >
                            <svg className="h-3.5 w-3.5 shrink-0 text-blue-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                              <path d="M13 2 3 14h7l-1 8 12-14h-7l1-6z" />
                            </svg>
                            <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground">@{tool.name}</span>
                            <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] ${statusClass}`}>{statusText}</span>
                            {(tool.finishAt || tool.lastAt) && (
                              <span className="shrink-0 text-[10px] text-muted-foreground">{new Date(tool.finishAt || tool.lastAt).toLocaleTimeString()}</span>
                            )}
                          </button>

                          {isActive && (
                            <div className="mt-2 space-y-2 border-t border-border/70 pt-2">
                              <div className="text-[10px] font-mono text-muted-foreground truncate" title={tool.key}>id: {tool.key}</div>

                              {argsText && (
                                <div>
                                  <div className="text-[11px] font-medium text-foreground/90">参数</div>
                                  <pre className="mt-1 max-h-28 overflow-y-auto rounded-md border border-border bg-muted/20 p-2 text-[11px] leading-5 text-muted-foreground whitespace-pre-wrap break-words">{argsText}</pre>
                                </div>
                              )}

                              {resultText ? (
                                <div>
                                  <div className="text-[11px] font-medium text-foreground/90">结果</div>
                                  <pre className="mt-1 max-h-40 overflow-y-auto rounded-md border border-border bg-muted/20 p-2 text-[11px] leading-5 text-muted-foreground whitespace-pre-wrap break-words">{resultText}</pre>
                                </div>
                              ) : (
                                <div className="text-[11px] text-muted-foreground">等待工具返回...</div>
                              )}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>

            <div ref={bodyRef} className="min-w-0 overflow-y-auto px-5 py-4 space-y-4 bg-background">
              <div className="flex justify-end">
                <div className="max-w-[88%] min-w-0 rounded-2xl rounded-tr-sm px-4 py-3 bg-primary text-primary-foreground">
                  <div className="text-xs opacity-80 mb-1">你</div>
                  <div className="text-sm whitespace-pre-wrap break-words">{task || '（未提供 task）'}</div>
                </div>
              </div>

              <div className="flex justify-start">
                <div className="w-full max-w-[90%] min-w-0 rounded-2xl rounded-tl-sm px-4 py-3 bg-muted border border-border">
                  <div className="text-xs text-muted-foreground mb-2">Sub-Agent</div>
                  {assistantText.trim().length > 0 ? (
                    <div className="run-sub-agent-markdown-wrap">
                      <MarkdownRenderer content={assistantText} isStreaming={status === 'running'} className="run-sub-agent-markdown" />
                    </div>
                  ) : (
                    <div className="text-sm text-muted-foreground">等待模型返回...</div>
                  )}
                  {status === 'running' && (
                    <div className="mt-2 inline-flex items-center gap-1 text-xs text-muted-foreground">
                      <span className="w-1.5 h-1.5 rounded-full bg-current animate-pulse" />
                      <span>流式返回中</span>
                    </div>
                  )}
                </div>
              </div>

              {reasoningText.trim().length > 0 && (
                <div className="rounded-lg border border-border bg-card px-4 py-3 space-y-2">
                  <div className="text-xs font-medium text-muted-foreground">思考摘要</div>
                  <div className="text-sm leading-6 text-foreground/90 whitespace-pre-wrap break-words">{reasoningText}</div>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(modalContent, document.body);
};

export default RunSubAgentModal;
