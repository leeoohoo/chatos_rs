import { memo, Suspense, type ReactNode } from 'react';
import type { WebDesignTokens } from '../../src/schema';
import type { LibraryPreviewPointerEvent, LibraryPreviewSelection } from '../library-runtime/element-selection';
import { hasOfficialRuntimeComponent, officialRuntimePresentation } from '../library-runtime/registry';
import { LibraryRuntimeComponent, type LibraryRuntimeDescriptor } from './LibraryRuntimeComponent';
import { sameLibraryRuntimeBoundary } from './render-boundaries';

interface LibraryCanvasComponentProps {
  component: LibraryRuntimeDescriptor;
  preview: boolean;
  showcase?: boolean;
  tokens?: WebDesignTokens;
  slotContent?: Record<string, ReactNode>;
  pickItems?: boolean;
  onPickItem?: (selection: LibraryPreviewSelection) => void;
  onPickPointerEvent?: (event: LibraryPreviewPointerEvent) => void;
  onContentHeight?: (height: number) => void;
}

function LibraryCanvasComponentInner({ component, preview, showcase = false, tokens, slotContent = {}, pickItems = false, onPickItem, onPickPointerEvent, onContentHeight }: LibraryCanvasComponentProps) {
  const library = component.library?.name;
  const componentSlug = String(component.library?.props.componentSlug ?? component.library?.component.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase() ?? '');
  const officialPresentation = officialRuntimePresentation(library, componentSlug);
  const renderer = hasOfficialRuntimeComponent(library, componentSlug)
    ? <LibraryRuntimeComponent component={{ ...component, library: component.library ? { ...component.library, props: { ...component.library.props, componentSlug } } : undefined }} preview={preview} slotContent={slotContent.content} layout={officialPresentation?.layout ?? 'intrinsic'} autoSize={showcase} pickItems={pickItems} onPickItem={onPickItem} onPickPointerEvent={onPickPointerEvent} onContentHeight={onContentHeight} />
    : null;
  return <Suspense fallback={<span className="library-loading-placeholder">加载组件…</span>}>{renderer}</Suspense>;
}

export const LibraryCanvasComponent = memo(LibraryCanvasComponentInner, (left, right) => sameLibraryRuntimeBoundary(
  { component: left.component, preview: left.preview, showcase: left.showcase ?? false, tokens: left.tokens, pickItems: left.pickItems ?? false, slotContent: left.slotContent ?? {} },
  { component: right.component, preview: right.preview, showcase: right.showcase ?? false, tokens: right.tokens, pickItems: right.pickItems ?? false, slotContent: right.slotContent ?? {} }
));
