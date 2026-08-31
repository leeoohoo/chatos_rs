import { unzipSync, type UnzipFileInfo } from 'fflate';
import {
  MAX_INSPECTED_XML_BYTES,
  MAX_OOXML_ENTRIES,
  MAX_OOXML_UNPACKED_BYTES
} from '../constants.js';
import { DocumentError } from '../errors.js';
import { attributeValues, countTags, firstTagText } from './xml.js';

type OoxmlKind = 'docx' | 'xlsx' | 'pptx';

const TEXT_DECODER = new TextDecoder('utf-8', { fatal: false });
const ALWAYS_READ = new Set(['[Content_Types].xml', 'docProps/core.xml', 'docProps/app.xml']);

function safeArchiveName(name: string): boolean {
  if (!name || name.includes('\\') || name.startsWith('/') || /^[A-Za-z]:/.test(name)) return false;
  return !name.split('/').some((component) => !component || component === '.' || component === '..');
}

function shouldRead(name: string): boolean {
  return ALWAYS_READ.has(name)
    || name === 'word/document.xml'
    || name === 'xl/workbook.xml'
    || name === 'ppt/presentation.xml';
}

function detectKind(names: string[]): OoxmlKind {
  if (names.includes('word/document.xml')) return 'docx';
  if (names.includes('xl/workbook.xml')) return 'xlsx';
  if (names.includes('ppt/presentation.xml')) return 'pptx';
  throw new DocumentError('INVALID_DOCUMENT', 'The ZIP file is not a supported OOXML document.');
}

function extensionKind(extension: string): OoxmlKind | undefined {
  if (extension === '.docx') return 'docx';
  if (extension === '.xlsx') return 'xlsx';
  if (extension === '.pptx') return 'pptx';
  return undefined;
}

function xml(entries: Record<string, Uint8Array>, name: string): string | undefined {
  const bytes = entries[name];
  return bytes ? TEXT_DECODER.decode(bytes) : undefined;
}

function coreMetadata(coreXml: string | undefined): Record<string, string> {
  const values: Array<[string, string | undefined]> = [
    ['title', firstTagText(coreXml, 'dc:title')],
    ['subject', firstTagText(coreXml, 'dc:subject')],
    ['creator', firstTagText(coreXml, 'dc:creator')],
    ['lastModifiedBy', firstTagText(coreXml, 'cp:lastModifiedBy')],
    ['created', firstTagText(coreXml, 'dcterms:created')],
    ['modified', firstTagText(coreXml, 'dcterms:modified')]
  ];
  return Object.fromEntries(values.filter((entry): entry is [string, string] => Boolean(entry[1])));
}

export function inspectOoxml(data: Uint8Array, extension: string): {
  format: OoxmlKind;
  mimeType: string;
  metadata: Record<string, string>;
  structure: Record<string, unknown>;
  archive: { entries: number; declaredUnpackedBytes: number };
} {
  let entryCount = 0;
  let declaredUnpackedBytes = 0;
  const names: string[] = [];
  let entries: Record<string, Uint8Array>;

  try {
    entries = unzipSync(data, {
      filter(file: UnzipFileInfo): boolean {
        entryCount += 1;
        if (entryCount > MAX_OOXML_ENTRIES) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The OOXML archive contains too many entries.');
        }
        if (!safeArchiveName(file.name)) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The OOXML archive contains an unsafe entry path.');
        }
        names.push(file.name);
        declaredUnpackedBytes += file.originalSize;
        if (declaredUnpackedBytes > MAX_OOXML_UNPACKED_BYTES) {
          throw new DocumentError('UNSAFE_ARCHIVE', 'The OOXML archive expands beyond the allowed size.');
        }
        return shouldRead(file.name) && file.originalSize <= MAX_INSPECTED_XML_BYTES;
      }
    });
  } catch (error) {
    if (error instanceof DocumentError) throw error;
    throw new DocumentError('INVALID_DOCUMENT', 'The OOXML ZIP container could not be parsed.');
  }

  if (!names.includes('[Content_Types].xml')) {
    throw new DocumentError('INVALID_DOCUMENT', 'The OOXML document is missing [Content_Types].xml.');
  }
  const kind = detectKind(names);
  const expected = extensionKind(extension);
  if (expected && expected !== kind) {
    throw new DocumentError('FORMAT_MISMATCH', `The file extension indicates ${expected}, but the content is ${kind}.`);
  }

  const metadata = coreMetadata(xml(entries, 'docProps/core.xml'));
  let structure: Record<string, unknown>;
  let mimeType: string;

  if (kind === 'docx') {
    const documentXml = xml(entries, 'word/document.xml');
    if (!documentXml) throw new DocumentError('INVALID_DOCUMENT', 'The DOCX document body is missing or too large to inspect.');
    structure = {
      paragraphs: countTags(documentXml, 'w:p'),
      tables: countTags(documentXml, 'w:tbl'),
      embeddedMediaFiles: names.filter((name) => name.startsWith('word/media/')).length
    };
    mimeType = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';
  } else if (kind === 'xlsx') {
    const workbookXml = xml(entries, 'xl/workbook.xml');
    if (!workbookXml) throw new DocumentError('INVALID_DOCUMENT', 'The XLSX workbook definition is missing or too large to inspect.');
    const sheetNames = attributeValues(workbookXml, 'sheet', 'name');
    structure = {
      worksheets: sheetNames.length,
      sheetNames: sheetNames.slice(0, 200),
      worksheetXmlFiles: names.filter((name) => /^xl\/worksheets\/sheet\d+\.xml$/i.test(name)).length,
      embeddedMediaFiles: names.filter((name) => name.startsWith('xl/media/')).length
    };
    mimeType = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
  } else {
    const slideFiles = names.filter((name) => /^ppt\/slides\/slide\d+\.xml$/i.test(name));
    structure = {
      slides: slideFiles.length,
      notes: names.filter((name) => /^ppt\/notesSlides\/notesSlide\d+\.xml$/i.test(name)).length,
      embeddedMediaFiles: names.filter((name) => name.startsWith('ppt/media/')).length
    };
    mimeType = 'application/vnd.openxmlformats-officedocument.presentationml.presentation';
  }

  return {
    format: kind,
    mimeType,
    metadata,
    structure,
    archive: { entries: entryCount, declaredUnpackedBytes }
  };
}
