import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import hljs from 'highlight.js';
import ApiClient from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type { Project, FsEntry, FsReadResult, ChangeLogItem } from '../types';
import { cn, formatFileSize } from '../lib/utils';

interface ProjectExplorerProps {
  project: Project | null;
  className?: string;
}

const normalizeEntry = (raw: any): FsEntry => ({
  name: raw?.name ?? '',
  path: raw?.path ?? '',
  isDir: raw?.is_dir ?? raw?.isDir ?? false,
  size: raw?.size ?? null,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
});

const normalizeFile = (raw: any): FsReadResult => ({
  path: raw?.path ?? '',
  name: raw?.name ?? '',
  size: raw?.size ?? 0,
  contentType: raw?.content_type ?? raw?.contentType ?? 'application/octet-stream',
  isBinary: raw?.is_binary ?? raw?.isBinary ?? false,
  modifiedAt: raw?.modified_at ?? raw?.modifiedAt ?? null,
  content: raw?.content ?? '',
});

const normalizeChangeLog = (raw: any): ChangeLogItem => ({
  id: raw?.id ?? '',
  serverName: raw?.server_name ?? raw?.serverName ?? '',
  path: raw?.path ?? '',
  action: raw?.action ?? '',
  bytes: raw?.bytes ?? 0,
  sha256: raw?.sha256 ?? null,
  diff: raw?.diff ?? null,
  sessionId: raw?.session_id ?? raw?.sessionId ?? null,
  runId: raw?.run_id ?? raw?.runId ?? null,
  createdAt: raw?.created_at ?? raw?.createdAt ?? '',
  sessionTitle: raw?.session_title ?? raw?.sessionTitle ?? null,
});

const EXT_LANGUAGE_MAP: Record<string, string> = {
  rs: 'rust',
  toml: 'toml',
  lock: 'toml',
  md: 'markdown',
  txt: 'plaintext',
  json: 'json',
  yml: 'yaml',
  yaml: 'yaml',
  xml: 'xml',
  html: 'xml',
  htm: 'xml',
  vue: 'vue',
  svelte: 'svelte',
  astro: 'astro',
  css: 'css',
  scss: 'scss',
  less: 'less',
  js: 'javascript',
  jsx: 'javascript',
  ts: 'typescript',
  tsx: 'typescript',
  mjs: 'javascript',
  cjs: 'javascript',
  py: 'python',
  go: 'go',
  java: 'java',
  kt: 'kotlin',
  swift: 'swift',
  c: 'c',
  cc: 'cpp',
  cpp: 'cpp',
  h: 'cpp',
  hpp: 'cpp',
  cs: 'csharp',
  php: 'php',
  rb: 'ruby',
  sh: 'bash',
  bash: 'bash',
  zsh: 'bash',
  ps1: 'powershell',
  bat: 'dos',
  sql: 'sql',
  ini: 'ini',
  conf: 'ini',
  env: 'ini',
  log: 'plaintext',
  gradle: 'gradle',
  properties: 'ini',
  cfg: 'ini',
  proto: 'protobuf',
  graphql: 'graphql',
  dart: 'dart',
  lua: 'lua',
  r: 'r',
  m: 'objectivec',
  mm: 'objectivec',
  scala: 'scala',
  cmake: 'cmake',
  make: 'makefile',
  dockerfile: 'dockerfile',
};

const getHighlightLanguage = (filename: string): string | null => {
  const lower = filename.toLowerCase();
  if (lower === 'dockerfile') return hljs.getLanguage('dockerfile') ? 'dockerfile' : null;
  if (lower === 'makefile') return hljs.getLanguage('makefile') ? 'makefile' : null;
  if (lower === 'cmakelists.txt') return hljs.getLanguage('cmake') ? 'cmake' : null;
  const parts = lower.split('.');
  if (parts.length < 2) return null;
  const ext = parts[parts.length - 1];
  const lang = EXT_LANGUAGE_MAP[ext];
  if (!lang) return null;
  return hljs.getLanguage(lang) ? lang : null;
};

const escapeHtml = (value: string) => (
  value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
);

export const ProjectExplorer: React.FC<ProjectExplorerProps> = ({ project, className }) => {
  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || new ApiClient(), [apiClientFromContext]);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const resizeStartX = useRef(0);
  const resizeStartWidth = useRef(0);

  const [entriesMap, setEntriesMap] = useState<Record<string, FsEntry[]>>({});
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<FsReadResult | null>(null);
  const [loadingFile, setLoadingFile] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [changeLogs, setChangeLogs] = useState<ChangeLogItem[]>([]);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [logsError, setLogsError] = useState<string | null>(null);
  const [selectedLogId, setSelectedLogId] = useState<string | null>(null);
  const [treeWidth, setTreeWidth] = useState(() => {
    if (typeof window === 'undefined') return 288;
    const saved = window.localStorage.getItem('project_explorer_tree_width');
    const parsed = saved ? Number(saved) : NaN;
    return Number.isFinite(parsed) ? Math.min(Math.max(parsed, 200), 640) : 288;
  });
  const [isResizing, setIsResizing] = useState(false);

  const loadEntries = useCallback(async (path: string) => {
    setLoadingPaths(prev => new Set(prev).add(path));
    setError(null);
    try {
      const data = await client.listFsEntries(path);
      const entries = Array.isArray(data?.entries) ? data.entries.map(normalizeEntry) : [];
      setEntriesMap(prev => ({ ...prev, [path]: entries }));
    } catch (err: any) {
      setError(err?.message || '加载目录失败');
    } finally {
      setLoadingPaths(prev => {
        const next = new Set(prev);
        next.delete(path);
        return next;
      });
    }
  }, [client]);

  const toggleDir = useCallback(async (entry: FsEntry) => {
    if (!entry.isDir) return;
    setSelectedPath(entry.path);
    setSelectedFile(null);
    setExpandedPaths(prev => {
      const next = new Set(prev);
      if (next.has(entry.path)) {
        next.delete(entry.path);
      } else {
        next.add(entry.path);
      }
      return next;
    });
    if (!entriesMap[entry.path]) {
      await loadEntries(entry.path);
    }
  }, [entriesMap, loadEntries]);

  const openFile = useCallback(async (entry: FsEntry) => {
    setSelectedPath(entry.path);
    setSelectedFile(null);
    setLoadingFile(true);
    setError(null);
    try {
      const data = await client.readFsFile(entry.path);
      setSelectedFile(normalizeFile(data));
    } catch (err: any) {
      setError(err?.message || '读取文件失败');
    } finally {
      setLoadingFile(false);
    }
  }, [client]);

  useEffect(() => {
    if (!project?.rootPath) {
      setEntriesMap({});
      setExpandedPaths(new Set());
      setSelectedPath(null);
      setSelectedFile(null);
      setChangeLogs([]);
      setLogsError(null);
      setSelectedLogId(null);
      return;
    }
    const root = project.rootPath;
    setEntriesMap({});
    const saved = project.id ? localStorage.getItem(`project_explorer_expanded_${project.id}`) : null;
    let nextExpanded = new Set<string>([root]);
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (Array.isArray(parsed)) {
          nextExpanded = new Set(parsed.filter((p) => typeof p === 'string'));
          nextExpanded.add(root);
        }
      } catch {
        nextExpanded = new Set([root]);
      }
    }
    setExpandedPaths(nextExpanded);
    setSelectedPath(root);
    setSelectedFile(null);
    setChangeLogs([]);
    setLogsError(null);
    setSelectedLogId(null);
    loadEntries(root);
    nextExpanded.forEach((p) => {
      if (p !== root) {
        loadEntries(p);
      }
    });
  }, [project?.id, project?.rootPath, loadEntries]);

  useEffect(() => {
    if (!project?.id || !project?.rootPath) return;
    const next = Array.from(expandedPaths);
    if (!next.includes(project.rootPath)) {
      next.push(project.rootPath);
    }
    localStorage.setItem(`project_explorer_expanded_${project.id}`, JSON.stringify(next));
  }, [expandedPaths, project?.id, project?.rootPath]);

  useEffect(() => {
    if (!isResizing) return;
    const handleMove = (event: MouseEvent) => {
      const delta = event.clientX - resizeStartX.current;
      const next = Math.min(Math.max(resizeStartWidth.current + delta, 200), 640);
      setTreeWidth(next);
    };
    const handleUp = () => {
      setIsResizing(false);
    };
    window.addEventListener('mousemove', handleMove);
    window.addEventListener('mouseup', handleUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    return () => {
      window.removeEventListener('mousemove', handleMove);
      window.removeEventListener('mouseup', handleUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
  }, [isResizing]);

  useEffect(() => {
    localStorage.setItem('project_explorer_tree_width', String(treeWidth));
  }, [treeWidth]);

  useEffect(() => {
    if (!project?.id || !selectedFile?.path) {
      setChangeLogs([]);
      setLogsError(null);
      setSelectedLogId(null);
      return;
    }
    let cancelled = false;
    const loadLogs = async () => {
      setLoadingLogs(true);
      setLogsError(null);
      try {
        const list = await client.listProjectChangeLogs(project.id, { path: selectedFile.path, limit: 100 });
        if (!cancelled) {
          const normalized = Array.isArray(list) ? list.map(normalizeChangeLog) : [];
          setChangeLogs(normalized);
        }
      } catch (err: any) {
        if (!cancelled) {
          setLogsError(err?.message || '加载变更记录失败');
          setChangeLogs([]);
          setSelectedLogId(null);
        }
      } finally {
        if (!cancelled) {
          setLoadingLogs(false);
        }
      }
    };
    loadLogs();
    return () => { cancelled = true; };
  }, [client, project?.id, selectedFile?.path]);

  useEffect(() => {
    if (selectedLogId && !changeLogs.find(log => log.id === selectedLogId)) {
      setSelectedLogId(null);
    }
  }, [changeLogs, selectedLogId]);

  const renderEntries = (path: string, depth: number): React.ReactNode => {
    const entries = entriesMap[path] || [];
    if (!entries.length) {
      return null;
    }
    return entries.map((entry) => {
      const isExpanded = expandedPaths.has(entry.path);
      const isActive = selectedPath === entry.path;
      return (
        <div key={entry.path}>
          <button
            type="button"
            onClick={() => (entry.isDir ? toggleDir(entry) : openFile(entry))}
            className={cn(
              'min-w-full w-max grid grid-cols-[12px_auto_64px] items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
              isActive && 'bg-accent'
            )}
            style={{ paddingLeft: 12 + depth * 14 }}
          >
            <span className="text-xs text-muted-foreground w-3 shrink-0">
              {entry.isDir ? (isExpanded ? '▾' : '▸') : ''}
            </span>
            <span
              className={cn(
                'text-sm whitespace-nowrap',
                entry.isDir ? 'text-foreground' : 'text-muted-foreground'
              )}
            >
              {entry.name}
            </span>
            <span className="text-[11px] text-muted-foreground text-right tabular-nums whitespace-nowrap">
              {!entry.isDir && entry.size != null ? formatFileSize(entry.size) : ''}
            </span>
          </button>
          {entry.isDir && isExpanded && renderEntries(entry.path, depth + 1)}
        </div>
      );
    });
  };

  const selectedLog = useMemo(
    () => (selectedLogId ? changeLogs.find(log => log.id === selectedLogId) || null : null),
    [changeLogs, selectedLogId]
  );

  const preview = useMemo(() => {
    if (loadingFile) {
      return <div className="p-4 text-sm text-muted-foreground">加载文件中...</div>;
    }
    if (!selectedFile) {
      return <div className="p-4 text-sm text-muted-foreground">请选择文件以预览</div>;
    }
    const isImage = selectedFile.contentType.startsWith('image/');
    if (isImage && selectedFile.isBinary) {
      const src = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
      return (
        <div className="p-4 overflow-auto h-full">
          <img src={src} alt={selectedFile.name} className="max-w-full max-h-full rounded border border-border" />
        </div>
      );
    }
    if (!selectedFile.isBinary) {
      const language = getHighlightLanguage(selectedFile.name);
      let highlighted = '';
      try {
        if (language) {
          highlighted = hljs.highlight(selectedFile.content, { language }).value;
        } else {
          highlighted = hljs.highlightAuto(selectedFile.content).value;
        }
      } catch {
        highlighted = escapeHtml(selectedFile.content);
      }
      const lines = highlighted.split(/\r?\n/);
      return (
        <div className="h-full overflow-auto bg-muted/30">
          <div className="flex min-h-full text-sm">
            <div className="shrink-0 py-4 pr-3 pl-2 border-r border-border text-right text-muted-foreground select-none">
              {lines.map((_, idx) => (
                <div key={idx} className="leading-5">
                  {idx + 1}
                </div>
              ))}
            </div>
            <div className="flex-1 min-w-0 py-4 pl-3 pr-4 hljs">
              {lines.map((line, idx) => (
                <div
                  key={idx}
                  className="leading-5 font-mono whitespace-pre w-full"
                  dangerouslySetInnerHTML={{ __html: line || '&nbsp;' }}
                />
              ))}
            </div>
          </div>
        </div>
      );
    }
    const downloadHref = `data:${selectedFile.contentType};base64,${selectedFile.content}`;
    return (
      <div className="p-4 text-sm text-muted-foreground space-y-2">
        <div>该文件为二进制内容，暂不支持直接预览。</div>
        <a
          href={downloadHref}
          download={selectedFile.name || 'file'}
          className="inline-flex items-center px-3 py-1.5 rounded bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
        >
          下载文件
        </a>
      </div>
    );
  }, [selectedFile, loadingFile]);

  const parseUnifiedDiff = useCallback((diffText: string) => {
    const lines = diffText.split(/\r?\n/);
    const parsed: Array<{ type: 'meta' | 'hunk' | 'add' | 'del' | 'context'; oldLine?: number | null; newLine?: number | null; text: string }> = [];
    let oldLine = 0;
    let newLine = 0;
    let inHunk = false;
    const hunkRegex = /^@@\s+-(\d+)(?:,(\d+))?\s+\+(\d+)(?:,(\d+))?\s+@@/;
    for (const line of lines) {
      const hunkMatch = hunkRegex.exec(line);
      if (hunkMatch) {
        oldLine = parseInt(hunkMatch[1], 10);
        newLine = parseInt(hunkMatch[3], 10);
        inHunk = true;
        parsed.push({ type: 'hunk', text: line });
        continue;
      }
      if (!inHunk) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      if (line.startsWith('+++') || line.startsWith('---')) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      if (line.startsWith('+')) {
        parsed.push({ type: 'add', oldLine: null, newLine, text: line });
        newLine += 1;
        continue;
      }
      if (line.startsWith('-')) {
        parsed.push({ type: 'del', oldLine, newLine: null, text: line });
        oldLine += 1;
        continue;
      }
      if (line.startsWith('\\')) {
        parsed.push({ type: 'meta', text: line });
        continue;
      }
      parsed.push({ type: 'context', oldLine, newLine, text: line });
      oldLine += 1;
      newLine += 1;
    }
    return parsed;
  }, []);

  const renderDiffRows = useCallback((diffText: string) => {
    const rows = parseUnifiedDiff(diffText);
    if (!rows.length) {
      return <div className="text-muted-foreground">该记录没有 diff 内容</div>;
    }
    return (
      <div className="font-mono text-xs">
        {rows.map((row, idx) => {
          let lineClass = 'text-foreground';
          if (row.type === 'hunk' || row.type === 'meta') {
            lineClass = 'text-muted-foreground';
          } else if (row.type === 'add') {
            lineClass = 'text-emerald-600 dark:text-emerald-400';
          } else if (row.type === 'del') {
            lineClass = 'text-rose-600 dark:text-rose-400';
          }
          return (
            <div key={`${idx}-${row.text}`} className={cn('grid grid-cols-[3rem_3rem_1fr] gap-2 leading-5', lineClass)}>
              <div className="text-right pr-2 text-muted-foreground">
                {row.oldLine ?? ''}
              </div>
              <div className="text-right pr-2 text-muted-foreground">
                {row.newLine ?? ''}
              </div>
              <div className="whitespace-pre">
                {row.text === '' ? ' ' : row.text}
              </div>
            </div>
          );
        })}
      </div>
    );
  }, [parseUnifiedDiff]);

  const diffPanel = useMemo(() => {
    if (!selectedLog) return null;
    const title = selectedLog.sessionTitle || selectedLog.sessionId || '未知会话';
    const time = selectedLog.createdAt ? new Date(selectedLog.createdAt).toLocaleString() : '';
    return (
      <div className="border-b border-border bg-muted/30 max-h-64 overflow-hidden flex flex-col">
        <div className="px-4 py-2 text-xs font-medium text-foreground flex items-center gap-2">
          <span>变更内容</span>
          <span className="text-muted-foreground">{selectedLog.action}</span>
          <span className="text-muted-foreground ml-auto">{time}</span>
        </div>
        <div className="px-4 pb-3 text-xs overflow-auto min-h-0">
          <div className="text-[11px] text-muted-foreground mb-2 truncate" title={title}>
            会话：{title}
          </div>
          {selectedLog.diff ? renderDiffRows(selectedLog.diff) : (
            <div className="text-muted-foreground">该记录没有 diff 内容</div>
          )}
        </div>
      </div>
    );
  }, [selectedLog, renderDiffRows]);

  const changeLogPanel = useMemo(() => {
    if (!selectedFile) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">请选择文件以查看变更记录</div>;
    }
    if (loadingLogs) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">加载变更记录中...</div>;
    }
    if (logsError) {
      return <div className="px-4 py-3 text-xs text-destructive">{logsError}</div>;
    }
    if (!changeLogs.length) {
      return <div className="px-4 py-3 text-xs text-muted-foreground">暂无变更记录</div>;
    }
    return (
      <div className="divide-y divide-border">
        {changeLogs.map((log) => {
          const isSelected = selectedLogId === log.id;
          const title = log.sessionTitle || log.sessionId || '未知会话';
          const time = log.createdAt ? new Date(log.createdAt).toLocaleString() : '';
          return (
            <button
              key={log.id}
              type="button"
              onClick={() => setSelectedLogId(prev => (prev === log.id ? null : log.id))}
              className={cn(
                'w-full px-4 py-2 text-xs text-left hover:bg-accent transition-colors',
                isSelected && 'bg-accent'
              )}
            >
              <div className="flex items-center gap-2">
                <span className="text-muted-foreground w-3">{isSelected ? '▾' : '▸'}</span>
                <span className="font-medium text-foreground">{log.action}</span>
                <span className="text-muted-foreground">{formatFileSize(log.bytes || 0)}</span>
                <span className="text-muted-foreground ml-auto">{time}</span>
              </div>
              <div className="text-[11px] text-muted-foreground truncate" title={title}>
                会话：{title}
              </div>
            </button>
          );
        })}
      </div>
    );
  }, [selectedFile, loadingLogs, logsError, changeLogs, selectedLogId]);

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  return (
    <div ref={containerRef} className={cn('flex h-full overflow-hidden', className)}>
      <div className="border-r border-border bg-card flex flex-col shrink-0" style={{ width: treeWidth }}>
        <div className="px-3 py-2 border-b border-border">
          <div className="text-xs text-muted-foreground">项目目录</div>
          <div className="text-sm font-medium text-foreground truncate" title={project.rootPath}>
            {project.name}
          </div>
          <div className="text-[11px] text-muted-foreground truncate" title={project.rootPath}>
            {project.rootPath}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto overflow-x-auto py-2">
          {renderEntries(project.rootPath, 0)}
          {loadingPaths.has(project.rootPath) && (
            <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
          )}
          {!loadingPaths.has(project.rootPath) && (entriesMap[project.rootPath]?.length ?? 0) === 0 && (
            <div className="px-3 py-2 text-xs text-muted-foreground">目录为空</div>
          )}
        </div>
      </div>
      <div
        className={cn('w-1 cursor-col-resize bg-border/60 hover:bg-border', isResizing && 'bg-border')}
        onMouseDown={(event) => {
          resizeStartX.current = event.clientX;
          resizeStartWidth.current = treeWidth;
          setIsResizing(true);
        }}
      />
      <div className="flex-1 flex overflow-hidden">
        <div className="flex-1 flex flex-col overflow-hidden">
          <div className="px-4 py-2 border-b border-border bg-card flex items-center justify-between">
            <div className="min-w-0">
              <div className="text-sm font-medium text-foreground truncate">
                {selectedFile?.name || '文件预览'}
              </div>
              <div className="text-[11px] text-muted-foreground truncate">
                {selectedFile?.path || '请选择文件'}
              </div>
            </div>
            {selectedFile && (
              <div className="text-[11px] text-muted-foreground">
                {formatFileSize(selectedFile.size)}
              </div>
            )}
          </div>
          <div className="flex-1 overflow-hidden flex flex-col">
            {diffPanel}
            <div className="flex-1 min-h-0 overflow-hidden">
              {error ? (
                <div className="p-4 text-sm text-destructive">{error}</div>
              ) : (
                preview
              )}
            </div>
          </div>
        </div>
        {(loadingLogs || logsError || changeLogs.length > 0) && (
          <div className="w-72 border-l border-border bg-card/60 flex flex-col overflow-hidden">
            <div className="px-4 py-2 text-xs font-medium text-foreground border-b border-border">变更记录</div>
            <div className="flex-1 min-h-0 overflow-auto">
              {changeLogPanel}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default ProjectExplorer;
