// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React, { useEffect, useRef, useState } from 'react';

import pdfWorkerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';

interface BrowserPdfPreviewProps {
  dataUrl: string;
  width: number;
  height: number;
  cropOffsetY: number;
  label: string;
  loadingLabel: string;
  errorLabel: string;
}

const decodePdfDataUrl = (dataUrl: string): Uint8Array => {
  const separator = dataUrl.indexOf(',');
  if (separator < 0 || !dataUrl.slice(0, separator).includes(';base64')) {
    throw new Error('Invalid PDF data URL');
  }
  const binary = window.atob(dataUrl.slice(separator + 1));
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
};

const BrowserPdfPreview: React.FC<BrowserPdfPreviewProps> = ({
  dataUrl,
  width,
  height,
  cropOffsetY,
  label,
  loadingLabel,
  errorLabel,
}) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');

  useEffect(() => {
    let cancelled = false;
    let loadingTask: { destroy: () => Promise<void> } | null = null;

    const render = async () => {
      setStatus('loading');
      try {
        const pdfjs = await import('pdfjs-dist/legacy/build/pdf.mjs');
        pdfjs.GlobalWorkerOptions.workerSrc = pdfWorkerUrl;
        const task = pdfjs.getDocument({
          data: decodePdfDataUrl(dataUrl),
          useWorkerFetch: false,
        });
        loadingTask = task;
        const document = await task.promise;
        if (cancelled) {
          await task.destroy();
          loadingTask = null;
          return;
        }

        const output = canvasRef.current;
        const outputContext = output?.getContext('2d', { alpha: false });
        if (!output || !outputContext) {
          throw new Error('Canvas is unavailable');
        }
        output.width = width;
        output.height = height;
        outputContext.fillStyle = '#ffffff';
        outputContext.fillRect(0, 0, width, height);

        const desiredStart = Math.max(0, Math.min(cropOffsetY, height));
        const desiredEnd = desiredStart + height;
        for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber += 1) {
          if (cancelled) {
            break;
          }
          const page = await document.getPage(pageNumber);
          const unscaled = page.getViewport({ scale: 1 });
          const scale = width / unscaled.width;
          const viewport = page.getViewport({ scale });
          const pageCanvas = window.document.createElement('canvas');
          pageCanvas.width = Math.max(1, Math.round(viewport.width));
          pageCanvas.height = Math.max(1, Math.round(viewport.height));
          const pageContext = pageCanvas.getContext('2d', { alpha: false });
          if (!pageContext) {
            throw new Error('Page canvas is unavailable');
          }
          await page.render({ canvas: pageCanvas, canvasContext: pageContext, viewport }).promise;

          const pageTop = (pageNumber - 1) * height;
          const sourceStart = Math.max(0, desiredStart - pageTop);
          const sourceEnd = Math.min(pageCanvas.height, desiredEnd - pageTop);
          if (sourceEnd > sourceStart) {
            const sourceHeight = sourceEnd - sourceStart;
            outputContext.drawImage(
              pageCanvas,
              0,
              sourceStart,
              pageCanvas.width,
              sourceHeight,
              0,
              pageTop + sourceStart - desiredStart,
              width,
              sourceHeight,
            );
          }
        }
        await task.destroy();
        loadingTask = null;
        if (!cancelled) {
          setStatus('ready');
        }
      } catch {
        if (!cancelled) {
          setStatus('error');
        }
      }
    };

    void render();
    return () => {
      cancelled = true;
      if (loadingTask) {
        void loadingTask.destroy();
      }
    };
  }, [cropOffsetY, dataUrl, height, width]);

  return (
    <div className="relative flex max-h-full max-w-full items-center justify-center">
      <canvas
        ref={canvasRef}
        aria-label={label}
        className={`max-h-full max-w-full rounded border border-white/10 bg-white object-contain shadow-xl ${status === 'ready' ? 'opacity-100' : 'opacity-0'}`}
      />
      {status !== 'ready' ? (
        <div className="absolute inset-0 flex items-center justify-center text-sm text-neutral-400">
          {status === 'error' ? errorLabel : loadingLabel}
        </div>
      ) : null}
    </div>
  );
};

export default BrowserPdfPreview;
