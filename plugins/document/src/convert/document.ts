import { randomUUID } from 'node:crypto';
import { copyFile, lstat, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { PDFDocument } from 'pdf-lib';
import { MAX_INPUT_BYTES, MAX_RENDER_PAGES, MAX_RENDER_TOTAL_PIXELS } from '../constants.js';
import { DocumentError } from '../errors.js';
import { sha256File } from '../inspect/hash.js';
import { inspectOoxml } from '../inspect/ooxml.js';
import { renderOfficeDocumentStack, renderOfficePages, renderOfficeRange, OFFICECLI_VERSION } from '../render/office.js';
import { encodeRgbaPng, inspectPng, splitOfficeDocumentPageStack } from '../render/png.js';
import { EXCLUSIVE_COPY_FLAG, resolveArtifactPaths } from '../security/artifacts.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { spreadsheetRenderRanges, type SpreadsheetRenderRange } from '../spreadsheet/range.js';

const SUPPORTED_EXTENSIONS = new Set(['.docx', '.xlsx', '.pptx']);
const A4_PORTRAIT = [595.28, 841.89] as const;
const A4_LANDSCAPE = [841.89, 595.28] as const;
const PAGE_MARGIN = 24;

interface RenderedImage {
  bytes: Buffer;
  width: number;
  height: number;
  mapping: Record<string, unknown>;
  maximumScale?: number;
}

function boundedInteger(value: unknown, defaultValue: number, minimum: number, maximum: number, field: string): number {
  const result = value === undefined ? defaultValue : value;
  if (!Number.isInteger(result) || (result as number) < minimum || (result as number) > maximum) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be an integer between ${minimum} and ${maximum}.`);
  }
  return result as number;
}

function selectedPageNumbers(value: unknown, maximumPage?: number): number[] | undefined {
  if (value === undefined) return undefined;
  if (!Array.isArray(value) || value.length < 1 || value.length > MAX_RENDER_PAGES) {
    throw new DocumentError('INVALID_ARGUMENT', `pages must contain between 1 and ${MAX_RENDER_PAGES} page numbers.`);
  }
  const seen = new Set<number>();
  return value.map((page, index) => {
    if (!Number.isInteger(page) || (page as number) < 1 || (maximumPage !== undefined && (page as number) > maximumPage)) {
      throw new DocumentError('INVALID_ARGUMENT', `pages[${index}] is outside the document page range.`);
    }
    if (seen.has(page as number)) throw new DocumentError('INVALID_ARGUMENT', 'pages must not contain duplicates.');
    seen.add(page as number);
    return page as number;
  });
}

function selectedSheets(value: unknown, available: SpreadsheetRenderRange[]): SpreadsheetRenderRange[] {
  if (value === undefined) {
    if (available.length > MAX_RENDER_PAGES) {
      throw new DocumentError('INVALID_ARGUMENT', `The workbook has more than ${MAX_RENDER_PAGES} worksheets; select sheets explicitly.`);
    }
    return available;
  }
  if (!Array.isArray(value) || value.length < 1 || value.length > MAX_RENDER_PAGES) {
    throw new DocumentError('INVALID_ARGUMENT', `sheets must contain between 1 and ${MAX_RENDER_PAGES} worksheet names.`);
  }
  const byName = new Map(available.map((sheet) => [sheet.name, sheet]));
  const seen = new Set<string>();
  return value.map((name, index) => {
    if (typeof name !== 'string' || name.length < 1 || name.length > 31) {
      throw new DocumentError('INVALID_ARGUMENT', `sheets[${index}] must be a valid worksheet name.`);
    }
    if (seen.has(name)) throw new DocumentError('INVALID_ARGUMENT', 'sheets must not contain duplicates.');
    seen.add(name);
    const sheet = byName.get(name);
    if (!sheet) throw new DocumentError('INVALID_ARGUMENT', `Worksheet not found: ${name}`);
    return sheet;
  });
}

function officeRange(sheet: string, range: string): string {
  const name = /^[A-Za-z_][A-Za-z0-9_.]*$/.test(sheet) ? sheet : `'${sheet.replaceAll("'", "''")}'`;
  return `${name}!${range}`;
}

function tempPng(root: string, suffix: string): string {
  return path.join(root, `.document-mcp-${randomUUID()}-${suffix}.png`);
}

function checkedImage(bytes: Buffer, mapping: Record<string, unknown>, maximumScale?: number): RenderedImage {
  const dimensions = inspectPng(bytes);
  return { bytes, ...dimensions, mapping, ...(maximumScale === undefined ? {} : { maximumScale }) };
}

async function renderDocx(
  absolutePath: string,
  root: string,
  pages: number[] | undefined,
  viewportWidth: number,
  viewportHeight: number,
  temporaryFiles: string[],
  warnings: string[]
): Promise<RenderedImage[]> {
  const maximumRequestedPage = pages ? Math.max(...pages) : MAX_RENDER_PAGES;
  if (maximumRequestedPage > MAX_RENDER_PAGES) {
    throw new DocumentError('INVALID_ARGUMENT', `DOCX conversion currently supports only the first ${MAX_RENDER_PAGES} pages.`);
  }
  const stackPath = tempPng(root, 'docx-stack');
  temporaryFiles.push(stackPath);
  await renderOfficeDocumentStack(
    absolutePath,
    root,
    stackPath,
    maximumRequestedPage,
    viewportWidth,
    viewportHeight
  );
  const stack = await readFile(stackPath);
  const stackDimensions = inspectPng(stack);
  if (stackDimensions.width * stackDimensions.height > MAX_RENDER_TOTAL_PIXELS) {
    throw new DocumentError('VALIDATION_FAILED', 'The rendered DOCX exceeds the total pixel limit.');
  }
  const split = splitOfficeDocumentPageStack(stack);
  if (split.length === 0) throw new DocumentError('VALIDATION_FAILED', 'No DOCX pages could be isolated from the renderer output.');
  const selected = pages ?? Array.from({ length: split.length }, (_, index) => index + 1);
  if (selected.some((page) => page > split.length)) {
    throw new DocumentError('INVALID_ARGUMENT', 'A requested DOCX page is outside the rendered document page range.');
  }
  if (!pages && split.length === MAX_RENDER_PAGES) {
    warnings.push(`Only the first ${MAX_RENDER_PAGES} DOCX pages were converted; the source may contain additional pages.`);
  }
  return selected.map((page) => checkedImage(split[page - 1] as Buffer, { page }));
}

async function renderPptx(
  absolutePath: string,
  root: string,
  slideCount: number,
  pages: number[] | undefined,
  viewportWidth: number,
  viewportHeight: number,
  temporaryFiles: string[]
): Promise<RenderedImage[]> {
  if (slideCount < 1) throw new DocumentError('INVALID_DOCUMENT', 'The PPTX presentation contains no slides.');
  if (!pages && slideCount > MAX_RENDER_PAGES) {
    throw new DocumentError('INVALID_ARGUMENT', `The presentation has more than ${MAX_RENDER_PAGES} slides; select pages explicitly.`);
  }
  const selected = pages ?? Array.from({ length: slideCount }, (_, index) => index + 1);
  const targets = selected.map((page) => {
    const outputPath = tempPng(root, `slide-${page}`);
    temporaryFiles.push(outputPath);
    return { page, outputPath };
  });
  await renderOfficePages(absolutePath, root, targets, viewportWidth, viewportHeight);
  const images: RenderedImage[] = [];
  for (const target of targets) images.push(checkedImage(await readFile(target.outputPath), { slide: target.page }));
  return images;
}

async function renderXlsx(
  absolutePath: string,
  root: string,
  sheets: SpreadsheetRenderRange[],
  viewportWidth: number,
  viewportHeight: number,
  temporaryFiles: string[]
): Promise<RenderedImage[]> {
  const images: RenderedImage[] = [];
  for (const sheet of sheets) {
    if (sheet.empty) {
      const width = Math.min(viewportWidth, 800);
      const height = Math.min(viewportHeight, 600);
      const pixels = Buffer.alloc(width * height * 4, 255);
      images.push(checkedImage(
        encodeRgbaPng(pixels, width, height),
        { sheet: sheet.name, range: sheet.range, hidden: sheet.hidden, empty: true },
        2
      ));
      continue;
    }
    const outputPath = tempPng(root, `sheet-${images.length + 1}`);
    temporaryFiles.push(outputPath);
    await renderOfficeRange(
      absolutePath,
      root,
      outputPath,
      officeRange(sheet.name, sheet.range),
      viewportWidth,
      viewportHeight
    );
    images.push(checkedImage(
      await readFile(outputPath),
      { sheet: sheet.name, range: sheet.range, hidden: sheet.hidden, empty: false },
      2
    ));
  }
  return images;
}

async function assemblePdf(images: RenderedImage[]): Promise<PDFDocument> {
  const document = await PDFDocument.create();
  document.setCreator('ChatOS Document MCP');
  document.setProducer('ChatOS Document MCP');
  for (const image of images) {
    const pageSize = image.width > image.height ? A4_LANDSCAPE : A4_PORTRAIT;
    const page = document.addPage([pageSize[0], pageSize[1]]);
    const embedded = await document.embedPng(image.bytes);
    const availableWidth = pageSize[0] - PAGE_MARGIN * 2;
    const availableHeight = pageSize[1] - PAGE_MARGIN * 2;
    const scale = Math.min(
      availableWidth / image.width,
      availableHeight / image.height,
      image.maximumScale ?? Number.POSITIVE_INFINITY
    );
    const width = image.width * scale;
    const height = image.height * scale;
    page.drawImage(embedded, {
      x: (pageSize[0] - width) / 2,
      y: (pageSize[1] - height) / 2,
      width,
      height
    });
  }
  return document;
}

export async function convertDocument(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string' || typeof args.outputName !== 'string') {
    throw new DocumentError('INVALID_ARGUMENT', 'inputPath and outputName are required.');
  }
  const source = await resolveWorkspaceFile(args.inputPath);
  if (source.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const extension = path.extname(source.relativePath).toLowerCase();
  if (!SUPPORTED_EXTENSIONS.has(extension)) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'document_convert supports .docx, .xlsx, and .pptx input files.');
  }
  if (extension === '.xlsx' && args.pages !== undefined) {
    throw new DocumentError('INVALID_ARGUMENT', 'pages is not supported for XLSX conversion; use sheets.');
  }
  if (extension !== '.xlsx' && args.sheets !== undefined) {
    throw new DocumentError('INVALID_ARGUMENT', 'sheets is supported only for XLSX conversion.');
  }

  const viewportWidth = boundedInteger(args.viewportWidth, 1600, 320, 2400, 'viewportWidth');
  const viewportHeight = boundedInteger(args.viewportHeight, 1200, 240, 2400, 'viewportHeight');
  const sourceBytes = await readFile(source.absolutePath);
  const inspection = inspectOoxml(sourceBytes, extension);
  const output = await resolveArtifactPaths(args.outputName, '.pdf');
  const temporaryFiles = [output.temporaryPath];
  const warnings = [
    'The PDF contains page images and has no searchable or selectable source text.',
    'Office content is rendered with the cross-platform HTML preview engine; installed fonts can affect layout.'
  ];

  try {
    let images: RenderedImage[];
    if (extension === '.docx') {
      images = await renderDocx(
        source.absolutePath,
        output.root,
        selectedPageNumbers(args.pages),
        viewportWidth,
        viewportHeight,
        temporaryFiles,
        warnings
      );
    } else if (extension === '.pptx') {
      const slideCount = inspection.structure.slides;
      if (!Number.isInteger(slideCount)) throw new DocumentError('INVALID_DOCUMENT', 'The PPTX slide count is unavailable.');
      images = await renderPptx(
        source.absolutePath,
        output.root,
        slideCount as number,
        selectedPageNumbers(args.pages, slideCount as number),
        viewportWidth,
        viewportHeight,
        temporaryFiles
      );
    } else {
      const available = await spreadsheetRenderRanges(source.absolutePath);
      images = await renderXlsx(
        source.absolutePath,
        output.root,
        selectedSheets(args.sheets, available),
        viewportWidth,
        viewportHeight,
        temporaryFiles
      );
    }

    let totalPixels = 0;
    for (const image of images) {
      totalPixels += image.width * image.height;
      if (totalPixels > MAX_RENDER_TOTAL_PIXELS) {
        throw new DocumentError('VALIDATION_FAILED', 'Rendered Office pages exceeded the total pixel limit.');
      }
    }
    const pdf = await assemblePdf(images);
    const bytes = await pdf.save({ useObjectStreams: true, addDefaultPage: false });
    await writeFile(output.temporaryPath, bytes, { flag: 'wx', mode: 0o600 });
    const verified = await PDFDocument.load(await readFile(output.temporaryPath), {
      ignoreEncryption: false,
      updateMetadata: false
    }).catch(() => undefined);
    if (!verified || verified.getPageCount() !== images.length) {
      throw new DocumentError('VALIDATION_FAILED', 'The converted PDF failed validation.');
    }
    await copyFile(output.temporaryPath, output.outputPath, EXCLUSIVE_COPY_FLAG).catch((error: NodeJS.ErrnoException) => {
      if (error.code === 'EEXIST') throw new DocumentError('OUTPUT_EXISTS', 'The output artifact was created by another operation.');
      throw error;
    });
    const metadata = await lstat(output.outputPath);
    return {
      ok: true,
      operation: 'document_convert',
      conversionMode: 'raster',
      searchableText: false,
      layoutFidelity: 'preview',
      source: {
        relativePath: source.relativePath,
        format: inspection.format,
        size: source.size,
        sha256: await sha256File(source.absolutePath)
      },
      engine: { name: 'OfficeCLI', version: OFFICECLI_VERSION, renderMode: 'html' },
      pages: images.map((image, index) => ({
        outputPage: index + 1,
        ...image.mapping,
        sourceWidth: image.width,
        sourceHeight: image.height
      })),
      artifact: {
        relativePath: output.outputName,
        mimeType: 'application/pdf',
        size: metadata.size,
        sha256: await sha256File(output.outputPath),
        pages: verified.getPageCount()
      },
      validation: { status: 'passed', warnings },
      warnings
    };
  } finally {
    await Promise.all(temporaryFiles.map((temporaryPath) => rm(temporaryPath, { force: true })));
  }
}
