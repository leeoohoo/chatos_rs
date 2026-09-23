import assert from 'node:assert/strict';
import test from 'node:test';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

function assertProviderPortableSchema(schema, path) {
  assert.ok(schema && typeof schema === 'object' && !Array.isArray(schema), `${path} must be a non-empty JSON Schema object`);
  assert.notEqual(Object.keys(schema).length, 0, `${path} must not use an unconstrained empty schema`);
  assert.equal(Object.hasOwn(schema, 'const'), false, `${path} must use a typed single-value enum instead of const`);
  if (Object.hasOwn(schema, 'enum')) {
    assert.ok(Object.hasOwn(schema, 'type'), `${path} enum must declare its JSON type`);
  }
  if (schema.type === 'array') {
    assert.ok(schema.items, `${path} array must declare an item schema`);
  }

  for (const [name, child] of Object.entries(schema.properties ?? {})) {
    assertProviderPortableSchema(child, `${path}.properties.${name}`);
  }
  if (schema.items) assertProviderPortableSchema(schema.items, `${path}.items`);
  if (schema.additionalProperties && typeof schema.additionalProperties === 'object') {
    assertProviderPortableSchema(schema.additionalProperties, `${path}.additionalProperties`);
  }
  for (const keyword of ['anyOf', 'oneOf', 'allOf']) {
    for (const [index, child] of (schema[keyword] ?? []).entries()) {
      assertProviderPortableSchema(child, `${path}.${keyword}[${index}]`);
    }
  }
}

test('all exposed MCP inputs stay inside the portable provider JSON Schema subset', async () => {
  const client = new Client({ name: 'web-design-schema-audit', version: '1.0.0' });
  const transport = new StdioClientTransport({
    command: process.execPath,
    args: ['dist/mcp-server.mjs', 'mcp']
  });
  try {
    await client.connect(transport);
    const listed = await client.listTools();
    for (const tool of listed.tools) {
      assertProviderPortableSchema(tool.inputSchema, tool.name);
    }
  } finally {
    await client.close().catch(() => undefined);
  }
});
