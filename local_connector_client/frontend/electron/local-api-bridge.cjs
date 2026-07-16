// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const http = require('node:http');

const MAX_API_ENDPOINT_LENGTH = 4096;
const MAX_API_REQUEST_BODY_BYTES = 2 * 1024 * 1024;
const MAX_API_RESPONSE_BODY_BYTES = 8 * 1024 * 1024;
const DEFAULT_API_TIMEOUT_MS = 15_000;
const LOCAL_CHAT_TIMEOUT_MS = 10 * 60 * 1000;
const LOCAL_TOOLS_TIMEOUT_MS = 2 * 60 * 1000;
const ALLOWED_API_METHODS = new Set(['GET', 'POST', 'DELETE', 'PUT', 'PATCH']);
const FORWARDED_RENDERER_HEADERS = new Set(['accept', 'content-type']);

function normalizeApiEndpoint(endpoint, endpointPrefix) {
  if (typeof endpoint !== 'string') {
    throw new Error('Local API endpoint must be a string');
  }
  const trimmed = endpoint.trim();
  if (!trimmed || trimmed.length > MAX_API_ENDPOINT_LENGTH || /^https?:\/\//i.test(trimmed)) {
    throw new Error('Local API endpoint is not allowed');
  }
  const url = new URL(trimmed, 'http://local-connector.internal');
  if (!url.pathname.startsWith(endpointPrefix)) {
    throw new Error('Local API endpoint is outside the local connector API');
  }
  return `${url.pathname}${url.search}`;
}

function normalizeApiMethod(method) {
  const normalized = typeof method === 'string' && method.trim()
    ? method.trim().toUpperCase()
    : 'GET';
  if (!ALLOWED_API_METHODS.has(normalized)) {
    throw new Error(`Local API method is not allowed: ${normalized}`);
  }
  return normalized;
}

function normalizeApiRequestBody(body) {
  if (body === undefined || body === null) {
    return null;
  }
  if (typeof body !== 'string') {
    throw new Error('Local API request body must be a string');
  }
  if (Buffer.byteLength(body, 'utf8') > MAX_API_REQUEST_BODY_BYTES) {
    throw new Error('Local API request body is too large');
  }
  return body;
}

function localApiTimeoutMs(endpoint) {
  if (endpoint === '/api/local/runtime/chat/send') {
    return LOCAL_CHAT_TIMEOUT_MS;
  }
  if (/^\/api\/local\/runtime\/sessions\/[^/]+\/review-repair(?:\?|$)/.test(endpoint)) {
    return LOCAL_CHAT_TIMEOUT_MS;
  }
  if (/^\/api\/local\/runtime\/sessions\/[^/]+\/tools(?:\?|$)/.test(endpoint)) {
    return LOCAL_TOOLS_TIMEOUT_MS;
  }
  return DEFAULT_API_TIMEOUT_MS;
}

function localApiHeaders(desktopAuthToken, hasBody) {
  const headers = {
    Accept: 'application/json',
    Host: 'local-connector.ipc',
    Authorization: `Bearer ${desktopAuthToken}`,
  };
  if (hasBody) {
    headers['Content-Type'] = 'application/json';
  }
  return headers;
}

function sanitizeRendererHeaders(inputHeaders, hasBody, desktopAuthToken) {
  const headers = localApiHeaders(desktopAuthToken, false);

  if (inputHeaders && typeof inputHeaders === 'object') {
    for (const [name, value] of Object.entries(inputHeaders)) {
      const lowerName = name.toLowerCase();
      if (!FORWARDED_RENDERER_HEADERS.has(lowerName)) {
        continue;
      }
      if (typeof value === 'string' && value.length <= 1024) {
        headers[name] = value;
      }
    }
  }

  if (hasBody && !Object.keys(headers).some((name) => name.toLowerCase() === 'content-type')) {
    headers['Content-Type'] = 'application/json';
  }
  return headers;
}

function isTransientIpcError(error) {
  return ['ENOENT', 'ECONNREFUSED', 'ECONNRESET', 'EPIPE', 'ERROR_PIPE_BUSY'].includes(error?.code);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function createLocalApiBridge({
  getIpcEndpoint,
  getDesktopAuthToken,
  isTrustedSender,
  endpointPrefix = '/api/local/',
}) {
  async function requestLocalApiOverIpc(payload) {
    if (!isTrustedSender(payload?.sender)) {
      throw new Error('Local API access is only available to the main connector window');
    }

    const endpoint = normalizeApiEndpoint(payload.endpoint, endpointPrefix);
    const method = normalizeApiMethod(payload.method);
    const body = normalizeApiRequestBody(payload.body);
    const headers = sanitizeRendererHeaders(payload.headers, body !== null, getDesktopAuthToken());
    let lastError = null;

    for (let attempt = 0; attempt < 30; attempt += 1) {
      try {
        return await sendIpcHttpRequest({ endpoint, method, headers, body });
      } catch (error) {
        lastError = error;
        if (!isTransientIpcError(error)) {
          throw error;
        }
        await delay(100);
      }
    }
    throw lastError || new Error('Local connector core is not available');
  }

  function sendIpcHttpRequest({ endpoint, method, headers, body }) {
    const ipcEndpoint = getIpcEndpoint();
    if (!ipcEndpoint) {
      return Promise.reject(new Error('Local connector core IPC endpoint is not initialized'));
    }

    return new Promise((resolve, reject) => {
      let settled = false;
      const request = http.request(
        {
          socketPath: ipcEndpoint,
          method,
          path: endpoint,
          headers,
          timeout: localApiTimeoutMs(endpoint),
        },
        (response) => {
          const chunks = [];
          let totalBytes = 0;

          response.on('data', (chunk) => {
            totalBytes += chunk.length;
            if (totalBytes > MAX_API_RESPONSE_BODY_BYTES) {
              settled = true;
              request.destroy(new Error('Local API response body is too large'));
              reject(new Error('Local API response body is too large'));
              return;
            }
            chunks.push(chunk);
          });

          response.on('end', () => {
            if (settled) {
              return;
            }
            settled = true;
            const responseBody = Buffer.concat(chunks).toString('utf8');
            resolve({
              status: response.statusCode || 0,
              ok: Boolean(response.statusCode && response.statusCode >= 200 && response.statusCode < 300),
              headers: response.headers,
              body: responseBody,
            });
          });
        },
      );

      request.on('timeout', () => {
        request.destroy(new Error('Local API request timed out'));
      });
      request.on('error', (error) => {
        if (!settled) {
          settled = true;
          reject(error);
        }
      });

      if (body !== null) {
        request.write(body);
      }
      request.end();
    });
  }

  return {
    delay,
    requestLocalApiOverIpc,
    sendIpcHttpRequest,
    localApiHeaders: (hasBody) => localApiHeaders(getDesktopAuthToken(), hasBody),
  };
}

module.exports = {
  createLocalApiBridge,
};
