import { DocumentError } from '../errors.js';

/** Classify known engine failures without returning document paths or raw stderr. */
export function officeCliFailure(error: unknown): DocumentError {
  const value = error as { killed?: boolean; signal?: string; code?: unknown; stdout?: string; stderr?: string } | null;
  if (value?.killed || value?.signal === 'SIGTERM') {
    return new DocumentError('ENGINE_TIMEOUT', 'OfficeCLI exceeded the operation timeout.');
  }
  let engineCode: string | undefined;
  try {
    const output = JSON.parse(value?.stdout ?? '');
    if (typeof output?.error?.code === 'string') engineCode = output.error.code;
  } catch { /* Non-JSON output is intentionally not exposed. */ }
  if (engineCode === 'no_screenshot_backend') {
    return new DocumentError('ENGINE_UNAVAILABLE',
      'The headless document renderer could not start. Check the installed Chromium browser and the Plugin isolated cache permissions.',
      { engineCode, operation: 'render', ...(typeof value?.code === 'number' ? { exitCode: value.code } : {}) });
  }
  return new DocumentError('ENGINE_ERROR', 'OfficeCLI could not complete the requested document operation.',
    typeof value?.code === 'number' ? { exitCode: value.code } : undefined);
}
