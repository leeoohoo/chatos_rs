import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { unzipSync, type UnzipFileInfo } from 'fflate';
import {
  MAX_EXTRACTED_XML_BYTES,
  MAX_INPUT_BYTES,
  MAX_OOXML_ENTRIES,
  MAX_OOXML_UNPACKED_BYTES
} from '../constants.js';
import { DocumentError } from '../errors.js';
import { decodeXmlText } from '../inspect/xml.js';
import { sha256File } from '../inspect/hash.js';
import { editOfficeArtifact } from '../office/artifact.js';
import { resolveWorkspaceFile } from '../security/paths.js';

const DECODER = new TextDecoder('utf-8', { fatal: false });
const MAX_RANGE_AREA = 10_000;
const MAX_WRITE_CELLS = 2_000;
const MAX_RETURNED_CELLS = 500;
const MAX_RETURNED_TEXT = 20_000;

interface CellCoordinate {
  column: number;
  row: number;
}

interface CellRange {
  start: CellCoordinate;
  end: CellCoordinate;
  normalized: string;
  rows: number;
  columns: number;
  area: number;
}

function safeArchiveName(name: string): boolean {
  if (!name || name.includes('\\') || name.startsWith('/') || /^[A-Za-z]:/.test(name)) return false;
  return !name.split('/').some((component) => !component || component === '.' || component === '..');
}

function attribute(attributes: string, name: string): string | undefined {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = attributes.match(new RegExp(`(?:^|\\s)${escaped}=(?:"([^"]*)"|'([^']*)')`, 'i'));
  const value = match?.[1] ?? match?.[2];
  return value === undefined ? undefined : decodeXmlText(value);
}

function localTagTexts(source: string, localName: string): string[] {
  const escaped = localName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const expression = new RegExp(
    `<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?${escaped}(?:\\s[^>]*)?>([\\s\\S]*?)<\\/(?:[A-Za-z_][A-Za-z0-9_.-]*:)?${escaped}>`,
    'gi'
  );
  return Array.from(source.matchAll(expression), (match) =>
    decodeXmlText((match[1] ?? '').replace(/<[^>]+>/g, '')).trim()
  );
}

function columnNumber(letters: string): number {
  let result = 0;
  for (const letter of letters.toUpperCase()) result = result * 26 + letter.charCodeAt(0) - 64;
  return result;
}

function columnLetters(column: number): string {
  let value = column;
  let result = '';
  while (value > 0) {
    const remainder = (value - 1) % 26;
    result = String.fromCharCode(65 + remainder) + result;
    value = Math.floor((value - 1) / 26);
  }
  return result;
}

function parseCellAddress(value: unknown, field: string): CellCoordinate {
  if (typeof value !== 'string') throw new DocumentError('INVALID_ARGUMENT', `${field} must use A1 notation.`);
  const match = value.trim().toUpperCase().match(/^([A-Z]{1,3})([1-9][0-9]{0,6})$/);
  if (!match?.[1] || !match[2]) throw new DocumentError('INVALID_ARGUMENT', `${field} must use A1 notation.`);
  const column = columnNumber(match[1]);
  const row = Number.parseInt(match[2], 10);
  if (column > 16_384 || row > 1_048_576) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} is outside the XLSX worksheet limits.`);
  }
  return { column, row };
}

function cellAddress(coordinate: CellCoordinate): string {
  return `${columnLetters(coordinate.column)}${coordinate.row}`;
}

function parseRange(value: unknown): CellRange {
  if (typeof value !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'range must use A1:C10 notation.');
  const parts = value.split(':');
  if (parts.length < 1 || parts.length > 2 || !parts[0]) {
    throw new DocumentError('INVALID_ARGUMENT', 'range must use A1:C10 notation.');
  }
  const start = parseCellAddress(parts[0], 'range start');
  const end = parseCellAddress(parts[1] ?? parts[0], 'range end');
  if (end.column < start.column || end.row < start.row) {
    throw new DocumentError('INVALID_ARGUMENT', 'range end must not precede range start.');
  }
  const rows = end.row - start.row + 1;
  const columns = end.column - start.column + 1;
  const area = rows * columns;
  if (area > MAX_RANGE_AREA) {
    throw new DocumentError('INVALID_ARGUMENT', `The requested range exceeds the ${MAX_RANGE_AREA} cell limit.`);
  }
  return { start, end, normalized: `${cellAddress(start)}:${cellAddress(end)}`, rows, columns, area };
}

function sheetName(value: unknown): string {
  if (typeof value !== 'string' || value.length < 1 || value.length > 31 || /[\\/*?:[\]]/.test(value)) {
    throw new DocumentError('INVALID_ARGUMENT', 'sheet must be a valid worksheet name of at most 31 characters.');
  }
  return value;
}

function readParts(data: Uint8Array): Record<string, Uint8Array> {
  let entryCount = 0;
  let totalSize = 0;
  try {
    return unzipSync(data, {
      filter(file: UnzipFileInfo): boolean {
        entryCount += 1;
        totalSize += file.originalSize;
        if (entryCount > MAX_OOXML_ENTRIES || totalSize > MAX_OOXML_UNPACKED_BYTES) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The XLSX archive exceeds extraction safety limits.');
        }
        if (!safeArchiveName(file.name)) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The XLSX archive contains an unsafe entry path.');
        }
        return (
          file.name === 'xl/workbook.xml'
          || file.name === 'xl/_rels/workbook.xml.rels'
          || file.name === 'xl/sharedStrings.xml'
          || /^xl\/worksheets\/[^/]+\.xml$/i.test(file.name)
        ) && file.originalSize <= MAX_EXTRACTED_XML_BYTES;
      }
    });
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw new DocumentError('INVALID_DOCUMENT', 'The XLSX archive could not be parsed.');
  }
}

function xml(parts: Record<string, Uint8Array>, name: string): string | undefined {
  const bytes = parts[name];
  return bytes ? DECODER.decode(bytes) : undefined;
}

function sharedStrings(parts: Record<string, Uint8Array>): string[] {
  const source = xml(parts, 'xl/sharedStrings.xml');
  if (!source) return [];
  return Array.from(source.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?si(?:\s[^>]*)?>([\s\S]*?)<\/(?:[A-Za-z_][A-Za-z0-9_.-]*:)?si>/gi), (match) =>
    localTagTexts(match[1] ?? '', 't').join('')
  );
}

function worksheetPath(parts: Record<string, Uint8Array>, requestedSheet: string): string {
  const workbook = xml(parts, 'xl/workbook.xml');
  const relationships = xml(parts, 'xl/_rels/workbook.xml.rels');
  if (!workbook || !relationships) {
    throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook or its relationships are missing.');
  }
  let relationshipId: string | undefined;
  for (const match of workbook.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?sheet\s+([^>]*?)(?:\/?>)/gi)) {
    const attributes = match[1] ?? '';
    if (attribute(attributes, 'name') === requestedSheet) {
      relationshipId = attribute(attributes, 'r:id');
      break;
    }
  }
  if (!relationshipId) throw new DocumentError('INVALID_ARGUMENT', `Worksheet not found: ${requestedSheet}`);

  let target: string | undefined;
  for (const match of relationships.matchAll(/<Relationship\s+([^>]*?)(?:\/?>)/gi)) {
    const attributes = match[1] ?? '';
    if (attribute(attributes, 'Id') === relationshipId && attribute(attributes, 'TargetMode') !== 'External') {
      target = attribute(attributes, 'Target');
      break;
    }
  }
  if (!target) throw new DocumentError('INVALID_DOCUMENT', 'The worksheet relationship is missing.');
  const normalized = target.startsWith('/')
    ? path.posix.normalize(target.slice(1))
    : path.posix.normalize(path.posix.join('xl', target));
  if (!normalized.startsWith('xl/worksheets/') || !parts[normalized]) {
    throw new DocumentError('INVALID_DOCUMENT', 'The worksheet relationship target is invalid.');
  }
  return normalized;
}

export interface SpreadsheetRenderRange {
  name: string;
  range: string;
  hidden: boolean;
  empty: boolean;
}

function worksheetUsedRange(source: string): string {
  const dimension = source.match(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?dimension\s+([^>]*?)(?:\/?>)/i);
  const declared = dimension ? attribute(dimension[1] ?? '', 'ref') : undefined;
  if (declared && /^([A-Za-z]{1,3}[1-9][0-9]{0,6})(?::([A-Za-z]{1,3}[1-9][0-9]{0,6}))?$/.test(declared)) {
    const [start, end = start] = declared.toUpperCase().split(':');
    if (start && end) return `${start}:${end}`;
  }

  let minimumColumn = Number.POSITIVE_INFINITY;
  let minimumRow = Number.POSITIVE_INFINITY;
  let maximumColumn = 0;
  let maximumRow = 0;
  for (const match of source.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c(?:\s([^>]*))?(?:>|\/)/gi)) {
    const reference = attribute(match[1] ?? '', 'r');
    if (!reference) continue;
    const parsed = reference.toUpperCase().match(/^([A-Z]{1,3})([1-9][0-9]{0,6})$/);
    if (!parsed?.[1] || !parsed[2]) continue;
    const column = columnNumber(parsed[1]);
    const row = Number.parseInt(parsed[2], 10);
    if (column > 16_384 || row > 1_048_576) continue;
    minimumColumn = Math.min(minimumColumn, column);
    minimumRow = Math.min(minimumRow, row);
    maximumColumn = Math.max(maximumColumn, column);
    maximumRow = Math.max(maximumRow, row);
  }
  if (maximumColumn === 0 || maximumRow === 0) return 'A1:A1';
  return `${columnLetters(minimumColumn)}${minimumRow}:${columnLetters(maximumColumn)}${maximumRow}`;
}

export async function spreadsheetRenderRanges(absolutePath: string): Promise<SpreadsheetRenderRange[]> {
  const parts = readParts(await readFile(absolutePath));
  const workbook = xml(parts, 'xl/workbook.xml');
  if (!workbook) throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook definition is missing or too large.');
  const results: SpreadsheetRenderRange[] = [];
  for (const match of workbook.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?sheet\s+([^>]*?)(?:\/?>)/gi)) {
    const attributes = match[1] ?? '';
    const name = attribute(attributes, 'name');
    if (!name) throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook contains a worksheet without a name.');
    const source = xml(parts, worksheetPath(parts, name));
    if (!source) throw new DocumentError('INVALID_DOCUMENT', `Worksheet is missing or too large: ${name}`);
    results.push({
      name,
      range: worksheetUsedRange(source),
      hidden: ['hidden', 'veryHidden'].includes(attribute(attributes, 'state') ?? ''),
      empty: !/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c(?:\s[^>]*)?(?:>|\/)/i.test(source)
    });
    if (results.length > 200) {
      throw new DocumentError('INVALID_DOCUMENT', 'The workbook contains more than 200 worksheets and exceeds the inspection limit.');
    }
  }
  if (results.length === 0) throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook contains no worksheets.');
  return results;
}

function cellValue(body: string, type: string | undefined, shared: string[]): unknown {
  if (type === 'inlineStr') return localTagTexts(body, 't').join('');
  const raw = localTagTexts(body, 'v')[0] ?? localTagTexts(body, 't')[0] ?? '';
  if (type === 's') {
    const index = Number.parseInt(raw, 10);
    return Number.isSafeInteger(index) ? shared[index] ?? '' : '';
  }
  if (type === 'b') return raw === '1';
  if (type === 'e' || type === 'str') return raw;
  if (raw === '') return null;
  const number = Number(raw);
  return Number.isFinite(number) ? number : raw;
}

function valueType(type: string | undefined, value: unknown, formula: string | undefined): string {
  if (formula) return 'formula';
  if (type === 'e') return 'error';
  if (typeof value === 'boolean') return 'boolean';
  if (typeof value === 'number') return 'number';
  if (value === null) return 'blank';
  return 'string';
}

function boundedValue(value: unknown, remaining: number): { value: unknown; chars: number; truncated: boolean } {
  if (typeof value !== 'string') return { value, chars: 0, truncated: false };
  const allowed = Math.max(0, Math.min(2_000, remaining));
  return {
    value: value.slice(0, allowed),
    chars: Math.min(value.length, allowed),
    truncated: value.length > allowed
  };
}

export async function readSpreadsheetRange(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string') throw new DocumentError('INVALID_ARGUMENT', 'inputPath is required.');
  const source = await resolveWorkspaceFile(args.inputPath);
  if (path.extname(source.relativePath).toLowerCase() !== '.xlsx') {
    throw new DocumentError('UNSUPPORTED_FORMAT', 'spreadsheet_read_range requires an .xlsx input file.');
  }
  if (source.size > MAX_INPUT_BYTES) {
    throw new DocumentError('FILE_TOO_LARGE', `The input file exceeds the ${MAX_INPUT_BYTES} byte limit.`);
  }
  const requestedSheet = sheetName(args.sheet);
  const requestedRange = parseRange(args.range);
  const maxCells = args.maxCells === undefined ? MAX_RETURNED_CELLS : args.maxCells;
  if (!Number.isInteger(maxCells) || (maxCells as number) < 1 || (maxCells as number) > MAX_RETURNED_CELLS) {
    throw new DocumentError('INVALID_ARGUMENT', `maxCells must be an integer between 1 and ${MAX_RETURNED_CELLS}.`);
  }

  const parts = readParts(await readFile(source.absolutePath));
  const sheetXml = xml(parts, worksheetPath(parts, requestedSheet));
  if (!sheetXml) throw new DocumentError('INVALID_DOCUMENT', 'The requested worksheet is missing or too large.');
  const shared = sharedStrings(parts);
  const cells: Array<Record<string, unknown>> = [];
  let populatedCellCount = 0;
  let returnedChars = 0;
  let truncated = false;
  for (const match of sheetXml.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c(?:\s([^>]*))?>([\s\S]*?)<\/(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c>/gi)) {
    const attributes = match[1] ?? '';
    const reference = attribute(attributes, 'r');
    if (!reference) continue;
    let coordinate: CellCoordinate;
    try {
      coordinate = parseCellAddress(reference, 'cell reference');
    } catch {
      throw new DocumentError('INVALID_DOCUMENT', 'The worksheet contains an invalid cell reference.');
    }
    if (
      coordinate.column < requestedRange.start.column
      || coordinate.column > requestedRange.end.column
      || coordinate.row < requestedRange.start.row
      || coordinate.row > requestedRange.end.row
    ) continue;
    populatedCellCount += 1;
    if (cells.length >= (maxCells as number) || returnedChars >= MAX_RETURNED_TEXT) {
      truncated = true;
      continue;
    }
    const body = match[2] ?? '';
    const type = attribute(attributes, 't');
    const formulaText = localTagTexts(body, 'f')[0];
    const formula = formulaText ? `=${formulaText}` : undefined;
    const bounded = boundedValue(cellValue(body, type, shared), MAX_RETURNED_TEXT - returnedChars);
    returnedChars += bounded.chars + (formula?.length ?? 0);
    if (returnedChars > MAX_RETURNED_TEXT) {
      truncated = true;
      continue;
    }
    cells.push({
      address: reference.toUpperCase(),
      row: coordinate.row,
      column: coordinate.column,
      type: valueType(type, bounded.value, formula),
      value: bounded.value,
      ...(formula ? { formula } : {}),
      ...(bounded.truncated ? { valueTruncated: true } : {})
    });
    if (bounded.truncated) truncated = true;
  }

  return {
    ok: true,
    operation: 'spreadsheet_read_range',
    source: {
      relativePath: source.relativePath,
      size: source.size,
      sha256: await sha256File(source.absolutePath)
    },
    sheet: requestedSheet,
    range: {
      address: requestedRange.normalized,
      rows: requestedRange.rows,
      columns: requestedRange.columns,
      cells: requestedRange.area
    },
    populatedCellCount,
    returnedCellCount: cells.length,
    truncated,
    cells
  };
}

type WriteCell = string | number | boolean | null | { formula: string };

function writeCell(value: unknown, row: number, column: number): WriteCell {
  if (value === null || typeof value === 'boolean') return value;
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new DocumentError('INVALID_ARGUMENT', `values[${row}][${column}] must be finite.`);
    return value;
  }
  if (typeof value === 'string') {
    if (value.length > 20_000) throw new DocumentError('INVALID_ARGUMENT', `values[${row}][${column}] is too long.`);
    return value;
  }
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    const item = value as Record<string, unknown>;
    if (Object.keys(item).length === 1 && typeof item.formula === 'string' && /^=.{1,7999}$/s.test(item.formula)) {
      return { formula: item.formula };
    }
  }
  throw new DocumentError('INVALID_ARGUMENT', `values[${row}][${column}] must be a primitive, null, or { formula }.`);
}

export async function writeSpreadsheetRange(args: Record<string, unknown>): Promise<Record<string, unknown>> {
  if (typeof args.inputPath !== 'string' || typeof args.outputName !== 'string') {
    throw new DocumentError('INVALID_ARGUMENT', 'inputPath and outputName are required.');
  }
  const requestedSheet = sheetName(args.sheet);
  const start = parseCellAddress(args.startCell, 'startCell');
  if (!Array.isArray(args.values) || args.values.length < 1 || args.values.length > 100) {
    throw new DocumentError('INVALID_ARGUMENT', 'values must contain between 1 and 100 rows.');
  }
  const rows = args.values.map((row, rowIndex) => {
    if (!Array.isArray(row) || row.length < 1 || row.length > 100) {
      throw new DocumentError('INVALID_ARGUMENT', `values[${rowIndex}] must contain between 1 and 100 cells.`);
    }
    return row.map((cell, columnIndex) => writeCell(cell, rowIndex, columnIndex));
  });
  const columns = rows[0]?.length ?? 0;
  if (rows.some((row) => row.length !== columns)) {
    throw new DocumentError('INVALID_ARGUMENT', 'values must be a rectangular matrix.');
  }
  if (rows.length * columns > MAX_RANGE_AREA) {
    throw new DocumentError('INVALID_ARGUMENT', `values exceeds the ${MAX_RANGE_AREA} cell limit.`);
  }
  const end = { column: start.column + columns - 1, row: start.row + rows.length - 1 };
  if (end.column > 16_384 || end.row > 1_048_576) {
    throw new DocumentError('INVALID_ARGUMENT', 'The written range exceeds XLSX worksheet limits.');
  }

  const operations: Array<Record<string, unknown>> = [];
  for (const [rowIndex, row] of rows.entries()) {
    for (const [columnIndex, cell] of row.entries()) {
      if (cell === null) continue;
      const address = cellAddress({ column: start.column + columnIndex, row: start.row + rowIndex });
      operations.push({
        type: 'spreadsheet_set_cell',
        sheet: requestedSheet,
        address,
        ...(typeof cell === 'object' ? { formula: cell.formula } : { value: cell })
      });
    }
  }
  if (operations.length < 1) {
    throw new DocumentError('INVALID_ARGUMENT', 'values does not contain any cells to write; null means leave unchanged.');
  }
  if (operations.length > MAX_WRITE_CELLS) {
    throw new DocumentError('INVALID_ARGUMENT', `values contains more than ${MAX_WRITE_CELLS} cells to write.`);
  }
  const result = await editOfficeArtifact({
    inputPath: args.inputPath,
    outputName: args.outputName,
    operations
  }, MAX_WRITE_CELLS);
  return {
    ...result,
    operation: 'spreadsheet_write_range',
    sheet: requestedSheet,
    range: `${cellAddress(start)}:${cellAddress(end)}`,
    rows: rows.length,
    columns,
    writtenCells: operations.length,
    nullCellsSkipped: rows.length * columns - operations.length
  };
}
