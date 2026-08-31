import { DocumentError } from '../errors.js';

const LATIN1 = new TextDecoder('latin1');

function pdfLiteral(text: string, key: string): string | undefined {
  const match = text.match(new RegExp(`\\/${key}\\s*\\(([^)]{0,4096})\\)`));
  return match?.[1]?.replace(/\\([()\\])/g, '$1').trim() || undefined;
}

export function inspectPdf(data: Uint8Array): {
  format: 'pdf';
  mimeType: 'application/pdf';
  metadata: Record<string, string>;
  structure: Record<string, unknown>;
} {
  const header = LATIN1.decode(data.subarray(0, Math.min(data.length, 16)));
  if (!header.startsWith('%PDF-')) {
    throw new DocumentError('FORMAT_MISMATCH', 'The file extension indicates PDF, but the PDF signature is missing.');
  }
  const text = LATIN1.decode(data);
  const pageCount = text.match(/\/Type\s*\/Page(?!s)\b/g)?.length ?? 0;
  const metadata = Object.fromEntries(
    [
      ['title', pdfLiteral(text, 'Title')],
      ['author', pdfLiteral(text, 'Author')],
      ['subject', pdfLiteral(text, 'Subject')],
      ['creator', pdfLiteral(text, 'Creator')],
      ['producer', pdfLiteral(text, 'Producer')]
    ].filter((entry): entry is [string, string] => Boolean(entry[1]))
  );
  return {
    format: 'pdf',
    mimeType: 'application/pdf',
    metadata,
    structure: {
      approximatePages: pageCount,
      pageCountIsApproximate: true,
      encrypted: /\/Encrypt\b/.test(text),
      hasAcroForm: /\/AcroForm\b/.test(text),
      version: header.slice(5).trim()
    }
  };
}
