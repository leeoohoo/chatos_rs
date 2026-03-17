import React, { useState, useRef, useCallback, useEffect, useMemo } from 'react';
import { apiClient as globalApiClient } from '../lib/api/client';
import { useChatApiClientFromContext } from '../lib/store/ChatStoreContext';
import { cn } from '../lib/utils';
import type { FsEntry, InputAreaProps } from '../types';
import {
  compactSearchText,
  formatFileSize,
  fuzzyMatch,
  isLikelyCodeFileName,
  MAX_ATTACHMENTS,
  MAX_FILE_BYTES,
  MAX_TOTAL_BYTES,
  normalizeFsEntry,
} from './inputArea/fileUtils';

const AGENT_BUILDER_MCP_ID = 'builtin_agent_builder';
const PROJECT_REQUIRED_MCP_IDS = new Set([
  'builtin_code_maintainer',
  'builtin_code_maintainer_read',
  'builtin_code_maintainer_write',
  'builtin_terminal_controller',
]);

interface SelectableMcpConfig {
  id: string;
  name: string;
  displayName: string;
  builtin: boolean;
}

export const InputArea: React.FC<InputAreaProps> = ({
  onSend,
  onStop,
  disabled = false,
  isStreaming = false,
  isStopping = false,
  placeholder = 'Type your message...',
  maxLength = 4000,
  allowAttachments = false,
  supportedFileTypes = [
    'image/*',
    'text/*',
    'application/json',
    'application/pdf',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  ],
  reasoningSupported = false,
  reasoningEnabled = false,
  onReasoningToggle,
  showModelSelector = false,
  selectedModelId = null,
  availableModels = [],
  onModelChange,
  availableProjects = [],
  selectedProjectId = null,
  onProjectChange,
  mcpEnabled = true,
  enabledMcpIds = [],
  onMcpEnabledChange,
  onEnabledMcpIdsChange,
}) => {
  const [message, setMessage] = useState('');
  const [attachments, setAttachments] = useState<File[]>([]);
  const [attachError, setAttachError] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement>(null);
  const [mcpPickerOpen, setMcpPickerOpen] = useState(false);
  const mcpPickerRef = useRef<HTMLDivElement>(null);
  const [availableMcpConfigs, setAvailableMcpConfigs] = useState<SelectableMcpConfig[]>([]);
  const [mcpConfigsLoading, setMcpConfigsLoading] = useState(false);
  const [mcpConfigsError, setMcpConfigsError] = useState<string | null>(null);
  // 记录全局拖拽层级，避免 dragenter/dragleave 抖动
  const globalDragCounter = useRef(0);

  const apiClientFromContext = useChatApiClientFromContext();
  const client = useMemo(() => apiClientFromContext || globalApiClient, [apiClientFromContext]);
  const projectFilePickerRef = useRef<HTMLDivElement>(null);
  const [projectFilePickerOpen, setProjectFilePickerOpen] = useState(false);
  const [projectFileEntries, setProjectFileEntries] = useState<FsEntry[]>([]);
  const [projectFilePath, setProjectFilePath] = useState<string | null>(null);
  const [projectFileParent, setProjectFileParent] = useState<string | null>(null);
  const [projectFileFilter, setProjectFileFilter] = useState('');
  const [projectFileLoading, setProjectFileLoading] = useState(false);
  const [projectFileSearching, setProjectFileSearching] = useState(false);
  const [projectFileSearchResults, setProjectFileSearchResults] = useState<FsEntry[]>([]);
  const [projectFileSearchTruncated, setProjectFileSearchTruncated] = useState(false);
  const [projectFileError, setProjectFileError] = useState<string | null>(null);
  const [projectFileAttachingPath, setProjectFileAttachingPath] = useState<string | null>(null);

  const normalizePath = useCallback((value: string) => {
    const normalized = value.replace(/\\/g, '/').replace(/\/+/g, '/');
    if (normalized.length > 1 && normalized.endsWith('/')) {
      return normalized.slice(0, -1);
    }
    return normalized;
  }, []);

  const isPathWithinRoot = useCallback((candidate: string, root: string) => {
    const normalizedCandidate = normalizePath(candidate);
    const normalizedRoot = normalizePath(root);
    return normalizedCandidate === normalizedRoot || normalizedCandidate.startsWith(`${normalizedRoot}/`);
  }, [normalizePath]);

  const selectedRuntimeProject = useMemo(() => {
    if (!selectedProjectId) {
      return null;
    }
    return (availableProjects || []).find((p: any) => p.id === selectedProjectId) || null;
  }, [availableProjects, selectedProjectId]);
  const hasRuntimeProject = Boolean(selectedRuntimeProject?.id && selectedRuntimeProject?.rootPath);
  const selectedModel = useMemo(
    () => (selectedModelId ? (availableModels || []).find(m => (m as any).id === selectedModelId) : null),
    [availableModels, selectedModelId]
  );
  const enabledModels = useMemo(
    () => (availableModels || []).filter((m: any) => m.enabled),
    [availableModels]
  );
  const hasAiOptions = (availableModels && availableModels.length > 0);
  const availableMcpIds = useMemo(
    () => availableMcpConfigs.map((item) => item.id),
    [availableMcpConfigs],
  );
  const selectableMcpIds = useMemo(
    () => availableMcpIds.filter((id) => hasRuntimeProject || !PROJECT_REQUIRED_MCP_IDS.has(id)),
    [availableMcpIds, hasRuntimeProject],
  );
  const selectableMcpIdSet = useMemo(
    () => new Set(selectableMcpIds),
    [selectableMcpIds],
  );
  const sanitizedEnabledMcpIds = useMemo(() => {
    if (availableMcpIds.length === 0) {
      return enabledMcpIds;
    }
    if (enabledMcpIds.length === 0) {
      return hasRuntimeProject ? [] : [...selectableMcpIds];
    }
    return enabledMcpIds.filter((id) => selectableMcpIdSet.has(id));
  }, [
    availableMcpIds.length,
    enabledMcpIds,
    hasRuntimeProject,
    selectableMcpIdSet,
    selectableMcpIds,
  ]);
  const isAllMcpSelected = enabledMcpIds.length === 0
    || (selectableMcpIds.length > 0 && sanitizedEnabledMcpIds.length === selectableMcpIds.length);
  const selectedMcpCount = isAllMcpSelected ? selectableMcpIds.length : sanitizedEnabledMcpIds.length;
  const builtinMcpConfigs = useMemo(
    () => availableMcpConfigs.filter((item) => item.builtin),
    [availableMcpConfigs],
  );
  const customMcpConfigs = useMemo(
    () => availableMcpConfigs.filter((item) => !item.builtin),
    [availableMcpConfigs],
  );
  const projectForFilePicker = useMemo(
    () => selectedRuntimeProject || null,
    [selectedRuntimeProject],
  );
  const projectRootForFilePicker = useMemo(() => {
    if (!projectForFilePicker?.rootPath) return null;
    return normalizePath(projectForFilePicker.rootPath);
  }, [projectForFilePicker?.rootPath, normalizePath]);
  const isHiddenProjectPath = useCallback((candidatePath: string) => {
    if (!projectRootForFilePicker) return false;
    const normalizedCandidate = normalizePath(candidatePath || '');
    if (!normalizedCandidate) return false;
    const normalizedRoot = normalizePath(projectRootForFilePicker);
    if (!normalizedRoot) return false;

    let relativePath = normalizedCandidate;
    if (normalizedCandidate === normalizedRoot) {
      relativePath = '';
    } else if (normalizedCandidate.startsWith(`${normalizedRoot}/`)) {
      relativePath = normalizedCandidate.slice(normalizedRoot.length + 1);
    }

    if (!relativePath) return false;
    return relativePath.split('/').some((segment) => segment.startsWith('.'));
  }, [normalizePath, projectRootForFilePicker]);
  const showProjectFilePicker = Boolean(projectRootForFilePicker);
  const currentAiLabel = useMemo(
    () => (selectedModel ? `Model: ${(selectedModel as any).name}` : '选择模型'),
    [selectedModel]
  );
  const projectFilePathLabel = useMemo(() => {
    if (!projectFilePath || !projectRootForFilePicker) return '';
    const normalized = normalizePath(projectFilePath);
    if (normalized === projectRootForFilePicker) return '/';
    const prefix = `${projectRootForFilePicker}/`;
    if (normalized.startsWith(prefix)) {
      return `/${normalized.slice(prefix.length)}`;
    }
    return normalized;
  }, [normalizePath, projectFilePath, projectRootForFilePicker]);
  const filteredProjectFileEntries = useMemo(() => {
    const keywordRaw = projectFileFilter.trim().toLocaleLowerCase();
    const keywordCompact = compactSearchText(keywordRaw);
    const source = (projectFileEntries || [])
      .filter((entry) => !isHiddenProjectPath(entry.path))
      .slice()
      .sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    if (!keywordRaw) return source;

    const matches = (value: string) => {
      const text = value.toLocaleLowerCase();
      const compactText = compactSearchText(text);
      if (fuzzyMatch(text, keywordRaw) || fuzzyMatch(compactText, keywordCompact)) {
        return true;
      }
      return false;
    };

    return source.filter((entry) => {
      const nameText = entry.name;
      const normalizedEntryPath = normalizePath(entry.path);
      let relativePathText = normalizedEntryPath;
      if (projectRootForFilePicker) {
        const normalizedRoot = normalizePath(projectRootForFilePicker);
        const prefix = `${normalizedRoot}/`;
        if (normalizedEntryPath.startsWith(prefix)) {
          relativePathText = normalizedEntryPath.slice(prefix.length);
        }
      }
      return matches(nameText) || matches(relativePathText);
    });
  }, [isHiddenProjectPath, normalizePath, projectFileEntries, projectFileFilter, projectRootForFilePicker]);
  const projectFileKeywordActive = projectFileFilter.trim().length > 0;
  const displayedProjectFileEntries = projectFileKeywordActive
    ? projectFileSearchResults
    : filteredProjectFileEntries;
  const projectFileBusy = projectFileKeywordActive ? projectFileSearching : projectFileLoading;

  const toRelativeProjectPath = useCallback((absolutePath: string) => {
    if (!projectRootForFilePicker) return absolutePath;
    const normalized = normalizePath(absolutePath);
    if (normalized === projectRootForFilePicker) {
      return absolutePath;
    }
    const prefix = `${projectRootForFilePicker}/`;
    if (normalized.startsWith(prefix)) {
      return normalized.slice(prefix.length);
    }
    return normalized;
  }, [normalizePath, projectRootForFilePicker]);

  // 自动调整文本框高度
  const adjustTextareaHeight = useCallback(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      const scrollHeight = textarea.scrollHeight;
      const maxHeight = 200; // 最大高度
      textarea.style.height = `${Math.min(scrollHeight, maxHeight)}px`;
    }
  }, []);

  // 关闭下拉：点击外部
  useEffect(() => {
    if (!pickerOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (!pickerRef.current) return;
      if (!pickerRef.current.contains(e.target as Node)) {
        setPickerOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [pickerOpen]);

  useEffect(() => {
    if (!projectFilePickerOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (!projectFilePickerRef.current) return;
      if (!projectFilePickerRef.current.contains(e.target as Node)) {
        setProjectFilePickerOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [projectFilePickerOpen]);

  useEffect(() => {
    if (!mcpPickerOpen) return;
    const onDocClick = (e: MouseEvent) => {
      if (!mcpPickerRef.current) return;
      if (!mcpPickerRef.current.contains(e.target as Node)) {
        setMcpPickerOpen(false);
      }
    };
    document.addEventListener('mousedown', onDocClick);
    return () => document.removeEventListener('mousedown', onDocClick);
  }, [mcpPickerOpen]);

  useEffect(() => {
    setProjectFilePickerOpen(false);
    setProjectFileEntries([]);
    setProjectFileSearchResults([]);
    setProjectFileSearchTruncated(false);
    setProjectFileSearching(false);
    setProjectFilePath(null);
    setProjectFileParent(null);
    setProjectFileFilter('');
    setProjectFileError(null);
    setProjectFileAttachingPath(null);
  }, [projectRootForFilePicker]);

  useEffect(() => {
    if (!projectFilePickerOpen || !projectRootForFilePicker) return;

    const keyword = projectFileFilter.trim();
    if (!keyword) {
      setProjectFileSearchResults([]);
      setProjectFileSearchTruncated(false);
      setProjectFileSearching(false);
      return;
    }

    let cancelled = false;
    const timer = window.setTimeout(async () => {
      setProjectFileSearching(true);
      setProjectFileError(null);
      try {
        const data = await client.searchFsEntries(projectRootForFilePicker, keyword, 300);
        if (cancelled) return;

        const entriesRaw: any[] = Array.isArray(data?.entries) ? data.entries : [];
        const normalizedEntries = entriesRaw
          .map((raw: any) => normalizeFsEntry(raw))
          .filter((entry: FsEntry) => (
            !entry.isDir
            && entry.path
            && isPathWithinRoot(entry.path, projectRootForFilePicker)
            && !isHiddenProjectPath(entry.path)
          ));

        setProjectFileSearchResults(normalizedEntries);
        setProjectFileSearchTruncated(Boolean(data?.truncated));
      } catch (error: any) {
        if (cancelled) return;
        setProjectFileError(error?.message || '搜索项目文件失败');
        setProjectFileSearchResults([]);
        setProjectFileSearchTruncated(false);
      } finally {
        if (!cancelled) {
          setProjectFileSearching(false);
        }
      }
    }, 150);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [client, isHiddenProjectPath, isPathWithinRoot, projectFileFilter, projectFilePickerOpen, projectRootForFilePicker]);

  const loadAvailableMcpConfigs = useCallback(async () => {
    setMcpConfigsLoading(true);
    setMcpConfigsError(null);
    try {
      const rows = await client.getMcpConfigs();
      const seenIds = new Set<string>();
      const normalized = (Array.isArray(rows) ? rows : [])
        .map((item: any) => {
          const id = typeof item?.id === 'string' ? item.id.trim() : '';
          if (!id || id === AGENT_BUILDER_MCP_ID) {
            return null;
          }
          if (seenIds.has(id)) {
            return null;
          }
          seenIds.add(id);
          const enabled = typeof item?.enabled === 'boolean' ? item.enabled : true;
          if (!enabled) {
            return null;
          }
          const displayNameRaw = typeof item?.display_name === 'string' ? item.display_name.trim() : '';
          const nameRaw = typeof item?.name === 'string' ? item.name.trim() : '';
          return {
            id,
            name: nameRaw || id,
            displayName: displayNameRaw || nameRaw || id,
            builtin: item?.builtin === true,
          } satisfies SelectableMcpConfig;
        })
        .filter((item: SelectableMcpConfig | null): item is SelectableMcpConfig => item !== null)
        .sort((left, right) => {
          if (left.builtin !== right.builtin) {
            return left.builtin ? -1 : 1;
          }
          return left.displayName.localeCompare(right.displayName, 'zh-Hans-CN');
        });

      setAvailableMcpConfigs(normalized);
    } catch (error: any) {
      setMcpConfigsError(error?.message || '加载 MCP 列表失败');
      setAvailableMcpConfigs([]);
    } finally {
      setMcpConfigsLoading(false);
    }
  }, [client]);

  useEffect(() => {
    if (!mcpEnabled) {
      return;
    }
    if (availableMcpConfigs.length > 0 || mcpConfigsLoading) {
      return;
    }
    void loadAvailableMcpConfigs();
  }, [availableMcpConfigs.length, loadAvailableMcpConfigs, mcpConfigsLoading, mcpEnabled]);

  useEffect(() => {
    if (!mcpPickerOpen) {
      return;
    }
    if (availableMcpConfigs.length > 0 || mcpConfigsLoading) {
      return;
    }
    void loadAvailableMcpConfigs();
  }, [availableMcpConfigs.length, loadAvailableMcpConfigs, mcpConfigsLoading, mcpPickerOpen]);

  useEffect(() => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (availableMcpIds.length === 0) {
      return;
    }
    const sameLength = enabledMcpIds.length === sanitizedEnabledMcpIds.length;
    const sameValues = sameLength && enabledMcpIds.every((id, index) => id === sanitizedEnabledMcpIds[index]);
    if (sameValues) {
      return;
    }
    onEnabledMcpIdsChange(sanitizedEnabledMcpIds);
  }, [
    availableMcpIds.length,
    enabledMcpIds,
    onEnabledMcpIdsChange,
    sanitizedEnabledMcpIds,
  ]);

  const handleToggleMcpPicker = useCallback(() => {
    if (disabled || isStreaming || isStopping) return;
    setMcpPickerOpen((prev) => !prev);
  }, [disabled, isStopping, isStreaming]);

  const applySelectedMcpIds = useCallback((ids: string[]) => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    const uniqueIds: string[] = [];
    for (const id of ids) {
      const trimmed = id.trim();
      if (!trimmed || uniqueIds.includes(trimmed)) {
        continue;
      }
      uniqueIds.push(trimmed);
    }
    if (hasRuntimeProject && uniqueIds.length === selectableMcpIds.length) {
      onEnabledMcpIdsChange([]);
      return;
    }
    onEnabledMcpIdsChange(uniqueIds);
  }, [hasRuntimeProject, onEnabledMcpIdsChange, selectableMcpIds.length]);

  const handleSelectAllMcp = useCallback(() => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (hasRuntimeProject) {
      onEnabledMcpIdsChange([]);
      return;
    }
    onEnabledMcpIdsChange([...selectableMcpIds]);
  }, [hasRuntimeProject, onEnabledMcpIdsChange, selectableMcpIds]);

  const handleToggleMcpSelection = useCallback((mcpId: string) => {
    if (!onEnabledMcpIdsChange) {
      return;
    }
    if (!selectableMcpIdSet.has(mcpId)) {
      return;
    }
    const baseSelected = isAllMcpSelected ? [...selectableMcpIds] : [...sanitizedEnabledMcpIds];
    const exists = baseSelected.includes(mcpId);
    const nextSelected = exists
      ? baseSelected.filter((id) => id !== mcpId)
      : [...baseSelected, mcpId];
    if (nextSelected.length === 0) {
      onMcpEnabledChange?.(false);
      onEnabledMcpIdsChange([]);
      return;
    }
    applySelectedMcpIds(nextSelected);
  }, [
    applySelectedMcpIds,
    selectableMcpIdSet,
    selectableMcpIds,
    isAllMcpSelected,
    onEnabledMcpIdsChange,
    onMcpEnabledChange,
    sanitizedEnabledMcpIds,
  ]);

  const isFileTypeAllowed = useCallback((file: File) => {
    if (!supportedFileTypes || supportedFileTypes.length === 0) return true;
    const type = String(file.type || '').toLowerCase();
    const name = String(file.name || '').toLowerCase();
    const matched = supportedFileTypes.some((pattern) => {
      const p = String(pattern || '').toLowerCase().trim();
      if (!p) return false;
      if (p === '*/*') return true;
      if (p.endsWith('/*')) {
        if (type) return type.startsWith(p.slice(0, -1));
        return false;
      }
      if (p.startsWith('.')) return name.endsWith(p);
      if (type) return type === p;
      if (!type && p === 'application/pdf') return name.endsWith('.pdf');
      if (!type && p === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document') return name.endsWith('.docx');
      return false;
    });

    if (matched) return true;

    // Fallback: allow common source/config files even when browser MIME is generic.
    return isLikelyCodeFileName(name);
  }, [supportedFileTypes]);

  // 统一添加文件并校验限制
  const addFiles = useCallback((incoming: File[]) => {
    if (!incoming || incoming.length === 0) return;
    setAttachments(prev => {
      const currentTotal = prev.reduce((s, f) => s + (f?.size || 0), 0);
      const errors: string[] = [];
      const accepted: File[] = [];
      let total = currentTotal;
      let count = prev.length;
      for (const f of incoming) {
        if (!f) continue;
        if (!isFileTypeAllowed(f)) {
          errors.push(`${f.name} 类型不支持`);
          continue;
        }
        if (f.size > MAX_FILE_BYTES) {
          errors.push(`${f.name} 超过单文件上限(${formatFileSize(MAX_FILE_BYTES)})`);
          continue;
        }
        if (count + 1 > MAX_ATTACHMENTS) {
          errors.push(`${f.name} 超过数量上限(${MAX_ATTACHMENTS} 个)`);
          continue;
        }
        if (total + f.size > MAX_TOTAL_BYTES) {
          errors.push(`${f.name} 导致总大小超限(${formatFileSize(MAX_TOTAL_BYTES)})`);
          continue;
        }
        accepted.push(f);
        total += f.size;
        count += 1;
      }
      if (errors.length > 0) {
        setAttachError(`部分文件未添加：${errors.join('；')}。限制：单文件≤${formatFileSize(MAX_FILE_BYTES)}，总计≤${formatFileSize(MAX_TOTAL_BYTES)}，最多 ${MAX_ATTACHMENTS} 个。`);
      } else {
        setAttachError(null);
      }
      return accepted.length > 0 ? [...prev, ...accepted] : prev;
    });
  }, [isFileTypeAllowed]);

  const loadProjectFileEntries = useCallback(async (nextPath?: string | null) => {
    if (!projectRootForFilePicker) return;

    const fallbackRoot = normalizePath(projectRootForFilePicker);
    const preferredPath = nextPath ? normalizePath(nextPath) : fallbackRoot;
    let safePath = isPathWithinRoot(preferredPath, fallbackRoot) ? preferredPath : fallbackRoot;
    if (isHiddenProjectPath(safePath)) {
      safePath = fallbackRoot;
    }

    setProjectFileLoading(true);
    setProjectFileError(null);
    try {
      const data = await client.listFsEntries(safePath);
      const currentPathRaw = typeof data?.path === 'string' && data.path ? data.path : safePath;
      const normalizedCurrent = normalizePath(currentPathRaw);
      const entriesRaw: any[] = Array.isArray(data?.entries) ? data.entries : [];
      const normalizedEntries = entriesRaw
        .map((raw: any) => normalizeFsEntry(raw))
        .filter((entry: FsEntry) => (
          entry.path
          && isPathWithinRoot(entry.path, fallbackRoot)
          && !isHiddenProjectPath(entry.path)
        ));
      const parentRaw = typeof data?.parent === 'string' ? normalizePath(data.parent) : null;

      setProjectFilePath(isPathWithinRoot(normalizedCurrent, fallbackRoot) ? normalizedCurrent : fallbackRoot);
      if (parentRaw && isPathWithinRoot(parentRaw, fallbackRoot) && parentRaw !== fallbackRoot && !isHiddenProjectPath(parentRaw)) {
        setProjectFileParent(parentRaw);
      } else {
        setProjectFileParent(null);
      }
      setProjectFileEntries(normalizedEntries);
    } catch (error: any) {
      setProjectFileError(error?.message || '加载项目文件失败');
      setProjectFileEntries([]);
    } finally {
      setProjectFileLoading(false);
    }
  }, [client, isHiddenProjectPath, isPathWithinRoot, normalizePath, projectRootForFilePicker]);

  const handleToggleProjectFilePicker = useCallback(async () => {
    if (!showProjectFilePicker || disabled) return;

    if (projectFilePickerOpen) {
      setProjectFilePickerOpen(false);
      return;
    }

    const initialPath = projectFilePath && projectRootForFilePicker && isPathWithinRoot(projectFilePath, projectRootForFilePicker)
      ? projectFilePath
      : projectRootForFilePicker;

    setProjectFilePickerOpen(true);
    setProjectFileFilter('');
    await loadProjectFileEntries(initialPath);
  }, [
    disabled,
    isPathWithinRoot,
    loadProjectFileEntries,
    projectFilePath,
    projectFilePickerOpen,
    projectRootForFilePicker,
    showProjectFilePicker,
  ]);

  const handleAttachProjectFile = useCallback(async (entry: FsEntry) => {
    if (!projectRootForFilePicker) return;

    if (entry.isDir) {
      await loadProjectFileEntries(entry.path);
      return;
    }

    setProjectFileAttachingPath(entry.path);
    setProjectFileError(null);
    try {
      const rawFile = await client.readFsFile(entry.path);
      const isBinary = rawFile?.is_binary ?? rawFile?.isBinary;
      if (isBinary) {
        throw new Error('暂不支持二进制文件，请选择文本文件');
      }
      const content = typeof rawFile?.content === 'string' ? rawFile.content : '';
      const rawContentType = String(rawFile?.content_type || rawFile?.contentType || '').trim().toLowerCase();
      const normalizedContentType = (
        rawContentType.startsWith('text/')
        || rawContentType === 'application/json'
        || rawContentType === 'application/pdf'
        || rawContentType === 'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
      )
        ? rawContentType
        : 'text/plain';
      const relativePath = toRelativeProjectPath(entry.path) || entry.name;
      const fileToAttach = new File([content], relativePath, { type: normalizedContentType });
      addFiles([fileToAttach]);
      setProjectFilePickerOpen(false);
    } catch (error: any) {
      const rawMessage = error?.message || '读取项目文件失败';
      if (String(rawMessage).includes('413')) {
        setProjectFileError('文件过大，当前最多支持 2MB 的项目文件');
      } else {
        setProjectFileError(rawMessage);
      }
    } finally {
      setProjectFileAttachingPath(null);
    }
  }, [addFiles, client, loadProjectFileEntries, projectRootForFilePicker, toRelativeProjectPath]);

  // 全局拖拽支持：允许把文件拖到整个应用任意位置
  useEffect(() => {
    if (!allowAttachments || disabled) return;

    const hasFiles = (e: DragEvent | ClipboardEvent) => {
      const dt = (e as DragEvent).dataTransfer;
      if (!dt) return false;
      try {
        // 检查包含文件
        if (dt.types && Array.from(dt.types).includes('Files')) return true;
        if (dt.files && dt.files.length > 0) return true;
      } catch (_) {}
      return false;
    };

    const onDragOver = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      setIsDragging(true);
      try { (e.dataTransfer as DataTransfer).dropEffect = 'copy'; } catch (_) {}
    };

    const onDragEnter = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current += 1;
      setIsDragging(true);
    };

    const onDragLeave = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current = Math.max(0, globalDragCounter.current - 1);
      if (globalDragCounter.current === 0) setIsDragging(false);
    };

    const onDrop = (e: DragEvent) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      e.stopPropagation();
      globalDragCounter.current = 0;
      setIsDragging(false);
      const dt = e.dataTransfer as DataTransfer;
      const files = Array.from(dt.files || []);
      if (files.length > 0) {
        addFiles(files);
      }
    };

    window.addEventListener('dragover', onDragOver);
    window.addEventListener('dragenter', onDragEnter);
    window.addEventListener('dragleave', onDragLeave);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragover', onDragOver);
      window.removeEventListener('dragenter', onDragEnter);
      window.removeEventListener('dragleave', onDragLeave);
      window.removeEventListener('drop', onDrop);
    };
  }, [allowAttachments, disabled, addFiles]);

  // 处理输入变化
  const handleInputChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    if (value.length <= maxLength) {
      setMessage(value);
      adjustTextareaHeight();
    }
  };

  // 处理键盘事件
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  // 处理粘贴附件（图片/文件）
  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    try {
      if (!allowAttachments || disabled) return;
      const dt = e.clipboardData;
      const collected: File[] = [];
      // 优先从 items 读取（可区分种类）
      if (dt && dt.items && dt.items.length > 0) {
        for (let i = 0; i < dt.items.length; i++) {
          const it = dt.items[i];
          if (it.kind === 'file') {
            const f = it.getAsFile();
            if (f && f.size > 0) collected.push(f);
          }
        }
      }
      // 兼容从 files 读取
      if (collected.length === 0 && dt && dt.files && dt.files.length > 0) {
        for (let i = 0; i < dt.files.length; i++) {
          const f = dt.files[i];
          if (f && f.size > 0) collected.push(f);
        }
      }

      if (collected.length > 0) {
        addFiles(collected);
        // 对于仅图片/文件粘贴，textarea 通常不会插入任何内容，无需阻止默认行为；
        // 若存在文本同时粘贴，保留文本默认粘贴到输入框。
      }
    } catch (_) {}
  };

  // 发送消息
  const handleSend = () => {
    const trimmedMessage = message.trim();
    if (!trimmedMessage && attachments.length === 0) return;
    if (disabled) return;

    // 检查是否选择了模型
    if (showModelSelector && !selectedModelId) {
      alert('请先选择一个模型');
      return;
    }

    const runtimeProjectId = selectedRuntimeProject?.id?.trim() || '0';
    const runtimeProjectRoot = runtimeProjectId === '0'
      ? null
      : (selectedRuntimeProject?.rootPath || null);

    onSend(trimmedMessage, attachments, {
      mcpEnabled,
      enabledMcpIds: sanitizedEnabledMcpIds,
      projectId: runtimeProjectId,
      projectRoot: runtimeProjectRoot,
    });
    setMessage('');
    setAttachments([]);
    
    // 重置文本框高度
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  // 处理文件选择
  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    addFiles(files);
    
    // 清空input以允许重复选择同一文件
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // 移除附件
  const removeAttachment = (index: number) => {
    setAttachments(prev => prev.filter((_, i) => i !== index));
  };

  // 拖拽处理
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (allowAttachments && !disabled) {
      setIsDragging(true);
    }
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    
    if (!allowAttachments || disabled) return;
    
    const files = Array.from(e.dataTransfer.files);
    addFiles(files);
  };

  return (
    <div className="border-t bg-background p-3 sm:p-4">
      {/* 附件预览 */}
      {attachments.length > 0 && (
        <div className="mb-3 flex flex-wrap gap-2">
          {attachments.map((file, index) => (
            <div
              key={index}
              className="flex items-center gap-2 bg-muted px-3 py-2 rounded-lg text-sm"
            >
              <svg className="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
              </svg>
              <span className="truncate max-w-32">{file.name}</span>
              <span className="text-xs text-muted-foreground">({formatFileSize(file.size)})</span>
              <button
                onClick={() => removeAttachment(index)}
                className="text-muted-foreground hover:text-destructive transition-colors"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
          ))}
        </div>
      )}
      {attachError && (
        <div className="-mt-2 mb-3 text-xs text-destructive">{attachError}</div>
      )}
      {projectFileError && (
        <div className="-mt-2 mb-3 text-xs text-destructive">{projectFileError}</div>
      )}

      {/* 输入区域 */}
      <div
        className={cn(
          'relative flex items-end gap-3 p-3 border rounded-lg transition-colors',
          'focus-within:border-primary',
          isDragging && 'border-primary bg-primary/5',
          disabled && 'opacity-50 cursor-not-allowed'
        )}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        {/* 浮动的 AI 选择标签（不占用垂直空间） */}
        {showModelSelector && hasAiOptions && (
          <div className="absolute -top-3 left-3 z-10" ref={pickerRef}>
            <button
              type="button"
              onClick={() => setPickerOpen((v) => !v)}
              disabled={disabled}
              className={cn(
                'px-2 py-0.5 rounded-full border bg-background text-xs shadow-sm',
                'hover:bg-accent hover:text-accent-foreground transition-colors',
                'disabled:opacity-50 disabled:cursor-not-allowed'
              )}
              title="选择模型"
            >
              {currentAiLabel}
              <span className="ml-1">▾</span>
            </button>
            {pickerOpen && (
              <div className="absolute left-0 bottom-full mb-2 w-64 max-h-64 overflow-auto bg-popover text-popover-foreground border rounded-md shadow-lg">
                {enabledModels.length > 0 && (
                  <>
                    <div className="px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">模型</div>
                    {enabledModels.map((model: any) => (
                        <button
                          key={model.id}
                          className={cn('w-full text-left px-3 py-1.5 text-sm hover:bg-accent hover:text-accent-foreground', selectedModelId === model.id && 'bg-accent/40')}
                          onClick={() => { onModelChange?.(model.id); setPickerOpen(false); }}
                        >
                          {model.name} ({model.model_name})
                        </button>
                    ))}
                  </>
                )}
                <div className="border-t" />
                <button
                  className="w-full text-left px-3 py-1.5 text-sm text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                  onClick={() => { onModelChange?.(null); setPickerOpen(false); }}
                >
                  清除选择
                </button>
              </div>
            )}
          </div>
        )}
        {/* 附件按钮 */}
        {allowAttachments && (
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={disabled}
            className="flex-shrink-0 p-2 text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            title="Attach files"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.172 7l-6.586 6.586a2 2 0 102.828 2.828l6.414-6.586a4 4 0 00-5.656-5.656l-6.415 6.585a6 6 0 108.486 8.486L20.5 13" />
            </svg>
          </button>
        )}

        {allowAttachments && showProjectFilePicker && (
          <div className="relative flex-shrink-0" ref={projectFilePickerRef}>
            <button
              type="button"
              onClick={() => { void handleToggleProjectFilePicker(); }}
              disabled={disabled || projectFileAttachingPath !== null}
              className={cn(
                'px-2 py-1 rounded-md border text-xs transition-colors',
                'text-muted-foreground hover:text-foreground hover:bg-accent',
                (disabled || projectFileAttachingPath !== null) && 'opacity-50 cursor-not-allowed'
              )}
              title="从当前项目选择文件"
            >
              项目文件
              <span className="ml-1">▾</span>
            </button>
            {projectFilePickerOpen && (
              <div className="absolute left-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
                <div className="px-3 py-2 border-b space-y-2">
                  <div className="space-y-1">
                    <div
                      className="text-[11px] text-muted-foreground truncate"
                      title={projectForFilePicker?.name || '当前项目'}
                    >
                      项目: {projectForFilePicker?.name || '当前项目'}
                    </div>
                    <div
                      className="text-[11px] text-muted-foreground truncate font-mono"
                      title={projectFilePathLabel || '/'}
                    >
                      路径: {projectFilePathLabel || '/'}
                    </div>
                  </div>
                  <input
                    type="text"
                    value={projectFileFilter}
                    onChange={(event) => setProjectFileFilter(event.target.value)}
                    placeholder="筛选文件（不区分大小写，支持模糊）..."
                    className="w-full rounded border bg-background px-2 py-1 text-xs outline-none focus:border-primary"
                  />
                </div>
                <div className="max-h-64 overflow-auto py-1">
                  {projectFileBusy ? (
                    <div className="px-3 py-2 text-xs text-muted-foreground">
                      {projectFileKeywordActive ? '搜索中...' : '加载中...'}
                    </div>
                  ) : (
                    <>
                      {!projectFileKeywordActive && projectFileParent && (
                        <button
                          type="button"
                          className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent"
                          onClick={() => { void loadProjectFileEntries(projectFileParent); }}
                        >
                          ..
                        </button>
                      )}
                      {displayedProjectFileEntries.map((entry) => (
                        <button
                          key={entry.path}
                          type="button"
                          className="w-full px-3 py-1.5 text-left text-sm hover:bg-accent flex items-center justify-between gap-2"
                          onClick={() => { void handleAttachProjectFile(entry); }}
                          disabled={projectFileAttachingPath !== null}
                        >
                          <span className="min-w-0 flex-1 truncate">
                            <span className="inline-flex items-center gap-1.5 min-w-0 max-w-full">
                              {entry.isDir ? (
                                <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                                  <path strokeLinecap="round" strokeLinejoin="round" d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                                </svg>
                              ) : (
                                <svg className="w-4 h-4 text-muted-foreground shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                                  <path strokeLinecap="round" strokeLinejoin="round" d="M7 3h7l5 5v13a1 1 0 01-1 1H7a1 1 0 01-1-1V4a1 1 0 011-1z" />
                                  <path strokeLinecap="round" strokeLinejoin="round" d="M14 3v6h6" />
                                </svg>
                              )}
                              <span className="truncate">{entry.name}</span>
                            </span>
                            {projectFileKeywordActive && !entry.isDir && (
                              <span className="block truncate text-[11px] text-muted-foreground">
                                {toRelativeProjectPath(entry.path)}
                              </span>
                            )}
                          </span>
                          {projectFileAttachingPath === entry.path && (
                            <span className="text-[11px] text-muted-foreground">处理中...</span>
                          )}
                        </button>
                      ))}
                      {displayedProjectFileEntries.length === 0 && !projectFileBusy && (
                        <div className="px-3 py-2 text-xs text-muted-foreground">
                          {projectFileKeywordActive ? '没有匹配的文件' : '当前目录没有可选文件'}
                        </div>
                      )}
                      {projectFileKeywordActive && projectFileSearchTruncated && (
                        <div className="px-3 py-2 text-[11px] text-muted-foreground border-t">
                          结果过多，已截断显示前 300 条
                        </div>
                      )}
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        )}

        {availableProjects.length > 0 && (
          <select
            value={selectedProjectId || ''}
            onChange={(event) => onProjectChange?.(event.target.value || null)}
            disabled={disabled || isStreaming || isStopping}
            className={cn(
              'flex-shrink-0 px-2 py-1 text-xs rounded-md border bg-background',
              'text-foreground focus:outline-none focus:ring-1 focus:ring-primary',
              (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
            )}
            title="发送时透传 project_root"
          >
            <option value="">请选择项目</option>
            {availableProjects.map((project: any) => (
              <option key={project.id} value={project.id}>
                {project.name}
              </option>
            ))}
          </select>
        )}

        <div className="relative flex-shrink-0" ref={mcpPickerRef}>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => onMcpEnabledChange?.(!mcpEnabled)}
              disabled={disabled || isStreaming || isStopping}
              className={cn(
                'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
                mcpEnabled
                  ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                  : 'bg-muted text-muted-foreground hover:text-foreground',
                (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
              )}
              title={mcpEnabled ? 'MCP 已开启' : 'MCP 已关闭'}
            >
              MCP {mcpEnabled ? '开' : '关'}
            </button>
            <button
              type="button"
              onClick={handleToggleMcpPicker}
              disabled={disabled || isStreaming || isStopping}
              className={cn(
                'px-2 py-1 rounded-md border text-xs transition-colors',
                'text-muted-foreground hover:text-foreground hover:bg-accent',
                (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
              )}
              title="选择当前对话可用 MCP"
            >
              MCP 选择
              <span className="ml-1">▾</span>
            </button>
          </div>
          {mcpPickerOpen && (
            <div className="absolute right-0 bottom-full mb-2 z-30 w-80 bg-popover text-popover-foreground border rounded-md shadow-lg">
              <div className="px-3 py-2 border-b flex items-center justify-between gap-2">
                <div className="min-w-0">
                  <div className="text-xs font-medium">MCP 选择</div>
                  <div className="text-[11px] text-muted-foreground">
                    {mcpEnabled
                      ? (isAllMcpSelected
                        ? `已选全部 (${selectableMcpIds.length || 0})`
                        : `已选 ${selectedMcpCount}/${selectableMcpIds.length || 0}`)
                      : 'MCP 总开关已关闭'}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => { void loadAvailableMcpConfigs(); }}
                  disabled={mcpConfigsLoading}
                  className="px-2 py-0.5 text-[11px] rounded border text-muted-foreground hover:text-foreground hover:bg-accent disabled:opacity-50"
                >
                  刷新
                </button>
              </div>

              <div className="max-h-72 overflow-auto py-1">
                {mcpConfigsLoading ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">加载中...</div>
                ) : mcpConfigsError ? (
                  <div className="px-3 py-3 text-xs text-destructive">{mcpConfigsError}</div>
                ) : availableMcpConfigs.length === 0 ? (
                  <div className="px-3 py-3 text-xs text-muted-foreground">暂无可用 MCP</div>
                ) : (
                  <>
                    <label className="w-full px-3 py-2 text-sm flex items-center gap-2 border-b">
                      <input
                        type="checkbox"
                        checked={isAllMcpSelected}
                        onChange={() => {
                          if (!mcpEnabled) {
                            onMcpEnabledChange?.(true);
                          }
                          handleSelectAllMcp();
                        }}
                        disabled={disabled || isStreaming || isStopping}
                      />
                      <span>全部可用</span>
                    </label>

                    {builtinMcpConfigs.length > 0 && (
                      <>
                        <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                          内置 MCP
                        </div>
                        {builtinMcpConfigs.map((item) => {
                          const projectDisabled = !hasRuntimeProject && PROJECT_REQUIRED_MCP_IDS.has(item.id);
                          const checked = !projectDisabled && (isAllMcpSelected || sanitizedEnabledMcpIds.includes(item.id));
                          return (
                            <label
                              key={item.id}
                              className={cn(
                                'w-full px-3 py-1.5 text-sm flex items-center gap-2',
                                projectDisabled ? 'opacity-50 cursor-not-allowed' : 'hover:bg-accent',
                              )}
                            >
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={() => {
                                  if (projectDisabled) {
                                    return;
                                  }
                                  if (!mcpEnabled) {
                                    onMcpEnabledChange?.(true);
                                  }
                                  handleToggleMcpSelection(item.id);
                                }}
                                disabled={disabled || isStreaming || isStopping || projectDisabled}
                              />
                              <span className="truncate" title={item.displayName}>{item.displayName}</span>
                              {projectDisabled && (
                                <span className="text-[11px] text-muted-foreground">需选择项目</span>
                              )}
                            </label>
                          );
                        })}
                      </>
                    )}

                    {customMcpConfigs.length > 0 && (
                      <>
                        <div className="px-3 pt-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                          自定义 MCP
                        </div>
                        {customMcpConfigs.map((item) => {
                          const checked = isAllMcpSelected || sanitizedEnabledMcpIds.includes(item.id);
                          return (
                            <label key={item.id} className="w-full px-3 py-1.5 text-sm flex items-center gap-2 hover:bg-accent">
                              <input
                                type="checkbox"
                                checked={checked}
                                onChange={() => {
                                  if (!mcpEnabled) {
                                    onMcpEnabledChange?.(true);
                                  }
                                  handleToggleMcpSelection(item.id);
                                }}
                                disabled={disabled || isStreaming || isStopping}
                              />
                              <span className="truncate" title={item.displayName}>{item.displayName}</span>
                            </label>
                          );
                        })}
                      </>
                    )}
                  </>
                )}
              </div>
            </div>
          )}
        </div>

        {reasoningSupported && (
          <button
            type="button"
            onClick={() => onReasoningToggle?.(!reasoningEnabled)}
            disabled={disabled || isStreaming || isStopping}
            className={cn(
              'flex-shrink-0 px-2 py-1 text-xs rounded-md transition-colors',
              reasoningEnabled
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'bg-muted text-muted-foreground hover:text-foreground',
              (disabled || isStreaming || isStopping) && 'opacity-50 cursor-not-allowed'
            )}
            title={reasoningEnabled ? '推理已开启' : '推理已关闭'}
          >
            推理 {reasoningEnabled ? '开' : '关'}
          </button>
        )}

        {/* 移除行内选择器，使用右上角浮动标签 */}

        {/* 文本输入 */}
        <textarea
          ref={textareaRef}
          value={message}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onPaste={handlePaste}
          placeholder={placeholder}
          disabled={disabled}
          className={cn(
            'flex-1 resize-none bg-transparent border-none outline-none',
            'placeholder:text-muted-foreground',
            'disabled:cursor-not-allowed'
          )}
          rows={1}
          style={{ minHeight: '24px', maxHeight: '200px' }}
        />

        {/* 右侧不再放选择器，避免靠近发送按钮 */}

        {/* 字符计数 */}
        <div className="flex-shrink-0 text-[11px] sm:text-xs text-muted-foreground tabular-nums">
          {message.length}/{maxLength}
        </div>

        {/* 发送/停止按钮 */}
        {isStreaming ? (
          <button
            onClick={() => {
              if (onStop && !isStopping) {
                onStop();
              }
            }}
            disabled={isStopping}
            className={cn(
              'flex-shrink-0 p-2 rounded-md transition-colors',
              isStopping
                ? 'bg-amber-500 text-white'
                : 'bg-red-500 text-white hover:bg-red-600',
              'disabled:opacity-50 disabled:cursor-not-allowed'
            )}
            title={isStopping ? '停止中...' : '停止生成'}
            style={{ backgroundColor: isStopping ? '#f59e0b' : '#ef4444', color: 'white' }}
          >
            {isStopping ? (
              <svg className="w-5 h-5 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3a9 9 0 109 9" />
              </svg>
            ) : (
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 6h12v12H6z" />
              </svg>
            )}
          </button>
        ) : (
          <button
            onClick={handleSend}
            disabled={disabled || isStreaming || isStopping || (!message.trim() && attachments.length === 0)}
            className={cn(
              'flex-shrink-0 p-2 rounded-md transition-colors',
              'disabled:opacity-50 disabled:cursor-not-allowed',
              (message.trim() || attachments.length > 0) && !disabled && !isStreaming && !isStopping
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'text-muted-foreground'
            )}
            title={showModelSelector && !selectedModelId ? "请先选择模型" : "Send message"}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
            </svg>
          </button>
        )}

        {/* 隐藏的文件输入 */}
        {allowAttachments && (
          <input
            ref={fileInputRef}
            type="file"
            multiple
            accept={supportedFileTypes.join(',')}
            onChange={handleFileSelect}
            className="hidden"
          />
        )}
      </div>

      {/* 拖拽提示 */}
      {isDragging && allowAttachments && (
        <div className="absolute inset-0 bg-primary/10 border-2 border-dashed border-primary rounded-lg flex items-center justify-center">
          <div className="text-center">
            <svg className="w-8 h-8 mx-auto text-primary mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
            </svg>
            <p className="text-sm font-medium text-primary">Drop files here to attach</p>
            <p className="text-[11px] text-muted-foreground mt-1">单文件≤{formatFileSize(MAX_FILE_BYTES)}，总计≤{formatFileSize(MAX_TOTAL_BYTES)}，最多 {MAX_ATTACHMENTS} 个</p>
          </div>
        </div>
      )}
    </div>
  );
};

export default InputArea;
