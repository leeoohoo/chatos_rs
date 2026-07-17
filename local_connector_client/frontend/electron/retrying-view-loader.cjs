// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

function attachRetryingViewLoader({
  webContents,
  load,
  shouldRetry,
  retryDelayMs = 1000,
  onLoadError = () => {},
  setTimer = setTimeout,
  clearTimer = clearTimeout,
}) {
  let disposed = false;
  let retryTimer = null;

  const cancelRetry = () => {
    if (retryTimer !== null) {
      clearTimer(retryTimer);
      retryTimer = null;
    }
  };

  const scheduleRetry = (error) => {
    onLoadError(error);
    if (disposed || retryTimer !== null || !shouldRetry()) {
      return;
    }
    retryTimer = setTimer(() => {
      retryTimer = null;
      startLoad();
    }, retryDelayMs);
  };

  const startLoad = () => {
    cancelRetry();
    if (disposed || webContents.isDestroyed()) {
      return;
    }
    try {
      Promise.resolve(load()).catch(scheduleRetry);
    } catch (error) {
      scheduleRetry(error);
    }
  };

  const onDidFailLoad = (_event, errorCode, errorDescription, validatedUrl, isMainFrame) => {
    if (!isMainFrame || errorCode === -3) {
      return;
    }
    scheduleRetry(new Error(
      `Chat OS page load failed (${errorCode}): ${errorDescription}; ${validatedUrl}`,
    ));
  };
  const onDidFinishLoad = () => cancelRetry();

  const dispose = () => {
    if (disposed) {
      return;
    }
    disposed = true;
    cancelRetry();
    webContents.removeListener('did-fail-load', onDidFailLoad);
    webContents.removeListener('did-finish-load', onDidFinishLoad);
  };

  webContents.on('did-fail-load', onDidFailLoad);
  webContents.on('did-finish-load', onDidFinishLoad);
  webContents.once('destroyed', dispose);
  startLoad();

  return { dispose, reload: startLoad };
}

module.exports = { attachRetryingViewLoader };
