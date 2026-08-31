import { copyFile, lstat, readFile, rm, writeFile } from 'node:fs/promises';
import {
  degrees,
  PDFCheckBox,
  PDFDocument,
  PDFDropdown,
  PDFOptionList,
  PDFRadioGroup,
  PDFTextField
} from 'pdf-lib';
import { MAX_INPUT_BYTES } from '../constants.js';
import { DocumentError } from '../errors.js';
import { sha256File } from '../inspect/hash.js';
import { EXCLUSIVE_COPY_FLAG, resolveArtifactPaths } from '../security/artifacts.js';
import { resolveWorkspaceFile } from '../security/paths.js';

interface LoadedPdf {
  document: PDFDocument;
  relativePath: string;
  absolutePath: string;
  size: number;
  sha256: string;
}

async function loadPdf(inputPath: unknown): Promise<LoadedPdf> {
  if (typeof inputPath !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'inputPath must be a string.');
  const source = await resolveWorkspaceFile(inputPath);
  if (!source.relativePath.toLowerCase().endsWith('.pdf')) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'The input file must be a PDF.');
  }
  if (source.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const bytes = await readFile(source.absolutePath);
  try {
    return {
      document: await PDFDocument.load(bytes, { ignoreEncryption: false, updateMetadata: false }),
      relativePath: source.relativePath,
      absolutePath: source.absolutePath,
      size: source.size,
      sha256: await sha256File(source.absolutePath)
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : '';
    if (/encrypt/i.test(message)) throw new DocumentError('ENCRYPTED_FILE', 'Encrypted PDF files are not supported.');
    throw new DocumentError('INVALID_DOCUMENT', 'The PDF could not be parsed.');
  }
}

function sourceSummary(source: LoadedPdf): Record<string, unknown> {
  return {
    relativePath: source.relativePath,
    size: source.size,
    sha256: source.sha256,
    pages: source.document.getPageCount()
  };
}

async function publishPdf(
  outputName: unknown,
  document: PDFDocument,
  operation: string,
  extra: Record<string, unknown> = {}
): Promise<Record<string, unknown>> {
  if (typeof outputName !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'outputName is required.');
  const paths = await resolveArtifactPaths(outputName, '.pdf');
  try {
    const bytes = await document.save({ useObjectStreams: true, addDefaultPage: false });
    await writeFile(paths.temporaryPath, bytes, { flag: 'wx', mode: 0o600 });
    const verified = await PDFDocument.load(await readFile(paths.temporaryPath), {
      ignoreEncryption: false,
      updateMetadata: false
    }).catch(() => undefined);
    if (!verified) throw new DocumentError('VALIDATION_FAILED', 'The generated PDF failed validation.');
    const pages = verified.getPageCount();
    await copyFile(paths.temporaryPath, paths.outputPath, EXCLUSIVE_COPY_FLAG).catch((error: NodeJS.ErrnoException) => {
      if (error.code === 'EEXIST') throw new DocumentError('OUTPUT_EXISTS', 'The output artifact was created by another operation.');
      throw error;
    });
    const metadata = await lstat(paths.outputPath);
    return {
      ok: true,
      operation,
      ...extra,
      artifact: {
        relativePath: paths.outputName,
        mimeType: 'application/pdf',
        size: metadata.size,
        sha256: await sha256File(paths.outputPath),
        pages
      },
      validation: { status: 'passed', warnings: [] }
    };
  } finally {
    await rm(paths.temporaryPath, { force: true });
  }
}

function pageNumbers(value: unknown, pageCount: number, field = 'pages'): number[] {
  if (!Array.isArray(value) || value.length === 0 || value.length > 2_000) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must contain between 1 and 2000 page numbers.`);
  }
  return value.map((page, index) => {
    if (!Number.isInteger(page) || (page as number) < 1 || (page as number) > pageCount) {
      throw new DocumentError('INVALID_ARGUMENT', `${field}[${index}] is outside the document page range.`);
    }
    return page as number;
  });
}

export async function mergePdfs(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (!Array.isArray(args.inputPaths) || args.inputPaths.length < 2 || args.inputPaths.length > 20) {
    throw new DocumentError('INVALID_ARGUMENT', 'inputPaths must contain between 2 and 20 PDF paths.');
  }
  const inputs = [] as LoadedPdf[];
  for (const inputPath of args.inputPaths) inputs.push(await loadPdf(inputPath));
  const merged = await PDFDocument.create();
  for (const source of inputs) {
    const pages = await merged.copyPages(source.document, source.document.getPageIndices());
    for (const page of pages) merged.addPage(page);
  }
  return await publishPdf(args.outputName, merged, 'pdf_merge', {
    sources: inputs.map(sourceSummary)
  });
}

export async function extractPdfPages(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const source = await loadPdf(args.inputPath);
  const selected = pageNumbers(args.pages, source.document.getPageCount());
  const output = await PDFDocument.create();
  const pages = await output.copyPages(source.document, selected.map((page) => page - 1));
  for (const page of pages) output.addPage(page);
  return await publishPdf(args.outputName, output, 'pdf_extract_pages', {
    source: sourceSummary(source),
    selectedPages: selected
  });
}

function metadataValue(value: unknown, field: string): string | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== 'string' || value.length > 4_000) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be a string of at most 4000 characters.`);
  }
  return value;
}

export async function transformPdf(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const source = await loadPdf(args.inputPath);
  const pageCount = source.document.getPageCount();
  let output: PDFDocument;
  let order: number[];
  if (args.pageOrder !== undefined) {
    order = pageNumbers(args.pageOrder, pageCount, 'pageOrder');
    output = await PDFDocument.create();
    const pages = await output.copyPages(source.document, order.map((page) => page - 1));
    for (const page of pages) output.addPage(page);
  } else {
    output = source.document;
    order = Array.from({ length: pageCount }, (_, index) => index + 1);
  }

  if (args.rotations !== undefined) {
    if (!Array.isArray(args.rotations) || args.rotations.length > 2_000) {
      throw new DocumentError('INVALID_ARGUMENT', 'rotations must be an array containing at most 2000 items.');
    }
    for (const [index, rotation] of args.rotations.entries()) {
      if (!rotation || typeof rotation !== 'object' || Array.isArray(rotation)) {
        throw new DocumentError('INVALID_ARGUMENT', `rotations[${index}] must be an object.`);
      }
      const item = rotation as Record<string, unknown>;
      if (!Number.isInteger(item.page) || (item.page as number) < 1 || (item.page as number) > output.getPageCount()) {
        throw new DocumentError('INVALID_ARGUMENT', `rotations[${index}].page is outside the output page range.`);
      }
      if (![0, 90, 180, 270].includes(item.degrees as number)) {
        throw new DocumentError('INVALID_ARGUMENT', `rotations[${index}].degrees must be 0, 90, 180, or 270.`);
      }
      output.getPage((item.page as number) - 1).setRotation(degrees(item.degrees as number));
    }
  }

  if (args.metadata !== undefined) {
    if (!args.metadata || typeof args.metadata !== 'object' || Array.isArray(args.metadata)) {
      throw new DocumentError('INVALID_ARGUMENT', 'metadata must be an object.');
    }
    const metadata = args.metadata as Record<string, unknown>;
    const title = metadataValue(metadata.title, 'metadata.title');
    const author = metadataValue(metadata.author, 'metadata.author');
    const subject = metadataValue(metadata.subject, 'metadata.subject');
    const creator = metadataValue(metadata.creator, 'metadata.creator');
    const producer = metadataValue(metadata.producer, 'metadata.producer');
    const keywords = metadataValue(metadata.keywords, 'metadata.keywords');
    if (title !== undefined) output.setTitle(title);
    if (author !== undefined) output.setAuthor(author);
    if (subject !== undefined) output.setSubject(subject);
    if (creator !== undefined) output.setCreator(creator);
    if (producer !== undefined) output.setProducer(producer);
    if (keywords !== undefined) output.setKeywords(keywords.split(',').map((value) => value.trim()).filter(Boolean));
  }

  return await publishPdf(args.outputName, output, 'pdf_transform', {
    source: sourceSummary(source),
    pageOrder: order
  });
}

function fieldType(field: unknown): string {
  if (field instanceof PDFTextField) return 'text';
  if (field instanceof PDFCheckBox) return 'checkbox';
  if (field instanceof PDFRadioGroup) return 'radio';
  if (field instanceof PDFDropdown) return 'dropdown';
  if (field instanceof PDFOptionList) return 'optionList';
  return 'unknown';
}

export async function listPdfForm(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const source = await loadPdf(args.inputPath);
  const fields = source.document.getForm().getFields();
  if (fields.length > 500) throw new DocumentError('INVALID_DOCUMENT', 'The PDF form contains more than 500 fields.');
  return {
    ok: true,
    operation: 'pdf_form_list',
    source: sourceSummary(source),
    fields: fields.map((field) => ({ name: field.getName(), type: fieldType(field) }))
  };
}

export async function fillPdfForm(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  const source = await loadPdf(args.inputPath);
  if (!Array.isArray(args.fields) || args.fields.length === 0 || args.fields.length > 500) {
    throw new DocumentError('INVALID_ARGUMENT', 'fields must contain between 1 and 500 form field values.');
  }
  const form = source.document.getForm();
  const available = new Map(form.getFields().map((field) => [field.getName(), field]));
  for (const [index, entry] of args.fields.entries()) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      throw new DocumentError('INVALID_ARGUMENT', `fields[${index}] must be an object.`);
    }
    const item = entry as Record<string, unknown>;
    if (typeof item.name !== 'string' || item.name.length > 500) {
      throw new DocumentError('INVALID_ARGUMENT', `fields[${index}].name is invalid.`);
    }
    const field = available.get(item.name);
    if (!field) throw new DocumentError('INVALID_ARGUMENT', `PDF form field not found: ${item.name}`);
    if (field instanceof PDFTextField && typeof item.value === 'string') field.setText(item.value);
    else if (field instanceof PDFCheckBox && typeof item.value === 'boolean') item.value ? field.check() : field.uncheck();
    else if (field instanceof PDFRadioGroup && typeof item.value === 'string') field.select(item.value);
    else if (field instanceof PDFDropdown && typeof item.value === 'string') field.select(item.value);
    else if (field instanceof PDFOptionList && typeof item.value === 'string') field.select(item.value);
    else throw new DocumentError('INVALID_ARGUMENT', `Value type does not match PDF form field: ${item.name}`);
  }
  if (args.flatten === true) form.flatten();
  return await publishPdf(args.outputName, source.document, 'pdf_form_fill', {
    source: sourceSummary(source),
    filledFields: args.fields.length,
    flattened: args.flatten === true
  });
}
