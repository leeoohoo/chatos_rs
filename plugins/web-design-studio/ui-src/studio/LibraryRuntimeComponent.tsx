import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { WebDesignComponent } from '../../src/schema';

export function LibraryRuntimeComponent({ component, preview, slotContent, layout = 'fill', autoSize = false }: {
  component: WebDesignComponent;
  preview: boolean;
  slotContent?: ReactNode;
  layout?: 'fill' | 'intrinsic';
  autoSize?: boolean;
}) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const recoveryTimerRef = useRef<number | undefined>(undefined);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [contentHeight, setContentHeight] = useState<number>();
  const [recoveryAttempt, setRecoveryAttempt] = useState(0);
  const props = component.library?.props ?? {};
  const library = component.library?.name ?? '';
  const slug = String(props.componentSlug ?? '');
  const instance = component.id;
  const detachedContent = props.editorDetachedContent === true;
  const source = useMemo(() => {
    const url = new URL(window.location.href);
    url.search = '';
    url.hash = '';
    url.searchParams.set('library-runtime', '1');
    url.searchParams.set('library', library);
    url.searchParams.set('component', slug);
    url.searchParams.set('instance', instance);
    url.searchParams.set('layout', layout);
    if (recoveryAttempt > 0) url.searchParams.set('runtime-retry', String(recoveryAttempt));
    return url.toString();
  }, [instance, layout, library, recoveryAttempt, slug]);

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
          setContentHeight(Math.max(118, Math.min(720, Math.ceil(measuredHeight))));
        }
      }
      if (message.data.event === 'error') setStatus('error');
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  });

  useEffect(sendProps, [props, instance, component.content]);

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

  const autoSizeStyle = autoSize && contentHeight ? { height: contentHeight, minHeight: contentHeight } : undefined;
  return <div className={`library-runtime-component library-${library} component-${slug} status-${status} ${autoSize ? 'auto-size' : ''}`} style={autoSizeStyle}>
    <iframe
      ref={frameRef}
      src={source}
      title={`${component.library?.name ?? 'UI'} ${component.library?.component ?? 'component'}`}
      onLoad={handleFrameLoad}
      sandbox="allow-scripts allow-same-origin"
      aria-hidden={detachedContent || undefined}
      style={{ pointerEvents: preview ? 'auto' : 'none', visibility: detachedContent ? 'hidden' : undefined }}
    />
    {!detachedContent && status === 'loading' && <div className="library-runtime-status">正在载入官方组件…</div>}
    {!detachedContent && status === 'error' && <div className="library-runtime-status error">官方组件运行失败</div>}
    {slotContent && <div className="library-runtime-slot">{slotContent}</div>}
  </div>;
}
