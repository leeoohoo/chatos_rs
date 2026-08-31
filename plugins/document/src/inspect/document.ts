import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { MAX_INPUT_BYTES } from '../constants.js';
import { DocumentError } from '../errors.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { sha256File } from './hash.js';
import { inspectOoxml } from './ooxml.js';
import { inspectPdf } from './pdf.js';

const SUPPORTED_EXTENSIONS = new Set(['.docx', '.xlsx', '.pptx', '.pdf']);

export async function inspectDocument(inputPath: string): Promise<Record<string, unknown>> {
  const resolved = await resolveWorkspaceFile(inputPath);
  if (resolved.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const extension = path.extname(resolved.relativePath).toLowerCase();
  if (!SUPPORTED_EXTENSIONS.has(extension)) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'Supported extensions are .docx, .xlsx, .pptx, and .pdf.');
  }

  const [data, sha256] = await Promise.all([
    readFile(resolved.absolutePath),
    sha256File(resolved.absolutePath)
  ]);
  const inspected = extension === '.pdf'
    ? inspectPdf(data)
    : inspectOoxml(data, extension);

  return {
    ok: true,
    operation: 'document_inspect',
    source: {
      relativePath: resolved.relativePath,
      size: resolved.size,
      sha256
    },
    ...inspected,
    capabilities: {
      inspect: true,
      extractText: true,
      edit: false,
      render: true,
      validate: true,
      note: 'Edits create new managed artifacts; workspace input files are never overwritten.'
    }
  };
}
