import React, { Suspense } from 'react';

interface LazyMarkdownRendererProps {
  content: string;
  isStreaming?: boolean;
  className?: string;
  onApplyCode?: (code: string, language: string) => void;
}

const MarkdownRenderer = React.lazy(async () => {
  const module = await import('./MarkdownRenderer');
  return { default: module.MarkdownRenderer };
});

export const LazyMarkdownRenderer: React.FC<LazyMarkdownRendererProps> = (props) => {
  return (
    <Suspense fallback={<div className="text-xs text-muted-foreground">Loading content...</div>}>
      <MarkdownRenderer {...props} />
    </Suspense>
  );
};

export default LazyMarkdownRenderer;
