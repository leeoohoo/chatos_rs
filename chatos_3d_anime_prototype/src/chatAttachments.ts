import type { ChatAttachmentPayload } from './types';

export const MAX_ATTACHMENT_TOTAL_BYTES = 20 * 1024 * 1024;
const MAX_INLINE_FILE_BYTES = 5 * 1024 * 1024;
const IMAGE_TARGET_BYTES = 900 * 1024;
const IMAGE_MAX_EDGE = 1600;

const readAsDataUrl = (file: Blob): Promise<string> => new Promise((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result || ''));
  reader.onerror = () => reject(reader.error || new Error('附件读取失败'));
  reader.readAsDataURL(file);
});

const readAsText = (file: Blob): Promise<string> => new Promise((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result || ''));
  reader.onerror = () => reject(reader.error || new Error('附件读取失败'));
  reader.readAsText(file);
});

const canvasToBlob = (canvas: HTMLCanvasElement, quality: number): Promise<Blob | null> => (
  new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', quality))
);

const compressImage = async (file: File): Promise<Blob> => {
  if (!file.type.startsWith('image/') || file.type === 'image/gif' || file.type === 'image/svg+xml') {
    return file;
  }
  if (file.size <= IMAGE_TARGET_BYTES) return file;

  const bitmap = typeof createImageBitmap === 'function' ? await createImageBitmap(file).catch(() => null) : null;
  if (!bitmap) return file;
  const ratio = Math.min(1, IMAGE_MAX_EDGE / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * ratio));
  const height = Math.max(1, Math.round(bitmap.height * ratio));
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) {
    bitmap.close();
    return file;
  }
  context.fillStyle = '#fff';
  context.fillRect(0, 0, width, height);
  context.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();

  for (const quality of [0.82, 0.7, 0.58, 0.46]) {
    const blob = await canvasToBlob(canvas, quality);
    if (blob && blob.size <= IMAGE_TARGET_BYTES) return blob;
  }
  const fallback = await canvasToBlob(canvas, 0.42);
  return fallback && fallback.size < file.size ? fallback : file;
};

export const formatFileSize = (bytes: number): string => {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
};

export const validateAttachmentFiles = (files: File[]): void => {
  const total = files.reduce((sum, file) => sum + Math.max(0, file.size), 0);
  if (total > MAX_ATTACHMENT_TOTAL_BYTES) {
    throw new Error(`附件总大小 ${formatFileSize(total)}，超过 20 MB 限制`);
  }
};

export const prepareAttachmentPayloads = async (files: File[]): Promise<ChatAttachmentPayload[]> => {
  validateAttachmentFiles(files);
  return Promise.all(files.map(async (file) => {
    const isImage = file.type.startsWith('image/');
    const isAudio = file.type.startsWith('audio/');
    const isText = file.type.startsWith('text/') || file.type === 'application/json' || /\.(md|tsx?|jsx?|css|html|ya?ml|toml|rs|py|go|java|sql|sh)$/i.test(file.name);
    const isInlineDocument = file.size <= MAX_INLINE_FILE_BYTES && (/\.pdf$/i.test(file.name) || /\.docx$/i.test(file.name));
    const type: ChatAttachmentPayload['type'] = isImage ? 'image' : isAudio ? 'audio' : 'file';
    const base = { name: file.name, mimeType: file.type || 'application/octet-stream', size: file.size, type };
    if (isImage) {
      const compressed = await compressImage(file);
      return { ...base, mimeType: compressed === file ? base.mimeType : 'image/jpeg', size: compressed.size, dataUrl: await readAsDataUrl(compressed) };
    }
    if (isText) return { ...base, text: await readAsText(file) };
    if (isInlineDocument) return { ...base, dataUrl: await readAsDataUrl(file) };
    return base;
  }));
};
