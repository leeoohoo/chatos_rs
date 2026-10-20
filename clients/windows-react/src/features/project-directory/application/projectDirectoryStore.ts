import { create } from "zustand";
import type {
  ProjectContentMatch,
  ProjectDirectoryListing,
  ProjectFileContent,
  ProjectFileEntry,
  ProjectSearchSnapshot,
} from "../domain/projectDirectory";
import { fileNameFromPath } from "../domain/projectDirectory";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";

interface ProjectDirectoryState {
  ownerUserId: string | null;
  projectId: string | null;
  listings: Record<string, ProjectDirectoryListing>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedEntry: ProjectFileEntry | null;
  selectedFile: ProjectFileContent | null;
  selectedLine: number | null;
  previewLoading: boolean;
  previewError: string | null;
  searchText: string;
  searchSnapshot: ProjectSearchSnapshot | null;
  searchLoading: boolean;
  searchError: string | null;
  draft: string;
  isEditing: boolean;
  mutationKind: "save" | "create" | "delete" | null;
  mutationError: string | null;
  rootLoading: boolean;
  error: string | null;
  generation: number;
  requestSequence: number;
  activeRequests: Record<string, number>;
  activePreviewRequest: number | null;
  activeSearchRequest: number | null;
  activeMutationRequest: number | null;
  activate(port: ProjectDirectoryPort, ownerUserId: string, projectId: string): Promise<void>;
  toggle(port: ProjectDirectoryPort, entry: ProjectFileEntry): Promise<void>;
  openFile(port: ProjectDirectoryPort, entry: ProjectFileEntry, targetLine?: number): Promise<void>;
  openContentMatch(port: ProjectDirectoryPort, match: ProjectContentMatch): Promise<void>;
  openSearchEntry(port: ProjectDirectoryPort, entry: ProjectFileEntry): Promise<void>;
  setSearchText(value: string): void;
  search(port: ProjectDirectoryPort, query: string): Promise<void>;
  beginEditing(): void;
  setDraft(value: string): void;
  cancelEditing(): void;
  save(port: ProjectDirectoryPort): Promise<void>;
  createEntry(port: ProjectDirectoryPort, name: string, kind: "file" | "directory"): Promise<void>;
  deleteSelected(port: ProjectDirectoryPort): Promise<void>;
  refresh(port: ProjectDirectoryPort): Promise<void>;
  reset(): void;
}

const emptySearch = (query: string): ProjectSearchSnapshot => ({
  query,
  entries: [],
  content: [],
  entryTruncated: false,
  contentTruncated: false,
});

export const useProjectDirectoryStore = create<ProjectDirectoryState>((set, get) => ({
  ownerUserId: null,
  projectId: null,
  listings: {},
  expandedPaths: new Set(),
  loadingPaths: new Set(),
  selectedEntry: null,
  selectedFile: null,
  selectedLine: null,
  previewLoading: false,
  previewError: null,
  searchText: "",
  searchSnapshot: null,
  searchLoading: false,
  searchError: null,
  draft: "",
  isEditing: false,
  mutationKind: null,
  mutationError: null,
  rootLoading: false,
  error: null,
  generation: 0,
  requestSequence: 0,
  activeRequests: {},
  activePreviewRequest: null,
  activeSearchRequest: null,
  activeMutationRequest: null,

  async activate(port, ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projectId = projectInput.trim();
    if (!ownerUserId || !projectId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId === ownerUserId && current.projectId === projectId) {
      if (!current.listings[""] && !current.rootLoading) await loadPath(port, "", false, get, set);
      return;
    }
    set({
      ...resetPresentation(current),
      ownerUserId,
      projectId,
      generation: current.generation + 1,
    });
    await loadPath(port, "", false, get, set);
  },

  async toggle(port, entry) {
    if (!entry.isDirectory) {
      await get().openFile(port, entry);
      return;
    }
    invalidatePreview(get, set, entry);
    const current = get();
    if (current.expandedPaths.has(entry.path)) {
      const expandedPaths = new Set(current.expandedPaths);
      for (const path of expandedPaths) {
        if (path === entry.path || path.startsWith(`${entry.path}/`)) expandedPaths.delete(path);
      }
      const loadingPaths = new Set(current.loadingPaths);
      const activeRequests = { ...current.activeRequests };
      for (const path of Object.keys(activeRequests)) {
        if (path === entry.path || path.startsWith(`${entry.path}/`)) {
          delete activeRequests[path];
          loadingPaths.delete(path);
        }
      }
      set({ expandedPaths, loadingPaths, activeRequests });
      return;
    }
    set({ expandedPaths: new Set([...current.expandedPaths, entry.path]) });
    if (!current.listings[entry.path]) await loadPath(port, entry.path, false, get, set);
  },

  async openFile(port, entry, targetLine) {
    if (entry.isDirectory) return;
    const current = get();
    if (!current.ownerUserId || !current.projectId) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activePreviewRequest: requestId,
      selectedEntry: entry,
      selectedFile: null,
      selectedLine: targetLine ?? null,
      previewLoading: true,
      previewError: null,
      draft: "",
      isEditing: false,
      mutationError: null,
    });
    try {
      const file = await port.readFile({ ownerUserId, projectId, path: entry.path });
      if (!acceptsPreview(get(), ownerUserId, projectId, entry.path, generation, requestId)) return;
      if (file.projectId !== projectId || file.path !== entry.path) throw new Error("项目文件返回了不匹配的路径。");
      set({ selectedFile: file, previewLoading: false, previewError: null, activePreviewRequest: null });
    } catch (error) {
      if (!acceptsPreview(get(), ownerUserId, projectId, entry.path, generation, requestId)) return;
      set({ selectedFile: null, previewLoading: false, previewError: errorMessage(error), activePreviewRequest: null });
    }
  },

  async openContentMatch(port, match) {
    await get().openFile(port, {
      name: fileNameFromPath(match.path),
      path: match.path,
      isDirectory: false,
      isWritable: false,
      size: null,
      modifiedAtUnixMs: null,
    }, match.line);
  },

  async openSearchEntry(port, entry) {
    if (!entry.isDirectory) {
      await get().openFile(port, entry);
      return;
    }
    get().setSearchText("");
    invalidatePreview(get, set, entry);
    const current = get();
    set({ expandedPaths: new Set([...current.expandedPaths, entry.path]) });
    await loadPath(port, entry.path, false, get, set);
  },

  setSearchText(value) {
    const current = get();
    if (value === current.searchText) return;
    const requestId = current.requestSequence + 1;
    set({
      searchText: value,
      requestSequence: requestId,
      activeSearchRequest: null,
      searchLoading: false,
      searchSnapshot: value.trim() ? current.searchSnapshot : null,
      searchError: null,
    });
  },

  async search(port, queryInput) {
    const query = queryInput.trim();
    const current = get();
    if (!query || query !== current.searchText.trim() || !current.ownerUserId || !current.projectId) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeSearchRequest: requestId,
      searchLoading: true,
      searchSnapshot: emptySearch(query),
      searchError: null,
    });
    const [entryResult, contentResult] = await Promise.allSettled([
      port.searchEntries({ ownerUserId, projectId, path: "", query, limit: 100 }),
      port.searchContent({ ownerUserId, projectId, path: "", query, limit: 200 }),
    ]);
    if (!acceptsSearch(get(), ownerUserId, projectId, query, generation, requestId)) return;
    const errors: string[] = [];
    let snapshot = emptySearch(query);
    if (entryResult.status === "fulfilled") {
      if (entryResult.value.projectId !== projectId || entryResult.value.query !== query) errors.push("文件名搜索返回不匹配的上下文。");
      else snapshot = { ...snapshot, entries: entryResult.value.matches, entryTruncated: entryResult.value.isTruncated };
    } else errors.push(`文件名搜索：${errorMessage(entryResult.reason)}`);
    if (contentResult.status === "fulfilled") {
      if (contentResult.value.projectId !== projectId || contentResult.value.query !== query) errors.push("内容搜索返回不匹配的上下文。");
      else snapshot = { ...snapshot, content: contentResult.value.matches, contentTruncated: contentResult.value.isTruncated };
    } else errors.push(`内容搜索：${errorMessage(contentResult.reason)}`);
    set({
      searchSnapshot: snapshot,
      searchLoading: false,
      searchError: errors.length ? errors.join("；") : null,
      activeSearchRequest: null,
    });
  },

  beginEditing() {
    const current = get();
    const file = current.selectedFile;
    if (!file || file.isBinary || !file.isWritable || current.mutationKind) return;
    set({ draft: file.content, isEditing: true, mutationError: null });
  },

  setDraft(value) {
    if (!get().isEditing) return;
    set({ draft: value });
  },

  cancelEditing() {
    const current = get();
    if (current.mutationKind) return;
    set({ draft: current.selectedFile?.content ?? "", isEditing: false, mutationError: null });
  },

  async save(port) {
    const current = get();
    const file = current.selectedFile;
    if (!current.ownerUserId || !current.projectId || !current.isEditing || !file
      || file.isBinary || !file.isWritable || current.activeMutationRequest != null) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const path = file.path;
    const expectedRevision = file.revision;
    const content = current.draft;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeMutationRequest: requestId,
      mutationKind: "save",
      mutationError: null,
    });
    try {
      const result = await port.writeFile({ ownerUserId, projectId, path, expectedRevision, content });
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      if (result.projectId !== projectId || result.path !== path) throw new Error("文件保存返回了不匹配的路径。");
      const latest = get();
      if (latest.selectedEntry?.path !== path || latest.selectedFile?.path !== path) {
        finishMutation(get, set, requestId);
        return;
      }
      if (result.file) {
        if (result.file.projectId !== projectId || result.file.path !== path) throw new Error("文件保存返回了不匹配的内容。");
        set({
          selectedFile: result.file,
          selectedLine: null,
          draft: result.file.content,
          isEditing: false,
          mutationKind: null,
          mutationError: null,
          activeMutationRequest: null,
        });
      } else {
        set({
          selectedFile: { ...latest.selectedFile, content, size: new TextEncoder().encode(content).byteLength, isWritable: false },
          selectedLine: null,
          draft: content,
          isEditing: false,
          mutationKind: null,
          mutationError: "文件已保存，但最新内容暂时无法重新读取，请刷新目录。",
          activeMutationRequest: null,
        });
      }
    } catch (error) {
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      const stillSelected = get().selectedEntry?.path === path && get().selectedFile?.path === path;
      set({
        mutationKind: null,
        mutationError: stillSelected ? errorMessage(error) : null,
        activeMutationRequest: null,
      });
    }
  },

  async createEntry(port, nameInput, kind) {
    const current = get();
    const name = nameInput.trim();
    if (!current.ownerUserId || !current.projectId || current.activeMutationRequest != null) return;
    if (!name) {
      set({ mutationError: kind === "directory" ? "请输入文件夹名称。" : "请输入文件名称。" });
      return;
    }
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const parentPath = creationParent(current.selectedEntry);
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeMutationRequest: requestId,
      mutationKind: "create",
      mutationError: null,
    });
    try {
      const result = await port.createEntry({ ownerUserId, projectId, parentPath, name, kind });
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      const expectedPath = parentPath ? `${parentPath}/${name}` : name;
      if (result.projectId !== projectId || result.parentPath !== parentPath || result.path !== expectedPath) {
        throw new Error("项目文件创建返回了不匹配的目录。");
      }
      const expandedPaths = parentPath ? new Set([...get().expandedPaths, parentPath]) : get().expandedPaths;
      if (result.listing) {
        if (result.listing.projectId !== projectId || result.listing.path !== parentPath) throw new Error("项目文件创建返回了不匹配的目录内容。");
        set({
          listings: { ...get().listings, [parentPath]: result.listing },
          expandedPaths,
          mutationKind: null,
          mutationError: null,
          activeMutationRequest: null,
        });
      } else {
        set({
          expandedPaths,
          mutationKind: null,
          mutationError: "项目文件已创建，但目录暂时无法刷新。",
          activeMutationRequest: null,
        });
      }
    } catch (error) {
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      set({ mutationKind: null, mutationError: errorMessage(error), activeMutationRequest: null });
    }
  },

  async deleteSelected(port) {
    const current = get();
    const entry = current.selectedEntry;
    if (!current.ownerUserId || !current.projectId || !entry || current.activeMutationRequest != null) return;
    const selectedFile = current.selectedFile;
    if (!entry.isDirectory && (!selectedFile || selectedFile.path !== entry.path)) {
      set({ mutationError: "删除文件前需要先读取最新内容版本。" });
      return;
    }
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const path = entry.path;
    const kind = entry.isDirectory ? "directory" : "file";
    const expectedRevision = entry.isDirectory ? null : selectedFile!.revision;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeMutationRequest: requestId,
      mutationKind: "delete",
      mutationError: null,
    });
    try {
      const result = await port.deleteEntry({ ownerUserId, projectId, path, kind, expectedRevision });
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      if (result.projectId !== projectId || result.path !== path || result.parentPath !== parentPath(path)) {
        throw new Error("项目文件删除返回了不匹配的路径。");
      }
      const latest = get();
      const selectedInsideDeletedPath = latest.selectedEntry != null && isPathInside(latest.selectedEntry.path, path);
      const listings = { ...latest.listings };
      for (const cachedPath of Object.keys(listings)) {
        if (isPathInside(cachedPath, path)) delete listings[cachedPath];
      }
      if (result.listing) {
        if (result.listing.projectId !== projectId || result.listing.path !== result.parentPath) throw new Error("项目文件删除返回了不匹配的目录内容。");
        listings[result.parentPath] = result.listing;
      }
      const expandedPaths = new Set([...latest.expandedPaths].filter((expanded) => !isPathInside(expanded, path)));
      set({
        listings,
        expandedPaths,
        selectedEntry: selectedInsideDeletedPath ? null : latest.selectedEntry,
        selectedFile: selectedInsideDeletedPath ? null : latest.selectedFile,
        selectedLine: selectedInsideDeletedPath ? null : latest.selectedLine,
        draft: selectedInsideDeletedPath ? "" : latest.draft,
        isEditing: selectedInsideDeletedPath ? false : latest.isEditing,
        mutationKind: null,
        mutationError: result.listing ? null : "项目文件已删除，但目录暂时无法刷新。",
        activeMutationRequest: null,
      });
    } catch (error) {
      if (!acceptsMutation(get(), ownerUserId, projectId, generation, requestId)) return;
      set({ mutationKind: null, mutationError: errorMessage(error), activeMutationRequest: null });
    }
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId) return;
    const expanded = [...current.expandedPaths].sort(pathDepthOrder);
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activePreviewRequest: null,
      activeSearchRequest: null,
      selectedEntry: null,
      selectedFile: null,
      selectedLine: null,
      previewLoading: false,
      previewError: null,
      searchText: "",
      searchSnapshot: null,
      searchLoading: false,
      searchError: null,
      draft: "",
      isEditing: false,
      mutationKind: null,
      mutationError: null,
      activeMutationRequest: null,
    });
    await loadPath(port, "", true, get, set);
    for (const path of expanded) {
      const latest = get();
      if (!latest.expandedPaths.has(path)) continue;
      await loadPath(port, path, true, get, set);
    }
  },

  reset() {
    set((state) => ({ ...resetPresentation(state), generation: state.generation + 1 }));
  },
}));

function resetPresentation(state: ProjectDirectoryState): Partial<ProjectDirectoryState> {
  return {
    ownerUserId: null,
    projectId: null,
    listings: {},
    expandedPaths: new Set(),
    loadingPaths: new Set(),
    selectedEntry: null,
    selectedFile: null,
    selectedLine: null,
    previewLoading: false,
    previewError: null,
    searchText: "",
    searchSnapshot: null,
    searchLoading: false,
    searchError: null,
    draft: "",
    isEditing: false,
    mutationKind: null,
    mutationError: null,
    rootLoading: false,
    error: null,
    requestSequence: state.requestSequence + 1,
    activeRequests: {},
    activePreviewRequest: null,
    activeSearchRequest: null,
    activeMutationRequest: null,
  };
}

function invalidatePreview(
  get: () => ProjectDirectoryState,
  set: (partial: Partial<ProjectDirectoryState>) => void,
  selectedEntry: ProjectFileEntry,
) {
  const current = get();
  set({
    requestSequence: current.requestSequence + 1,
    activePreviewRequest: null,
    selectedEntry,
    selectedFile: null,
    selectedLine: null,
    previewLoading: false,
    previewError: null,
    draft: "",
    isEditing: false,
    mutationError: null,
  });
}

async function loadPath(
  port: ProjectDirectoryPort,
  path: string,
  forceRefresh: boolean,
  get: () => ProjectDirectoryState,
  set: (partial: Partial<ProjectDirectoryState>) => void,
) {
  const current = get();
  if (!current.ownerUserId || !current.projectId) return;
  const ownerUserId = current.ownerUserId;
  const projectId = current.projectId;
  const generation = current.generation;
  const requestId = current.requestSequence + 1;
  set({
    requestSequence: requestId,
    activeRequests: { ...current.activeRequests, [path]: requestId },
    loadingPaths: new Set([...current.loadingPaths, path]),
    rootLoading: path === "" ? current.listings[""] == null : current.rootLoading,
    error: null,
  });
  try {
    const listing = await port.loadDirectory({ ownerUserId, projectId, path, forceRefresh });
    if (!acceptsDirectory(get(), ownerUserId, projectId, path, generation, requestId)) return;
    if (listing.projectId !== projectId || listing.path !== path) throw new Error("项目目录返回了不匹配的路径。");
    const latest = get();
    const loadingPaths = new Set(latest.loadingPaths);
    loadingPaths.delete(path);
    const activeRequests = { ...latest.activeRequests };
    delete activeRequests[path];
    const listings = { ...latest.listings, [path]: listing };
    const childDirectories = new Set(listing.entries.filter((entry) => entry.isDirectory).map((entry) => entry.path));
    const expandedPaths = new Set([...latest.expandedPaths].filter((expanded) => (
      !isDirectChild(expanded, path) || childDirectories.has(expanded)
    )));
    set({ listings, expandedPaths, loadingPaths, activeRequests, rootLoading: false, error: null });
  } catch (error) {
    if (!acceptsDirectory(get(), ownerUserId, projectId, path, generation, requestId)) return;
    const latest = get();
    const loadingPaths = new Set(latest.loadingPaths);
    loadingPaths.delete(path);
    const activeRequests = { ...latest.activeRequests };
    delete activeRequests[path];
    set({ loadingPaths, activeRequests, rootLoading: false, error: errorMessage(error) });
  }
}

function acceptsDirectory(state: ProjectDirectoryState, ownerUserId: string, projectId: string, path: string, generation: number, requestId: number) {
  return state.ownerUserId === ownerUserId && state.projectId === projectId && state.generation === generation
    && state.activeRequests[path] === requestId && (path === "" || state.expandedPaths.has(path));
}

function acceptsPreview(state: ProjectDirectoryState, ownerUserId: string, projectId: string, path: string, generation: number, requestId: number) {
  return state.ownerUserId === ownerUserId && state.projectId === projectId && state.generation === generation
    && state.activePreviewRequest === requestId && state.selectedEntry?.path === path;
}

function acceptsSearch(state: ProjectDirectoryState, ownerUserId: string, projectId: string, query: string, generation: number, requestId: number) {
  return state.ownerUserId === ownerUserId && state.projectId === projectId && state.generation === generation
    && state.activeSearchRequest === requestId && state.searchText.trim() === query;
}

function acceptsMutation(state: ProjectDirectoryState, ownerUserId: string, projectId: string, generation: number, requestId: number) {
  return state.ownerUserId === ownerUserId && state.projectId === projectId && state.generation === generation
    && state.activeMutationRequest === requestId;
}

function finishMutation(
  get: () => ProjectDirectoryState,
  set: (partial: Partial<ProjectDirectoryState>) => void,
  requestId: number,
) {
  if (get().activeMutationRequest !== requestId) return;
  set({ mutationKind: null, mutationError: null, activeMutationRequest: null });
}

function creationParent(entry: ProjectFileEntry | null) {
  if (!entry) return "";
  if (entry.isDirectory) return entry.path;
  return parentPath(entry.path);
}

function parentPath(path: string) {
  const separator = path.lastIndexOf("/");
  return separator < 0 ? "" : path.slice(0, separator);
}

function isPathInside(candidate: string, root: string) {
  return candidate === root || candidate.startsWith(`${root}/`);
}

function isDirectChild(candidate: string, parent: string) {
  const prefix = parent === "" ? "" : `${parent}/`;
  if (!candidate.startsWith(prefix)) return false;
  return !candidate.slice(prefix.length).includes("/");
}

function pathDepthOrder(left: string, right: string) {
  return left.split("/").length - right.split("/").length || left.localeCompare(right);
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "项目目录暂时不可用。";
}
