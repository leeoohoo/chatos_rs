export type DocumentErrorCode =
  | 'INVALID_ARGUMENT'
  | 'INVALID_PATH'
  | 'WORKSPACE_NOT_CONFIGURED'
  | 'ARTIFACT_NOT_CONFIGURED'
  | 'FILE_NOT_FOUND'
  | 'UNSUPPORTED_FORMAT'
  | 'FORMAT_MISMATCH'
  | 'FILE_TOO_LARGE'
  | 'UNSAFE_ARCHIVE'
  | 'INVALID_DOCUMENT'
  | 'ENCRYPTED_FILE'
  | 'OUTPUT_EXISTS'
  | 'ENGINE_UNAVAILABLE'
  | 'ENGINE_ERROR'
  | 'ENGINE_TIMEOUT'
  | 'VALIDATION_FAILED'
  | 'INTERNAL_ERROR';

export class DocumentError extends Error {
  readonly code: DocumentErrorCode;
  readonly details: Record<string, unknown> | undefined;

  constructor(code: DocumentErrorCode, message: string, details?: Record<string, unknown>) {
    super(message);
    this.name = 'DocumentError';
    this.code = code;
    this.details = details;
  }
}

export function toPublicError(error: unknown): {
  ok: false;
  error: { code: DocumentErrorCode; message: string; details?: Record<string, unknown> };
} {
  if (error instanceof DocumentError) {
    return {
      ok: false,
      error: {
        code: error.code,
        message: error.message,
        ...(error.details ? { details: error.details } : {})
      }
    };
  }
  return {
    ok: false,
    error: {
      code: 'INTERNAL_ERROR',
      message: 'The document operation failed unexpectedly.'
    }
  };
}
