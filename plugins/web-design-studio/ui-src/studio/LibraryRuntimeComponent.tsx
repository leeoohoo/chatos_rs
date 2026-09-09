import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { libraryPreviewSelection, translateLibraryPreviewPointerEvent, type LibraryPreviewPointerEvent, type LibraryPreviewSelection } from '../library-runtime/element-selection';

export interface LibraryRuntimeDescriptor {
  id: string;
  width: number;
  height: number;
  content: string;
  library?: {
    name: string;
    component: string;
    props: Record<string, unknown>;
  };
}

export function LibraryRuntimeComponent({ component, preview, slotContent, layout = 'fill', autoSize = false, pickItems = false, onPickItem, onPickPointerEvent, onContentHeight }: {
  component: LibraryRuntimeDescriptor;
  preview: boolean;
  slotContent?: ReactNode;
  layout?: 'fill' | 'intrinsic';
  autoSize?: boolean;
  pickItems?: boolean;
  onPickItem?: (selection: LibraryPreviewSelection) => void;
  onPickPointerEvent?: (event: LibraryPreviewPointerEvent) => void;
  onContentHeight?: (height: number) => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const frameRef = useRef<HTMLIFrameElement>(null);
  const recoveryTimerRef = useRef<number | undefined>(undefined);
  const onPickItemRef = useRef(onPickItem);
  const onPickPointerEventRef = useRef(onPickPointerEvent);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [contentHeight, setContentHeight] = useState<number>();
  const [hostSize, setHostSize] = useState({ width: component.width, height: component.height });
  const [recoveryAttempt, setRecoveryAttempt] = useState(0);
  const props = component.library?.props ?? {};
  const library = component.library?.name ?? '';
  const slug = String(props.componentSlug ?? '');
  const instance = component.id;
  const detachedContent = props.editorDetachedContent === true;
  const selectedElement = libraryPreviewSelection(props.registryElement);
  useEffect(() => { onPickItemRef.current = onPickItem; }, [onPickItem]);
  useEffect(() => { onPickPointerEventRef.current = onPickPointerEvent; }, [onPickPointerEvent]);
  const source = useMemo(() => {
    const url = new URL(window.location.href);
    url.search = '';
    url.hash = '';
    url.searchParams.set('library-runtime', '1');
    url.searchParams.set('library', library);
    url.searchParams.set('component', slug);
    url.searchParams.set('instance', instance);
    url.searchParams.set('layout', layout);
    if (pickItems) url.searchParams.set('picker', '1');
    if (recoveryAttempt > 0) url.searchParams.set('runtime-retry', String(recoveryAttempt));
    return url.toString();
  }, [instance, layout, library, pickItems, recoveryAttempt, slug]);

  const sendProps = () => frameRef.current?.contentWindow?.postMessage({
    source: 'web-design-studio',
    instance,
    type: 'props',
    props,
    content: component.content
  }, window.location.origin);

  useEffect(() => {
    const receive = (message: MessageEvent) => {
      if (message.origin !== window.location.origin || message.data?.source !== 'web-design-library-runtime' || message.data.instance !== instance) return;
      if (message.data.event === 'request-props') sendProps();
      if (message.data.event === 'ready' || message.data.event === 'mounted') {
        setStatus('ready');
        sendProps();
      }
      if (message.data.event === 'content-size' && autoSize) {
        const measuredHeight = Number(message.data.detail?.height);
        if (Number.isFinite(measuredHeight) && measuredHeight > 0) {
          const nextHeight = Math.max(118, Math.min(720, Math.ceil(measuredHeight)));
          setContentHeight(nextHeight);
          onContentHeight?.(nextHeight);
        }
      }
      if (message.data.event === 'preview-select' && pickItems) {
        const selection = libraryPreviewSelection(message.data.detail);
        if (selection) onPickItemRef.current?.(selection);
      }
      if (message.data.event === 'preview-pointer' && pickItems) {
        const frame = frameRef.current;
        if (!frame) return;
        const bounds = frame.getBoundingClientRect();
        const pointerEvent = translateLibraryPreviewPointerEvent(message.data.detail, bounds.left, bounds.top);
        if (pointerEvent) onPickPointerEventRef.current?.(pointerEvent);
      }
      if (message.data.event === 'error') setStatus('error');
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  }, [autoSize, instance, onContentHeight, pickItems]);

  useEffect(sendProps, [props, instance, component.content]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const height = entry.contentRect.height;
      if (width > 0 && height > 0) setHostSize({ width, height });
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  useEffect(() => () => window.clearTimeout(recoveryTimerRef.current), []);

  const handleFrameLoad = () => {
    sendProps();
    const bodyText = frameRef.current?.contentDocument?.body?.innerText?.trim() ?? '';
    const missingStudioShell = bodyText.includes('ENOENT') && bodyText.includes('index.html');
    if (!missingStudioShell) return;
    window.clearTimeout(recoveryTimerRef.current);
    if (recoveryAttempt >= 60) {
      setStatus('error');
      return;
    }
    setStatus('loading');
    recoveryTimerRef.current = window.setTimeout(() => setRecoveryAttempt((attempt) => attempt + 1), 750);
  };

  const autoSizeStyle = autoSize && contentHeight && !selectedElement ? { height: contentHeight, minHeight: contentHeight } : undefined;
  const selectionScaleX = selectedElement ? hostSize.width / selectedElement.width : 1;
  const selectionScaleY = selectedElement ? hostSize.height / selectedElement.height : 1;
  const selectedFrameStyle = selectedElement ? {
    width: selectedElement.viewportWidth,
    height: selectedElement.viewportHeight,
    maxWidth: 'none',
    maxHeight: 'none',
    position: 'absolute' as const,
    left: 0,
    top: 0,
    transformOrigin: '0 0',
    transform: `matrix(${selectionScaleX},0,0,${selectionScaleY},${-selectedElement.x * selectionScaleX},${-selectedElement.y * selectionScaleY})`
  } : undefined;
  return <div ref={hostRef} className={`library-runtime-component library-${library} component-${slug} status-${status} ${autoSize ? 'auto-size' : ''} ${selectedElement ? 'selected-registry-element' : ''}`} style={autoSizeStyle}>
    <iframe
      ref={frameRef}
      src={source}
      title={`${component.library?.name ?? 'UI'} ${component.library?.component ?? 'component'}`}
      onLoad={handleFrameLoad}
      sandbox="allow-scripts allow-same-origin"
      aria-hidden={detachedContent || undefined}
      style={{ ...selectedFrameStyle, pointerEvents: preview ? 'auto' : 'none', visibility: detachedContent ? 'hidden' : undefined }}
    />
    {!detachedContent && status === 'loading' && <div className="library-runtime-status">正在载入官方组件…</div>}
    {!detachedContent && status === 'error' && <div className="library-runtime-status error">官方组件运行失败</div>}
    {slotContent && <div className="library-runtime-slot">{slotContent}</div>}
  </div>;
}
