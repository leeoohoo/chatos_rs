// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const path = require('node:path');
const test = require('node:test');
const { pathToFileURL } = require('node:url');

const {
  isAllowedLocalFrontendUrl,
  isAllowedOriginUrl,
  isSafeExternalUrl,
} = require('../electron/navigation-policy.cjs');

test('allows only the packaged shell and settings documents', () => {
  const indexPath = path.resolve('/application/dist/index.html');
  const shellUrl = pathToFileURL(indexPath);
  shellUrl.searchParams.set('view', 'shell');
  const settingsUrl = pathToFileURL(indexPath);
  settingsUrl.searchParams.set('view', 'settings');

  assert.equal(isAllowedLocalFrontendUrl(shellUrl.toString(), indexPath), true);
  assert.equal(isAllowedLocalFrontendUrl(settingsUrl.toString(), indexPath), true);
  assert.equal(
    isAllowedLocalFrontendUrl('https://example.com/index.html?view=shell', indexPath),
    false,
  );
  assert.equal(
    isAllowedLocalFrontendUrl('file:///application/dist/other.html?view=shell', indexPath),
    false,
  );
  assert.equal(
    isAllowedLocalFrontendUrl(`${pathToFileURL(indexPath)}?view=admin`, indexPath),
    false,
  );
});

test('matches Chat OS origins and restricts external URL protocols', () => {
  assert.equal(
    isAllowedOriginUrl('http://127.0.0.1:43123/projects', 'http://127.0.0.1:43123'),
    true,
  );
  assert.equal(
    isAllowedOriginUrl('https://example.com/', 'http://127.0.0.1:43123'),
    false,
  );
  assert.equal(isSafeExternalUrl('https://example.com/docs'), true);
  assert.equal(isSafeExternalUrl('http://127.0.0.1:8080'), true);
  assert.equal(isSafeExternalUrl('file:///etc/passwd'), false);
  assert.equal(isSafeExternalUrl('chatos-local-connector://auth?ticket=secret'), false);
});
