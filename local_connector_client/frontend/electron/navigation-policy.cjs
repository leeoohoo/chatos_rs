// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const { pathToFileURL } = require('node:url');

const LOCAL_FRONTEND_VIEWS = new Set(['shell', 'settings']);

function isAllowedLocalFrontendUrl(candidateUrl, indexPath) {
  try {
    const candidate = new URL(candidateUrl);
    const expected = pathToFileURL(indexPath);
    return candidate.protocol === 'file:'
      && candidate.host === expected.host
      && candidate.pathname === expected.pathname
      && LOCAL_FRONTEND_VIEWS.has(candidate.searchParams.get('view') || '');
  } catch {
    return false;
  }
}

function isAllowedOriginUrl(candidateUrl, expectedOrigin) {
  try {
    return new URL(candidateUrl).origin === expectedOrigin;
  } catch {
    return false;
  }
}

function isSafeExternalUrl(candidateUrl) {
  try {
    const protocol = new URL(candidateUrl).protocol;
    return protocol === 'https:' || protocol === 'http:';
  } catch {
    return false;
  }
}

module.exports = {
  isAllowedLocalFrontendUrl,
  isAllowedOriginUrl,
  isSafeExternalUrl,
};
