import React from 'react';
import { createPortal } from 'react-dom';

import type { MermaidExportNotice, MermaidPreviewStatus } from './mermaid';

interface MermaidPreviewModalProps {
  isOpen: boolean;
  status: MermaidPreviewStatus;
  error: string;
  previewCode: string;
  exportNotice: MermaidExportNotice | null;
  previewContainerRef: React.Ref<HTMLDivElement>;
  onClose: () => void;
  onCopyImage: () => void;
  onDownloadImage: () => void;
}

export const MermaidPreviewModal: React.FC<MermaidPreviewModalProps> = ({
  isOpen,
  status,
  error,
  previewCode,
  exportNotice,
  previewContainerRef,
  onClose,
  onCopyImage,
  onDownloadImage,
}) => {
  if (!isOpen || typeof document === 'undefined') {
    return null;
  }

  return createPortal(
    <div
      className="mermaid-preview-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="mermaid-preview-dialog" role="dialog" aria-modal="true" aria-label="Mermaid 图表预览">
        <div className="mermaid-preview-header">
          <span className="mermaid-preview-title">Mermaid 图表预览</span>
          <div className="code-actions">
            <button
              className="code-action-btn mermaid-image-copy-btn"
              onClick={onCopyImage}
              title="复制图表为图片到剪贴板"
              disabled={status !== 'rendered'}
            >
              复制图片
            </button>
            <button
              className="code-action-btn mermaid-image-download-btn"
              onClick={onDownloadImage}
              title="下载图表为图片"
              disabled={status !== 'rendered'}
            >
              下载图片
            </button>
            <button className="code-action-btn mermaid-close-btn" onClick={onClose} title="关闭图表弹窗">
              关闭
            </button>
          </div>
        </div>
        <div className="mermaid-preview-body">
          {exportNotice && (
            <div className={`mermaid-preview-notice ${exportNotice.type}`}>
              {exportNotice.text}
            </div>
          )}
          {status === 'loading' && (
            <div className="mermaid-preview-loading">● 正在渲染图表...</div>
          )}
          <div
            ref={previewContainerRef}
            className={`mermaid-preview-diagram ${status === 'error' ? 'hidden' : ''}`}
          />
          {status === 'error' && (
            <>
              <div className="mermaid-preview-error">{error}</div>
              <pre className="mermaid-preview-fallback"><code>{previewCode}</code></pre>
            </>
          )}
        </div>
      </div>
    </div>,
    document.body,
  );
};
