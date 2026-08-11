// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const {
  DEFAULT_RUNTIME_SETTINGS,
  normalizeLoopbackDeveloperUrl,
} = require('./runtime-settings-store.cjs');

const DEFAULT_PRODUCTION_CHATOS_WEB_URL = 'https://app.jgoool.com';
const DEFAULT_PRODUCTION_CLOUD_BASE_URL = 'https://local-connector.jgoool.com';

function normalizeRemoteUrl(value, fallback) {
  if (typeof value !== 'string') {
    return fallback;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    return fallback;
  }
  try {
    const parsed = new URL(trimmed);
    if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
      return fallback;
    }
    parsed.hash = '';
    return parsed.toString();
  } catch {
    return fallback;
  }
}

function resolveChatosWebUrl({ developerMode, env = process.env, runtimeSettings = DEFAULT_RUNTIME_SETTINGS }) {
  if (developerMode) {
    const configuredByEnv = normalizeLoopbackDeveloperUrl(
      env.LOCAL_CONNECTOR_DEVELOPER_CHATOS_WEB_URL,
      '',
    );
    if (configuredByEnv) {
      return configuredByEnv;
    }
    return normalizeLoopbackDeveloperUrl(
      runtimeSettings.developer_chatos_web_url,
      DEFAULT_RUNTIME_SETTINGS.developer_chatos_web_url,
    );
  }
  return normalizeRemoteUrl(
    env.LOCAL_CONNECTOR_CHATOS_WEB_URL,
    DEFAULT_PRODUCTION_CHATOS_WEB_URL,
  );
}

function resolveCloudBaseUrl({ developerMode, env = process.env, runtimeSettings = DEFAULT_RUNTIME_SETTINGS }) {
  if (developerMode) {
    const configuredByEnv = normalizeLoopbackDeveloperUrl(
      env.LOCAL_CONNECTOR_DEVELOPER_CLOUD_BASE_URL,
      '',
    );
    if (configuredByEnv) {
      return configuredByEnv;
    }
    return normalizeLoopbackDeveloperUrl(
      runtimeSettings.developer_cloud_base_url,
      DEFAULT_RUNTIME_SETTINGS.developer_cloud_base_url,
    );
  }
  return normalizeRemoteUrl(
    env.LOCAL_CONNECTOR_CLOUD_BASE_URL,
    DEFAULT_PRODUCTION_CLOUD_BASE_URL,
  );
}

module.exports = {
  DEFAULT_PRODUCTION_CHATOS_WEB_URL,
  DEFAULT_PRODUCTION_CLOUD_BASE_URL,
  resolveChatosWebUrl,
  resolveCloudBaseUrl,
};
