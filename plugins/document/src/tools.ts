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

const officeOperationSchema = {
  oneOf: [
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_paragraph' },
        text: { type: 'string', maxLength: 20000 },
        pageBreakBefore: { type: 'boolean', default: false }
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
        pageBreakBefore: { type: 'boolean', default: false }
      },
      required: ['type', 'level', 'text'],
      additionalProperties: false
    },
    {
      type: 'object',
      properties: {
        type: { const: 'word_add_list_item' },
        text: { type: 'string', maxLength: 20000 },
        ordered: { type: 'boolean', default: false },
        level: { type: 'integer', minimum: 0, maximum: 8, default: 0 }
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
        align: { type: 'string', enum: ['left', 'center', 'right', 'justify'] },
        style: { type: 'string', minLength: 1, maxLength: 128 },
        bold: { type: 'boolean' },
        italic: { type: 'boolean' }
      },
      required: ['type', 'paragraph'],
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
        height: { type: 'string', maxLength: 24 }
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
        text: { type: 'string', maxLength: 20000 }
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

const TOOL_DEFINITIONS_BASE = [
  {
    name: 'document_inspect',
    description: 'Inspect a DOCX, XLSX, PPTX, or PDF from the bound workspace or a managed artifact created earlier in the current session.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: {
          type: 'string',
          minLength: 1,
          maxLength: 1024,
          description: 'Relative workspace path or relativePath returned for a current-session managed artifact.'
        }
      },
      required: ['inputPath'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read'],
      'chatos/riskLevel': 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 30_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'document_extract_text',
    description: 'Extract bounded text from a workspace document or a managed artifact created earlier in the current session.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: {
          type: 'string',
          minLength: 1,
          maxLength: 1024,
          description: 'Relative workspace path or relativePath returned for a current-session managed artifact.'
        },
        maxChars: {
          type: 'integer',
          minimum: 1,
          maximum: 30000,
          default: 30000,
          description: 'Maximum number of extracted text characters returned.'
        }
      },
      required: ['inputPath'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read'],
      'chatos/riskLevel': 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 60_000,
      'chatos/toolResultMaxChars': 50_000
    }
  },
  {
    name: 'document_render',
    description: 'Render up to 50 selected pages from a DOCX, XLSX, PPTX, or PDF as individual PNG artifacts plus a JSON manifest.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: {
          type: 'string',
          minLength: 1,
          maxLength: 1024,
          description: 'Relative workspace path or relativePath returned for a current-session managed artifact.'
        },
        outputPrefix: {
          type: 'string',
          minLength: 1,
          maxLength: 150,
          description: 'Safe file-name prefix for page PNGs and the render manifest.'
        },
        pages: {
          type: 'array',
          minItems: 1,
          maxItems: 50,
          uniqueItems: true,
          default: [1],
          items: { type: 'integer', minimum: 1, maximum: 2000 }
        },
        dpi: {
          type: 'integer',
          minimum: 72,
          maximum: 300,
          default: 144,
          description: 'PDF render DPI; ignored for Office documents.'
        },
        viewportWidth: {
          type: 'integer',
          minimum: 320,
          maximum: 2400,
          default: 1600,
          description: 'Office HTML render viewport width; ignored for PDFs.'
        },
        viewportHeight: {
          type: 'integer',
          minimum: 240,
          maximum: 2400,
          default: 1200,
          description: 'Office HTML render viewport height; ignored for PDFs.'
        }
      },
      required: ['inputPath', 'outputPrefix'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 50_000
    }
  },
  {
    name: 'document_convert',
    description: 'Convert a workspace or current-session DOCX, XLSX, or PPTX into a downloadable image-based PDF artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: {
          type: 'string',
          minLength: 1,
          maxLength: 1024,
          description: 'Relative workspace path or relativePath returned for a current-session managed artifact.'
        },
        outputName: {
          type: 'string',
          minLength: 1,
          maxLength: 200,
          description: 'Safe PDF artifact file name.'
        },
        pages: {
          type: 'array',
          minItems: 1,
          maxItems: 50,
          uniqueItems: true,
          items: { type: 'integer', minimum: 1, maximum: 2000 },
          description: 'DOCX pages or PPTX slides to convert, in output order. DOCX currently supports the first 50 pages.'
        },
        sheets: {
          type: 'array',
          minItems: 1,
          maxItems: 50,
          uniqueItems: true,
          items: { type: 'string', minLength: 1, maxLength: 31 },
          description: 'XLSX worksheet names to convert, in output order.'
        },
        viewportWidth: {
          type: 'integer',
          minimum: 320,
          maximum: 2400,
          default: 1600,
          description: 'Office HTML render viewport width.'
        },
        viewportHeight: {
          type: 'integer',
          minimum: 240,
          maximum: 2400,
          default: 1200,
          description: 'Office HTML render viewport height.'
        }
      },
      required: ['inputPath', 'outputName'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 50_000
    }
  },
  {
    name: 'document_validate',
    description: 'Validate document structure, reopen it with the format engine, and optionally render up to 10 pages for bounded visual verification.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: {
          type: 'string',
          minLength: 1,
          maxLength: 1024,
          description: 'Relative workspace path or relativePath returned for a current-session managed artifact.'
        },
        renderPages: {
          type: 'array',
          minItems: 1,
          maxItems: 10,
          uniqueItems: true,
          items: { type: 'integer', minimum: 1, maximum: 2000 }
        },
        dpi: { type: 'integer', minimum: 72, maximum: 200, default: 144 },
        viewportWidth: { type: 'integer', minimum: 320, maximum: 2000, default: 1600 },
        viewportHeight: { type: 'integer', minimum: 240, maximum: 2000, default: 1200 }
      },
      required: ['inputPath'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read'],
      'chatos/riskLevel': 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 50_000
    }
  },
  {
    name: 'spreadsheet_read_range',
    description: 'Read a bounded XLSX cell range with typed values, formulas, and cached results.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        sheet: { type: 'string', minLength: 1, maxLength: 31 },
        range: {
          type: 'string',
          minLength: 2,
          maxLength: 32,
          pattern: '^[A-Za-z]{1,3}[1-9][0-9]{0,6}(?::[A-Za-z]{1,3}[1-9][0-9]{0,6})?$'
        },
        maxCells: { type: 'integer', minimum: 1, maximum: 500, default: 500 }
      },
      required: ['inputPath', 'sheet', 'range'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read'],
      'chatos/riskLevel': 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 60_000,
      'chatos/toolResultMaxChars': 50_000
    }
  },
  {
    name: 'spreadsheet_write_range',
    description: 'Copy an XLSX workspace file, write a bounded rectangular matrix, and create a new XLSX artifact; null cells are left unchanged.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        sheet: { type: 'string', minLength: 1, maxLength: 31 },
        startCell: {
          type: 'string',
          pattern: '^[A-Za-z]{1,3}[1-9][0-9]{0,6}$'
        },
        values: {
          type: 'array',
          minItems: 1,
          maxItems: 100,
          items: {
            type: 'array',
            minItems: 1,
            maxItems: 100,
            items: {
              oneOf: [
                { type: 'string', maxLength: 20000 },
                { type: 'number' },
                { type: 'boolean' },
                { type: 'null' },
                {
                  type: 'object',
                  properties: { formula: { type: 'string', pattern: '^=.+', maxLength: 8000 } },
                  required: ['formula'],
                  additionalProperties: false
                }
              ]
            }
          }
        }
      },
      required: ['inputPath', 'outputName', 'sheet', 'startCell', 'values'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'spreadsheet_manage_sheets',
    description: 'Copy an XLSX file and apply bounded add, rename, delete, color, visibility, or freeze-pane sheet operations.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        operations: {
          type: 'array',
          minItems: 1,
          maxItems: 100,
          items: {
            oneOf: officeOperationSchema.oneOf.slice(7, 11)
          }
        }
      },
      required: ['inputPath', 'outputName', 'operations'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'office_create',
    description: 'Create a downloadable managed DOCX, XLSX, or PPTX artifact. The result is not written into the project workspace; use its returned relativePath for follow-up document tools in the same session.',
    inputSchema: {
      type: 'object',
      properties: {
        format: { type: 'string', enum: ['docx', 'xlsx', 'pptx'] },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        locale: { type: 'string', minLength: 2, maxLength: 32, default: 'en-US' },
        operations: { type: 'array', maxItems: 500, items: officeOperationSchema }
      },
      required: ['format', 'outputName', 'operations'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'office_edit_batch',
    description: 'Copy a workspace or current-session DOCX, XLSX, or PPTX, apply typed edits atomically, and create a new downloadable managed artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        operations: { type: 'array', minItems: 1, maxItems: 500, items: officeOperationSchema }
      },
      required: ['inputPath', 'outputName', 'operations'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'pdf_merge',
    description: 'Merge 2 to 20 workspace or current-session PDF files in order and create a new downloadable PDF artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPaths: { type: 'array', minItems: 2, maxItems: 20, items: { type: 'string', minLength: 1, maxLength: 1024 } },
        outputName: { type: 'string', minLength: 1, maxLength: 200 }
      },
      required: ['inputPaths', 'outputName'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'pdf_extract_pages',
    description: 'Copy selected 1-based pages from a workspace or current-session PDF into a new downloadable PDF artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        pages: { type: 'array', minItems: 1, maxItems: 2000, items: { type: 'integer', minimum: 1 } },
        outputName: { type: 'string', minLength: 1, maxLength: 200 }
      },
      required: ['inputPath', 'pages', 'outputName'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'pdf_transform',
    description: 'Reorder or duplicate pages, rotate output pages, and update PDF metadata in a new artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        pageOrder: { type: 'array', minItems: 1, maxItems: 2000, items: { type: 'integer', minimum: 1 } },
        rotations: {
          type: 'array',
          maxItems: 2000,
          items: {
            type: 'object',
            properties: {
              page: { type: 'integer', minimum: 1 },
              degrees: { type: 'integer', enum: [0, 90, 180, 270] }
            },
            required: ['page', 'degrees'],
            additionalProperties: false
          }
        },
        metadata: {
          type: 'object',
          properties: {
            title: { type: 'string', maxLength: 4000 },
            author: { type: 'string', maxLength: 4000 },
            subject: { type: 'string', maxLength: 4000 },
            creator: { type: 'string', maxLength: 4000 },
            producer: { type: 'string', maxLength: 4000 },
            keywords: { type: 'string', maxLength: 4000 }
          },
          additionalProperties: false
        }
      },
      required: ['inputPath', 'outputName'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'pdf_form_list',
    description: 'List up to 500 AcroForm fields in a workspace or current-session PDF without modifying it.',
    inputSchema: {
      type: 'object',
      properties: { inputPath: { type: 'string', minLength: 1, maxLength: 1024 } },
      required: ['inputPath'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read'],
      'chatos/riskLevel': 'low',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 60_000,
      'chatos/toolResultMaxChars': 30_000
    }
  },
  {
    name: 'pdf_form_fill',
    description: 'Fill typed AcroForm fields in a workspace or current-session PDF and create a new downloadable PDF artifact.',
    inputSchema: {
      type: 'object',
      properties: {
        inputPath: { type: 'string', minLength: 1, maxLength: 1024 },
        outputName: { type: 'string', minLength: 1, maxLength: 200 },
        flatten: { type: 'boolean', default: false },
        fields: {
          type: 'array',
          minItems: 1,
          maxItems: 500,
          items: {
            type: 'object',
            properties: {
              name: { type: 'string', minLength: 1, maxLength: 500 },
              value: { oneOf: [{ type: 'string', maxLength: 20000 }, { type: 'boolean' }] }
            },
            required: ['name', 'value'],
            additionalProperties: false
          }
        }
      },
      required: ['inputPath', 'outputName', 'fields'],
      additionalProperties: false
    },
    _meta: {
      'chatos/policyVersion': 1,
      'chatos/requiredPermissions': ['workspace.read', 'artifact.create'],
      'chatos/riskLevel': 'medium',
      'chatos/approvalMode': 'none',
      'chatos/timeoutMs': 120_000,
      'chatos/toolResultMaxChars': 30_000
    }
  }
] as const;

const documentSkillEvidence = {
  type: 'array',
  minItems: 1,
  maxItems: 8,
  items: { type: 'string', minLength: 1 },
  description: 'Platform-issued activation evidence for the Document router and any required format Skill. ChatOS validates and removes this field before local execution.'
} as const;

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
    inputSchema: {
      ...tool.inputSchema,
      properties: {
        ...tool.inputSchema.properties,
        skillEvidence: documentSkillEvidence
      },
      required: [...tool.inputSchema.required, 'skillEvidence']
    },
    _meta: {
      ...tool._meta,
      'chatos/skillGate': {
        evidenceArgument: 'skillEvidence',
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
