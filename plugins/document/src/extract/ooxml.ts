import { unzipSync, type UnzipFileInfo } from 'fflate';
import path from 'node:path';
import {
  MAX_EXTRACTED_XML_BYTES,
  MAX_EXTRACT_SECTIONS,
  MAX_OOXML_ENTRIES,
  MAX_OOXML_UNPACKED_BYTES
} from '../constants.js';
import { DocumentError } from '../errors.js';
import { attributeValues, decodeXmlText, tagTexts } from '../inspect/xml.js';

export interface TextSection {
  kind: 'document' | 'sheet' | 'slide' | 'page';
  index: number;
  name?: string;
  text: string;
}

const DECODER = new TextDecoder('utf-8', { fatal: false });

function safeArchiveName(name: string): boolean {
  if (!name || name.includes('\\') || name.startsWith('/') || /^[A-Za-z]:/.test(name)) return false;
  return !name.split('/').some((component) => !component || component === '.' || component === '..');
}

function wantedPart(name: string): boolean {
  return name === 'word/document.xml'
    || name === 'xl/workbook.xml'
    || name === 'xl/_rels/workbook.xml.rels'
    || name === 'xl/sharedStrings.xml'
    || /^xl\/worksheets\/sheet\d+\.xml$/i.test(name)
    || name === 'ppt/presentation.xml'
    || name === 'ppt/_rels/presentation.xml.rels'
    || /^ppt\/slides\/slide\d+\.xml$/i.test(name);
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
          throw new DocumentError('UNSAFE_ARCHIVE', 'The OOXML archive exceeds extraction safety limits.');
        }
        if (!safeArchiveName(file.name)) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The OOXML archive contains an unsafe entry path.');
        }
        return wantedPart(file.name) && file.originalSize <= MAX_EXTRACTED_XML_BYTES;
      }
    });
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw new DocumentError('INVALID_DOCUMENT', 'The OOXML document could not be opened for text extraction.');
  }
}

function xml(parts: Record<string, Uint8Array>, name: string): string | undefined {
  const bytes = parts[name];
  return bytes ? DECODER.decode(bytes) : undefined;
}

function numberedParts(parts: Record<string, Uint8Array>, expression: RegExp): Array<[number, string]> {
  return Object.keys(parts)
    .flatMap((name): Array<[number, string]> => {
      const match = name.match(expression);
      return match?.[1] ? [[Number.parseInt(match[1], 10), name]] : [];
    })
    .sort((left, right) => left[0] - right[0])
    .slice(0, MAX_EXTRACT_SECTIONS);
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
  ).filter(Boolean);
}

function relationshipTargets(source: string | undefined, base: string): Map<string, string> {
  const targets = new Map<string, string>();
  if (!source) return targets;
  for (const match of source.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?Relationship\s+([^>]*?)(?:\/?>)/gi)) {
    const attributes = match[1] ?? '';
    if (attribute(attributes, 'TargetMode') === 'External') continue;
    const id = attribute(attributes, 'Id');
    const target = attribute(attributes, 'Target');
    if (!id || !target) continue;
    const normalized = target.startsWith('/')
      ? path.posix.normalize(target.slice(1))
      : path.posix.normalize(path.posix.join(base, target));
    if (!normalized.startsWith(`${base}/`) || normalized.includes('..')) continue;
    targets.set(id, normalized);
  }
  return targets;
}

function extractWord(parts: Record<string, Uint8Array>): TextSection[] {
  const documentXml = xml(parts, 'word/document.xml');
  if (!documentXml) throw new DocumentError('INVALID_DOCUMENT', 'The DOCX document body is missing or too large.');
  const paragraphs = Array.from(documentXml.matchAll(/<w:p(?:\s[^>]*)?>([\s\S]*?)<\/w:p>/gi), (match) => {
    const body = match[1] ?? '';
    return tagTexts(body, 'w:t').join('');
  }).filter(Boolean);
  return [{ kind: 'document', index: 1, text: paragraphs.join('\n') }];
}

function sharedStrings(parts: Record<string, Uint8Array>): string[] {
  const sharedXml = xml(parts, 'xl/sharedStrings.xml');
  if (!sharedXml) return [];
  return Array.from(sharedXml.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?si(?:\s[^>]*)?>([\s\S]*?)<\/(?:[A-Za-z_][A-Za-z0-9_.-]*:)?si>/gi), (match) =>
    localTagTexts(match[1] ?? '', 't').join('')
  );
}

function cellText(cellXml: string, type: string | undefined, shared: string[]): string {
  if (type === 'inlineStr') return localTagTexts(cellXml, 't').join('');
  const raw = localTagTexts(cellXml, 'v')[0] ?? localTagTexts(cellXml, 't')[0] ?? '';
  if (type === 's') {
    const index = Number.parseInt(raw, 10);
    return Number.isSafeInteger(index) ? shared[index] ?? '' : '';
  }
  if (type === 'b') return raw === '1' ? 'TRUE' : 'FALSE';
  return raw;
}

function extractSpreadsheet(parts: Record<string, Uint8Array>): TextSection[] {
  const workbookXml = xml(parts, 'xl/workbook.xml');
  if (!workbookXml) throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook definition is missing or too large.');
  const shared = sharedStrings(parts);
  const relationships = relationshipTargets(xml(parts, 'xl/_rels/workbook.xml.rels'), 'xl');
  const ordered = Array.from(
    workbookXml.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?sheet\s+([^>]*?)(?:\/?>)/gi),
    (match) => ({
      name: attribute(match[1] ?? '', 'name'),
      target: relationships.get(attribute(match[1] ?? '', 'r:id') ?? '')
    })
  ).filter((entry): entry is { name: string; target: string } => Boolean(entry.name && entry.target && parts[entry.target]));
  const sheets = ordered.length > 0
    ? ordered.map((entry, index) => ({ index: index + 1, name: entry.name, target: entry.target }))
    : numberedParts(parts, /^xl\/worksheets\/sheet(\d+)\.xml$/i).map(([index, target], position) => ({
        index,
        name: attributeValues(workbookXml, 'sheet', 'name')[position] ?? `Sheet${index}`,
        target
      }));
  return sheets.slice(0, MAX_EXTRACT_SECTIONS).map((sheet) => {
    const sheetXml = xml(parts, sheet.target) ?? '';
    const lines = Array.from(sheetXml.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c(?:\s([^>]*))?>([\s\S]*?)<\/(?:[A-Za-z_][A-Za-z0-9_.-]*:)?c>/gi), (match) => {
      const attributes = match[1] ?? '';
      const reference = attributes.match(/\br=(?:"([^"]+)"|'([^']+)')/i)?.slice(1).find(Boolean) ?? '';
      const type = attributes.match(/\bt=(?:"([^"]+)"|'([^']+)')/i)?.slice(1).find(Boolean);
      const value = cellText(match[2] ?? '', type, shared);
      return value ? `${reference}\t${value}` : '';
    }).filter(Boolean);
    return {
      kind: 'sheet' as const,
      index: sheet.index,
      name: sheet.name,
      text: lines.join('\n')
    };
  });
}

function extractPresentation(parts: Record<string, Uint8Array>): TextSection[] {
  const presentation = xml(parts, 'ppt/presentation.xml');
  const relationships = relationshipTargets(xml(parts, 'ppt/_rels/presentation.xml.rels'), 'ppt');
  const ordered = presentation
    ? Array.from(
        presentation.matchAll(/<(?:[A-Za-z_][A-Za-z0-9_.-]*:)?sldId\s+([^>]*?)(?:\/?>)/gi),
        (match) => relationships.get(attribute(match[1] ?? '', 'r:id') ?? '')
      ).filter((target): target is string => Boolean(target && parts[target]))
    : [];
  const slides = ordered.length > 0
    ? ordered.map((target, index): [number, string] => [index + 1, target])
    : numberedParts(parts, /^ppt\/slides\/slide(\d+)\.xml$/i);
  return slides.slice(0, MAX_EXTRACT_SECTIONS).map(([index, name]) => ({
    kind: 'slide' as const,
    index,
    text: tagTexts(xml(parts, name), 'a:t').join('\n')
  }));
}

export function extractOoxmlText(data: Uint8Array, extension: string): TextSection[] {
  const parts = readParts(data);
  if (extension === '.docx') return extractWord(parts);
  if (extension === '.xlsx') return extractSpreadsheet(parts);
  if (extension === '.pptx') return extractPresentation(parts);
  throw new DocumentError('UNSUPPORTED_FORMAT', 'Unsupported OOXML format.');
}

export function normalizeText(value: string): string {
  return decodeXmlText(value).replace(/\r\n?/g, '\n').replace(/[ \t]+\n/g, '\n').trim();
}
