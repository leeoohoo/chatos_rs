import { Suspense, type ReactNode } from 'react';
import type { WebDesignComponent, WebDesignTokens } from '../../src/schema';
import { hasOfficialRuntimeComponent, officialRuntimePresentation } from '../library-runtime/registry';
import { LibraryRuntimeComponent } from './LibraryRuntimeComponent';

export function LibraryCanvasComponent({ component, preview, showcase = false, tokens, slotContent = {} }: {
  component: WebDesignComponent;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
}) {
  const library = component.library?.name;
  const componentSlug = String(component.library?.props.componentSlug ?? component.library?.component.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase() ?? '');
  const officialPresentation = officialRuntimePresentation(library, componentSlug);
  const renderer = hasOfficialRuntimeComponent(library, componentSlug)
    ? <LibraryRuntimeComponent component={{ ...component, library: component.library ? { ...component.library, props: { ...component.library.props, componentSlug } } : undefined }} preview={preview} slotContent={slotContent.content} layout={officialPresentation?.layout ?? 'intrinsic'} autoSize={showcase} />
    : null;
  return <Suspense fallback={<span className="library-loading-placeholder">加载组件…</span>}>{renderer}</Suspense>;
}
