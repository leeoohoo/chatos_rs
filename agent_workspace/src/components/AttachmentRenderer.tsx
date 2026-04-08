import React, { useEffect } from 'react';
import { cn, formatFileSize } from '../lib/utils';
import type { Attachment } from '../types';

interface AttachmentRendererProps {
  attachment: Attachment;
  // 如果是用户自己发送的附件，则隐藏下载按钮
  isUser?: boolean;
  customRenderer?: (attachment: Attachment) => React.ReactNode;
  className?: string;
}

export const AttachmentRenderer: React.FC<AttachmentRendererProps> = ({
  attachment,
  isUser = false,
  customRenderer,
  className,
}) => {
  const isImage = attachment.type === 'image';
  const isAudio = attachment.type === 'audio';
  // const isFile = attachment.type === 'file';

  useEffect(() => {
    return () => {
      if (attachment.url && attachment.url.startsWith('blob:')) {
        URL.revokeObjectURL(attachment.url);
      }
    };
  }, [attachment.url]);

  // 使用自定义渲染器
  if (customRenderer) {
    return <div>{customRenderer(attachment)}</div>;
  }

  const getFileIcon = () => {
    if (isImage) {
      return (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      );
    }
    
    if (isAudio) {
      return (
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
        </svg>
      );
    }
    
    return (
      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
      </svg>
    );
  };

  return (
    <div className={cn(
      'p-2 bg-muted rounded-lg border max-w-sm',
      className
    )}>
      <div className="flex items-center gap-2">
      {/* 文件图标 */}
      <div className="flex-shrink-0 text-muted-foreground">
        {getFileIcon()}
      </div>
      
      {/* 文件信息 */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium truncate">
            {attachment.name}
          </span>
          <span className="text-xs text-muted-foreground flex-shrink-0">
            {formatFileSize(attachment.size)}
          </span>
        </div>
        
        {attachment.mimeType && (
          <div className="text-xs text-muted-foreground mt-0.5">
            {attachment.mimeType}
          </div>
        )}
      </div>
      
      {/* 操作按钮：用户自己的非图片附件隐藏下载按钮 */}
      <div className="flex-shrink-0">
        {isImage ? (
          <button
            onClick={() => attachment.url && window.open(attachment.url, '_blank')}
            className="p-1 hover:bg-background rounded transition-colors"
            title="View image"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
            </svg>
          </button>
        ) : (
          !isUser && (
            <a
              href={attachment.url}
              download={attachment.name}
              className="p-1 hover:bg-background rounded transition-colors inline-block"
              title="Download file"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
            </a>
          )
        )}
      </div>
      </div>

      {/* 图片预览 */}
      {isImage && (
        <div className="w-full mt-2">
          <img
            src={attachment.url}
            alt={attachment.name}
            className="max-w-full h-auto rounded border object-contain"
            style={{ maxHeight: '200px' }}
            loading="lazy"
          />
        </div>
      )}
      
      {/* 音频播放器 */}
      {isAudio && (
        <div className="w-full mt-2">
          <audio
            controls
            className="w-full"
            preload="metadata"
          >
            <source src={attachment.url} type={attachment.mimeType} />
            Your browser does not support the audio element.
          </audio>
        </div>
      )}
    </div>
  );
};

export default AttachmentRenderer;
