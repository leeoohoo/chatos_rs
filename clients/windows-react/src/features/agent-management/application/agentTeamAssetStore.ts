import { create } from "zustand";
import type {
  AgentTeamAsset,
  AgentTeamAssetRevision,
  SaveAgentTeamAssetInput,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamAssetState {
  ownerUserId: string | null;
  teamId: string | null;
  assets: AgentTeamAsset[];
  revisions: Record<string, AgentTeamAssetRevision[]>;
  isLoading: boolean;
  mutationAssetId: string | null;
  loadingRevisionAssetId: string | null;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: AgentManagementPort, ownerUserId: string, teamId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  save(port: AgentManagementPort, input: SaveAgentTeamAssetInput): Promise<boolean>;
  archive(port: AgentManagementPort, asset: AgentTeamAsset): Promise<boolean>;
  loadRevisions(port: AgentManagementPort, asset: AgentTeamAsset, force?: boolean): Promise<void>;
  reset(): void;
}

export const useAgentTeamAssetStore = create<AgentTeamAssetState>((set, get) => ({
  ownerUserId: null,
  teamId: null,
  assets: [],
  revisions: {},
  isLoading: false,
  mutationAssetId: null,
  loadingRevisionAssetId: null,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, teamInput) {
    const ownerUserId = ownerInput.trim();
    const teamId = teamInput.trim();
    if (!ownerUserId || !teamId || !port.listTeamAssets) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.teamId !== teamId) {
      set({
        ownerUserId,
        teamId,
        assets: [],
        revisions: {},
        isLoading: false,
        mutationAssetId: null,
        loadingRevisionAssetId: null,
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.requestId !== 0 || !port.listTeamAssets) return;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoading: current.assets.length === 0, error: null });
    try {
      const assets = await port.listTeamAssets(ownerUserId, teamId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      if (assets.some((asset) => asset.teamId !== teamId || asset.status !== "active")) {
        throw new Error("团队共享资产身份不一致，请刷新后重试。");
      }
      set({ assets, isLoading: false, requestId: 0 });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      set({ isLoading: false, error: errorMessage(failure), requestId: 0 });
    }
  },

  async save(port, input) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.mutationAssetId || !port.saveTeamAsset) return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const mutationAssetId = input.assetId ?? "new";
    const sequence = current.sequence;
    set({ mutationAssetId, error: null });
    try {
      const saved = await port.saveTeamAsset(ownerUserId, teamId, input);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationAssetId !== mutationAssetId) return false;
      if (saved.teamId !== teamId || saved.status !== "active"
        || (input.assetId && saved.id !== input.assetId)
        || saved.revision !== (input.expectedRevision ?? 0) + 1) {
        throw new Error("团队共享资产保存结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        assets: [...latest.assets.filter((asset) => asset.id !== saved.id), saved]
          .sort(compareAssets),
        revisions: omitRevision(latest.revisions, saved.id),
        mutationAssetId: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationAssetId !== mutationAssetId) return false;
      set({ mutationAssetId: null, error: errorMessage(failure) });
      return false;
    }
  },

  async archive(port, asset) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.mutationAssetId || !port.archiveTeamAsset
      || asset.teamId !== current.teamId || asset.status !== "active") return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const sequence = current.sequence;
    set({ mutationAssetId: asset.id, error: null });
    try {
      const archived = await port.archiveTeamAsset(ownerUserId, teamId, asset.id, asset.revision);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationAssetId !== asset.id) return false;
      if (archived.id !== asset.id || archived.teamId !== teamId || archived.status !== "archived"
        || archived.revision !== asset.revision + 1) {
        throw new Error("团队共享资产归档结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        assets: latest.assets.filter((candidate) => candidate.id !== asset.id),
        revisions: omitRevision(latest.revisions, asset.id),
        mutationAssetId: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.mutationAssetId !== asset.id) return false;
      set({ mutationAssetId: null, error: errorMessage(failure) });
      return false;
    }
  },

  async loadRevisions(port, asset, force = false) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || !port.listTeamAssetRevisions
      || current.loadingRevisionAssetId || asset.teamId !== current.teamId
      || (!force && current.revisions[asset.id])) return;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const sequence = current.sequence;
    set({ loadingRevisionAssetId: asset.id, error: null });
    try {
      const revisions = await port.listTeamAssetRevisions(ownerUserId, teamId, asset.id);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.loadingRevisionAssetId !== asset.id) return;
      if (revisions.some((revision) => revision.assetId !== asset.id)) {
        throw new Error("团队共享资产版本身份不一致，请刷新后重试。");
      }
      set((latest) => ({
        revisions: { ...latest.revisions, [asset.id]: revisions },
        loadingRevisionAssetId: null,
      }));
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.sequence !== sequence
        || state.loadingRevisionAssetId !== asset.id) return;
      set({ loadingRevisionAssetId: null, error: errorMessage(failure) });
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      teamId: null,
      assets: [],
      revisions: {},
      isLoading: false,
      mutationAssetId: null,
      loadingRevisionAssetId: null,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

function compareAssets(left: AgentTeamAsset, right: AgentTeamAsset) {
  return left.category.localeCompare(right.category)
    || right.updatedAtUnixMs - left.updatedAtUnixMs
    || left.id.localeCompare(right.id);
}

function omitRevision(
  revisions: Record<string, AgentTeamAssetRevision[]>,
  assetId: string,
) {
  const next = { ...revisions };
  delete next[assetId];
  return next;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "团队共享资产暂时不可用。";
}
