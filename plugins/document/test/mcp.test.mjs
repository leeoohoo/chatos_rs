import assert from 'node:assert/strict';
import { copyFile, mkdtemp, mkdir, readFile, symlink, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { strFromU8, unzipSync, zipSync, strToU8 } from 'fflate';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { PDFDocument, StandardFonts, rgb } from 'pdf-lib';

const projectRoot = path.resolve(import.meta.dirname, '..');
const launcher = path.join(projectRoot, 'bin', 'chatos-document-mcp');

async function withClient(workspace, callback) {
  const artifact = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-artifacts-'));
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [launcher, 'mcp'],
    env: {
      ...process.env,
      CHATOS_WORKSPACE: workspace,
      CHATOS_PLUGIN_ARTIFACT_DIR: artifact,
      CHATOS_PLUGIN_ROOT: projectRoot
    },
    stderr: process.env.DOCUMENT_MCP_DEBUG === '1' ? 'inherit' : 'pipe'
  });
  const client = new Client({ name: 'document-mcp-test', version: '1.0.0' });
  await client.connect(transport);
  try {
    return await callback(client, { artifact });
  } finally {
    await client.close();
  }
}

function minimalDocx() {
  return zipSync({
    '[Content_Types].xml': strToU8('<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>'),
    'docProps/core.xml': strToU8('<?xml version="1.0"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>测试文档</dc:title><dc:creator>Chatos</dc:creator></cp:coreProperties>'),
    'word/document.xml': strToU8('<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:tbl></w:tbl></w:body></w:document>')
  });
}

function minimalXlsx() {
  return zipSync({
    '[Content_Types].xml': strToU8('<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>'),
    'xl/workbook.xml': strToU8('<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="数据" sheetId="1"/></sheets></workbook>'),
    'xl/sharedStrings.xml': strToU8('<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><t>Revenue</t></si></sst>'),
    'xl/worksheets/sheet1.xml': strToU8('<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row></sheetData></worksheet>')
  });
}

function minimalPptx() {
  return zipSync({
    '[Content_Types].xml': strToU8('<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"></Types>'),
    'ppt/presentation.xml': strToU8('<?xml version="1.0"?><p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"></p:presentation>'),
    'ppt/slides/slide1.xml': strToU8('<?xml version="1.0"?><p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><a:t>Quarterly Review</a:t><a:t>Growth</a:t></p:cSld></p:sld>')
  });
}

function minimalTextPdf() {
  const content = 'BT\n/F1 12 Tf\n72 720 Td\n(Hello PDF) Tj\nET';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    `<< /Length ${Buffer.byteLength(content)} >>\nstream\n${content}\nendstream`
  ];
  let body = '%PDF-1.4\n';
  const offsets = [0];
  for (let index = 0; index < objects.length; index += 1) {
    offsets.push(Buffer.byteLength(body));
    body += `${index + 1} 0 obj\n${objects[index]}\nendobj\n`;
  }
  const xrefOffset = Buffer.byteLength(body);
  body += `xref\n0 ${objects.length + 1}\n`;
  body += '0000000000 65535 f \n';
  for (const offset of offsets.slice(1)) {
    body += `${String(offset).padStart(10, '0')} 00000 n \n`;
  }
  body += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefOffset}\n%%EOF\n`;
  return body;
}

async function generatedPdf(labels) {
  const document = await PDFDocument.create();
  const font = await document.embedFont(StandardFonts.Helvetica);
  for (const label of labels) {
    const page = document.addPage([300, 200]);
    page.drawText(label, { x: 30, y: 120, size: 18, font, color: rgb(0, 0, 0) });
  }
  return await document.save();
}

async function generatedFormPdf() {
  const document = await PDFDocument.create();
  const page = document.addPage([300, 200]);
  const field = document.getForm().createTextField('customer_name');
  field.addToPage(page, { x: 30, y: 100, width: 200, height: 24 });
  return await document.save();
}

test('lists policy-annotated tools and inspects a DOCX', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await mkdir(path.join(workspace, 'docs'));
  await writeFile(path.join(workspace, 'docs', 'sample.docx'), minimalDocx());

  await withClient(workspace, async (client) => {
    const listed = await client.listTools();
    assert.equal(listed.tools.length, 15);
    assert.ok(Buffer.byteLength(JSON.stringify(listed.tools), 'utf8') < 512 * 1024);
    assert.equal(listed.tools[0].name, 'document_inspect');
    assert.equal(listed.tools[0]._meta['chatos/policyVersion'], 1);
    for (const tool of listed.tools) {
      assert.equal(tool._meta['chatos/policyVersion'], 1);
      assert.ok(tool._meta['chatos/timeoutMs'] >= 300 && tool._meta['chatos/timeoutMs'] <= 120_000);
      assert.ok(['low', 'medium', 'high', 'critical'].includes(tool._meta['chatos/riskLevel']));
      assert.ok(['none', 'per_call'].includes(tool._meta['chatos/approvalMode']));
      for (const permission of tool._meta['chatos/requiredPermissions']) {
        assert.ok(['workspace.read', 'artifact.create'].includes(permission));
      }
    }

    const response = await client.callTool({
      name: 'document_inspect',
      arguments: { inputPath: 'docs/sample.docx' }
    });
    assert.equal(response.isError, false, JSON.stringify(response.structuredContent));
    assert.equal(response.structuredContent.format, 'docx');
    assert.equal(response.structuredContent.metadata.title, '测试文档');
    assert.equal(response.structuredContent.structure.paragraphs, 1);
    assert.equal(response.structuredContent.structure.tables, 1);
    assert.match(response.structuredContent.source.sha256, /^[a-f0-9]{64}$/);

    const extracted = await client.callTool({
      name: 'document_extract_text',
      arguments: { inputPath: 'docs/sample.docx', maxChars: 100 }
    });
    assert.equal(extracted.isError, false);
    assert.equal(extracted.structuredContent.sections[0].text, 'Hello');
    assert.equal(extracted.structuredContent.truncated, false);
  });
});

test('rejects traversal, absolute paths, and symbolic links', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  const outside = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-outside-'));
  await writeFile(path.join(outside, 'secret.pdf'), '%PDF-1.7\n1 0 obj <</Type /Page>> endobj\n%%EOF');
  await symlink(path.join(outside, 'secret.pdf'), path.join(workspace, 'linked.pdf'));

  await withClient(workspace, async (client) => {
    for (const inputPath of ['../secret.pdf', path.join(outside, 'secret.pdf'), 'linked.pdf']) {
      const response = await client.callTool({ name: 'document_inspect', arguments: { inputPath } });
      assert.equal(response.isError, true);
      assert.ok(['INVALID_PATH', 'FILE_NOT_FOUND'].includes(response.structuredContent.error.code));
    }
  });
});

test('inspects a PDF without exposing an absolute path', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await writeFile(
    path.join(workspace, 'sample.pdf'),
    '%PDF-1.7\n1 0 obj <</Type /Catalog /AcroForm 2 0 R>> endobj\n2 0 obj <</Type /Page>> endobj\n3 0 obj <</Type /Page>> endobj\n%%EOF'
  );
  await withClient(workspace, async (client) => {
    const response = await client.callTool({ name: 'document_inspect', arguments: { inputPath: 'sample.pdf' } });
    assert.equal(response.isError, false);
    assert.equal(response.structuredContent.format, 'pdf');
    assert.equal(response.structuredContent.structure.approximatePages, 2);
    assert.equal(response.structuredContent.structure.hasAcroForm, true);
    assert.equal(response.structuredContent.source.relativePath, 'sample.pdf');
    assert.equal(JSON.stringify(response.structuredContent).includes(workspace), false);
  });
});

test('extracts bounded PDF text with PDF.js', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await writeFile(path.join(workspace, 'text.pdf'), minimalTextPdf());
  await withClient(workspace, async (client) => {
    const response = await client.callTool({
      name: 'document_extract_text',
      arguments: { inputPath: 'text.pdf', maxChars: 100 }
    });
    assert.equal(response.isError, false, JSON.stringify(response.structuredContent));
    assert.equal(response.structuredContent.sections[0].kind, 'page');
    assert.match(response.structuredContent.sections[0].text, /Hello PDF/);
  });
});

test('extracts structured spreadsheet and presentation text', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await writeFile(path.join(workspace, 'sample.xlsx'), minimalXlsx());
  await writeFile(path.join(workspace, 'sample.pptx'), minimalPptx());
  await withClient(workspace, async (client) => {
    const spreadsheet = await client.callTool({
      name: 'document_extract_text',
      arguments: { inputPath: 'sample.xlsx' }
    });
    assert.equal(spreadsheet.isError, false, JSON.stringify(spreadsheet.structuredContent));
    assert.equal(spreadsheet.structuredContent.sections[0].name, '数据');
    assert.match(spreadsheet.structuredContent.sections[0].text, /A1\tRevenue/);
    assert.match(spreadsheet.structuredContent.sections[0].text, /B1\t42/);

    const presentation = await client.callTool({
      name: 'document_extract_text',
      arguments: { inputPath: 'sample.pptx' }
    });
    assert.equal(presentation.isError, false, JSON.stringify(presentation.structuredContent));
    assert.equal(presentation.structuredContent.sections[0].kind, 'slide');
    assert.match(presentation.structuredContent.sections[0].text, /Quarterly Review/);
    assert.match(presentation.structuredContent.sections[0].text, /Growth/);
  });
});

test('creates and edits validated Office artifacts with typed operations', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await withClient(workspace, async (client, { artifact }) => {
    const created = await client.callTool({
      name: 'office_create',
      arguments: {
        format: 'docx',
        outputName: 'created.docx',
        locale: 'en-US',
        operations: [
          { type: 'word_add_paragraph', text: 'Created by Document MCP' },
          { type: 'word_add_heading', level: 1, text: 'Annual Report' },
          { type: 'word_add_list_item', text: 'First item', ordered: false },
          {
            type: 'word_add_table',
            rows: [
              ['Region', 'Revenue'],
              ['North', 42]
            ],
            style: 'medium2'
          }
        ]
      }
    });
    assert.equal(created.isError, false, JSON.stringify(created.structuredContent));
    assert.equal(created.structuredContent.artifact.relativePath, 'created.docx');
    assert.equal(created.structuredContent.engine.version, '1.0.144');
    assert.deepEqual(created._meta?.['chatos/artifacts']?.map((candidate) => ({
      relative_path: candidate.relative_path,
      display_name: candidate.display_name,
      media_type: candidate.media_type,
      size_bytes: candidate.size_bytes,
      sha256: candidate.sha256
    })), [{
      relative_path: 'created.docx',
      display_name: 'created.docx',
      media_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
      size_bytes: created.structuredContent.artifact.size,
      sha256: created.structuredContent.artifact.sha256
    }]);
    const createdPath = path.join(artifact, 'created.docx');
    const createdZip = unzipSync(await readFile(createdPath));
    const createdXml = strFromU8(createdZip['word/document.xml']);
    assert.match(createdXml, /Created by Document MCP/);
    assert.match(createdXml, /Annual Report/);
    assert.match(createdXml, /First item/);
    assert.match(createdXml, /North/);
    assert.match(createdXml, /<w:tbl/);

    const inspectedCreated = await client.callTool({
      name: 'document_inspect',
      arguments: { inputPath: created.structuredContent.artifact.relativePath }
    });
    assert.equal(inspectedCreated.isError, false, JSON.stringify(inspectedCreated.structuredContent));
    assert.equal(inspectedCreated.structuredContent.source.relativePath, 'created.docx');

    const twoPageXml = createdXml.replace(
      '</w:body>',
      '<w:p><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>Second page for PDF conversion</w:t></w:r></w:p></w:body>'
    );
    await writeFile(
      path.join(workspace, 'two-page.docx'),
      zipSync({ ...createdZip, 'word/document.xml': strToU8(twoPageXml) })
    );
    const convertedDocx = await client.callTool({
      name: 'document_convert',
      arguments: { inputPath: 'two-page.docx', outputName: 'two-page.pdf', viewportWidth: 800, viewportHeight: 600 }
    });
    assert.equal(convertedDocx.isError, false, JSON.stringify(convertedDocx.structuredContent));
    assert.equal(convertedDocx.structuredContent.conversionMode, 'raster');
    assert.equal(convertedDocx.structuredContent.searchableText, false);
    assert.equal(convertedDocx.structuredContent.layoutFidelity, 'preview');
    assert.equal(convertedDocx.structuredContent.artifact.pages, 2);
    assert.equal(JSON.stringify(convertedDocx.structuredContent).includes(workspace), false);
    assert.equal((await PDFDocument.load(await readFile(path.join(artifact, 'two-page.pdf')))).getPageCount(), 2);

    const duplicateConversion = await client.callTool({
      name: 'document_convert',
      arguments: { inputPath: 'two-page.docx', outputName: 'two-page.pdf' }
    });
    assert.equal(duplicateConversion.isError, true);
    assert.equal(duplicateConversion.structuredContent.error.code, 'OUTPUT_EXISTS');

    const edited = await client.callTool({
      name: 'office_edit_batch',
      arguments: {
        inputPath: created.structuredContent.artifact.relativePath,
        outputName: 'edited.docx',
        operations: [
          { type: 'word_set_paragraph_format', paragraph: 1, align: 'center', bold: true },
          { type: 'word_add_paragraph', text: 'Second paragraph' }
        ]
      }
    });
    assert.equal(edited.isError, false, JSON.stringify(edited.structuredContent));
    const editedZip = unzipSync(await readFile(path.join(artifact, 'edited.docx')));
    const editedXml = strFromU8(editedZip['word/document.xml']);
    assert.match(editedXml, /Created by Document MCP/);
    assert.match(editedXml, /Second paragraph/);
    assert.match(editedXml, /<w:jc[^>]+w:val="center"/);

    const spreadsheet = await client.callTool({
      name: 'office_create',
      arguments: {
        format: 'xlsx',
        outputName: 'created.xlsx',
        operations: [
          { type: 'spreadsheet_set_cell', sheet: 'Sheet1', address: 'A1', value: 'Revenue' },
          { type: 'spreadsheet_set_cell', sheet: 'Sheet1', address: 'B1', formula: '=40+2' }
        ]
      }
    });
    assert.equal(spreadsheet.isError, false, JSON.stringify(spreadsheet.structuredContent));
    await copyFile(path.join(artifact, 'created.xlsx'), path.join(workspace, 'created.xlsx'));

    const readRange = await client.callTool({
      name: 'spreadsheet_read_range',
      arguments: { inputPath: 'created.xlsx', sheet: 'Sheet1', range: 'A1:B2' }
    });
    assert.equal(readRange.isError, false, JSON.stringify(readRange.structuredContent));
    assert.equal(readRange.structuredContent.returnedCellCount, 2);
    assert.deepEqual(
      readRange.structuredContent.cells.map((cell) => ({ address: cell.address, value: cell.value, formula: cell.formula })),
      [
        { address: 'A1', value: 'Revenue', formula: undefined },
        { address: 'B1', value: 42, formula: '=40+2' }
      ]
    );

    const writeRange = await client.callTool({
      name: 'spreadsheet_write_range',
      arguments: {
        inputPath: 'created.xlsx',
        outputName: 'range-edited.xlsx',
        sheet: 'Sheet1',
        startCell: 'A2',
        values: [
          ['North', 100],
          [null, { formula: '=B2*2' }]
        ]
      }
    });
    assert.equal(writeRange.isError, false, JSON.stringify(writeRange.structuredContent));
    assert.equal(writeRange.structuredContent.writtenCells, 3);
    assert.equal(writeRange.structuredContent.nullCellsSkipped, 1);
    await copyFile(path.join(artifact, 'range-edited.xlsx'), path.join(workspace, 'range-edited.xlsx'));

    const rereadRange = await client.callTool({
      name: 'spreadsheet_read_range',
      arguments: { inputPath: 'range-edited.xlsx', sheet: 'Sheet1', range: 'A1:B3' }
    });
    assert.equal(rereadRange.isError, false, JSON.stringify(rereadRange.structuredContent));
    const rereadCells = new Map(rereadRange.structuredContent.cells.map((cell) => [cell.address, cell]));
    assert.equal(rereadCells.get('A2').value, 'North');
    assert.equal(rereadCells.get('B2').value, 100);
    assert.equal(rereadCells.get('B3').formula, '=B2*2');
    assert.equal(rereadCells.get('B3').value, 200);

    const bulkValues = Array.from({ length: 6 }, (_, row) =>
      Array.from({ length: 100 }, (_, column) => row * 100 + column)
    );
    const bulkWrite = await client.callTool({
      name: 'spreadsheet_write_range',
      arguments: {
        inputPath: 'created.xlsx',
        outputName: 'bulk-edited.xlsx',
        sheet: 'Sheet1',
        startCell: 'A10',
        values: bulkValues
      }
    });
    assert.equal(bulkWrite.isError, false, JSON.stringify(bulkWrite.structuredContent));
    assert.equal(bulkWrite.structuredContent.writtenCells, 600);
    await copyFile(path.join(artifact, 'bulk-edited.xlsx'), path.join(workspace, 'bulk-edited.xlsx'));
    const bulkRead = await client.callTool({
      name: 'spreadsheet_read_range',
      arguments: { inputPath: 'bulk-edited.xlsx', sheet: 'Sheet1', range: 'CV15' }
    });
    assert.equal(bulkRead.isError, false, JSON.stringify(bulkRead.structuredContent));
    assert.equal(bulkRead.structuredContent.cells[0].value, 599);

    const managedSheets = await client.callTool({
      name: 'spreadsheet_manage_sheets',
      arguments: {
        inputPath: 'created.xlsx',
        outputName: 'managed-sheets.xlsx',
        operations: [
          { type: 'spreadsheet_add_sheet', name: 'Summary', tabColor: '4472C4' },
          { type: 'spreadsheet_add_sheet', name: 'DeleteMe' },
          { type: 'spreadsheet_rename_sheet', sheet: 'Summary', name: 'Dashboard' },
          { type: 'spreadsheet_set_sheet_properties', sheet: 'Dashboard', freeze: 'B2' },
          { type: 'spreadsheet_delete_sheet', sheet: 'DeleteMe' }
        ]
      }
    });
    assert.equal(managedSheets.isError, false, JSON.stringify(managedSheets.structuredContent));
    assert.equal(managedSheets.structuredContent.appliedOperations, 5);
    await copyFile(path.join(artifact, 'managed-sheets.xlsx'), path.join(workspace, 'managed-sheets.xlsx'));
    const managedInspection = await client.callTool({
      name: 'document_inspect',
      arguments: { inputPath: 'managed-sheets.xlsx' }
    });
    assert.equal(managedInspection.isError, false, JSON.stringify(managedInspection.structuredContent));
    assert.deepEqual(managedInspection.structuredContent.structure.sheetNames, ['Sheet1', 'Dashboard']);

    const convertedSpreadsheet = await client.callTool({
      name: 'document_convert',
      arguments: {
        inputPath: 'managed-sheets.xlsx',
        outputName: 'workbook.pdf',
        sheets: ['Dashboard', 'Sheet1'],
        viewportWidth: 800,
        viewportHeight: 600
      }
    });
    assert.equal(convertedSpreadsheet.isError, false, JSON.stringify(convertedSpreadsheet.structuredContent));
    assert.equal(convertedSpreadsheet.structuredContent.artifact.pages, 2);
    assert.deepEqual(convertedSpreadsheet.structuredContent.pages.map((page) => page.sheet), ['Dashboard', 'Sheet1']);
    assert.equal((await PDFDocument.load(await readFile(path.join(artifact, 'workbook.pdf')))).getPageCount(), 2);

    const presentation = await client.callTool({
      name: 'office_create',
      arguments: {
        format: 'pptx',
        outputName: 'created.pptx',
        operations: [
          { type: 'presentation_add_slide', title: 'Q4 Report', background: '1A1A2E' },
          {
            type: 'presentation_add_textbox',
            slide: 1,
            text: 'Revenue grew 25%',
            x: '2cm',
            y: '5cm',
            width: '10cm',
            height: '2cm'
          },
          { type: 'presentation_add_slide', title: 'Second Slide' },
          { type: 'presentation_add_slide', title: 'Third Slide' }
        ]
      }
    });
    assert.equal(presentation.isError, false, JSON.stringify(presentation.structuredContent));
    const presentationPath = path.join(artifact, 'created.pptx');
    await copyFile(presentationPath, path.join(workspace, 'created.pptx'));

    const convertedPresentation = await client.callTool({
      name: 'document_convert',
      arguments: {
        inputPath: 'created.pptx',
        outputName: 'presentation.pdf',
        pages: [3, 1],
        viewportWidth: 800,
        viewportHeight: 600
      }
    });
    assert.equal(convertedPresentation.isError, false, JSON.stringify(convertedPresentation.structuredContent));
    assert.equal(convertedPresentation.structuredContent.artifact.pages, 2);
    assert.deepEqual(convertedPresentation.structuredContent.pages.map((page) => page.slide), [3, 1]);
    assert.equal((await PDFDocument.load(await readFile(path.join(artifact, 'presentation.pdf')))).getPageCount(), 2);

    const rendered = await client.callTool({
      name: 'document_render',
      arguments: {
        inputPath: 'created.pptx',
        outputPrefix: 'slides-preview',
        pages: [1],
        viewportWidth: 800,
        viewportHeight: 600
      }
    });
    assert.equal(rendered.isError, false, JSON.stringify(rendered.structuredContent));
    assert.equal(rendered.structuredContent.pages[0].relativePath, 'slides-preview-page-0001.png');
    assert.equal(rendered.structuredContent.pages[0].width, 800);
    assert.equal(rendered.structuredContent.pages[0].height, 600);
    assert.deepEqual(
      [...(await readFile(path.join(artifact, 'slides-preview-page-0001.png'))).subarray(0, 8)],
      [137, 80, 78, 71, 13, 10, 26, 10]
    );

    const validated = await client.callTool({
      name: 'document_validate',
      arguments: { inputPath: 'created.pptx', renderPages: [1], viewportWidth: 800, viewportHeight: 600 }
    });
    assert.equal(validated.isError, false, JSON.stringify(validated.structuredContent));
    assert.equal(validated.structuredContent.valid, true);
    assert.equal(validated.structuredContent.checks.at(-1).name, 'render');

    const rearranged = await client.callTool({
      name: 'office_edit_batch',
      arguments: {
        inputPath: 'created.pptx',
        outputName: 'rearranged.pptx',
        operations: [
          { type: 'presentation_move_slide', slide: 3, position: 1 },
          { type: 'presentation_delete_slide', slide: 2 },
          {
            type: 'presentation_set_slide_properties',
            slide: 1,
            background: '223344',
            name: 'Opening'
          }
        ]
      }
    });
    assert.equal(rearranged.isError, false, JSON.stringify(rearranged.structuredContent));
    await copyFile(path.join(artifact, 'rearranged.pptx'), path.join(workspace, 'rearranged.pptx'));
    const rearrangedText = await client.callTool({
      name: 'document_extract_text',
      arguments: { inputPath: 'rearranged.pptx' }
    });
    assert.equal(rearrangedText.isError, false, JSON.stringify(rearrangedText.structuredContent));
    assert.equal(rearrangedText.structuredContent.sections.length, 2);
    assert.match(rearrangedText.structuredContent.sections[0].text, /Third Slide/);
    assert.match(rearrangedText.structuredContent.sections[1].text, /Second Slide/);
  });
});

test('merges, extracts, transforms, and fills PDF artifacts', async () => {
  const workspace = await mkdtemp(path.join(os.tmpdir(), 'document-mcp-workspace-'));
  await writeFile(path.join(workspace, 'a.pdf'), await generatedPdf(['A1']));
  await writeFile(path.join(workspace, 'b.pdf'), await generatedPdf(['B1', 'B2']));
  await writeFile(path.join(workspace, 'form.pdf'), await generatedFormPdf());

  await withClient(workspace, async (client, { artifact }) => {
    const rendered = await client.callTool({
      name: 'document_render',
      arguments: { inputPath: 'b.pdf', outputPrefix: 'pdf-preview', pages: [2, 1], dpi: 144 }
    });
    assert.equal(rendered.isError, false, JSON.stringify(rendered.structuredContent));
    assert.equal(rendered.structuredContent.pages.length, 2);
    assert.equal(rendered.structuredContent.pages[0].page, 2);
    assert.equal(rendered.structuredContent.pages[0].width, 600);
    assert.equal(rendered.structuredContent.pages[0].height, 400);
    assert.equal(rendered.structuredContent.manifest.relativePath, 'pdf-preview-render-manifest.json');
    const renderManifest = JSON.parse(await readFile(path.join(artifact, 'pdf-preview-render-manifest.json'), 'utf8'));
    assert.equal(renderManifest.source.pages, 2);
    assert.deepEqual(renderManifest.pages.map((page) => page.page), [2, 1]);

    const validated = await client.callTool({
      name: 'document_validate',
      arguments: { inputPath: 'b.pdf', renderPages: [1] }
    });
    assert.equal(validated.isError, false, JSON.stringify(validated.structuredContent));
    assert.equal(validated.structuredContent.valid, true);
    assert.equal(validated.structuredContent.exactPageCount, 2);

    const merged = await client.callTool({
      name: 'pdf_merge',
      arguments: { inputPaths: ['a.pdf', 'b.pdf'], outputName: 'merged.pdf' }
    });
    assert.equal(merged.isError, false, JSON.stringify(merged.structuredContent));
    assert.equal(merged.structuredContent.artifact.pages, 3);
    await copyFile(path.join(artifact, 'merged.pdf'), path.join(workspace, 'merged.pdf'));

    const extracted = await client.callTool({
      name: 'pdf_extract_pages',
      arguments: { inputPath: 'merged.pdf', pages: [3, 1], outputName: 'selected.pdf' }
    });
    assert.equal(extracted.isError, false, JSON.stringify(extracted.structuredContent));
    assert.equal(extracted.structuredContent.artifact.pages, 2);
    await copyFile(path.join(artifact, 'selected.pdf'), path.join(workspace, 'selected.pdf'));

    const transformed = await client.callTool({
      name: 'pdf_transform',
      arguments: {
        inputPath: 'selected.pdf',
        outputName: 'transformed.pdf',
        pageOrder: [2, 1],
        rotations: [{ page: 1, degrees: 90 }],
        metadata: { title: 'Transformed document', author: 'Document MCP' }
      }
    });
    assert.equal(transformed.isError, false, JSON.stringify(transformed.structuredContent));
    const transformedPdf = await PDFDocument.load(await readFile(path.join(artifact, 'transformed.pdf')));
    assert.equal(transformedPdf.getPageCount(), 2);
    assert.equal(transformedPdf.getPage(0).getRotation().angle, 90);
    assert.equal(transformedPdf.getTitle(), 'Transformed document');

    const listed = await client.callTool({
      name: 'pdf_form_list',
      arguments: { inputPath: 'form.pdf' }
    });
    assert.equal(listed.isError, false, JSON.stringify(listed.structuredContent));
    assert.deepEqual(listed.structuredContent.fields, [{ name: 'customer_name', type: 'text' }]);

    const filled = await client.callTool({
      name: 'pdf_form_fill',
      arguments: {
        inputPath: 'form.pdf',
        outputName: 'filled.pdf',
        fields: [{ name: 'customer_name', value: 'Acme' }]
      }
    });
    assert.equal(filled.isError, false, JSON.stringify(filled.structuredContent));
    const filledPdf = await PDFDocument.load(await readFile(path.join(artifact, 'filled.pdf')));
    assert.equal(filledPdf.getForm().getTextField('customer_name').getText(), 'Acme');
  });
});
