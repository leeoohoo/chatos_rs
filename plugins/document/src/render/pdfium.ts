import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PDFiumLibrary } from '@hyzyla/pdfium';
import {
  MAX_PDF_PAGES,
  MAX_RENDER_PAGE_PIXELS,
  MAX_RENDER_TOTAL_PIXELS
} from '../constants.js';
import { DocumentError } from '../errors.js';
import { encodeRgbaPng } from './png.js';

export const PDFIUM_VERSION = '2.1.13';

const packageRoot = process.env.CHATOS_PLUGIN_ROOT?.trim()
  ? path.resolve(process.env.CHATOS_PLUGIN_ROOT)
  : path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export interface RenderedPdfPage {
  page: number;
  width: number;
  height: number;
  png: Buffer;
}

export type RenderedPdfPageInfo = Omit<RenderedPdfPage, 'png'>;

function exactArrayBuffer(bytes: Buffer): ArrayBuffer {
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}

async function initializePdfium() {
  const wasm = await readFile(path.join(packageRoot, 'dist', 'pdfium.wasm')).catch(() => undefined);
  if (!wasm) throw new DocumentError('ENGINE_UNAVAILABLE', 'The bundled PDFium WebAssembly binary is missing.');
  try {
    return await PDFiumLibrary.init({ wasmBinary: exactArrayBuffer(wasm) });
  } catch {
    throw new DocumentError('ENGINE_UNAVAILABLE', 'PDFium could not be initialized.');
  }
}

function mapPdfiumError(error: unknown): DocumentError {
  const message = error instanceof Error ? error.message : '';
  if (/password|security/i.test(message)) {
    return new DocumentError('ENCRYPTED_FILE', 'Encrypted PDF files are not supported.');
  }
  return new DocumentError('INVALID_DOCUMENT', 'The PDF could not be opened by PDFium.');
}

export async function openPdfWithPdfium(absolutePath: string): Promise<{ pageCount: number }> {
  const bytes = await readFile(absolutePath);
  const library = await initializePdfium();
  let document;
  try {
    document = await library.loadDocument(bytes);
    const pageCount = document.getPageCount();
    if (pageCount < 1 || pageCount > MAX_PDF_PAGES) {
      throw new DocumentError('INVALID_DOCUMENT', `The PDF must contain between 1 and ${MAX_PDF_PAGES} pages.`);
    }
    return { pageCount };
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw mapPdfiumError(error);
  } finally {
    document?.destroy();
    library.destroy();
  }
}

export async function renderPdfPages(
  absolutePath: string,
  pages: number[],
  dpi: number,
  onPage?: (page: RenderedPdfPage) => Promise<void>
): Promise<{ pageCount: number; pages: RenderedPdfPageInfo[] }> {
  const bytes = await readFile(absolutePath);
  const library = await initializePdfium();
  let document;
  try {
    document = await library.loadDocument(bytes);
    const pageCount = document.getPageCount();
    if (pageCount < 1 || pageCount > MAX_PDF_PAGES) {
      throw new DocumentError('INVALID_DOCUMENT', `The PDF must contain between 1 and ${MAX_PDF_PAGES} pages.`);
    }
    for (const page of pages) {
      if (page > pageCount) {
        throw new DocumentError('INVALID_ARGUMENT', `Requested page ${page} is outside the PDF page range.`);
      }
    }

    const scale = dpi / 72;
    let estimatedPixels = 0;
    for (const pageNumber of pages) {
      const page = document.getPage(pageNumber - 1);
      const size = page.getOriginalSize();
      const pixels = Math.ceil(size.originalWidth * scale) * Math.ceil(size.originalHeight * scale);
      if (pixels > MAX_RENDER_PAGE_PIXELS) {
        throw new DocumentError('INVALID_ARGUMENT', `Requested page ${pageNumber} exceeds the per-page render pixel limit.`);
      }
      estimatedPixels += pixels;
    }
    if (estimatedPixels > MAX_RENDER_TOTAL_PIXELS) {
      throw new DocumentError('INVALID_ARGUMENT', 'The requested PDF render exceeds the total pixel limit.');
    }

    const rendered: RenderedPdfPageInfo[] = [];
    let actualPixels = 0;
    for (const pageNumber of pages) {
      const page = document.getPage(pageNumber - 1);
      const image = await page.render({
        scale,
        renderFormFields: true,
        render: async ({ data, width, height }) => {
          if (width * height > MAX_RENDER_PAGE_PIXELS) {
            throw new DocumentError('INVALID_ARGUMENT', `Requested page ${pageNumber} exceeds the per-page render pixel limit.`);
          }
          return encodeRgbaPng(data, width, height);
        }
      });
      actualPixels += image.width * image.height;
      if (actualPixels > MAX_RENDER_TOTAL_PIXELS) {
        throw new DocumentError('INVALID_ARGUMENT', 'The rendered PDF exceeded the total pixel limit.');
      }
      const info = {
        page: pageNumber,
        width: image.width,
        height: image.height
      };
      if (onPage) await onPage({ ...info, png: Buffer.from(image.data) });
      rendered.push(info);
    }
    return { pageCount, pages: rendered };
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw mapPdfiumError(error);
  } finally {
    document?.destroy();
    library.destroy();
  }
}
