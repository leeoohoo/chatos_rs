// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { createPortal } from 'react-dom';

import type { MermaidExportNotice, MermaidPreviewStatus } from './mermaid';
import { useI18n } from '../../i18n/I18nProvider';

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
  const { t } = useI18n();

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
      <div className="mermaid-preview-dialog" role="dialog" aria-modal="true" aria-label={t('markdown.mermaidPreview.title')}>
        <div className="mermaid-preview-header">
          <span className="mermaid-preview-title">{t('markdown.mermaidPreview.title')}</span>
          <div className="code-actions">
            <button
              className="code-action-btn mermaid-image-copy-btn"
              onClick={onCopyImage}
              title={t('markdown.mermaidPreview.copyImageTitle')}
              disabled={status !== 'rendered'}
            >
              {t('markdown.mermaidPreview.copyImage')}
            </button>
            <button
              className="code-action-btn mermaid-image-download-btn"
              onClick={onDownloadImage}
              title={t('markdown.mermaidPreview.downloadImageTitle')}
              disabled={status !== 'rendered'}
            >
              {t('markdown.mermaidPreview.downloadImage')}
            </button>
            <button className="code-action-btn mermaid-close-btn" onClick={onClose} title={t('markdown.mermaidPreview.closeTitle')}>
              {t('common.close')}
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
            <div className="mermaid-preview-loading">● {t('markdown.mermaidPreview.loading')}</div>
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
