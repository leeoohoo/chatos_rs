// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { EyeOff, LoaderCircle, MonitorUp, PictureInPicture2 } from 'lucide-react';

import { api, type PluginRuntimeVisualSession } from '../api';

type PreviewMode = 'expanded' | 'collapsed';

export function PluginVisualPreview() {
  const [session, setSession] = React.useState<PluginRuntimeVisualSession | null>(null);
  const [mode, setMode] = React.useState<PreviewMode>('expanded');
  const currentSessionId = React.useRef<string | null>(null);

  React.useEffect(() => {
    document.documentElement.classList.add('visualPreviewDocument');
    return () => document.documentElement.classList.remove('visualPreviewDocument');
  }, []);

  React.useEffect(() => {
    let stopped = false;
    let timer: number | undefined;

    const refresh = async () => {
      try {
        const response = await api.pluginRuntimeVisualSession();
        if (stopped) return;
        const next = response.session;
        if (next?.session_id && next.session_id !== currentSessionId.current) {
          currentSessionId.current = next.session_id;
          setMode('expanded');
        }
        if (!next) {
          currentSessionId.current = null;
        }
        setSession(next);
      } catch {
        if (!stopped) {
          currentSessionId.current = null;
          setSession(null);
        }
      } finally {
        if (!stopped) {
          timer = window.setTimeout(refresh, 450);
        }
      }
    };

    void refresh();
    return () => {
      stopped = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, []);

  React.useEffect(() => {
    const nextMode = session ? mode : 'hidden';
    void window.chatosLocalConnector?.setVisualPreviewMode?.(nextMode);
  }, [mode, session]);

  if (!session) return null;

  if (mode === 'collapsed') {
    return (
      <button
        className="visualPreviewCollapsed"
        type="button"
        onClick={() => setMode('expanded')}
        title="显示电脑使用画面"
      >
        <span className="visualPreviewLiveDot" />
        <PictureInPicture2 size={15} />
        <span>电脑使用</span>
      </button>
    );
  }

  return (
    <section className="visualPreviewPanel" aria-label="电脑使用实时画面">
      <header className="visualPreviewHeader">
        <span className="visualPreviewIcon"><MonitorUp size={16} /></span>
        <div className="visualPreviewTitle">
          <strong>{session.title}</strong>
          <small>
            <span className="visualPreviewLiveDot" />
            {session.target_app ? `正在操作 ${session.target_app}` : '正在本机运行'}
          </small>
        </div>
        <button
          type="button"
          onClick={() => setMode('collapsed')}
          title="隐藏画中画"
          aria-label="隐藏画中画"
        >
          <EyeOff size={15} />
        </button>
      </header>
      <div className="visualPreviewFrame">
        {session.frame_data_url ? (
          <img src={session.frame_data_url} alt="Computer Use 隔离环境实时预览" />
        ) : (
          <div className="visualPreviewWaiting">
            <LoaderCircle size={22} />
            <span>正在建立隔离画面…</span>
          </div>
        )}
      </div>
      <footer className="visualPreviewFooter">
        <span>{session.plugin_id}</span>
        <span>仅在本机显示</span>
      </footer>
    </section>
  );
}
