import {
  ApiRequestError,
  guessFilenameFromPath,
  parseFilenameFromContentDisposition,
} from './shared';

export interface BinaryApiContext {
  baseUrl: string;
  accessToken: string | null;
  applyRefreshedAccessToken: (response: Response) => void;
}

export interface FsDownloadResult {
  blob: Blob;
  filename: string;
  contentType: string;
}

export const downloadFsEntry = async (
  context: BinaryApiContext,
  path: string,
): Promise<FsDownloadResult> => {
  const qs = `?path=${encodeURIComponent(path)}`;
  const headers = new Headers();
  if (context.accessToken) {
    headers.set('Authorization', `Bearer ${context.accessToken}`);
  }
  const response = await fetch(`${context.baseUrl}/fs/download${qs}`, {
    method: 'GET',
    headers,
  });

  context.applyRefreshedAccessToken(response);

  if (!response.ok) {
    const text = await response.text();
    let message = `HTTP error! status: ${response.status}`;
    let code: string | undefined;
    let payload: any = null;
    if (text) {
      try {
        const parsed = JSON.parse(text);
        payload = parsed;
        code = typeof parsed?.code === 'string' ? parsed.code : undefined;
        message =
          (typeof parsed?.error === 'string' && parsed.error) ||
          (typeof parsed?.message === 'string' && parsed.message) ||
          message;
      } catch {
        message = text;
      }
    }
    throw new ApiRequestError(message, {
      status: response.status,
      code,
      payload,
    });
  }

  const blob = await response.blob();
  const contentType = response.headers.get('content-type') || blob.type || 'application/octet-stream';
  const nameFromHeader = parseFilenameFromContentDisposition(response.headers.get('content-disposition'));
  let filename = nameFromHeader || guessFilenameFromPath(path);
  if (contentType.includes('application/zip') && !filename.toLowerCase().endsWith('.zip')) {
    filename = `${filename}.zip`;
  }
  return {
    blob,
    filename,
    contentType,
  };
};
