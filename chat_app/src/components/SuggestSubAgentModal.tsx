import React, { useEffect, useMemo, useRef } from 'react';
import { createPortal } from 'react-dom';
import { MarkdownRenderer } from './MarkdownRenderer';

interface SuggestSubAgentModalProps {
  toolCall: any;
  onClose: () => void;
}

interface ParsedSuggestResult {
  agentId: string;
  skills: string[];
  reason: string;
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

const parseSuggestResult = (text: string): ParsedSuggestResult => {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);

  let agentId = '';
  let reason = '';
  let skillsRaw = '';

  for (const line of lines) {
    const splitIndex = line.indexOf(':');
    if (splitIndex < 0) continue;
    const key = line.slice(0, splitIndex).trim().toLowerCase();
    const value = line.slice(splitIndex + 1).trim();
    if (key === 'agent_id') {
      agentId = value;
    } else if (key === 'skills') {
      skillsRaw = value;
    } else if (key === 'reason') {
      reason = value;
    }
  }

  const skills = skillsRaw
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);

  return {
    agentId,
    skills,
    reason,
  };
};

export const SuggestSubAgentModal: React.FC<SuggestSubAgentModalProps> = ({ toolCall, onClose }) => {
  const bodyRef = useRef<HTMLDivElement | null>(null);

  const task = useMemo(() => parseTask(toolCall?.arguments), [toolCall?.arguments]);
  const streamLog = safeString(toolCall?.streamLog);
  const finalText = safeString(toolCall?.finalResult);
  const fallbackText = safeString(toolCall?.result);
  const persistedText = safeString((toolCall as any)?.persistedResult || (toolCall as any)?.toolMessageContent);
  const errorText = safeString(toolCall?.error);

  const hasStream = streamLog.trim().length > 0;
  const hasFallback = fallbackText.trim().length > 0;
  const hasPersisted = persistedText.trim().length > 0;
  const hasFinal = finalText.trim().length > 0;
  const markedCompleted = (toolCall as any)?.completed === true;
  const inferredCompleted = hasStream && hasFallback && fallbackText !== streamLog;
  const isCompleted = markedCompleted || hasFinal || hasPersisted || inferredCompleted;
  const isError = errorText.trim().length > 0;
  const status = isError ? 'error' : (isCompleted ? 'completed' : 'running');

  const assistantText = useMemo(() => {
    if (isError) return errorText;

    if (status === 'running') {
      if (hasStream) return streamLog;
      if (hasFallback) return fallbackText;
      return '';
    }

    if (hasFinal) return finalText;
    if (hasPersisted) return persistedText;
    if (hasFallback) return fallbackText;
    if (hasStream) return streamLog;
    return '';
  }, [
    status,
    isError,
    hasFinal,
    hasPersisted,
    hasFallback,
    hasStream,
    errorText,
    finalText,
    persistedText,
    fallbackText,
    streamLog,
  ]);

  const parsedResult = useMemo(() => {
    if (!isCompleted || isError) return null;
    const parsed = parseSuggestResult(assistantText);
    if (!parsed.agentId && parsed.skills.length === 0 && !parsed.reason) {
      return null;
    }
    return parsed;
  }, [assistantText, isCompleted, isError]);

  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    body.scrollTo({ top: body.scrollHeight, behavior: 'smooth' });
  }, [assistantText, status]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onClose]);

  const modalContent = (
    <>
      <div
        className="fixed inset-0 bg-black/60 backdrop-blur-sm z-[70]"
        onClick={onClose}
      />
      <div className="fixed inset-0 z-[71] flex items-center justify-center p-4">
        <div className="w-full max-w-3xl h-[78vh] min-h-[560px] bg-card border border-border rounded-xl shadow-2xl flex flex-col overflow-hidden">
          <div className="flex items-center justify-between px-5 py-4 border-b border-border bg-muted/20">
            <div className="min-w-0">
              <div className="text-sm font-semibold text-foreground">Suggest Sub-Agent</div>
              <div className="text-xs text-muted-foreground truncate">{toolCall?.name || 'suggest_sub_agent'}</div>
            </div>
            <div className="flex items-center gap-2">
              <span
                className={[
                  'inline-flex items-center rounded-full px-2.5 py-1 text-xs font-medium border',
                  status === 'error'
                    ? 'bg-red-500/10 text-red-500 border-red-500/30'
                    : status === 'completed'
                      ? 'bg-emerald-500/10 text-emerald-500 border-emerald-500/30'
                      : 'bg-amber-500/10 text-amber-500 border-amber-500/30',
                ].join(' ')}
              >
                {status === 'error' ? '错误' : status === 'completed' ? '已完成' : '运行中'}
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

          <div ref={bodyRef} className="flex-1 overflow-y-auto px-5 py-4 space-y-4 bg-background">
            <div className="flex justify-end">
              <div className="max-w-[80%] rounded-2xl rounded-tr-sm px-4 py-3 bg-primary text-primary-foreground">
                <div className="text-xs opacity-80 mb-1">你</div>
                <div className="text-sm whitespace-pre-wrap break-words">{task || '（未提供 task）'}</div>
              </div>
            </div>

            <div className="flex justify-start">
              <div className="max-w-[88%] rounded-2xl rounded-tl-sm px-4 py-3 bg-muted border border-border">
                <div className="text-xs text-muted-foreground mb-2">Sub-Agent Router</div>
                {assistantText.trim().length > 0 ? (
                  <MarkdownRenderer content={assistantText} isStreaming={status === 'running'} />
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

            {parsedResult && (
              <div className="rounded-lg border border-border bg-card px-4 py-3 space-y-2">
                <div className="text-xs font-medium text-muted-foreground">结构化结果</div>
                {parsedResult.agentId && (
                  <div className="text-sm text-foreground">
                    <span className="text-muted-foreground mr-2">agent_id:</span>
                    <span className="font-medium">{parsedResult.agentId}</span>
                  </div>
                )}
                {parsedResult.skills.length > 0 && (
                  <div className="flex flex-wrap gap-2">
                    {parsedResult.skills.map((skill) => (
                      <span key={skill} className="text-xs px-2 py-1 rounded-full border border-border bg-muted text-foreground">
                        {skill}
                      </span>
                    ))}
                  </div>
                )}
                {parsedResult.reason && (
                  <div className="text-sm text-foreground">
                    <span className="text-muted-foreground mr-2">reason:</span>
                    <span>{parsedResult.reason}</span>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );

  if (typeof document === 'undefined') return null;
  return createPortal(modalContent, document.body);
};

export default SuggestSubAgentModal;
