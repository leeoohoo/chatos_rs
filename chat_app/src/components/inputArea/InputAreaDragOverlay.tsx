import { formatFileSize } from '../../lib/utils';

interface InputAreaDragOverlayProps {
  maxFileBytes: number;
  maxTotalBytes: number;
  maxAttachments: number;
}

export default function InputAreaDragOverlay({
  maxFileBytes,
  maxTotalBytes,
  maxAttachments,
}: InputAreaDragOverlayProps) {
  return (
    <div className="absolute inset-0 bg-primary/10 border-2 border-dashed border-primary rounded-lg flex items-center justify-center">
      <div className="text-center">
        <svg className="w-8 h-8 mx-auto text-primary mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
        </svg>
        <p className="text-sm font-medium text-primary">Drop files here to attach</p>
        <p className="text-[11px] text-muted-foreground mt-1">
          单文件≤{formatFileSize(maxFileBytes)}，总计≤{formatFileSize(maxTotalBytes)}，最多 {maxAttachments} 个
        </p>
      </div>
    </div>
  );
}
