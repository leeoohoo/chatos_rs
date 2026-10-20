import { create } from "zustand";
import type { ProjectDirectoryListing, ProjectFileEntry } from "../domain/projectDirectory";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";

interface ProjectDirectoryState {
  ownerUserId: string | null;
  projectId: string | null;
  listings: Record<string, ProjectDirectoryListing>;
  expandedPaths: Set<string>;
  loadingPaths: Set<string>;
  selectedEntry: ProjectFileEntry | null;
  rootLoading: boolean;
  error: string | null;
  generation: number;
  requestSequence: number;
  activeRequests: Record<string, number>;
  activate(port: ProjectDirectoryPort, ownerUserId: string, projectId: string): Promise<void>;
  toggle(port: ProjectDirectoryPort, entry: ProjectFileEntry): Promise<void>;
  select(entry: ProjectFileEntry): void;
  refresh(port: ProjectDirectoryPort): Promise<void>;
  reset(): void;
}

export const useProjectDirectoryStore = create<ProjectDirectoryState>((set, get) => ({
  ownerUserId: null,
  projectId: null,
  listings: {},
  expandedPaths: new Set(),
  loadingPaths: new Set(),
  selectedEntry: null,
  rootLoading: false,
  error: null,
  generation: 0,
  requestSequence: 0,
  activeRequests: {},

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
      ownerUserId,
      projectId,
      listings: {},
      expandedPaths: new Set(),
      loadingPaths: new Set(),
      selectedEntry: null,
      rootLoading: false,
      error: null,
      generation: current.generation + 1,
      activeRequests: {},
    });
    await loadPath(port, "", false, get, set);
  },

  async toggle(port, entry) {
    if (!entry.isDirectory) {
      set({ selectedEntry: entry });
      return;
    }
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
      set({ expandedPaths, loadingPaths, activeRequests, selectedEntry: entry });
      return;
    }
    set({ expandedPaths: new Set([...current.expandedPaths, entry.path]), selectedEntry: entry });
    if (!current.listings[entry.path]) await loadPath(port, entry.path, false, get, set);
  },

  select(entry) {
    set({ selectedEntry: entry });
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId) return;
    const expanded = [...current.expandedPaths].sort(pathDepthOrder);
    await loadPath(port, "", true, get, set);
    for (const path of expanded) {
      const latest = get();
      if (!latest.expandedPaths.has(path)) continue;
      await loadPath(port, path, true, get, set);
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      projectId: null,
      listings: {},
      expandedPaths: new Set(),
      loadingPaths: new Set(),
      selectedEntry: null,
      rootLoading: false,
      error: null,
      generation: state.generation + 1,
      activeRequests: {},
    }));
  },
}));

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
    if (!accepts(get(), ownerUserId, projectId, path, generation, requestId)) return;
    if (listing.projectId !== projectId || listing.path !== path) {
      throw new Error("项目目录返回了不匹配的路径。");
    }
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
    if (!accepts(get(), ownerUserId, projectId, path, generation, requestId)) return;
    const latest = get();
    const loadingPaths = new Set(latest.loadingPaths);
    loadingPaths.delete(path);
    const activeRequests = { ...latest.activeRequests };
    delete activeRequests[path];
    set({
      loadingPaths,
      activeRequests,
      rootLoading: false,
      error: errorMessage(error),
    });
  }
}

function accepts(
  state: ProjectDirectoryState,
  ownerUserId: string,
  projectId: string,
  path: string,
  generation: number,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeRequests[path] === requestId
    && (path === "" || state.expandedPaths.has(path));
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
