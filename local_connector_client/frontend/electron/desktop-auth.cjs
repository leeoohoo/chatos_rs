// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

function responseErrorMessage(response) {
  const fallback = `Local Connector login synchronization failed (HTTP ${response?.status || 0})`;
  const raw = typeof response?.body === 'string' ? response.body.trim() : '';
  if (!raw) {
    return fallback;
  }
  try {
    const payload = JSON.parse(raw);
    const detail = typeof payload?.detail === 'string' ? payload.detail.trim() : '';
    const message = typeof payload?.message === 'string' ? payload.message.trim() : '';
    const error = typeof payload?.error === 'string' ? payload.error.trim() : '';
    return detail || message || error || fallback;
  } catch {
    return raw || fallback;
  }
}

function parsedSuccessBody(response) {
  const raw = typeof response?.body === 'string' ? response.body.trim() : '';
  if (!raw) {
    return { ok: true };
  }
  try {
    return JSON.parse(raw);
  } catch {
    return { ok: true };
  }
}

function isTransientIpcError(error) {
  return ['ENOENT', 'ECONNREFUSED', 'ECONNRESET', 'EPIPE', 'ERROR_PIPE_BUSY'].includes(error?.code);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function localConnectorCoreUnavailableError(error) {
  const message = error instanceof Error ? error.message : String(error || 'unknown error');
  const wrapped = new Error(
    `Local Connector Core is not ready. Please wait a moment and try again. Original error: ${message}`,
  );
  if (error?.code) {
    wrapped.code = error.code;
  }
  return wrapped;
}

function createDesktopTicketAuthenticator({
  sendIpcHttpRequest,
  localApiHeaders,
  getCloudBaseUrl,
  retryAttempts = 30,
  retryDelayMs = 100,
}) {
  return async function authenticateDesktopTicket(ticket) {
    const trimmed = String(ticket || '').trim();
    if (!trimmed) {
      throw new Error('Local Connector authorization ticket is empty');
    }
    const request = {
      endpoint: '/api/local/auth/desktop-ticket',
      method: 'POST',
      headers: localApiHeaders(true),
      body: JSON.stringify({
        cloud_base_url: getCloudBaseUrl(),
        ticket: trimmed,
      }),
    };
    let response = null;
    let lastError = null;
    for (let attempt = 0; attempt < retryAttempts; attempt += 1) {
      try {
        response = await sendIpcHttpRequest(request);
        lastError = null;
        break;
      } catch (error) {
        lastError = error;
        if (!isTransientIpcError(error)) {
          throw error;
        }
        if (attempt + 1 >= retryAttempts) {
          throw localConnectorCoreUnavailableError(error);
        }
        await delay(retryDelayMs);
      }
    }
    if (!response) {
      throw localConnectorCoreUnavailableError(lastError);
    }
    if (!response?.ok) {
      throw new Error(responseErrorMessage(response));
    }
    return parsedSuccessBody(response);
  };
}

module.exports = {
  createDesktopTicketAuthenticator,
  isTransientIpcError,
};
