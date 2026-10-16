// Read-only installed-plugin probe. Never requests permissions or observes the desktop.
import assert from 'node:assert/strict';
import path from 'node:path';
const repository = path.resolve(import.meta.dirname, '../..');
const packageRoot = process.env.CHATOS_PLUGIN_TEST_PACKAGE_ROOT;
assert.ok(packageRoot && path.isAbsolute(packageRoot), 'Use an integrity-verified installed package');
const sdk = path.join(repository, 'plugins/document/node_modules/@modelcontextprotocol/sdk/dist/esm/client');
const { Client } = await import(path.join(sdk, 'index.js'));
const { StdioClientTransport } = await import(path.join(sdk, 'stdio.js'));
const client = new Client({ name: 'installed-permissions-test', version: '1.0.0' });
try {
  await client.connect(new StdioClientTransport({ command: process.execPath,
    args: [path.join(packageRoot, 'bin/open-computer-use'), 'mcp'], env: { ...process.env }, stderr: 'pipe' }));
  const response = await client.callTool({ name: 'check_permissions', arguments: {} });
  assert.notEqual(response.isError, true, 'check_permissions must succeed without requesting access');
  const permission = response.structuredContent || JSON.parse(response.content.find(item => item.type === 'text').text);
  for (const field of ['screenRecording', 'accessibility', 'allGranted', 'onboardingPresented']) {
    assert.equal(typeof permission[field], 'boolean', `${field} must be reported explicitly`);
  }
  assert.equal(permission.onboardingPresented, false, 'Read-only probe must not show onboarding');
  assert.equal(permission.allGranted, permission.screenRecording && permission.accessibility);
  assert.ok(Array.isArray(permission.missingPermissions));
  assert.ok(Array.isArray(permission.permissions) && permission.permissions.length === 2);
  console.log(JSON.stringify({ tool: 'check_permissions', passed: true,
    screenRecording: permission.screenRecording, accessibility: permission.accessibility,
    onboardingPresented: permission.onboardingPresented, requestedPermissions: false }));
} finally { await client.close(); }
