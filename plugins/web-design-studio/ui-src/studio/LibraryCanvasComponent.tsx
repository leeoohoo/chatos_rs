import { lazy, Suspense, type ReactNode } from 'react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';

const AntdCanvasComponent = lazy(() => import('./AntdCanvasComponent').then((module) => ({ default: module.AntdCanvasComponent })));
const ChakraCanvasComponent = lazy(() => import('./ChakraCanvasComponent').then((module) => ({ default: module.ChakraCanvasComponent })));
const ShadcnCanvasComponent = lazy(() => import('./ShadcnCanvasComponent').then((module) => ({ default: module.ShadcnCanvasComponent })));
const CreativeCanvasComponent = lazy(() => import('./CreativeCanvasComponent').then((module) => ({ default: module.CreativeCanvasComponent })));
const DaisyCanvasComponent = lazy(() => import('./DaisyCanvasComponent').then((module) => ({ default: module.DaisyCanvasComponent })));

export function LibraryCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const library = component.library?.name;
  const renderer = library === 'antd'
    ? <AntdCanvasComponent component={component} preview={preview} showcase={showcase} tokens={tokens} slotContent={slotContent} />
    : library === 'chakra'
      ? <ChakraCanvasComponent component={component} preview={preview} showcase={showcase} tokens={tokens} slotContent={slotContent} />
      : library === 'shadcn'
        ? <ShadcnCanvasComponent component={component} preview={preview} showcase={showcase} tokens={tokens} slotContent={slotContent} />
        : library === 'daisyui'
          ? <DaisyCanvasComponent component={component} preview={preview} showcase={showcase} tokens={tokens} slotContent={slotContent} />
        : library === 'magicui' || library === 'spell' || library === 'inspira'
          ? <CreativeCanvasComponent component={component} preview={preview} showcase={showcase} tokens={tokens} slotContent={slotContent} />
        : null;
  return <Suspense fallback={<span className="library-loading-placeholder">加载组件…</span>}>{renderer}</Suspense>;
}
