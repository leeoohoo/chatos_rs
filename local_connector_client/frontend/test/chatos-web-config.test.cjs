// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const assert = require('node:assert/strict');
const test = require('node:test');

const {
  DEFAULT_PRODUCTION_CHATOS_WEB_URL,
  DEFAULT_PRODUCTION_CLOUD_BASE_URL,
  resolveChatosWebUrl,
  resolveCloudBaseUrl,
} = require('../electron/chatos-web-config.cjs');

test('developer mode always resolves the ChatOS page to a loopback frontend', () => {
  assert.equal(
    resolveChatosWebUrl({
      developerMode: true,
      env: {},
      runtimeSettings: {
        developer_chatos_web_url: 'http://localhost:8089',
      },
    }),
    'http://localhost:8089',
  );
  assert.equal(
    resolveChatosWebUrl({
      developerMode: true,
      env: {
        LOCAL_CONNECTOR_DEVELOPER_CHATOS_WEB_URL: 'https://app.jgoool.com',
      },
      runtimeSettings: {
        developer_chatos_web_url: 'http://127.0.0.1:8088',
      },
    }),
    'http://127.0.0.1:8088',
  );
});

test('production mode resolves hosted ChatOS and connector service URLs from remote config', () => {
  assert.equal(
    resolveChatosWebUrl({
      developerMode: false,
      env: {
        LOCAL_CONNECTOR_CHATOS_WEB_URL: 'https://chatos.example.com/workbench',
      },
    }),
    'https://chatos.example.com/workbench',
  );
  assert.equal(
    resolveCloudBaseUrl({
      developerMode: false,
      env: {
        LOCAL_CONNECTOR_CLOUD_BASE_URL: 'https://connector.example.com/api',
      },
    }),
    'https://connector.example.com/api',
  );
});

test('invalid production URLs fall back to the built-in hosted defaults', () => {
  assert.equal(
    resolveChatosWebUrl({
      developerMode: false,
      env: {
        LOCAL_CONNECTOR_CHATOS_WEB_URL: 'file:///tmp/chatos/index.html',
      },
    }),
    DEFAULT_PRODUCTION_CHATOS_WEB_URL,
  );
  assert.equal(
    resolveCloudBaseUrl({
      developerMode: false,
      env: {
        LOCAL_CONNECTOR_CLOUD_BASE_URL: 'chatos-local-connector://auth',
      },
    }),
    DEFAULT_PRODUCTION_CLOUD_BASE_URL,
  );
});
