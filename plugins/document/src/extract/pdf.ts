import { getDocument, type PDFDocumentProxy } from 'pdfjs-dist/legacy/build/pdf.mjs';
import { MAX_EXTRACT_SECTIONS } from '../constants.js';
import { DocumentError } from '../errors.js';
import type { TextSection } from './ooxml.js';

export async function extractPdfText(data: Uint8Array): Promise<TextSection[]> {
  let document: PDFDocumentProxy | undefined;
  try {
    const loadingTask = getDocument({
      data: Uint8Array.from(data),
      useSystemFonts: false,
      isEvalSupported: false
    });
    document = await loadingTask.promise;
    const pages = Math.min(document.numPages, MAX_EXTRACT_SECTIONS);
    const sections: TextSection[] = [];
    for (let pageNumber = 1; pageNumber <= pages; pageNumber += 1) {
      const page = await document.getPage(pageNumber);
      const content = await page.getTextContent();
      let text = '';
      for (const item of content.items) {
        if (!('str' in item)) continue;
        text += item.str;
        text += item.hasEOL ? '\n' : ' ';
      }
      sections.push({ kind: 'page', index: pageNumber, text: text.trim() });
      page.cleanup();
    }
    return sections;
  } catch (error) {
    if (process.env.DOCUMENT_MCP_DEBUG === '1') {
      const message = error instanceof Error ? error.stack ?? error.message : String(error);
      process.stderr.write(`[document-mcp] PDF extraction failed: ${message}\n`);
    }
    throw new DocumentError('INVALID_DOCUMENT', 'The PDF could not be parsed for text extraction.');
  } finally {
    await document?.destroy();
  }
}
