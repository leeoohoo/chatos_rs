import React, { useCallback, useMemo, useRef } from 'react';

import { MermaidPreviewModal } from './markdownRenderer/MermaidPreviewModal';
import { buildMarkdownHtml, hasMermaidFence } from './markdownRenderer/markdownHtml';
import { useMermaidPreviewController } from './markdownRenderer/useMermaidPreviewController';
import './MarkdownRenderer.css';

interface MarkdownRendererProps {
  content: string;
  isStreaming?: boolean;
  className?: string;
  onApplyCode?: (code: string, language: string) => void;
}

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({
  content,
  isStreaming = false,
  className = '',
  onApplyCode: _onApplyCode,
}) => {
  void _onApplyCode;

  const markdownContainerRef = useRef<HTMLDivElement | null>(null);

  const copyToClipboard = useCallback(async (code: string) => {
    try {
      await navigator.clipboard.writeText(code);
    } catch (err) {
      console.error('Failed to copy code:', err);
    }
  }, []);

  const renderedHtml = useMemo(
    () => buildMarkdownHtml(content, isStreaming),
    [content, isStreaming],
  );

  const hasMermaidBlock = useMemo(
    () => hasMermaidFence(content),
    [content],
  );

  const {
    isMermaidPreviewOpen,
    mermaidPreviewCode,
    mermaidPreviewStatus,
    mermaidPreviewError,
    mermaidExportNotice,
    mermaidPreviewContainerRef,
    openMermaidPreview,
    closeMermaidPreview,
    copyMermaidPreviewImage,
    downloadMermaidPreviewImage,
  } = useMermaidPreviewController({
    markdownContainerRef,
    hasMermaidBlock,
    isStreaming,
    renderDependency: renderedHtml,
  });

  const handleClick = useCallback((event: React.MouseEvent) => {
    const target = event.target as HTMLElement;
    const button = target.closest('button');

    if (!button) {
      return;
    }

    if (button.classList.contains('copy-btn')) {
      const code = decodeURIComponent(button.getAttribute('data-code') || '');
      void copyToClipboard(code);
      return;
    }

    if (button.classList.contains('mermaid-open-btn')) {
      const code = decodeURIComponent(button.getAttribute('data-code') || '');
      openMermaidPreview(code);
      return;
    }

    if (button.classList.contains('expand-btn')) {
      const codeBlock = button.closest('.code-block');
      if (!codeBlock) {
        return;
      }
      const expanded = codeBlock.classList.toggle('expanded');
      button.setAttribute('title', expanded ? '收起' : '展开');
      const icon = button.querySelector('.icon');
      if (icon) {
        icon.classList.toggle('rotated', expanded);
      }
    }
  }, [copyToClipboard, openMermaidPreview]);

  return (
    <div
      ref={markdownContainerRef}
      className={`markdown-renderer ${className}`}
      onClick={handleClick}
    >
      <div
        dangerouslySetInnerHTML={{
          __html: renderedHtml,
        }}
      />
      <MermaidPreviewModal
        isOpen={isMermaidPreviewOpen}
        status={mermaidPreviewStatus}
        error={mermaidPreviewError}
        previewCode={mermaidPreviewCode}
        exportNotice={mermaidExportNotice}
        previewContainerRef={mermaidPreviewContainerRef}
        onClose={closeMermaidPreview}
        onCopyImage={() => {
          void copyMermaidPreviewImage();
        }}
        onDownloadImage={() => {
          void downloadMermaidPreviewImage();
        }}
      />
      {isStreaming && (
        <span className="streaming-cursor" />
      )}
    </div>
  );
};

export default MarkdownRenderer;
