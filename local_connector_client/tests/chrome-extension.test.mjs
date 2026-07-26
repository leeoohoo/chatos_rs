// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const clientDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const extensionDir = path.join(clientDir, 'chrome_extension');

function extensionIdFromKey(key) {
  const digest = crypto.createHash('sha256').update(Buffer.from(key, 'base64')).digest().subarray(0, 16);
  const alphabet = 'abcdefghijklmnop';
  return [...digest].map((byte) => `${alphabet[byte >> 4]}${alphabet[byte & 15]}`).join('');
}

test('Chrome extension has a stable identity and least-privilege permissions', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(extensionDir, 'manifest.json'), 'utf8'));
  assert.equal(manifest.manifest_version, 3);
  assert.equal(manifest.version, '1.3.0');
  assert.equal(extensionIdFromKey(manifest.key), 'eebkndlcocijhemeddoifdchmnifcgcm');
  assert.deepEqual(
    [...manifest.permissions].sort(),
    ['activeTab', 'nativeMessaging', 'scripting', 'storage'].sort(),
  );
  assert.deepEqual(manifest.optional_host_permissions, ['http://*/*', 'https://*/*']);
  for (const forbidden of [
    'tabs', 'cookies', 'history', 'downloads', 'bookmarks', 'clipboardRead',
    'clipboardWrite', 'debugger', 'webRequest', 'webRequestBlocking', '<all_urls>',
  ]) {
    assert.equal(manifest.permissions.includes(forbidden), false);
  }
});

test('Chrome extension scripts parse and keep site grants behind user gestures', () => {
  for (const script of ['background.js', 'popup.js']) {
    const result = spawnSync(process.execPath, ['--check', path.join(extensionDir, script)], {
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr);
  }
  const background = fs.readFileSync(path.join(extensionDir, 'background.js'), 'utf8');
  const popup = fs.readFileSync(path.join(extensionDir, 'popup.js'), 'utf8');
  assert.match(background, /com\.chatos\.chrome/);
  assert.match(background, /chrome\.runtime\.connectNative/);
  assert.match(background, /chrome\.scripting\.executeScript/);
  assert.match(background, /chrome\.tabs\.captureVisibleTab/);
  assert.match(background, /Chrome navigation is limited to the currently authorized exact origin/);
  assert.match(background, /The Chrome target changed\. Capture a fresh snapshot/);
  assert.match(background, /type === 'password'/);
  assert.match(background, /crypto\.subtle\.digest\('SHA-256'/);
  assert.match(background, /message\.type === 'cancel'/);
  assert.match(background, /chrome\.tabs\.goBack/);
  assert.match(background, /chrome\.tabs\.goForward/);
  assert.match(background, /chrome\.tabs\.update\(tabId, \{ active: true \}\)/);
  assert.match(background, /action === 'select_option'/);
  assert.match(background, /window\.scrollBy/);
  assert.doesNotMatch(background, /new KeyboardEvent/);
  assert.match(background, /MAX_UPLOAD_BYTES = 10 \* 1024 \* 1024/);
  assert.match(background, /MAX_DOWNLOAD_BYTES = 10 \* 1024 \* 1024/);
  assert.match(background, /MAX_DOWNLOAD_DATA_URL_CHARS = 14 \* 1024 \* 1024 \+ 4096/);
  for (const command of ['download_begin', 'download_chunk', 'download_finish', 'download_abort']) {
    assert.match(background, new RegExp("case '" + command + "'"));
  }
  assert.match(background, /credentials: 'include'/);
  assert.match(background, /response\.body\?\.getReader\(\)/);
  assert.match(background, /const boundedAnchorHref = \(element\) =>/);
  assert.match(background, /boundedAnchorHref\(element\) \|\| ''/);
  assert.match(background, /input\[type=\"password\"\]/);
  assert.doesNotMatch(background, /chrome\.cookies/);
  assert.doesNotMatch(background, /chrome\.history/);
  assert.doesNotMatch(background, /chrome\.downloads/);
  assert.match(popup, /chrome\.permissions\.request/);
  assert.match(popup, /chrome\.permissions\.remove/);
});
