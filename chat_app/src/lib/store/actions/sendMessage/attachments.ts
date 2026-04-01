import type {
  ApiAttachmentPayload,
  PreviewAttachment,
} from './types';

const MAX_EMBED_BYTES = 5 * 1024 * 1024; // 5MB 上限，超出不内联内容

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

export async function prepareAttachmentsForStreaming(
  attachments: File[],
  supportsImages: boolean,
): Promise<{
  previewAttachments: PreviewAttachment[];
  apiAttachments: ApiAttachmentPayload[];
}> {
  const safeAttachments = Array.isArray(attachments)
    ? (
      supportsImages
        ? attachments
        : attachments.filter((file) => !file.type.startsWith('image/'))
    )
    : [];

  const previewAttachments = await Promise.all((safeAttachments || []).map(makePreviewAttachment));
  const apiAttachments = await Promise.all((safeAttachments || []).map(makeApiAttachment));

  return { previewAttachments, apiAttachments };
}
