import {
  ApiRequestError,
  buildParsedJsonErrorPayload,
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
    const fallbackMessage = `HTTP error! status: ${response.status}`;
    const {
      message,
      code,
      payload,
    } = buildParsedJsonErrorPayload(text, fallbackMessage);
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
