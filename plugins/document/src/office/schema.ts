const lengthProperty = {
  type: 'string',
  pattern: '^[0-9]+(?:\\.[0-9]+)?(?:cm|mm|in|pt)$',
  maxLength: 24
} as const;

const fontSizeProperty = {
  oneOf: [
    { type: 'number', minimum: 1, maximum: 96 },
    { type: 'string', pattern: '^[0-9]+(?:\\.[0-9]+)?pt$', maxLength: 12 }
  ]
} as const;

const colorProperty = { type: 'string', pattern: '^#?[0-9A-Fa-f]{6}$' } as const;
const alignmentProperty = {
  type: 'string',
  enum: ['left', 'center', 'right', 'justify', 'distribute']
} as const;

const wordParagraphFormattingProperties = {
  align: alignmentProperty,
  style: { type: 'string', minLength: 1, maxLength: 128 },
  bold: { type: 'boolean' },
  italic: { type: 'boolean' },
  font: { type: 'string', minLength: 1, maxLength: 128 },
  size: fontSizeProperty,
  color: colorProperty,
  fill: colorProperty,
  lineSpacing: {
    type: 'string',
    pattern: '^[0-9]+(?:\\.[0-9]+)?(?:x|pt)$',
    maxLength: 16
  },
  lineRule: { type: 'string', enum: ['auto', 'exact', 'atLeast'] },
  spaceBefore: lengthProperty,
  spaceAfter: lengthProperty,
  firstLineIndent: lengthProperty,
  hangingIndent: lengthProperty,
  indent: lengthProperty,
  rightIndent: lengthProperty,
  keepNext: { type: 'boolean' },
  keepLines: { type: 'boolean' },
  widowControl: { type: 'boolean' },
  pageBreakBefore: { type: 'boolean', default: false }
} as const;

const wordTextFormattingProperties = {
  bold: { type: 'boolean' },
  italic: { type: 'boolean' },
  font: { type: 'string', minLength: 1, maxLength: 128 },
  size: fontSizeProperty,
  color: colorProperty,
  underline: { type: 'boolean' }
} as const;

export const officeOperationSchema = {
  oneOf: [
    {
      type: 'object',
      properties: {
        type: { const: 'word_set_document_layout' },
        pageSize: { type: 'string', enum: ['a4', 'letter'] },
        orientation: { type: 'string', enum: ['portrait', 'landscape'] },
        marginTop: lengthProperty,
        marginBottom: lengthProperty,
        marginLeft: lengthProperty,
        marginRight: lengthProperty,
        marginHeader: lengthProperty,
        marginFooter: lengthProperty,
        columns: { type: 'integer', minimum: 1, maximum: 4 }
      },
      required: ['type'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_define_style' },
        id: { type: 'string', pattern: '^[A-Za-z][A-Za-z0-9_-]{0,63}$' },
        name: { type: 'string', minLength: 1, maxLength: 128 },
        styleType: { type: 'string', enum: ['paragraph', 'character'], default: 'paragraph' },
        basedOn: { type: 'string', minLength: 1, maxLength: 128 },
        next: { type: 'string', minLength: 1, maxLength: 128 },
        ...wordParagraphFormattingProperties
      },
      required: ['type', 'id', 'name'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_paragraph' },
        text: { type: 'string', maxLength: 20000 },
        ...wordParagraphFormattingProperties
      },
      required: ['type', 'text'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_replace_text' },
        find: { type: 'string', minLength: 1, maxLength: 2000 },
        replace: { type: 'string', maxLength: 20000 }
      },
      required: ['type', 'find', 'replace'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_heading' },
        level: { type: 'integer', minimum: 1, maximum: 6 },
        text: { type: 'string', maxLength: 20000 },
        ...wordParagraphFormattingProperties
      },
      required: ['type', 'level', 'text'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_list_item' },
        text: {
          type: 'string',
          maxLength: 20000,
          description: 'Plain item text only. Do not include a bullet, dash, or numeric list marker.'
        },
        ordered: { type: 'boolean', default: false },
        level: { type: 'integer', minimum: 0, maximum: 8, default: 0 },
        ...wordParagraphFormattingProperties
      },
      required: ['type', 'text'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_table' },
        rows: {
          type: 'array',
          minItems: 1,
          maxItems: 50,
          items: {
            type: 'array',
            minItems: 1,
            maxItems: 20,
            items: {
              oneOf: [
                { type: 'string', maxLength: 5000 },
                { type: 'number' },
                { type: 'boolean' }
              ]
            }
          }
        },
        style: {
          type: 'string',
          enum: ['medium1', 'medium2', 'medium3', 'medium4', 'light1', 'light2', 'light3', 'dark1', 'dark2', 'none'],
          default: 'medium2'
        }
      },
      required: ['type', 'rows'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_set_paragraph_format' },
        paragraph: { type: 'integer', minimum: 1, maximum: 100000 },
        ...wordParagraphFormattingProperties
      },
      required: ['type', 'paragraph'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_format_text' },
        paragraph: { type: 'integer', minimum: 1, maximum: 100000 },
        find: { type: 'string', minLength: 1, maxLength: 2000 },
        ...wordTextFormattingProperties
      },
      required: ['type', 'paragraph', 'find'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_header_footer' },
        location: { type: 'string', enum: ['header', 'footer'] },
        kind: { type: 'string', enum: ['default', 'first', 'even'], default: 'default' },
        text: { type: 'string', maxLength: 2000 },
        field: { type: 'string', enum: ['page', 'numpages', 'date', 'author', 'title', 'time', 'filename'] },
        align: alignmentProperty,
        ...wordTextFormattingProperties
      },
      required: ['type', 'location'],
      anyOf: [{ required: ['text'] }, { required: ['field'] }],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'spreadsheet_set_cell' },
        sheet: { type: 'string', minLength: 1, maxLength: 31, default: 'Sheet1' },
        address: { type: 'string', pattern: '^[A-Za-z]{1,3}[1-9][0-9]{0,6}$' },
        value: {
          oneOf: [
            { type: 'string', maxLength: 20000 },
            { type: 'number' },
            { type: 'boolean' }
          ]
        },
        formula: { type: 'string', maxLength: 8000 }
      },
      required: ['type', 'address'],
      oneOf: [{ required: ['value'] }, { required: ['formula'] }],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'spreadsheet_add_sheet' },
        name: { type: 'string', minLength: 1, maxLength: 31 },
        tabColor: { type: 'string', pattern: '^[0-9A-Fa-f]{6}$' },
        hidden: { type: 'boolean' }
      },
      required: ['type', 'name'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'spreadsheet_rename_sheet' },
        sheet: { type: 'string', minLength: 1, maxLength: 31 },
        name: { type: 'string', minLength: 1, maxLength: 31 }
      },
      required: ['type', 'sheet', 'name'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'spreadsheet_delete_sheet' },
        sheet: { type: 'string', minLength: 1, maxLength: 31 }
      },
      required: ['type', 'sheet'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'spreadsheet_set_sheet_properties' },
        sheet: { type: 'string', minLength: 1, maxLength: 31 },
        hidden: { type: 'boolean' },
        tabColor: { type: 'string', pattern: '^[0-9A-Fa-f]{6}$' },
        freeze: {
          type: 'string',
          pattern: '^(?:[A-Za-z]{1,3}[1-9][0-9]{0,6}|none)$'
        }
      },
      required: ['type', 'sheet'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_add_slide' },
        title: { type: 'string', maxLength: 2000 },
        background: { type: 'string', pattern: '^[0-9A-Fa-f]{6}$' }
      },
      required: ['type'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_add_textbox' },
        slide: { type: 'integer', minimum: 1, maximum: 10000 },
        text: { type: 'string', maxLength: 20000 },
        x: { type: 'string', maxLength: 24 },
        y: { type: 'string', maxLength: 24 },
        width: { type: 'string', maxLength: 24 },
        height: { type: 'string', maxLength: 24 },
        color: colorProperty,
        size: fontSizeProperty
      },
      required: ['type', 'slide', 'text', 'x', 'y', 'width', 'height'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_set_text' },
        slide: { type: 'integer', minimum: 1, maximum: 10000 },
        shape: { type: 'integer', minimum: 1, maximum: 100000 },
        text: { type: 'string', maxLength: 20000 },
        color: colorProperty,
        size: fontSizeProperty
      },
      required: ['type', 'slide', 'shape', 'text'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_delete_slide' },
        slide: { type: 'integer', minimum: 1, maximum: 10000 }
      },
      required: ['type', 'slide'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_move_slide' },
        slide: { type: 'integer', minimum: 1, maximum: 10000 },
        position: { type: 'integer', minimum: 1, maximum: 10000 }
      },
      required: ['type', 'slide', 'position'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'presentation_set_slide_properties' },
        slide: { type: 'integer', minimum: 1, maximum: 10000 },
        background: { type: 'string', pattern: '^[0-9A-Fa-f]{6}$' },
        hidden: { type: 'boolean' },
        name: { type: 'string', minLength: 1, maxLength: 500 }
      },
      required: ['type', 'slide'],
      additionalProperties: false
    }
  ]
} as const;
