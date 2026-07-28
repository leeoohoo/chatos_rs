// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const fs = require('node:fs');
const path = require('node:path');

const DEFAULT_RUNTIME_SETTINGS = Object.freeze({
  developer_mode: false,
  browser_full_cdp_access_enabled: false,
  developer_cloud_base_url: 'http://127.0.0.1:39230',
  developer_user_service_base_url: 'http://127.0.0.1:39190',
  developer_chatos_web_url: 'http://127.0.0.1:8088',
});

function defaultStatePath() {
  return path.join(
    process.env.HOME || process.env.USERPROFILE || '.',
    '.chatos',
    'local_connector',
    'state.json',
  );
}

function runtimeStatePath() {
  const configured = String(process.env.LOCAL_CONNECTOR_STATE_PATH || '').trim();
  return configured || defaultStatePath();
}

function readStateValue(statePath = runtimeStatePath()) {
  try {
    if (!fs.existsSync(statePath)) {
      return {};
    }
    const parsed = JSON.parse(fs.readFileSync(statePath, 'utf8'));
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function normalizeLoopbackDeveloperUrl(value, fallback) {
  if (typeof value !== 'string') {
    return fallback;
  }
  const trimmed = value.trim().replace(/\/+$/, '');
  try {
    const parsed = new URL(trimmed);
    const host = parsed.hostname.toLowerCase();
    const loopback = host === 'localhost' || host === '127.0.0.1' || host.startsWith('127.');
    return parsed.protocol === 'http:' && loopback ? trimmed : fallback;
  } catch {
    return fallback;
  }
}

function normalizeRuntimeSettings(value = {}) {
  const input = value && typeof value === 'object' && !Array.isArray(value) ? value : {};
  return {
    developer_mode: Boolean(input.developer_mode),
    browser_full_cdp_access_enabled: Boolean(input.browser_full_cdp_access_enabled),
    developer_cloud_base_url: normalizeLoopbackDeveloperUrl(
      input.developer_cloud_base_url,
      DEFAULT_RUNTIME_SETTINGS.developer_cloud_base_url,
    ),
    developer_user_service_base_url: normalizeLoopbackDeveloperUrl(
      input.developer_user_service_base_url,
      DEFAULT_RUNTIME_SETTINGS.developer_user_service_base_url,
    ),
    developer_chatos_web_url: normalizeLoopbackDeveloperUrl(
      input.developer_chatos_web_url,
      DEFAULT_RUNTIME_SETTINGS.developer_chatos_web_url,
    ),
  };
}

function readRuntimeSettings(statePath = runtimeStatePath()) {
  const state = readStateValue(statePath);
  return normalizeRuntimeSettings(state.runtime_settings);
}

function writeStateValue(statePath, value) {
  const parent = path.dirname(statePath);
  fs.mkdirSync(parent, { recursive: true });
  const temporary = path.join(parent, `.state.json.partial-${process.pid}`);
  try {
    fs.writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
    fs.renameSync(temporary, statePath);
    if (process.platform !== 'win32') {
      fs.chmodSync(statePath, 0o600);
    }
  } finally {
    fs.rmSync(temporary, { force: true });
  }
}

function updateRuntimeSettings(patch, statePath = runtimeStatePath()) {
  const current = readRuntimeSettings(statePath);
  const next = normalizeRuntimeSettings({
    ...current,
    ...(patch && typeof patch === 'object' && !Array.isArray(patch) ? patch : {}),
  });
  if (
    next.browser_full_cdp_access_enabled
    && !current.browser_full_cdp_access_enabled
    && !patch?.acknowledge_browser_full_cdp_risk
  ) {
    throw new Error('enabling full browser CDP access requires explicit risk acknowledgement');
  }
  const state = readStateValue(statePath);
  writeStateValue(statePath, {
    ...state,
    runtime_settings: next,
  });
  return next;
}

module.exports = {
  DEFAULT_RUNTIME_SETTINGS,
  normalizeRuntimeSettings,
  readRuntimeSettings,
  runtimeStatePath,
  updateRuntimeSettings,
};
