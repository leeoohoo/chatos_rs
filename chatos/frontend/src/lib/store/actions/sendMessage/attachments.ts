// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ApiAttachmentPayload,
  PreviewAttachment,
} from './types';

const MAX_EMBED_BYTES = 5 * 1024 * 1024; // 5MB 上限，超出不内联内容
export const DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES = 20 * 1024 * 1024;
const REQUEST_BODY_BASE64_OVERHEAD_RATIO = 4 / 3;
const REQUEST_BODY_FIXED_OVERHEAD_BYTES = 1024 * 1024;
export const DEFAULT_AGENT_REQUEST_BODY_MAX_BYTES = Math.ceil(
  DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES * REQUEST_BODY_BASE64_OVERHEAD_RATIO
    + REQUEST_BODY_FIXED_OVERHEAD_BYTES,
);
const INLINE_IMAGE_TARGET_BYTES = 850 * 1024;
const INLINE_IMAGE_MAX_EDGE_STEPS = [1920, 1600, 1280, 1024];
const INLINE_IMAGE_QUALITY_STEPS = [0.84, 0.72, 0.6, 0.48, 0.36];

interface PrepareAttachmentsOptions {
  dropImagesWhenUnsupported?: boolean;
  maxTotalBytes?: number;
  uploadAttachments?: (files: File[]) => Promise<ApiAttachmentPayload[]>;
}

const readFileAsDataUrl = (file: File) => new Promise<string>((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result));
  reader.onerror = reject;
  reader.readAsDataURL(file);
});

const readFileAsText = (file: File) => new Promise<string>((resolve, reject) => {
  const reader = new FileReader();
  reader.onload = () => resolve(String(reader.result));
  reader.onerror = reject;
  reader.readAsText(file);
});

export const formatPayloadBytes = (bytes: number) => {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${bytes} B`;
};

export const resolveAttachmentTotalMaxBytes = (value: unknown): number => {
  const numeric = typeof value === 'number'
    ? value
    : (typeof value === 'string' ? Number(value.trim()) : NaN);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    return DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES;
  }
  return Math.max(1, Math.round(numeric));
};

export const estimateAttachmentTotalBytes = (attachments: File[]): number => (
  (Array.isArray(attachments) ? attachments : []).reduce((total, file) => {
    const size = typeof file?.size === 'number' && Number.isFinite(file.size)
      ? file.size
      : 0;
    return total + Math.max(0, size);
  }, 0)
);

export const requestPayloadMaxBytesForAttachmentTotalLimit = (maxAttachmentBytes: number): number => (
  Math.ceil(
    resolveAttachmentTotalMaxBytes(maxAttachmentBytes) * REQUEST_BODY_BASE64_OVERHEAD_RATIO
      + REQUEST_BODY_FIXED_OVERHEAD_BYTES,
  )
);

export const assertAttachmentsWithinTotalBudget = (
  attachments: File[],
  maxBytes = DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
) => {
  const resolvedMaxBytes = resolveAttachmentTotalMaxBytes(maxBytes);
  const totalBytes = estimateAttachmentTotalBytes(attachments);
  if (totalBytes <= resolvedMaxBytes) {
    return;
  }

  throw new Error(
    `附件总大小为 ${formatPayloadBytes(totalBytes)}，超过 ${formatPayloadBytes(resolvedMaxBytes)} 限制，请减少文件数量或换更小的文件重试。`,
  );
};

const renameFileWithExtension = (name: string, extension: string) => {
  const trimmedName = String(name || '').trim();
  if (!trimmedName) {
    return `attachment.${extension}`;
  }
  const nextBaseName = trimmedName.replace(/\.[^.]+$/u, '');
  return `${nextBaseName || 'attachment'}.${extension}`;
};

const blobToFile = (blob: Blob, originalFile: File, mimeType: string) => new File(
  [blob],
  mimeType === 'image/jpeg'
    ? renameFileWithExtension(originalFile.name, 'jpg')
    : originalFile.name,
  {
    type: mimeType,
    lastModified: originalFile.lastModified,
  },
);

const canvasToBlob = (
  canvas: HTMLCanvasElement,
  mimeType: string,
  quality?: number,
) => new Promise<Blob | null>((resolve) => {
  canvas.toBlob((blob) => resolve(blob), mimeType, quality);
});

const scaleDimensions = (width: number, height: number, maxEdge: number) => {
  if (width <= 0 || height <= 0) {
    return { width: 0, height: 0 };
  }
  const largestEdge = Math.max(width, height);
  if (largestEdge <= maxEdge) {
    return { width, height };
  }
  const ratio = maxEdge / largestEdge;
  return {
    width: Math.max(1, Math.round(width * ratio)),
    height: Math.max(1, Math.round(height * ratio)),
  };
};

interface LoadedImageSource {
  source: CanvasImageSource;
  width: number;
  height: number;
  dispose: () => void;
}

const loadImageSource = async (file: File): Promise<LoadedImageSource | null> => {
  if (typeof createImageBitmap === 'function') {
    const bitmap = await createImageBitmap(file);
    return {
      source: bitmap,
      width: bitmap.width,
      height: bitmap.height,
      dispose: () => {
        if (typeof bitmap.close === 'function') {
          bitmap.close();
        }
      },
    };
  }

  if (
    typeof document === 'undefined'
    || typeof Image === 'undefined'
    || typeof URL === 'undefined'
    || typeof URL.createObjectURL !== 'function'
  ) {
    return null;
  }

  const url = URL.createObjectURL(file);
  try {
    const image = await new Promise<HTMLImageElement>((resolve, reject) => {
      const nextImage = new Image();
      nextImage.onload = () => resolve(nextImage);
      nextImage.onerror = () => reject(new Error('图片读取失败'));
      nextImage.src = url;
    });
    return {
      source: image,
      width: image.naturalWidth || image.width,
      height: image.naturalHeight || image.height,
      dispose: () => URL.revokeObjectURL(url),
    };
  } catch (error) {
    URL.revokeObjectURL(url);
    throw error;
  }
};

const canCompressImage = (file: File) => (
  file.type.startsWith('image/')
  && file.type !== 'image/gif'
  && file.type !== 'image/svg+xml'
);

const compressImageForInlineTransport = async (file: File): Promise<File> => {
  if (!canCompressImage(file)) {
    return file;
  }

  const loaded = await loadImageSource(file).catch(() => null);
  if (!loaded) {
    return file;
  }

  const shouldResize = Math.max(loaded.width, loaded.height) > INLINE_IMAGE_MAX_EDGE_STEPS[0];
  if (!shouldResize && file.size <= INLINE_IMAGE_TARGET_BYTES) {
    loaded.dispose();
    return file;
  }

  const canvas = typeof document !== 'undefined'
    ? document.createElement('canvas')
    : null;
  if (!canvas) {
    loaded.dispose();
    return file;
  }

  let bestBlob: Blob | null = null;
  let bestSize = file.size;

  try {
    for (const maxEdge of INLINE_IMAGE_MAX_EDGE_STEPS) {
      const { width, height } = scaleDimensions(loaded.width, loaded.height, maxEdge);
      if (width <= 0 || height <= 0) {
        continue;
      }

      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext('2d');
      if (!context) {
        continue;
      }

      context.clearRect(0, 0, width, height);
      context.fillStyle = '#ffffff';
      context.fillRect(0, 0, width, height);
      context.drawImage(loaded.source, 0, 0, width, height);

      for (const quality of INLINE_IMAGE_QUALITY_STEPS) {
        const blob = await canvasToBlob(canvas, 'image/jpeg', quality);
        if (!blob) {
          continue;
        }
        if (blob.size < bestSize) {
          bestBlob = blob;
          bestSize = blob.size;
        }
        if (blob.size <= INLINE_IMAGE_TARGET_BYTES) {
          return blobToFile(blob, file, 'image/jpeg');
        }
      }
    }
  } finally {
    loaded.dispose();
  }

  if (bestBlob && bestBlob.size < file.size) {
    return blobToFile(bestBlob, file, 'image/jpeg');
  }

  return file;
};

const prepareFileForInlineTransport = async (file: File): Promise<File> => {
  if (file.type.startsWith('image/')) {
    return compressImageForInlineTransport(file);
  }
  return file;
};

const makePreviewAttachment = async (file: File): Promise<PreviewAttachment> => {
  const isImage = file.type.startsWith('image/');
  const isAudio = file.type.startsWith('audio/');
  const url = isImage || isAudio ? await readFileAsDataUrl(file) : URL.createObjectURL(file);
  return {
    id: `att_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    messageId: 'temp',
    type: isImage ? 'image' : (isAudio ? 'audio' : 'file'),
    name: file.name,
    url,
    size: file.size,
    mimeType: file.type,
    createdAt: new Date(),
  };
};

const makeApiAttachment = async (file: File): Promise<ApiAttachmentPayload> => {
  const isImage = file.type.startsWith('image/');
  const isText = file.type.startsWith('text/') || file.type === 'application/json';
  const isPdf = file.type === 'application/pdf' || /\.pdf$/i.test(file.name);
  const isDocx = file.type === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' || /\.docx$/i.test(file.name);

  if (isImage) {
    const dataUrl = await readFileAsDataUrl(file);
    return { name: file.name, mimeType: file.type, size: file.size, type: 'image', dataUrl };
  }
  if (isText) {
    const text = await readFileAsText(file);
    return { name: file.name, mimeType: file.type, size: file.size, type: 'file', text };
  }
  if ((isPdf || isDocx) && file.size <= MAX_EMBED_BYTES) {
    // 小体积 docx/pdf 以内联 base64，由后端负责抽取正文
    const dataUrl = await readFileAsDataUrl(file);
    return { name: file.name, mimeType: isPdf ? 'application/pdf' : 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', size: file.size, type: 'file', dataUrl };
  }
  // 其他或超限：仅元数据
  return { name: file.name, mimeType: file.type, size: file.size, type: 'file' };
};

export const estimateJsonPayloadBytes = (payload: unknown): number => {
  const serialized = JSON.stringify(payload ?? null);
  return new TextEncoder().encode(serialized).length;
};

export const assertPayloadWithinTransportBudget = (
  payload: unknown,
  maxBytes = DEFAULT_AGENT_REQUEST_BODY_MAX_BYTES,
) => {
  const payloadBytes = estimateJsonPayloadBytes(payload);
  if (payloadBytes <= maxBytes) {
    return;
  }

  throw new Error(
    `图片或附件过大，压缩后请求体仍有 ${formatPayloadBytes(payloadBytes)}，超过 ${formatPayloadBytes(maxBytes)} 限制，请减少图片数量或换更小的图片重试。`,
  );
};

export async function prepareAttachmentsForStreaming(
  attachments: File[],
  supportsImages: boolean,
  options: PrepareAttachmentsOptions = {},
): Promise<{
  previewAttachments: PreviewAttachment[];
  apiAttachments: ApiAttachmentPayload[];
}> {
  const dropImagesWhenUnsupported = options.dropImagesWhenUnsupported !== false;
  const safeAttachments = Array.isArray(attachments)
    ? (
      supportsImages || !dropImagesWhenUnsupported
        ? attachments
        : attachments.filter((file) => !file.type.startsWith('image/'))
    )
    : [];

  assertAttachmentsWithinTotalBudget(
    safeAttachments,
    options.maxTotalBytes ?? DEFAULT_ATTACHMENT_TOTAL_MAX_BYTES,
  );

  const previewAttachments = await Promise.all((safeAttachments || []).map(makePreviewAttachment));
  if (safeAttachments.length > 0 && options.uploadAttachments) {
    const apiAttachments = await options.uploadAttachments(safeAttachments);
    return { previewAttachments, apiAttachments };
  }

  const transportAttachments = await Promise.all(
    (safeAttachments || []).map(prepareFileForInlineTransport),
  );
  const apiAttachments = await Promise.all((transportAttachments || []).map(makeApiAttachment));

  return { previewAttachments, apiAttachments };
}
