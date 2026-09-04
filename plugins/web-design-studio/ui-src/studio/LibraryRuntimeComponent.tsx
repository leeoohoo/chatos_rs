import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { WebDesignComponent } from '../../src/schema';

export function LibraryRuntimeComponent({ component, preview, slotContent }: {
  component: WebDesignComponent;
  preview: boolean;
  slotContent?: ReactNode;
}) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const props = component.library?.props ?? {};
  const instance = component.id;
  const source = useMemo(() => {
    const url = new URL(window.location.href);
    url.search = '';
    url.hash = '';
    url.searchParams.set('library-runtime', '1');
    url.searchParams.set('library', component.library?.name ?? '');
    url.searchParams.set('component', String(props.componentSlug ?? ''));
    url.searchParams.set('instance', instance);
    return url.toString();
  }, [component.library?.name, instance, props.componentSlug]);

  const sendProps = () => frameRef.current?.contentWindow?.postMessage({
    source: 'web-design-studio',
    instance,
    type: 'props',
    props
  }, window.location.origin);

  useEffect(() => {
    const receive = (message: MessageEvent) => {
      if (message.origin !== window.location.origin || message.data?.source !== 'web-design-library-runtime' || message.data.instance !== instance) return;
      if (message.data.event === 'ready' || message.data.event === 'mounted') {
        setStatus('ready');
        sendProps();
      }
      if (message.data.event === 'error') setStatus('error');
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  });

  useEffect(sendProps, [props, instance]);

  return <div className={`library-runtime-component status-${status}`}>
    <iframe
      ref={frameRef}
      src={source}
      title={`${component.library?.name ?? 'UI'} ${component.library?.component ?? 'component'}`}
      onLoad={sendProps}
      sandbox="allow-scripts allow-same-origin"
      style={{ pointerEvents: preview ? 'auto' : 'none' }}
    />
    {status === 'loading' && <div className="library-runtime-status">正在载入官方组件…</div>}
    {status === 'error' && <div className="library-runtime-status error">官方组件运行失败</div>}
    {slotContent && <div className="library-runtime-slot">{slotContent}</div>}
  </div>;
}
