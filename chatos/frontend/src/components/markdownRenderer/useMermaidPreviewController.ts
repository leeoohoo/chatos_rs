// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useRef, useState } from 'react';

import {
  createMermaidSvgSnapshot,
  normalizeMermaidForRetry,
  type MermaidApi,
  type MermaidExportNotice,
  type MermaidPreviewStatus,
} from './mermaid';
import type { TranslateFn } from '../../i18n/I18nProvider';

interface UseMermaidPreviewControllerOptions {
  markdownContainerRef: React.RefObject<HTMLDivElement | null>;
  hasMermaidBlock: boolean;
  isStreaming: boolean;
  renderDependency: string;
  t: TranslateFn;
}

export const useMermaidPreviewController = ({
  markdownContainerRef,
  hasMermaidBlock,
  isStreaming,
  renderDependency,
  t,
}: UseMermaidPreviewControllerOptions) => {
  const mermaidRenderSeqRef = useRef(0);
  const mermaidApiRef = useRef<MermaidApi | null>(null);
  const mermaidThemeRef = useRef<'default' | 'dark' | ''>('');
  const mermaidPreviewContainerRef = useRef<HTMLDivElement | null>(null);
  const mermaidExportNoticeTimerRef = useRef<number | null>(null);
  const [isMermaidPreviewOpen, setIsMermaidPreviewOpen] = useState(false);
  const [mermaidPreviewCode, setMermaidPreviewCode] = useState('');
  const [mermaidPreviewStatus, setMermaidPreviewStatus] = useState<MermaidPreviewStatus>('idle');
  const [mermaidPreviewError, setMermaidPreviewError] = useState('');
  const [mermaidExportNotice, setMermaidExportNotice] = useState<MermaidExportNotice | null>(null);

  const showMermaidExportNotice = useCallback((type: MermaidExportNotice['type'], text: string) => {
    setMermaidExportNotice({ type, text });
    if (mermaidExportNoticeTimerRef.current !== null) {
      window.clearTimeout(mermaidExportNoticeTimerRef.current);
    }
    mermaidExportNoticeTimerRef.current = window.setTimeout(() => {
      setMermaidExportNotice(null);
      mermaidExportNoticeTimerRef.current = null;
    }, 2200);
  }, []);

  useEffect(() => () => {
    if (mermaidExportNoticeTimerRef.current !== null) {
      window.clearTimeout(mermaidExportNoticeTimerRef.current);
      mermaidExportNoticeTimerRef.current = null;
    }
  }, []);

  const getMermaidPreviewSvgSnapshot = useCallback(() => {
    const previewContainer = mermaidPreviewContainerRef.current;
    const svgElement = previewContainer?.querySelector('svg');
    if (!svgElement) {
      throw new Error('Mermaid svg not found');
    }
    return createMermaidSvgSnapshot(svgElement as SVGSVGElement);
  }, []);

  const buildMermaidPreviewSvgBlob = useCallback((): Blob => {
    const { svgText } = getMermaidPreviewSvgSnapshot();
    return new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
  }, [getMermaidPreviewSvgSnapshot]);

  const buildMermaidPreviewPngBlob = useCallback(async (): Promise<Blob> => {
    const { svgText, width, height } = getMermaidPreviewSvgSnapshot();
    const svgBlob = new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
    const blobUrl = URL.createObjectURL(svgBlob);
    const dataUrl = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svgText)}`;

    const loadImage = (src: string) => new Promise<HTMLImageElement>((resolve, reject) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => reject(new Error(`Failed to load svg image from ${src.startsWith('blob:') ? 'blob' : 'data'} url`));
      img.src = src;
    });

    try {
      const image = await (async () => {
        try {
          return await loadImage(blobUrl);
        } catch (blobLoadError) {
          console.warn('Failed to load Mermaid svg via blob url, trying data url fallback:', blobLoadError);
          return loadImage(dataUrl);
        }
      })();

      const scale = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.round(width * scale));
      canvas.height = Math.max(1, Math.round(height * scale));

      const context = canvas.getContext('2d');
      if (!context) {
        throw new Error('Canvas 2d context is unavailable');
      }
      context.scale(scale, scale);
      context.fillStyle = '#ffffff';
      context.fillRect(0, 0, width, height);
      context.drawImage(image, 0, 0, width, height);

      return new Promise<Blob>((resolve, reject) => {
        if (typeof canvas.toBlob === 'function') {
          canvas.toBlob(async (blob) => {
            if (blob) {
              resolve(blob);
              return;
            }
            try {
              const fallbackDataUrl = canvas.toDataURL('image/png');
              const fallbackBlob = await fetch(fallbackDataUrl).then((response) => response.blob());
              resolve(fallbackBlob);
            } catch (fallbackError) {
              reject(new Error(`Failed to encode png blob: ${(fallbackError as Error).message}`));
            }
          }, 'image/png');
          return;
        }
        try {
          const fallbackDataUrl = canvas.toDataURL('image/png');
          fetch(fallbackDataUrl)
            .then((response) => response.blob())
            .then(resolve)
            .catch((error) => reject(new Error(`Failed to convert canvas data url to blob: ${(error as Error).message}`)));
        } catch (error) {
          reject(new Error(`Failed to encode png blob: ${(error as Error).message}`));
        }
      });
    } finally {
      URL.revokeObjectURL(blobUrl);
    }
  }, [getMermaidPreviewSvgSnapshot]);

  const downloadBlobToLocal = useCallback((blob: Blob, filename: string) => {
    const downloadUrl = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = downloadUrl;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(downloadUrl);
  }, []);

  const closeMermaidPreview = useCallback(() => {
    setIsMermaidPreviewOpen(false);
    setMermaidPreviewStatus('idle');
    setMermaidPreviewError('');
    setMermaidExportNotice(null);
    if (mermaidExportNoticeTimerRef.current !== null) {
      window.clearTimeout(mermaidExportNoticeTimerRef.current);
      mermaidExportNoticeTimerRef.current = null;
    }
  }, []);

  const openMermaidPreview = useCallback((code: string) => {
    setMermaidPreviewCode(code);
    setMermaidPreviewStatus('idle');
    setMermaidPreviewError('');
    setMermaidExportNotice(null);
    setIsMermaidPreviewOpen(true);
  }, []);

  const copyMermaidPreviewImage = useCallback(async () => {
    if (mermaidPreviewStatus !== 'rendered') {
      showMermaidExportNotice('error', t('markdown.mermaid.copy.notReady'));
      return;
    }

    try {
      const clipboardWriter = navigator.clipboard?.write?.bind(navigator.clipboard);
      if (clipboardWriter && typeof ClipboardItem !== 'undefined') {
        try {
          const pngBlob = await buildMermaidPreviewPngBlob();
          await clipboardWriter([new ClipboardItem({ 'image/png': pngBlob })]);
          showMermaidExportNotice('success', t('markdown.mermaid.copyPngSuccess'));
          return;
        } catch (pngCopyError) {
          console.warn('Failed to copy Mermaid PNG to clipboard, trying SVG fallback:', pngCopyError);
        }

        try {
          const svgBlob = buildMermaidPreviewSvgBlob();
          await clipboardWriter([new ClipboardItem({ 'image/svg+xml': svgBlob })]);
          showMermaidExportNotice('success', t('markdown.mermaid.copySvgSuccess'));
          return;
        } catch (svgCopyError) {
          console.warn('Failed to copy Mermaid SVG to clipboard:', svgCopyError);
        }
      }

      const textWriter = navigator.clipboard?.writeText?.bind(navigator.clipboard);
      if (!textWriter) {
        throw new Error('Clipboard text write api unavailable');
      }
      const { svgText } = getMermaidPreviewSvgSnapshot();
      await textWriter(svgText);
      showMermaidExportNotice('success', t('markdown.mermaid.copySvgSourceSuccess'));
    } catch (error) {
      console.error('Failed to copy Mermaid preview image:', error);
      showMermaidExportNotice('error', t('markdown.mermaid.copyFailed'));
    }
  }, [
    buildMermaidPreviewPngBlob,
    buildMermaidPreviewSvgBlob,
    getMermaidPreviewSvgSnapshot,
    mermaidPreviewStatus,
    showMermaidExportNotice,
    t,
  ]);

  const downloadMermaidPreviewImage = useCallback(async () => {
    if (mermaidPreviewStatus !== 'rendered') {
      showMermaidExportNotice('error', t('markdown.mermaid.download.notReady'));
      return;
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    try {
      try {
        const imageBlob = await buildMermaidPreviewPngBlob();
        downloadBlobToLocal(imageBlob, `mermaid-preview-${timestamp}.png`);
        showMermaidExportNotice('success', t('markdown.mermaid.downloadPngSuccess'));
        return;
      } catch (pngDownloadError) {
        console.warn('Failed to download Mermaid PNG, trying SVG fallback:', pngDownloadError);
      }

      const svgBlob = buildMermaidPreviewSvgBlob();
      downloadBlobToLocal(svgBlob, `mermaid-preview-${timestamp}.svg`);
      showMermaidExportNotice('success', t('markdown.mermaid.downloadSvgFallback'));
    } catch (finalError) {
      console.error('Failed to download Mermaid preview image:', finalError);
      showMermaidExportNotice('error', t('markdown.mermaid.downloadFailed'));
    }
  }, [
    buildMermaidPreviewPngBlob,
    buildMermaidPreviewSvgBlob,
    downloadBlobToLocal,
    mermaidPreviewStatus,
    showMermaidExportNotice,
    t,
  ]);

  useEffect(() => {
    if (!isMermaidPreviewOpen) {
      return;
    }
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeMermaidPreview();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [closeMermaidPreview, isMermaidPreviewOpen]);

  useEffect(() => {
    if (!isMermaidPreviewOpen) {
      return;
    }
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [isMermaidPreviewOpen]);

  useEffect(() => {
    if (!hasMermaidBlock || isStreaming || !isMermaidPreviewOpen) {
      return;
    }
    if (!mermaidPreviewCode.trim()) {
      setMermaidPreviewStatus('error');
      setMermaidPreviewError(t('markdown.mermaid.empty'));
      return;
    }

    let cancelled = false;
    const root = markdownContainerRef.current;
    const previewContainer = mermaidPreviewContainerRef.current;
    if (!root || !previewContainer) {
      return;
    }
    previewContainer.innerHTML = '';

    const renderMermaid = async () => {
      setMermaidPreviewStatus('loading');
      setMermaidPreviewError('');
      try {
        if (!mermaidApiRef.current) {
          const mermaidModule = await import('mermaid');
          if (cancelled) {
            return;
          }
          mermaidApiRef.current = mermaidModule.default;
        }
        const mermaid = mermaidApiRef.current;
        const isDarkTheme = (
          Boolean(root.closest('.dark'))
          || document.documentElement.classList.contains('dark')
          || document.body.classList.contains('dark')
        );
        const theme: 'default' | 'dark' = isDarkTheme ? 'dark' : 'default';

        if (mermaidThemeRef.current !== theme) {
          mermaid.initialize({
            startOnLoad: false,
            securityLevel: 'strict',
            theme,
            suppressErrorRendering: true,
          });
          mermaidThemeRef.current = theme;
        }

        const renderDiagram = async (candidateCode: string) => {
          const parseResult = await mermaid.parse(candidateCode, { suppressErrors: true });
          if (parseResult === false) {
            throw new Error('Mermaid parse returned false');
          }

          mermaidRenderSeqRef.current += 1;
          const renderId = `mermaid-preview-${mermaidRenderSeqRef.current}`;
          const rendered = await mermaid.render(renderId, candidateCode);
          if (typeof rendered.svg !== 'string' || !rendered.svg.includes('<svg')) {
            throw new Error('Mermaid render returned invalid svg content');
          }

          const probe = document.createElement('div');
          probe.innerHTML = rendered.svg;
          const svgElement = probe.querySelector('svg');
          if (!svgElement) {
            throw new Error('Rendered SVG element not found');
          }
          const viewBox = svgElement.getAttribute('viewBox');
          if (viewBox) {
            const numbers = viewBox
              .split(/\s+/)
              .map((item) => Number(item))
              .filter((item) => Number.isFinite(item));
            if (numbers.length === 4) {
              const width = numbers[2];
              const height = numbers[3];
              if (!(width > 0) || !(height > 0)) {
                throw new Error(`Rendered SVG has invalid viewBox: ${viewBox}`);
              }
            }
          }

          return rendered;
        };

        let rendered;
        try {
          rendered = await renderDiagram(mermaidPreviewCode);
        } catch (error) {
          const normalization = normalizeMermaidForRetry(mermaidPreviewCode);
          if (!normalization.changed) {
            throw error;
          }
          rendered = await renderDiagram(normalization.code);
          console.warn('Mermaid preview recovered by normalization', {
            notes: normalization.notes,
          });
        }

        if (cancelled) {
          return;
        }
        previewContainer.innerHTML = rendered.svg;
        if (typeof rendered.bindFunctions === 'function') {
          rendered.bindFunctions(previewContainer);
        }
        setMermaidPreviewStatus('rendered');
      } catch (error) {
        console.error('Mermaid preview failed:', {
          error,
          diagramCode: mermaidPreviewCode,
        });
        if (cancelled) {
          return;
        }
        previewContainer.innerHTML = '';
        setMermaidPreviewStatus('error');
        setMermaidPreviewError(t('markdown.mermaid.renderFailed'));
      }
    };

    void renderMermaid();

    return () => {
      cancelled = true;
    };
  }, [
    closeMermaidPreview,
    hasMermaidBlock,
    isMermaidPreviewOpen,
    isStreaming,
    markdownContainerRef,
    mermaidPreviewCode,
    renderDependency,
    t,
  ]);

  return {
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
  };
};
