import { create, type StoreApi } from "zustand";
import type { MediaStudioModel } from "../../media-studio/domain/mediaStudio";
import type { MediaStudioPort } from "../../media-studio/ports/MediaStudioPort";
import type { StoryProject } from "../domain/storyStudio";
import type { StoryStudioPort } from "../ports/StoryStudioPort";

interface StoryVideoProgress {
  status: string;
  percent: number | null;
}

interface StoryStudioState {
  ownerUserId: string | null;
  presentationId: string | null;
  projects: StoryProject[];
  selectedProjectId: string | null;
  models: MediaStudioModel[];
  loading: boolean;
  busy: boolean;
  error: string | null;
  unreadableCount: number;
  videoProgress: Record<string, StoryVideoProgress>;
  activeMediaTarget: string | null;
  activate(storyPort: StoryStudioPort, mediaPort: MediaStudioPort, ownerUserId: string): Promise<void>;
  deactivate(storyPort: StoryStudioPort): void;
  selectProject(storyPort: StoryStudioPort, projectId: string): Promise<void>;
  createProject(storyPort: StoryStudioPort, input: {
    title: string;
    description: string;
    textModelId: string;
    imageModelId: string;
    videoModelId: string;
  }): Promise<void>;
  saveSource(storyPort: StoryStudioPort, input: {
    source: string;
    style: string;
    ratio: string;
  }): Promise<void>;
  startPlanning(storyPort: StoryStudioPort, stage: "outline" | "refine", targetIds: string[], userIdeas: string): Promise<void>;
  generateAssetImage(storyPort: StoryStudioPort, resourceId: string, userIdeas: string): Promise<void>;
  generateFrameImage(storyPort: StoryStudioPort, segmentId: string, frameRole: "first" | "last", referenceResourceIds: string[], userIdeas: string): Promise<void>;
  confirmLatestResources(storyPort: StoryStudioPort, resourceIds: string[]): Promise<void>;
  confirmLatestFrames(storyPort: StoryStudioPort, segmentId: string, useLastFrame: boolean): Promise<void>;
  startVideo(storyPort: StoryStudioPort, segmentId: string, size: "768P" | "2K", userIdeas: string): Promise<void>;
  resumeVideo(storyPort: StoryStudioPort, segmentId: string): Promise<void>;
  clearError(): void;
}

let activationSequence = 0;
const pollTimers = new Map<string, number>();

export const useStoryStudioStore = create<StoryStudioState>((set, get) => ({
  ownerUserId: null,
  presentationId: null,
  projects: [],
  selectedProjectId: null,
  models: [],
  loading: false,
  busy: false,
  error: null,
  unreadableCount: 0,
  videoProgress: {},
  activeMediaTarget: null,

  async activate(storyPort, mediaPort, ownerUserId) {
    const sequence = ++activationSequence;
    clearPollTimers();
    const presentationId = crypto.randomUUID();
    set({
      ownerUserId,
      presentationId,
      projects: [],
      selectedProjectId: null,
      models: [],
      loading: true,
      busy: false,
      error: null,
      unreadableCount: 0,
      videoProgress: {},
      activeMediaTarget: null,
    });
    try {
      const [page, models] = await Promise.all([
        storyPort.loadProjects({ ownerUserId, presentationId, limit: 80 }),
        mediaPort.listModels(ownerUserId),
      ]);
      if (!isActive(get(), sequence, ownerUserId, presentationId)) return;
      set({
        projects: page.items,
        selectedProjectId: page.items[0]?.id ?? null,
        models,
        loading: false,
        unreadableCount: page.unreadableCount,
      });
    } catch (error) {
      if (!isActive(get(), sequence, ownerUserId, presentationId)) return;
      set({ loading: false, error: errorMessage(error, "剧情项目暂时无法读取。") });
    }
  },

  deactivate(storyPort) {
    activationSequence += 1;
    clearPollTimers();
    const { ownerUserId, presentationId } = get();
    set({
      ownerUserId: null,
      presentationId: null,
      projects: [],
      selectedProjectId: null,
      models: [],
      loading: false,
      busy: false,
      error: null,
      unreadableCount: 0,
      videoProgress: {},
      activeMediaTarget: null,
    });
    if (ownerUserId && presentationId) {
      void storyPort.discardPresentation({ ownerUserId, presentationId }).catch(() => undefined);
    }
  },

  async selectProject(storyPort, projectId) {
    const scope = activeScope(get());
    if (!scope) return;
    set({ selectedProjectId: projectId, loading: true, error: null });
    try {
      const project = await storyPort.loadProject({ ...scope, projectId });
      if (!sameScope(get(), scope) || get().selectedProjectId !== projectId) return;
      set((state) => ({ projects: replaceProject(state.projects, project), loading: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ loading: false, error: errorMessage(error, "剧情项目暂时无法打开。") });
    }
  },

  async createProject(storyPort, input) {
    const scope = activeScope(get());
    if (!scope || get().busy) return;
    set({ busy: true, error: null });
    try {
      const project = await storyPort.createProject({ ...scope, ...input });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: [project, ...state.projects.filter((value) => value.id !== project.id)],
        selectedProjectId: project.id,
        busy: false,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情项目创建失败。") });
    }
  },

  async saveSource(storyPort, input) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set({ busy: true, error: null });
    try {
      const saved = await storyPort.updateSource({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        ...input,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), busy: false }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情原文保存失败。") });
    }
  },

  async startPlanning(storyPort, stage, targetIds, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project || get().busy) return;
    set({ busy: true, error: null });
    try {
      const planned = await storyPort.startPlanning({
        ...scope,
        projectId: project.id,
        stage,
        targetIds,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: planned ? replaceProject(state.projects, planned) : state.projects,
        busy: false,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ busy: false, error: errorMessage(error, "剧情规划没有完成。") });
    }
  },

  async generateAssetImage(storyPort, resourceId, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `resource:${resourceId}`, error: null });
    try {
      const saved = await storyPort.generateAssetImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        resourceId,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "剧情素材图片生成失败。") });
    }
  },

  async generateFrameImage(storyPort, segmentId, frameRole, referenceResourceIds, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `frame:${segmentId}:${frameRole}`, error: null });
    try {
      const saved = await storyPort.generateFrameImage({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        frameRole,
        referenceResourceIds,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        projects: replaceProject(state.projects, saved),
        activeMediaTarget: null,
      }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "剧情首尾帧生成失败。") });
    }
  },

  async confirmLatestResources(storyPort, resourceIds) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: "confirm:resources", error: null });
    try {
      const saved = await storyPort.confirmLatestResourceImages({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        resourceIds,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), activeMediaTarget: null }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "素材版本确认失败。") });
    }
  },

  async confirmLatestFrames(storyPort, segmentId, useLastFrame) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().activeMediaTarget) return;
    set({ activeMediaTarget: `confirm:frames:${segmentId}`, error: null });
    try {
      const saved = await storyPort.confirmLatestFrames({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        useConfirmedLastFrameForVideo: useLastFrame,
      });
      if (!sameScope(get(), scope)) return;
      set((state) => ({ projects: replaceProject(state.projects, saved), activeMediaTarget: null }));
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set({ activeMediaTarget: null, error: errorMessage(error, "首尾帧版本确认失败。") });
    }
  },

  async startVideo(storyPort, segmentId, size, userIdeas) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project?.baseFingerprint || get().busy) return;
    set((state) => ({
      error: null,
      videoProgress: { ...state.videoProgress, [segmentId]: { status: "submitting", percent: null } },
    }));
    try {
      const outcome = await storyPort.startVideo({
        ...scope,
        projectId: project.id,
        baseFingerprint: project.baseFingerprint,
        segmentId,
        size,
        userIdeas,
      });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, storyPort, scope, segmentId, outcome);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频提交失败。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  },

  async resumeVideo(storyPort, segmentId) {
    const scope = activeScope(get());
    const project = selectedProject(get());
    if (!scope || !project) return;
    set((state) => ({
      error: null,
      videoProgress: { ...state.videoProgress, [segmentId]: { status: "checking", percent: null } },
    }));
    try {
      const outcome = await storyPort.pollVideo({
        ...scope,
        projectId: project.id,
        segmentId,
      });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, storyPort, scope, segmentId, outcome);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频任务无法恢复。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  },

  clearError() {
    set({ error: null });
  },
}));

function acceptVideoOutcome(
  set: StoreApi<StoryStudioState>["setState"],
  get: StoreApi<StoryStudioState>["getState"],
  port: StoryStudioPort,
  scope: { ownerUserId: string; presentationId: string },
  segmentId: string,
  outcome: Awaited<ReturnType<StoryStudioPort["startVideo"]>>,
) {
  set((state) => ({
    projects: replaceProject(state.projects, outcome.project),
    videoProgress: outcome.terminal
      ? withoutKey(state.videoProgress, segmentId)
      : { ...state.videoProgress, [segmentId]: { status: outcome.status, percent: outcome.percent } },
  }));
  if (outcome.terminal) return;
  const key = `${scope.presentationId}:${outcome.project.id}:${segmentId}`;
  const timer = window.setTimeout(async () => {
    pollTimers.delete(key);
    if (!sameScope(get(), scope)) return;
    try {
      const next = await port.pollVideo({ ...scope, projectId: outcome.project.id, segmentId });
      if (!sameScope(get(), scope)) return;
      acceptVideoOutcome(set, get, port, scope, segmentId, next);
    } catch (error) {
      if (!sameScope(get(), scope)) return;
      set((state) => ({
        error: errorMessage(error, "剧情视频状态查询失败。"),
        videoProgress: withoutKey(state.videoProgress, segmentId),
      }));
    }
  }, 10_000);
  pollTimers.set(key, timer);
}

function selectedProject(state: StoryStudioState) {
  return state.projects.find((project) => project.id === state.selectedProjectId) ?? null;
}

function activeScope(state: StoryStudioState) {
  return state.ownerUserId && state.presentationId
    ? { ownerUserId: state.ownerUserId, presentationId: state.presentationId }
    : null;
}

function sameScope(state: StoryStudioState, scope: { ownerUserId: string; presentationId: string }) {
  return state.ownerUserId === scope.ownerUserId && state.presentationId === scope.presentationId;
}

function isActive(
  state: StoryStudioState,
  sequence: number,
  ownerUserId: string,
  presentationId: string,
) {
  return sequence === activationSequence && sameScope(state, { ownerUserId, presentationId });
}

function replaceProject(projects: StoryProject[], project: StoryProject) {
  return projects.map((value) => value.id === project.id ? project : value);
}

function withoutKey<T>(values: Record<string, T>, key: string) {
  const next = { ...values };
  delete next[key];
  return next;
}

function clearPollTimers() {
  for (const timer of pollTimers.values()) window.clearTimeout(timer);
  pollTimers.clear();
}

function errorMessage(error: unknown, fallback: string) {
  if (typeof error === "string" && error.trim()) return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  return fallback;
}
