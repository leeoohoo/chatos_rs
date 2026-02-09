import React, { useCallback, useEffect, useMemo, useState } from 'react';
import hljs from 'highlight.js';
import ApiClient from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import type { Project, FsEntry, FsReadResult } from '../types';
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

  const [entriesMap, setEntriesMap] = useState<Record<string, FsEntry[]>>({});
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedFile, setSelectedFile] = useState<FsReadResult | null>(null);
  const [loadingFile, setLoadingFile] = useState(false);
  const [error, setError] = useState<string | null>(null);

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
      return;
    }
    const root = project.rootPath;
    setEntriesMap({});
    setExpandedPaths(new Set([root]));
    setSelectedPath(root);
    setSelectedFile(null);
    loadEntries(root);
  }, [project?.id, project?.rootPath, loadEntries]);

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
              'w-full flex items-center gap-2 py-1.5 pr-2 text-left rounded hover:bg-accent transition-colors',
              isActive && 'bg-accent'
            )}
            style={{ paddingLeft: 12 + depth * 14 }}
          >
            <span className="text-xs text-muted-foreground w-3">
              {entry.isDir ? (isExpanded ? '▾' : '▸') : ''}
            </span>
            <span className={cn('text-sm', entry.isDir ? 'text-foreground' : 'text-muted-foreground')}>{entry.name}</span>
            {!entry.isDir && entry.size != null && (
              <span className="ml-auto text-[11px] text-muted-foreground">{formatFileSize(entry.size)}</span>
            )}
          </button>
          {entry.isDir && isExpanded && renderEntries(entry.path, depth + 1)}
        </div>
      );
    });
  };

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
      return (
        <pre className="p-4 text-sm overflow-auto h-full bg-muted/30">
          <code
            className="hljs"
            dangerouslySetInnerHTML={{ __html: highlighted }}
          />
        </pre>
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

  if (!project) {
    return (
      <div className={cn('flex items-center justify-center h-full text-muted-foreground', className)}>
        请选择一个项目查看文件
      </div>
    );
  }

  return (
    <div className={cn('flex h-full overflow-hidden', className)}>
      <div className="w-72 border-r border-border bg-card flex flex-col">
        <div className="px-3 py-2 border-b border-border">
          <div className="text-xs text-muted-foreground">项目目录</div>
          <div className="text-sm font-medium text-foreground truncate" title={project.rootPath}>
            {project.name}
          </div>
          <div className="text-[11px] text-muted-foreground truncate" title={project.rootPath}>
            {project.rootPath}
          </div>
        </div>
        <div className="flex-1 overflow-auto py-2">
          {renderEntries(project.rootPath, 0)}
          {loadingPaths.has(project.rootPath) && (
            <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
          )}
          {!loadingPaths.has(project.rootPath) && (entriesMap[project.rootPath]?.length ?? 0) === 0 && (
            <div className="px-3 py-2 text-xs text-muted-foreground">目录为空</div>
          )}
        </div>
      </div>
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
        <div className="flex-1 overflow-hidden">
          {error ? (
            <div className="p-4 text-sm text-destructive">{error}</div>
          ) : (
            preview
          )}
        </div>
      </div>
    </div>
  );
};

export default ProjectExplorer;
