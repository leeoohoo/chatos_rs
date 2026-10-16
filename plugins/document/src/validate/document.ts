import { mkdtemp, readFile, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { DocumentError } from '../errors.js';
import { inspectDocument } from '../inspect/document.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { boundedInteger, renderPageNumbers } from '../render/document.js';
import { openOfficeWithEngine, renderOfficePages, OFFICECLI_VERSION } from '../render/office.js';
import { openPdfWithPdfium, renderPdfPages, PDFIUM_VERSION } from '../render/pdfium.js';
import { inspectPng, isUniformPng } from '../render/png.js';

function validatePng(bytes: Buffer): { width: number; height: number; nonEmpty: boolean } {
  return { ...inspectPng(bytes), nonEmpty: true };
}

export async function validateDocument(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'inputPath is required.');
  const inspection = await inspectDocument(args.inputPath);
  const source = await resolveWorkspaceFile(args.inputPath);
  const format = inspection.format;
  if (format !== 'docx' && format !== 'xlsx' && format !== 'pptx' && format !== 'pdf') {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'The document format cannot be validated.');
  }

  const pages = args.renderPages === undefined ? [] : renderPageNumbers(args.renderPages, 10);
  const dpi = boundedInteger(args.dpi, 144, 72, 200, 'dpi');
  const viewportWidth = boundedInteger(args.viewportWidth, 1600, 320, 2000, 'viewportWidth');
  const viewportHeight = boundedInteger(args.viewportHeight, 1200, 240, 2000, 'viewportHeight');
  const checks: Array<Record<string, unknown>> = [
    { name: 'structure', status: 'passed', details: inspection.structure }
  ];
  const warnings: string[] = [];
  let exactPageCount: number | undefined;

  if (format === 'pdf') {
    if (pages.length > 0) {
      const rendered = await renderPdfPages(source.absolutePath, pages, dpi);
      exactPageCount = rendered.pageCount;
      checks.push({
        name: 'engine_open',
        status: 'passed',
        engine: { name: 'PDFium', version: PDFIUM_VERSION },
        pageCount: exactPageCount
      });
      checks.push({
        name: 'render',
        status: 'passed',
        pages: rendered.pages.map((page) => ({ page: page.page, width: page.width, height: page.height, nonEmpty: true }))
      });
    } else {
      const opened = await openPdfWithPdfium(source.absolutePath);
      exactPageCount = opened.pageCount;
      checks.push({
        name: 'engine_open',
        status: 'passed',
        engine: { name: 'PDFium', version: PDFIUM_VERSION },
        pageCount: exactPageCount
      });
    }
  } else {
    await openOfficeWithEngine(source.absolutePath, path.dirname(source.absolutePath));
    checks.push({
      name: 'engine_open',
      status: 'passed',
      engine: { name: 'OfficeCLI', version: OFFICECLI_VERSION }
    });
    if (pages.length > 0) {
      const temporaryDirectory = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-validation-'));
      try {
        const targets = pages.map((page) => ({
          page,
          outputPath: path.join(temporaryDirectory, `page-${String(page).padStart(4, '0')}.png`)
        }));
        await renderOfficePages(
          source.absolutePath,
          temporaryDirectory,
          targets,
          viewportWidth,
          viewportHeight
        );
        const results = [] as Array<Record<string, unknown>>;
        for (const target of targets) {
          const bytes = await readFile(target.outputPath);
          const unavailable = format === 'docx' && isUniformPng(bytes);
          results.push({
            page: target.page,
            ...validatePng(bytes),
            ...(unavailable ? { nonEmpty: false, status: 'needs_review', code: 'REQUESTED_PAGE_EMPTY_OR_UNAVAILABLE' } : {})
          });
          if (unavailable) warnings.push(`Page ${target.page}: REQUESTED_PAGE_EMPTY_OR_UNAVAILABLE. A uniform canvas cannot verify a document page; the requested page may be out of range.`);
        }
        checks.push({ name: 'render', status: results.some((page) => page.nonEmpty === false) ? 'needs_review' : 'passed', pages: results });
        warnings.push('Office validation uses the cross-platform HTML renderer; installed fonts can affect layout.');
      } finally {
        await rm(temporaryDirectory, { recursive: true, force: true });
      }
    }
  }

  if (pages.length > 0) {
    warnings.push('Rendered-page validation checks technical readability only. Use document_render and inspect its inline images for visual approval.');
  }

  return {
    ok: true,
    operation: 'document_validate',
    valid: checks.every((check) => check.status === 'passed'),
    validationScope: 'technical',
    visualReviewStatus: 'not_performed',
    format,
    source: inspection.source,
    ...(exactPageCount === undefined ? {} : { exactPageCount }),
    checks,
    warnings
  };
}
