// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const {
  readRuntimeSettings,
  updateRuntimeSettings,
} = require('../electron/runtime-settings-store.cjs');

test('persists runtime settings without overwriting unrelated local state', (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-runtime-settings-'));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const statePath = path.join(root, 'state.json');
  fs.writeFileSync(statePath, JSON.stringify({
    auth: { access_token: 'keep-token' },
    runtime_settings: { developer_mode: false },
  }));

  const saved = updateRuntimeSettings({ developer_mode: true }, statePath);

  assert.equal(saved.developer_mode, true);
  const state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  assert.equal(state.auth.access_token, 'keep-token');
  assert.equal(state.runtime_settings.developer_mode, true);
  assert.equal(state.runtime_settings.developer_cloud_base_url, 'http://127.0.0.1:39230');
});

test('normalizes developer URLs to loopback defaults', (context) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'chatos-runtime-settings-'));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const statePath = path.join(root, 'state.json');

  const saved = updateRuntimeSettings({
    developer_cloud_base_url: 'https://attacker.example.com',
    developer_user_service_base_url: 'http://192.168.1.10:39190',
    developer_chatos_web_url: 'http://localhost:8088/',
  }, statePath);

  assert.equal(saved.developer_cloud_base_url, 'http://127.0.0.1:39230');
  assert.equal(saved.developer_user_service_base_url, 'http://127.0.0.1:39190');
  assert.equal(saved.developer_chatos_web_url, 'http://localhost:8088');
  assert.deepEqual(readRuntimeSettings(statePath), saved);
});
