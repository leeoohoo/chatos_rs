import React, { useState, useRef, useCallback, useEffect, useMemo } from 'react';
import { cn } from '../lib/utils';
import type { InputAreaProps } from '../types';

const MAX_ATTACHMENTS = 20; // 个
const MAX_FILE_BYTES = 20 * 1024 * 1024; // 20MB
const MAX_TOTAL_BYTES = 50 * 1024 * 1024; // 50MB

const formatFileSize = (bytes: number) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const InputArea: React.FC<InputAreaProps> = ({
  onSend,
  onStop,
  disabled = false,
  isStreaming = false,
  placeholder = 'Type your message...',
  maxLength = 4000,
  allowAttachments = false,
  supportedFileTypes = [
    'image/*',
    'text/*',
    'application/json',
    'application/pdf',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  ],
  reasoningSupported = false,
  reasoningEnabled = false,
  onReasoningToggle,
  showModelSelector = false,
  selectedModelId = null,
  availableModels = [],
  onModelChange,
  selectedAgentId = null,
  availableAgents = [],
  onAgentChange,
  availableProjects = [],
}) => {
  const [message, setMessage] = useState('');
  const [attachments, setAttachments] = useState<File[]>([]);
  const [attachError, setAttachError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement>(null);
  // 记录全局拖拽层级，避免 dragenter/dragleave 抖动
  const globalDragCounter = useRef(0);

  const selectedAgent = useMemo(
    () => (selectedAgentId ? (availableAgents || []).find(a => a.id === selectedAgentId) : null),
    [availableAgents, selectedAgentId]
  );
  const selectedProject = useMemo(() => {
    if (!selectedAgent?.project_id) return null;
    return (availableProjects || []).find((p: any) => p.id === selectedAgent.project_id) || null;
  }, [availableProjects, selectedAgent]);
  const selectedModel = useMemo(
    () => (selectedModelId ? (availableModels || []).find(m => (m as any).id === selectedModelId) : null),
    [availableModels, selectedModelId]
  );
  const enabledAgents = useMemo(
    () => (availableAgents || []).filter((a: any) => a.enabled),
    [availableAgents]
  );
  const enabledModels = useMemo(
    () => (availableModels || []).filter((m: any) => m.enabled),
    [availableModels]
  );
  const hasAiOptions = (availableModels && availableModels.length > 0) || (availableAgents && availableAgents.length > 0);
  const currentAiLabel = useMemo(() => {
    if (selectedAgent) {
      const projectName = selectedProject?.name || (selectedAgent.project_id ? '未知项目' : '');
      return projectName
        ? `Agent: ${selectedAgent.name} · 项目: ${projectName}`
        : `Agent: ${selectedAgent.name}`;
    }
    return selectedModel ? `Model: ${(selectedModel as any).name}` : '选择 AI';
  }, [selectedAgent, selectedProject, selectedModel]);

  // 自动调整文本框高度
  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      const scrollHeight = textarea.scrollHeight;
      const maxHeight = 200; // 最大高度
      textarea.style.height = `${Math.min(scrollHeight, maxHeight)}px`;
    }
  }, []);

  // 关闭下拉：点击外部
  useEffect(() => {
    if (!pickerOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (!pickerRef.current) return;
      if (!pickerRef.current.contains(e.target as Node)) {
        setPickerOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [pickerOpen]);

  const isFileTypeAllowed = useCallback((file: File) => {
    if (!supportedFileTypes || supportedFileTypes.length === 0) return true;
    const type = String(file.type || '').toLowerCase();
    const name = String(file.name || '').toLowerCase();
    return supportedFileTypes.some((pattern) => {
      const p = String(pattern || '').toLowerCase().trim();
      if (!p) return false;
      if (p === '*/*') return true;
      if (p.endsWith('/*')) {
        if (type) return type.startsWith(p.slice(0, -1));
        return false;
      }
      if (p.startsWith('.')) return name.endsWith(p);
      if (type) return type === p;
      if (!type && p === 'application/pdf') return name.endsWith('.pdf');
      if (!type && p === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document') return name.endsWith('.docx');
      return false;
    });
  }, [supportedFileTypes]);

  // 统一添加文件并校验限制
  const addFiles = useCallback((incoming: File[]) => {
    if (!incoming || incoming.length === 0) return;
    setAttachments(prev => {
      const currentTotal = prev.reduce((s, f) => s + (f?.size || 0), 0);
      const errors: string[] = [];
      const accepted: File[] = [];
      let total = currentTotal;
      let count = prev.length;
      for (const f of incoming) {
        if (!f) continue;
        if (!isFileTypeAllowed(f)) {
          errors.push(`${f.name} 类型不支持`);
          continue;
        }
        if (f.size > MAX_FILE_BYTES) {
          errors.push(`${f.name} 超过单文件上限(${formatFileSize(MAX_FILE_BYTES)})`);
          continue;
        }
        if (count + 1 > MAX_ATTACHMENTS) {
          errors.push(`${f.name} 超过数量上限(${MAX_ATTACHMENTS} 个)`);
          continue;
        }
        if (total + f.size > MAX_TOTAL_BYTES) {
          errors.push(`${f.name} 导致总大小超限(${formatFileSize(MAX_TOTAL_BYTES)})`);
          continue;
        }
        accepted.push(f);
        total += f.size;
        count += 1;
      }
      if (errors.length > 0) {
        setAttachError(`部分文件未添加：${errors.join('；')}。限制：单文件≤${formatFileSize(MAX_FILE_BYTES)}，总计≤${formatFileSize(MAX_TOTAL_BYTES)}，最多 ${MAX_ATTACHMENTS} 个。`);
      } else {
        setAttachError(null);
      }
      return accepted.length > 0 ? [...prev, ...accepted] : prev;
    });
  }, [isFileTypeAllowed]);

  // 全局拖拽支持：允许把文件拖到整个应用任意位置
  useEffect(() => {
    if (!allowAttachments || disabled) return;

    const hasFiles = (e: DragEvent | ClipboardEvent) => {
      const dt = (e as DragEvent).dataTransfer;
      if (!dt) return false;
      try {
        // 检查包含文件
        if (dt.types && Array.from(dt.types).includes('Files')) return true;
        if (dt.files && dt.files.length > 0) return true;
      } catch (_) {}
      return false;
    };

    const onDragOver = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(true);
      try { (e.dataTransfer as DataTransfer).dropEffect = 'copy'; } catch (_) {}
    };

    const onDragEnter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current += 1;
      setIsDragging(true);
    };

    const onDragLeave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current = Math.max(0, globalDragCounter.current - 1);
      if (globalDragCounter.current === 0) setIsDragging(false);
    };

    const onDrop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current = 0;
      setIsDragging(false);
      const dt = e.dataTransfer as DataTransfer;
      const files = Array.from(dt.files || []);
      if (files.length > 0) {
        addFiles(files);
      }
    };

    window.addEventListener('dragover', onDragOver);
    window.addEventListener('dragenter', onDragEnter);
    window.addEventListener('dragleave', onDragLeave);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('dragenter', onDragEnter);
      window.removeEventListener('dragleave', onDragLeave);
      window.removeEventListener('drop', onDrop);
    };
  }, [allowAttachments, disabled, addFiles]);

  // 处理输入变化
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    if (value.length <= maxLength) {
      setMessage(value);
      adjustTextareaHeight();
    }
  };

  // 处理键盘事件
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // 处理粘贴附件（图片/文件）
  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    try {
      if (!allowAttachments || disabled) return;
      const dt = e.clipboardData;
      const collected: File[] = [];
      // 优先从 items 读取（可区分种类）
      if (dt && dt.items && dt.items.length > 0) {
        for (let i = 0; i < dt.items.length; i++) {
          const it = dt.items[i];
          if (it.kind === 'file') {
            const f = it.getAsFile();
            if (f && f.size > 0) collected.push(f);
          }
        }
      }
      // 兼容从 files 读取
      if (collected.length === 0 && dt && dt.files && dt.files.length > 0) {
        for (let i = 0; i < dt.files.length; i++) {
          const f = dt.files[i];
          if (f && f.size > 0) collected.push(f);
        }
      }

      if (collected.length > 0) {
        addFiles(collected);
        // 对于仅图片/文件粘贴，textarea 通常不会插入任何内容，无需阻止默认行为；
        // 若存在文本同时粘贴，保留文本默认粘贴到输入框。
      }
    } catch (_) {}
  };

  // 发送消息
  const handleSend = () => {
    const trimmedMessage = message.trim();
    if (!trimmedMessage && attachments.length === 0) return;
    if (disabled) return;

    // 检查是否选择了模型或智能体（二选一）
    if (showModelSelector && !selectedModelId && !selectedAgentId) {
      alert('请先选择一个模型或智能体');
      return;
    }

    onSend(trimmedMessage, attachments);
    setMessage('');
    setAttachments([]);
    
    // 重置文本框高度
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  // 处理文件选择
  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    addFiles(files);
    
    // 清空input以允许重复选择同一文件
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // 移除附件
  const removeAttachment = (index: number) => {
    setAttachments(prev => prev.filter((_, i) => i !== index));
  };

  // 拖拽处理
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (allowAttachments && !disabled) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    
    if (!allowAttachments || disabled) return;
    
    const files = Array.from(e.dataTransfer.files);
    addFiles(files);
  };

  return (
    <div className="border-t bg-background p-3 sm:p-4">
      {/* 附件预览 */}
      {attachments.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-2">
          {attachments.map((file, index) => (
            <div
              key={index}
              className="flex items-center gap-2 bg-muted px-3 py-2 rounded-lg text-sm"
            >
              <svg className="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
              </svg>
              <span className="truncate max-w-32">{file.name}</span>
              <span className="text-xs text-muted-foreground">({formatFileSize(file.size)})</span>
              <button
                onClick={() => removeAttachment(index)}
                className="text-muted-foreground hover:text-destructive transition-colors"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
      )}
      {attachError && (
        <div className="-mt-2 mb-3 text-xs text-destructive">{attachError}</div>
      )}

      {/* 输入区域 */}
      <div
        className={cn(
          'relative flex items-end gap-3 p-3 border rounded-lg transition-colors',
          'focus-within:border-primary',
          isDragging && 'border-primary bg-primary/5',
          disabled && 'opacity-50 cursor-not-allowed'
        )}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {/* 浮动的 AI 选择标签（不占用垂直空间） */}
        {showModelSelector && hasAiOptions && (
          <div className="absolute -top-3 left-3 z-10" ref={pickerRef}>
            <button
              type="button"
              onClick={() => setPickerOpen((v) => !v)}
              disabled={disabled}
              className={cn(
                'px-2 py-0.5 rounded-full border bg-background text-xs shadow-sm',
                'hover:bg-accent hover:text-accent-foreground transition-colors',
                'disabled:opacity-50 disabled:cursor-not-allowed'
              )}
              title="选择模型或智能体"
            >
              {currentAiLabel}
              <span className="ml-1">▾</span>
            </button>
            {pickerOpen && (
              <div className="absolute left-0 bottom-full mb-2 w-64 max-h-64 overflow-auto bg-popover text-popover-foreground border rounded-md shadow-lg">
                {enabledAgents.length > 0 && (
                  <>
                    <div className="px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">智能体</div>
                    {enabledAgents.map((agent: any) => (
                        <button
                          key={agent.id}
                          className={cn('w-full text-left px-3 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground', selectedAgentId === agent.id && 'bg-accent/40')}
                          onClick={() => { onAgentChange?.(agent.id); onModelChange?.(null); setPickerOpen(false); }}
                        >
                          [Agent] {agent.name}
                        </button>
                    ))}
                  </>
                )}
                {enabledModels.length > 0 && (
                  <>
                    <div className="px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground border-t">模型</div>
                    {enabledModels.map((model: any) => (
                        <button
                          key={model.id}
                          className={cn('w-full text-left px-3 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground', selectedModelId === model.id && 'bg-accent/40')}
                          onClick={() => { onModelChange?.(model.id); onAgentChange?.(null); setPickerOpen(false); }}
                        >
                          {model.name} ({model.model_name})
                        </button>
                    ))}
                  </>
                )}
                <div className="border-t" />
                <button
                  className="w-full text-left px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                  onClick={() => { onModelChange?.(null); onAgentChange?.(null); setPickerOpen(false); }}
                >
                  清除选择
                </button>
              </div>
            )}
          </div>
        )}
        {/* 附件按钮 */}
        {allowAttachments && (
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={disabled}
            className="flex-shrink-0 p-2 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title="Attach files"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
            </svg>
          </button>
        )}

        {reasoningSupported && (
          <button
            type="button"
            onClick={() => onReasoningToggle?.(!reasoningEnabled)}
            disabled={disabled || isStreaming}
            className={cn(
              'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
              reasoningEnabled
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'bg-muted text-muted-foreground hover:text-foreground',
              (disabled || isStreaming) && 'opacity-50 cursor-not-allowed'
            )}
            title={reasoningEnabled ? '推理已开启' : '推理已关闭'}
          >
            推理 {reasoningEnabled ? '开' : '关'}
          </button>
        )}

        {/* 移除行内选择器，使用右上角浮动标签 */}

        {/* 文本输入 */}
        <textarea
          ref={textareaRef}
          value={message}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={placeholder}
          disabled={disabled}
          className={cn(
            'flex-1 resize-none bg-transparent border-none outline-none',
            'placeholder:text-muted-foreground',
            'disabled:cursor-not-allowed'
          )}
          rows={1}
          style={{ minHeight: '24px', maxHeight: '200px' }}
        />

        {/* 右侧不再放选择器，避免靠近发送按钮 */}

        {/* 字符计数 */}
        <div className="flex-shrink-0 text-[11px] sm:text-xs text-muted-foreground tabular-nums">
          {message.length}/{maxLength}
        </div>

        {/* 发送/停止按钮 */}
        {isStreaming ? (
          <button
            onClick={() => {
              if (onStop) {
                onStop();
              }
            }}
            disabled={false}
            className={cn(
              'flex-shrink-0 p-2 rounded-md transition-colors',
              'bg-red-500 text-white hover:bg-red-600',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
            title="停止生成"
            style={{ backgroundColor: '#ef4444', color: 'white' }}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 6h12v12H6z" />
            </svg>
          </button>
        ) : (
          <button
            onClick={handleSend}
            disabled={disabled || isStreaming || (!message.trim() && attachments.length === 0)}
            className={cn(
              'flex-shrink-0 p-2 rounded-md transition-colors',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              (message.trim() || attachments.length > 0) && !disabled && !isStreaming
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'text-muted-foreground'
            )}
            title={showModelSelector && !selectedModelId && !selectedAgentId ? "请先选择模型或智能体" : "Send message"}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
            </svg>
          </button>
        )}

        {/* 隐藏的文件输入 */}
        {allowAttachments && (
          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept={supportedFileTypes.join(',')}
            onChange={handleFileSelect}
            className="hidden"
          />
        )}
      </div>

      {/* 拖拽提示 */}
      {isDragging && allowAttachments && (
        <div className="absolute inset-0 bg-primary/10 border-2 border-dashed border-primary rounded-lg flex items-center justify-center">
          <div className="text-center">
            <svg className="w-8 h-8 mx-auto text-primary mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
            </svg>
            <p className="text-sm font-medium text-primary">Drop files here to attach</p>
            <p className="text-[11px] text-muted-foreground mt-1">单文件≤{formatFileSize(MAX_FILE_BYTES)}，总计≤{formatFileSize(MAX_TOTAL_BYTES)}，最多 {MAX_ATTACHMENTS} 个</p>
          </div>
        </div>
      )}
    </div>
  );
};

export default InputArea;
