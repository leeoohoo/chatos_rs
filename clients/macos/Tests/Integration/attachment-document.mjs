// End-to-end fixture for native attachment import -> real document MCP inspection.
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const fixturePlugin = path.join(repository, 'plugins/document');
const plugin = process.env.CHATOS_ATTACHMENT_DOCUMENT_PLUGIN || fixturePlugin;
const require = createRequire(path.join(fixturePlugin, 'package.json'));
const { zipSync, strToU8 } = require('fflate');

if (process.argv[2] === 'fixture') {
  const bytes = zipSync({
    '[Content_Types].xml': strToU8('<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'),
    '_rels/.rels': strToU8('<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'),
    'word/document.xml': strToU8('<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Attachment integration fixture</w:t></w:r></w:p></w:body></w:document>'),
    // Stored padding reproduces a binary attachment exceeding model result budgets.
    'word/padding.bin': new Uint8Array(120_000).fill(255),
  }, { level: 0 });
  process.stdout.write(bytes);
} else {
  assert.equal(process.argv[2], 'inspect');
  const { Client } = await import(path.join(fixturePlugin, 'node_modules/@modelcontextprotocol/sdk/dist/esm/client/index.js'));
  const { StdioClientTransport } = await import(path.join(fixturePlugin, 'node_modules/@modelcontextprotocol/sdk/dist/esm/client/stdio.js'));
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: [path.join(plugin, 'bin/chatos-document-mcp'), 'mcp'],
    env: { ...process.env, CHATOS_WORKSPACE: process.argv[3], CHATOS_PLUGIN_ROOT: plugin },
    stderr: 'inherit',
  });
  const client = new Client({ name: 'native-attachment-integration', version: '1.0.0' });
  try {
    await client.connect(transport);
    const result = await client.callTool({ name: 'document_inspect', arguments: { inputPath: process.argv[4] } });
    assert.notEqual(result.isError, true, JSON.stringify(result));
    const content = result.content.find(item => item.type === 'text');
    assert.ok(content);
    process.stdout.write(content.text);
  } finally {
    await client.close();
  }
}
