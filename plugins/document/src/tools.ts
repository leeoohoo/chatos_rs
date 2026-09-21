import { DocumentError } from './errors.js';
import { convertDocument } from './convert/document.js';
import { inspectDocument } from './inspect/document.js';
import { extractDocumentText } from './extract/document.js';
import { createOfficeArtifact, editOfficeArtifact } from './office/artifact.js';
import { renderDocument } from './render/document.js';
import { validateDocument } from './validate/document.js';
import { readSpreadsheetRange, writeSpreadsheetRange } from './spreadsheet/range.js';
import { manageSpreadsheetSheets } from './spreadsheet/sheets.js';
import {
  extractPdfPages,
  fillPdfForm,
  listPdfForm,
  mergePdfs,
  transformPdf
} from './pdf/operations.js';
import { TOOL_DEFINITIONS_BASE } from './tool-definitions.js';

function requiredDocumentSkills(toolName: string): string[] {
  if (toolName.startsWith('spreadsheet_')) return ['document', 'document-spreadsheet'];
  if (toolName.startsWith('pdf_')) return ['document', 'document-pdf'];
  return ['document'];
}

export const TOOL_DEFINITIONS = TOOL_DEFINITIONS_BASE.map((tool) => {
  const selector = tool.name === 'office_create'
    ? {
        pointer: '/format',
        map: {
          docx: 'document-word',
          xlsx: 'document-spreadsheet',
          pptx: 'document-presentation'
        }
      }
    : undefined;
  return {
    ...tool,
    _meta: {
      ...tool._meta,
      'chatos/skillGate': {
        allOf: requiredDocumentSkills(tool.name),
        ...(selector ? { selectByArgument: selector } : {})
      }
    }
  };
});

export async function callTool(name: string, args: unknown): Promise<Record<string, unknown>> {
  if (![
    'document_inspect',
    'document_extract_text',
    'document_render',
    'document_convert',
    'document_validate',
    'spreadsheet_read_range',
    'spreadsheet_write_range',
    'spreadsheet_manage_sheets',
    'office_create',
    'office_edit_batch',
    'pdf_merge',
    'pdf_extract_pages',
    'pdf_transform',
    'pdf_form_list',
    'pdf_form_fill'
  ].includes(name)) {
    throw new DocumentError('INVALID_ARGUMENT', `Unknown tool: ${name}`);
  }
  if (!args || typeof args !== 'object' || Array.isArray(args)) {
    throw new DocumentError('INVALID_ARGUMENT', 'Tool arguments must be an object.');
  }
  const values = args as Record<string, unknown>;
  if (name === 'document_render') {
    const allowed = new Set(['inputPath', 'outputPrefix', 'pages', 'dpi', 'viewportWidth', 'viewportHeight']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'document_render received unknown arguments.');
    }
    return await renderDocument(values);
  }
  if (name === 'document_convert') {
    const allowed = new Set(['inputPath', 'outputName', 'pages', 'sheets', 'viewportWidth', 'viewportHeight']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'document_convert received unknown arguments.');
    }
    return await convertDocument(values);
  }
  if (name === 'document_validate') {
    const allowed = new Set(['inputPath', 'renderPages', 'dpi', 'viewportWidth', 'viewportHeight']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'document_validate received unknown arguments.');
    }
    return await validateDocument(values);
  }
  if (name === 'spreadsheet_read_range') {
    const allowed = new Set(['inputPath', 'sheet', 'range', 'maxCells']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'spreadsheet_read_range received unknown arguments.');
    }
    return await readSpreadsheetRange(values);
  }
  if (name === 'spreadsheet_write_range') {
    const allowed = new Set(['inputPath', 'outputName', 'sheet', 'startCell', 'values']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'spreadsheet_write_range received unknown arguments.');
    }
    return await writeSpreadsheetRange(values);
  }
  if (name === 'spreadsheet_manage_sheets') {
    const allowed = new Set(['inputPath', 'outputName', 'operations']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'spreadsheet_manage_sheets received unknown arguments.');
    }
    return await manageSpreadsheetSheets(values);
  }
  if (name === 'office_create') {
    const allowed = new Set(['format', 'outputName', 'locale', 'operations']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'office_create received unknown arguments.');
    }
    return await createOfficeArtifact(values);
  }
  if (name === 'office_edit_batch') {
    const allowed = new Set(['inputPath', 'outputName', 'operations']);
    if (Object.keys(values).some((key) => !allowed.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', 'office_edit_batch received unknown arguments.');
    }
    return await editOfficeArtifact(values);
  }
  const pdfAllowedKeys: Record<string, Set<string>> = {
    pdf_merge: new Set(['inputPaths', 'outputName']),
    pdf_extract_pages: new Set(['inputPath', 'pages', 'outputName']),
    pdf_transform: new Set(['inputPath', 'outputName', 'pageOrder', 'rotations', 'metadata']),
    pdf_form_list: new Set(['inputPath']),
    pdf_form_fill: new Set(['inputPath', 'outputName', 'fields', 'flatten'])
  };
  const pdfKeys = pdfAllowedKeys[name];
  if (pdfKeys) {
    if (Object.keys(values).some((key) => !pdfKeys.has(key))) {
      throw new DocumentError('INVALID_ARGUMENT', `${name} received unknown arguments.`);
    }
    if (name === 'pdf_merge') return await mergePdfs(values);
    if (name === 'pdf_extract_pages') return await extractPdfPages(values);
    if (name === 'pdf_transform') return await transformPdf(values);
    if (name === 'pdf_form_list') return await listPdfForm(values);
    return await fillPdfForm(values);
  }
  const allowed = name === 'document_inspect' ? new Set(['inputPath']) : new Set(['inputPath', 'maxChars']);
  if (Object.keys(values).some((key) => !allowed.has(key)) || typeof values.inputPath !== 'string') {
    throw new DocumentError('INVALID_ARGUMENT', `${name} received invalid arguments.`);
  }
  if (name === 'document_inspect') return await inspectDocument(values.inputPath);
  if (values.maxChars !== undefined && (!Number.isInteger(values.maxChars) || (values.maxChars as number) < 1 || (values.maxChars as number) > 30_000)) {
    throw new DocumentError('INVALID_ARGUMENT', 'maxChars must be an integer between 1 and 30000.');
  }
  return await extractDocumentText(values.inputPath, values.maxChars as number | undefined);
}
