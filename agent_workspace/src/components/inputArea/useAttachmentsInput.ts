import { useCallback, useEffect, useRef, useState } from 'react';

import {
  formatFileSize,
  isLikelyCodeFileName,
  MAX_ATTACHMENTS,
  MAX_FILE_BYTES,
  MAX_TOTAL_BYTES,
} from './fileUtils';

interface UseAttachmentsInputOptions {
  allowAttachments: boolean;
  disabled: boolean;
  supportedFileTypes: string[];
  fileInputRef: React.RefObject<HTMLInputElement>;
}

export const useAttachmentsInput = ({
  allowAttachments,
  disabled,
  supportedFileTypes,
  fileInputRef,
}: UseAttachmentsInputOptions) => {
  const [attachments, setAttachments] = useState<File[]>([]);
  const [attachError, setAttachError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const globalDragCounter = useRef(0);

  const isFileTypeAllowed = useCallback((file: File) => {
    if (!supportedFileTypes || supportedFileTypes.length === 0) return true;
    const type = String(file.type || '').toLowerCase();
    const name = String(file.name || '').toLowerCase();
    const matched = supportedFileTypes.some((pattern) => {
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

    if (matched) return true;
    return isLikelyCodeFileName(name);
  }, [supportedFileTypes]);

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

  useEffect(() => {
    if (!allowAttachments || disabled) return;

    const hasFiles = (e: DragEvent | ClipboardEvent) => {
      const dt = (e as DragEvent).dataTransfer;
      if (!dt) return false;
      try {
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

  const handlePaste = useCallback((e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    try {
      if (!allowAttachments || disabled) return;
      const dt = e.clipboardData;
      const collected: File[] = [];
      if (dt && dt.items && dt.items.length > 0) {
        for (let i = 0; i < dt.items.length; i++) {
          const it = dt.items[i];
          if (it.kind === 'file') {
            const f = it.getAsFile();
            if (f && f.size > 0) collected.push(f);
          }
        }
      }
      if (collected.length === 0 && dt && dt.files && dt.files.length > 0) {
        for (let i = 0; i < dt.files.length; i++) {
          const f = dt.files[i];
          if (f && f.size > 0) collected.push(f);
        }
      }

      if (collected.length > 0) {
        addFiles(collected);
      }
    } catch (_) {}
  }, [addFiles, allowAttachments, disabled]);

  const handleFileSelect = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    addFiles(files);
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  }, [addFiles, fileInputRef]);

  const removeAttachment = useCallback((index: number) => {
    setAttachments(prev => prev.filter((_, i) => i !== index));
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    if (allowAttachments && !disabled) {
      setIsDragging(true);
    }
  }, [allowAttachments, disabled]);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);

    if (!allowAttachments || disabled) return;

    const files = Array.from(e.dataTransfer.files);
    addFiles(files);
  }, [addFiles, allowAttachments, disabled]);

  const clearAttachments = useCallback(() => {
    setAttachments([]);
    setAttachError(null);
  }, []);

  return {
    attachments,
    attachError,
    isDragging,
    addFiles,
    handlePaste,
    handleFileSelect,
    removeAttachment,
    handleDragOver,
    handleDragLeave,
    handleDrop,
    clearAttachments,
  };
};
