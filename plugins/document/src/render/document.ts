import { copyFile, lstat, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  MAX_INPUT_BYTES,
  MAX_RENDER_PAGES,
  MAX_RENDER_TOTAL_PIXELS
} from '../constants.js';
import { DocumentError } from '../errors.js';
import { sha256File } from '../inspect/hash.js';
import { EXCLUSIVE_COPY_FLAG, resolveArtifactPaths, type ArtifactPaths } from '../security/artifacts.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { renderOfficePages, OFFICECLI_VERSION } from './office.js';
import { renderPdfPages, PDFIUM_VERSION } from './pdfium.js';
import { inspectPng } from './png.js';

const SUPPORTED_EXTENSIONS = new Set(['.docx', '.xlsx', '.pptx', '.pdf']);
const SAFE_PREFIX = /^[\p{L}\p{N}][\p{L}\p{N}._ ()-]{0,149}$/u;

export function renderPageNumbers(value: unknown, maximum = MAX_RENDER_PAGES): number[] {
  if (value === undefined) return [1];
  if (!Array.isArray(value) || value.length < 1 || value.length > maximum) {
    throw new DocumentError('INVALID_ARGUMENT', `pages must contain between 1 and ${maximum} page numbers.`);
  }
  const seen = new Set<number>();
  return value.map((page, index) => {
    if (!Number.isInteger(page) || (page as number) < 1 || (page as number) > 2_000) {
      throw new DocumentError('INVALID_ARGUMENT', `pages[${index}] must be an integer between 1 and 2000.`);
    }
    if (seen.has(page as number)) {
      throw new DocumentError('INVALID_ARGUMENT', 'pages must not contain duplicates.');
    }
    seen.add(page as number);
    return page as number;
  });
}

export function boundedInteger(
  value: unknown,
  defaultValue: number,
  minimum: number,
  maximum: number,
  field: string
): number {
  const result = value === undefined ? defaultValue : value;
  if (!Number.isInteger(result) || (result as number) < minimum || (result as number) > maximum) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be an integer between ${minimum} and ${maximum}.`);
  }
  return result as number;
}

function validatePrefix(value: unknown): string {
  if (typeof value !== 'string' || !SAFE_PREFIX.test(value) || path.basename(value) !== value) {
    throw new DocumentError('INVALID_PATH', 'outputPrefix must be a safe file-name prefix without directory components.');
  }
  return value;
}

async function publishAll(paths: ArtifactPaths[]): Promise<void> {
  const published: string[] = [];
  try {
    for (const item of paths) {
      await copyFile(item.temporaryPath, item.outputPath, EXCLUSIVE_COPY_FLAG).catch((error: NodeJS.ErrnoException) => {
        if (error.code === 'EEXIST') {
          throw new DocumentError('OUTPUT_EXISTS', 'A render artifact was created by another operation.');
        }
        throw error;
      });
      published.push(item.outputPath);
    }
  } catch (error) {
    await Promise.all(published.map((publishedPath) => rm(publishedPath, { force: true })));
    throw error;
  }
}

export async function renderDocument(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'inputPath is required.');
  const source = await resolveWorkspaceFile(args.inputPath);
  if (source.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const extension = path.extname(source.relativePath).toLowerCase();
  if (!SUPPORTED_EXTENSIONS.has(extension)) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'Supported extensions are .docx, .xlsx, .pptx, and .pdf.');
  }

  const outputPrefix = validatePrefix(args.outputPrefix);
  const pages = renderPageNumbers(args.pages);
  const dpi = boundedInteger(args.dpi, 144, 72, 300, 'dpi');
  const viewportWidth = boundedInteger(args.viewportWidth, 1600, 320, 2400, 'viewportWidth');
  const viewportHeight = boundedInteger(args.viewportHeight, 1200, 240, 2400, 'viewportHeight');
  if (extension !== '.pdf' && viewportWidth * viewportHeight * pages.length > MAX_RENDER_TOTAL_PIXELS) {
    throw new DocumentError('INVALID_ARGUMENT', 'The requested Office render exceeds the total pixel limit.');
  }

  const pagePaths = await Promise.all(pages.map((page) =>
    resolveArtifactPaths(`${outputPrefix}-page-${String(page).padStart(4, '0')}.png`, '.png')
  ));
  const manifestPaths = await resolveArtifactPaths(`${outputPrefix}-render-manifest.json`, '.json');
  const allPaths = [...pagePaths, manifestPaths];

  try {
    let exactPageCount: number | undefined;
    let engine: { name: string; version: string; mode?: string };
    if (extension === '.pdf') {
      let renderedIndex = 0;
      const result = await renderPdfPages(source.absolutePath, pages, dpi, async (rendered) => {
        const target = pagePaths[renderedIndex];
        if (!target) throw new DocumentError('INTERNAL_ERROR', 'A PDF render target was not allocated.');
        await writeFile(target.temporaryPath, rendered.png, { flag: 'wx', mode: 0o600 });
        renderedIndex += 1;
      });
      exactPageCount = result.pageCount;
      engine = { name: 'PDFium', version: PDFIUM_VERSION };
    } else {
      engine = { name: 'OfficeCLI', version: OFFICECLI_VERSION, mode: 'html' };
      const first = pagePaths[0];
      if (!first) throw new DocumentError('INTERNAL_ERROR', 'An Office render target was not allocated.');
      await renderOfficePages(
        source.absolutePath,
        first.root,
        pagePaths.map((item, index) => ({ page: pages[index] as number, outputPath: item.temporaryPath })),
        viewportWidth,
        viewportHeight
      );
    }

    let totalPixels = 0;
    const renderedPages = [] as Array<Record<string, unknown>>;
    for (const [index, item] of pagePaths.entries()) {
      const bytes = await readFile(item.temporaryPath);
      const dimensions = inspectPng(bytes);
      totalPixels += dimensions.width * dimensions.height;
      if (totalPixels > MAX_RENDER_TOTAL_PIXELS) {
        throw new DocumentError('VALIDATION_FAILED', 'Rendered pages exceeded the total pixel limit.');
      }
      renderedPages.push({
        page: pages[index],
        relativePath: item.outputName,
        mimeType: 'image/png',
        size: bytes.length,
        sha256: await sha256File(item.temporaryPath),
        ...dimensions
      });
    }

    const sourceSha256 = await sha256File(source.absolutePath);
    const warnings = extension === '.pdf'
      ? []
      : ['Office pages are rendered with the cross-platform HTML renderer; installed fonts can affect layout.'];
    const manifest = {
      schemaVersion: 1,
      operation: 'document_render',
      source: {
        relativePath: source.relativePath,
        size: source.size,
        sha256: sourceSha256,
        ...(exactPageCount === undefined ? {} : { pages: exactPageCount })
      },
      format: extension.slice(1),
      engine,
      settings: extension === '.pdf'
        ? { dpi }
        : { viewportWidth, viewportHeight, renderMode: 'html' },
      pages: renderedPages,
      warnings
    };
    await writeFile(manifestPaths.temporaryPath, `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
    const manifestSize = (await lstat(manifestPaths.temporaryPath)).size;
    const manifestSha256 = await sha256File(manifestPaths.temporaryPath);
    await publishAll(allPaths);
    return {
      ok: true,
      operation: 'document_render',
      format: extension.slice(1),
      source: manifest.source,
      engine,
      pages: renderedPages,
      manifest: {
        relativePath: manifestPaths.outputName,
        mimeType: 'application/json',
        size: manifestSize,
        sha256: manifestSha256
      },
      warnings
    };
  } finally {
    await Promise.all(allPaths.map((item) => rm(item.temporaryPath, { force: true })));
  }
}
