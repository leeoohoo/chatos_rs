import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { MAX_EXTRACT_CHARS, MAX_INPUT_BYTES } from '../constants.js';
import { DocumentError } from '../errors.js';
import { resolveWorkspaceFile } from '../security/paths.js';
import { sha256File } from '../inspect/hash.js';
import { extractOoxmlText, normalizeText, type TextSection } from './ooxml.js';
import { extractPdfText } from './pdf.js';

function boundSections(sections: TextSection[], maxChars: number): { sections: TextSection[]; truncated: boolean; chars: number } {
  const bounded: TextSection[] = [];
  let remaining = maxChars;
  let truncated = false;
  let chars = 0;
  for (const section of sections) {
    const text = normalizeText(section.text);
    if (!text) continue;
    if (remaining <= 0) {
      truncated = true;
      break;
    }
    const selected = text.slice(0, remaining);
    bounded.push({ ...section, text: selected });
    remaining -= selected.length;
    chars += selected.length;
    if (selected.length < text.length) {
      truncated = true;
      break;
    }
  }
  return { sections: bounded, truncated, chars };
}

export async function extractDocumentText(inputPath: string, requestedMaxChars?: number): Promise<Record<string, unknown>> {
  const resolved = await resolveWorkspaceFile(inputPath);
  if (resolved.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const extension = path.extname(resolved.relativePath).toLowerCase();
  if (!['.docx', '.xlsx', '.pptx', '.pdf'].includes(extension)) {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'Supported extensions are .docx, .xlsx, .pptx, and .pdf.');
  }
  const maxChars = Math.min(requestedMaxChars ?? MAX_EXTRACT_CHARS, MAX_EXTRACT_CHARS);
  const [data, sha256] = await Promise.all([readFile(resolved.absolutePath), sha256File(resolved.absolutePath)]);
  const extracted = extension === '.pdf'
    ? await extractPdfText(data)
    : extractOoxmlText(data, extension);
  const bounded = boundSections(extracted, maxChars);
  return {
    ok: true,
    operation: 'document_extract_text',
    source: {
      relativePath: resolved.relativePath,
      size: resolved.size,
      sha256
    },
    format: extension.slice(1),
    sections: bounded.sections,
    chars: bounded.chars,
    truncated: bounded.truncated || extracted.length > bounded.sections.length
  };
}
