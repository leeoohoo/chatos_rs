import { DocumentError } from '../errors.js';

export type OfficeFormat = 'docx' | 'xlsx' | 'pptx';

type JsonObject = Record<string, unknown>;

function stringValue(value: unknown, field: string, maxLength = 20_000): string {
  if (typeof value !== 'string' || value.length > maxLength) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be a string of at most ${maxLength} characters.`);
  }
  return value;
}

function positiveInteger(value: unknown, field: string, max: number): number {
  if (!Number.isInteger(value) || (value as number) < 1 || (value as number) > max) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be an integer between 1 and ${max}.`);
  }
  return value as number;
}

function cellAddress(value: unknown): string {
  const address = stringValue(value, 'address', 20).toUpperCase();
  if (!/^[A-Z]{1,3}[1-9][0-9]{0,6}$/.test(address)) {
    throw new DocumentError('INVALID_ARGUMENT', 'Spreadsheet cell addresses must use A1 notation.');
  }
  return address;
}

function sheetName(value: unknown): string {
  const name = stringValue(value, 'sheet', 31);
  if (!name || /[\\/*?:[\]]/.test(name)) {
    throw new DocumentError('INVALID_ARGUMENT', 'The spreadsheet sheet name is invalid.');
  }
  return name;
}

function hexColor(value: unknown, field: string): string {
  const color = stringValue(value, field, 6).toUpperCase();
  if (!/^[0-9A-F]{6}$/.test(color)) throw new DocumentError('INVALID_ARGUMENT', `${field} must be a 6-digit hex color.`);
  return color;
}

function dimension(value: unknown, field: string): string {
  const text = stringValue(value, field, 24);
  if (!/^[0-9]+(?:\.[0-9]+)?(?:cm|mm|in|pt|px)$/.test(text)) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must be a positive dimension such as 2cm or 24pt.`);
  }
  return text;
}

function spreadsheetValue(value: unknown, field: string): string | number | boolean {
  if (typeof value === 'string') return stringValue(value, field, 20_000);
  if (typeof value === 'boolean') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  throw new DocumentError('INVALID_ARGUMENT', `${field} must be a string, finite number, or boolean.`);
}

function optionalBoolean(value: unknown, field: string): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== 'boolean') throw new DocumentError('INVALID_ARGUMENT', `${field} must be a boolean.`);
  return value;
}

function wordTableData(value: unknown, field: string): string {
  if (!Array.isArray(value) || value.length < 1 || value.length > 50) {
    throw new DocumentError('INVALID_ARGUMENT', `${field} must contain between 1 and 50 rows.`);
  }
  let columns: number | undefined;
  let totalCharacters = 0;
  const encodedRows = value.map((row, rowIndex) => {
    if (!Array.isArray(row) || row.length < 1 || row.length > 20) {
      throw new DocumentError('INVALID_ARGUMENT', `${field}[${rowIndex}] must contain between 1 and 20 cells.`);
    }
    columns ??= row.length;
    if (row.length !== columns) throw new DocumentError('INVALID_ARGUMENT', `${field} must be a rectangular matrix.`);
    return row.map((cell, columnIndex) => {
      if (!['string', 'number', 'boolean'].includes(typeof cell)) {
        throw new DocumentError('INVALID_ARGUMENT', `${field}[${rowIndex}][${columnIndex}] must be a primitive value.`);
      }
      if (typeof cell === 'number' && !Number.isFinite(cell)) {
        throw new DocumentError('INVALID_ARGUMENT', `${field}[${rowIndex}][${columnIndex}] must be finite.`);
      }
      const text = String(cell);
      totalCharacters += text.length;
      if (text.length > 5_000 || totalCharacters > 100_000) {
        throw new DocumentError('INVALID_ARGUMENT', `${field} contains too much text.`);
      }
      return /[,;"\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
    }).join(',');
  });
  return encodedRows.join(';');
}

export function translateOperations(format: OfficeFormat, operations: unknown, maximum = 500): JsonObject[] {
  if (!Array.isArray(operations) || operations.length > maximum) {
    throw new DocumentError('INVALID_ARGUMENT', `operations must be an array containing at most ${maximum} items.`);
  }
  return operations.map((operation, index) => {
    if (!operation || typeof operation !== 'object' || Array.isArray(operation)) {
      throw new DocumentError('INVALID_ARGUMENT', `operations[${index}] must be an object.`);
    }
    const item = operation as JsonObject;
    const type = stringValue(item.type, `operations[${index}].type`, 64);
    if (type === 'word_add_paragraph' && format === 'docx') {
      return {
        command: 'add',
        parent: '/body',
        type: 'paragraph',
        props: { text: stringValue(item.text, `operations[${index}].text`) }
      };
    }
    if (type === 'word_replace_text' && format === 'docx') {
      return {
        command: 'set',
        path: '/body',
        props: {
          find: stringValue(item.find, `operations[${index}].find`, 2_000),
          replace: stringValue(item.replace, `operations[${index}].replace`, 20_000)
        }
      };
    }
    if (type === 'word_add_heading' && format === 'docx') {
      const level = positiveInteger(item.level, `operations[${index}].level`, 6);
      return {
        command: 'add',
        parent: '/body',
        type: 'paragraph',
        props: {
          text: stringValue(item.text, `operations[${index}].text`),
          style: `Heading${level}`
        }
      };
    }
    if (type === 'word_add_list_item' && format === 'docx') {
      const ordered = optionalBoolean(item.ordered, `operations[${index}].ordered`) ?? false;
      const level = item.level === undefined ? 0 : positiveInteger((item.level as number) + 1, `operations[${index}].level`, 9) - 1;
      return {
        command: 'add',
        parent: '/body',
        type: 'paragraph',
        props: {
          text: stringValue(item.text, `operations[${index}].text`),
          listStyle: ordered ? 'ordered' : 'bullet',
          numLevel: level
        }
      };
    }
    if (type === 'word_add_table' && format === 'docx') {
      const style = item.style === undefined ? 'medium2' : stringValue(item.style, `operations[${index}].style`, 16);
      if (!['medium1', 'medium2', 'medium3', 'medium4', 'light1', 'light2', 'light3', 'dark1', 'dark2', 'none'].includes(style)) {
        throw new DocumentError('INVALID_ARGUMENT', 'Word table style is not allowed.');
      }
      return {
        command: 'add',
        parent: '/body',
        type: 'table',
        props: { data: wordTableData(item.rows, `operations[${index}].rows`), style }
      };
    }
    if (type === 'word_set_paragraph_format' && format === 'docx') {
      const paragraph = positiveInteger(item.paragraph, `operations[${index}].paragraph`, 100_000);
      const props: JsonObject = {};
      if (item.align !== undefined) {
        const align = stringValue(item.align, `operations[${index}].align`, 16);
        if (!['left', 'center', 'right', 'justify'].includes(align)) {
          throw new DocumentError('INVALID_ARGUMENT', 'Word paragraph alignment is not allowed.');
        }
        props.align = align;
      }
      if (item.style !== undefined) props.style = stringValue(item.style, `operations[${index}].style`, 128);
      const bold = optionalBoolean(item.bold, `operations[${index}].bold`);
      const italic = optionalBoolean(item.italic, `operations[${index}].italic`);
      if (bold !== undefined) props.bold = bold;
      if (italic !== undefined) props.italic = italic;
      if (Object.keys(props).length === 0) {
        throw new DocumentError('INVALID_ARGUMENT', 'word_set_paragraph_format requires at least one format property.');
      }
      return { command: 'set', path: `/body/p[${paragraph}]`, props };
    }
    if (type === 'spreadsheet_set_cell' && format === 'xlsx') {
      const sheet = sheetName(item.sheet ?? 'Sheet1');
      const value = item.formula !== undefined
        ? stringValue(item.formula, `operations[${index}].formula`, 8_000)
        : spreadsheetValue(item.value, `operations[${index}].value`);
      return {
        command: 'set',
        path: `/${sheet}/${cellAddress(item.address)}`,
        props: item.formula !== undefined ? { formula: value } : { value }
      };
    }
    if (type === 'spreadsheet_add_sheet' && format === 'xlsx') {
      const props: JsonObject = { name: sheetName(item.name) };
      if (item.tabColor !== undefined) props.tabColor = hexColor(item.tabColor, `operations[${index}].tabColor`);
      const hidden = optionalBoolean(item.hidden, `operations[${index}].hidden`);
      if (hidden !== undefined) props.hidden = hidden;
      return { command: 'add', parent: '/', type: 'sheet', props };
    }
    if (type === 'spreadsheet_rename_sheet' && format === 'xlsx') {
      return {
        command: 'set',
        path: `/${sheetName(item.sheet)}`,
        props: { name: sheetName(item.name) }
      };
    }
    if (type === 'spreadsheet_delete_sheet' && format === 'xlsx') {
      return { command: 'remove', path: `/${sheetName(item.sheet)}` };
    }
    if (type === 'spreadsheet_set_sheet_properties' && format === 'xlsx') {
      const props: JsonObject = {};
      const hidden = optionalBoolean(item.hidden, `operations[${index}].hidden`);
      if (hidden !== undefined) props.hidden = hidden;
      if (item.tabColor !== undefined) props.tabColor = hexColor(item.tabColor, `operations[${index}].tabColor`);
      if (item.freeze !== undefined) {
        const freeze = stringValue(item.freeze, `operations[${index}].freeze`, 20);
        if (freeze !== 'none' && !/^[A-Za-z]{1,3}[1-9][0-9]{0,6}$/.test(freeze)) {
          throw new DocumentError('INVALID_ARGUMENT', 'freeze must be an A1 cell reference or none.');
        }
        props.freeze = freeze.toUpperCase();
      }
      if (Object.keys(props).length === 0) {
        throw new DocumentError('INVALID_ARGUMENT', 'spreadsheet_set_sheet_properties requires at least one property.');
      }
      return { command: 'set', path: `/${sheetName(item.sheet)}`, props };
    }
    if (type === 'presentation_add_slide' && format === 'pptx') {
      const props: JsonObject = {};
      if (item.title !== undefined) props.title = stringValue(item.title, `operations[${index}].title`, 2_000);
      if (item.background !== undefined) {
        const color = stringValue(item.background, `operations[${index}].background`, 6).toUpperCase();
        if (!/^[0-9A-F]{6}$/.test(color)) throw new DocumentError('INVALID_ARGUMENT', 'Slide background must be a 6-digit hex color.');
        props.background = color;
      }
      return { command: 'add', parent: '/', type: 'slide', props };
    }
    if (type === 'presentation_add_textbox' && format === 'pptx') {
      const slide = positiveInteger(item.slide, `operations[${index}].slide`, 10_000);
      return {
        command: 'add',
        parent: `/slide[${slide}]`,
        type: 'shape',
        props: {
          text: stringValue(item.text, `operations[${index}].text`),
          x: dimension(item.x, `operations[${index}].x`),
          y: dimension(item.y, `operations[${index}].y`),
          width: dimension(item.width, `operations[${index}].width`),
          height: dimension(item.height, `operations[${index}].height`)
        }
      };
    }
    if (type === 'presentation_set_text' && format === 'pptx') {
      const slide = positiveInteger(item.slide, `operations[${index}].slide`, 10_000);
      const shape = positiveInteger(item.shape, `operations[${index}].shape`, 100_000);
      return {
        command: 'set',
        path: `/slide[${slide}]/shape[${shape}]`,
        props: { text: stringValue(item.text, `operations[${index}].text`) }
      };
    }
    if (type === 'presentation_delete_slide' && format === 'pptx') {
      const slide = positiveInteger(item.slide, `operations[${index}].slide`, 10_000);
      return { command: 'remove', path: `/slide[${slide}]` };
    }
    if (type === 'presentation_move_slide' && format === 'pptx') {
      const slide = positiveInteger(item.slide, `operations[${index}].slide`, 10_000);
      const position = positiveInteger(item.position, `operations[${index}].position`, 10_000);
      return { command: 'move', path: `/slide[${slide}]`, index: position - 1 };
    }
    if (type === 'presentation_set_slide_properties' && format === 'pptx') {
      const slide = positiveInteger(item.slide, `operations[${index}].slide`, 10_000);
      const props: JsonObject = {};
      if (item.background !== undefined) props.background = hexColor(item.background, `operations[${index}].background`);
      const hidden = optionalBoolean(item.hidden, `operations[${index}].hidden`);
      if (hidden !== undefined) props.hidden = hidden;
      if (item.name !== undefined) props.name = stringValue(item.name, `operations[${index}].name`, 500);
      if (Object.keys(props).length === 0) {
        throw new DocumentError('INVALID_ARGUMENT', 'presentation_set_slide_properties requires at least one property.');
      }
      return { command: 'set', path: `/slide[${slide}]`, props };
    }
    throw new DocumentError('INVALID_ARGUMENT', `Operation ${type} is not allowed for ${format}.`);
  });
}
